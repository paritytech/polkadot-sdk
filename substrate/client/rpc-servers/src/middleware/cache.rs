// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! RPC middleware for caching deterministic responses to block queries.
//!
//! Caches the result payload of RPC responses for methods in the allowlist when the
//! request includes an explicit block hash. Requests without a block hash (targeting
//! "latest") are not cached since the result changes every block.
//!
//! Caching is safe for any block hash because the response is deterministic — a block hash
//! uniquely identifies the block and its state. Non-finalized block entries are naturally
//! evicted by LRU when the cache is full.
//!
//! **Metrics note**: The cache layer sits outside the RPC metrics layer, so cache hits are
//! NOT reflected in `substrate_rpc_calls_total` or latency histograms. Use
//! `substrate_rpc_cache_hits_total` (broken down by method) to account for cached call
//! volume.

// TODO: Consider resolving "latest" (missing block hash) to the current best block hash
// so that repeated queries targeting the same "latest" block can hit the cache within the
// same block production interval (~6s). This would require the middleware to have access
// to the blockchain backend.

use std::{
	borrow::Cow,
	collections::hash_map::DefaultHasher,
	hash::{Hash, Hasher},
	sync::Arc,
};

use futures::future::{BoxFuture, FutureExt};
use jsonrpsee::{
	server::middleware::rpc::RpcServiceT,
	types::{Id, Request},
	MethodResponse, ResponsePayload,
};
use parking_lot::Mutex;
use prometheus_endpoint::{
	register, Counter, CounterVec, Gauge, Opts, PrometheusError, Registry, U64,
};
use schnellru::LruMap;

/// Returns `true` if the method is cacheable.
///
/// All `state_*` and `childstate_*` methods are cacheable — they run against block state
/// and always take `Option<Hash>` as the **last** parameter.
/// Subscription-related methods (`subscribe`/`unsubscribe`) are excluded.
fn is_cacheable(method: &str) -> bool {
	(method.starts_with("state_") || method.starts_with("childstate_")) &&
		!method.contains("subscribe")
}

/// Returns `true` if `s` looks like a 32-byte hex-encoded block hash (`0x` + 64 hex chars).
fn is_block_hash(s: &str) -> bool {
	s.len() == 66 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Check whether the last element of a JSON params array is a block hash.
/// Returns the canonical (whitespace-normalized) params on success.
///
/// A block hash is a 66-character hex string (`0x` + 64 hex chars = 32 bytes).
///
/// Returns `Cow::Borrowed` when the input is already in canonical form (the
/// common case for JSON-RPC clients), avoiding an allocation.
fn check_block_hash_param(params: &str) -> Option<Cow<'_, str>> {
	let parsed: serde_json::Value = serde_json::from_str(params).ok()?;
	let arr = parsed.as_array().filter(|a| !a.is_empty())?;
	match arr.last().and_then(|v| v.as_str()) {
		Some(s) if is_block_hash(s) => {},
		_ => return None,
	}
	let canonical = serde_json::to_string(&parsed).expect("re-serialization cannot fail");
	if canonical == params {
		Some(Cow::Borrowed(params))
	} else {
		Some(Cow::Owned(canonical))
	}
}

/// Compute a cache key from the method name and params string.
fn cache_key(method: &str, canonical_params: &str) -> u64 {
	let mut hasher = DefaultHasher::new();
	method.hash(&mut hasher);
	canonical_params.hash(&mut hasher);
	hasher.finish()
}

/// A cached RPC response. Stores the JSON `"result"` field value (not the full envelope).
struct CachedResponse {
	/// Method name used to verify cache key integrity on hit.
	method: String,
	/// Canonical params used to verify cache key integrity on hit.
	params: String,
	/// The `"result"` field from the JSON-RPC response, stored as pre-validated
	/// `RawValue` so cache hits can build a `MethodResponse` without re-parsing.
	result: Box<serde_json::value::RawValue>,
	/// Byte size estimate for the limiter.
	/// Estimated memory footprint: struct overhead + string content.
	byte_size: usize,
}

impl CachedResponse {
	fn new(method: String, params: String, result: Box<serde_json::value::RawValue>) -> Self {
		let byte_size =
			std::mem::size_of::<Self>() + method.len() + params.len() + result.get().len();
		Self { method, params, result, byte_size }
	}

	/// Returns `true` if this entry matches the given method and params.
	fn matches(&self, method: &str, params: &str) -> bool {
		self.method == method && self.params == params
	}
}

/// Extract the `"result"` field from a JSON-RPC response string as a `RawValue`.
fn extract_result(response_json: &str) -> Option<Box<serde_json::value::RawValue>> {
	#[derive(serde::Deserialize)]
	struct Envelope {
		result: Box<serde_json::value::RawValue>,
	}

	let envelope: Envelope = serde_json::from_str(response_json).ok()?;
	Some(envelope.result)
}

/// Reconstruct a [`MethodResponse`] from a cached `RawValue` result and a request ID.
///
/// `MethodResponse::response` serializes immediately, so the borrow from the cached
/// `Box<RawValue>` only needs to live for this call.
fn response_from_cache(id: Id<'_>, result: &Box<serde_json::value::RawValue>) -> MethodResponse {
	MethodResponse::response(id, ResponsePayload::success_borrowed(result), usize::MAX)
}

// --- Byte-size limiter for schnellru ---

struct ByteSizeLimiter {
	max_bytes: usize,
	current_bytes: usize,
	metrics: Option<CacheMetrics>,
}

impl ByteSizeLimiter {
	fn new(max_bytes: usize, metrics: Option<CacheMetrics>) -> Self {
		Self { max_bytes, current_bytes: 0, metrics }
	}
}

impl schnellru::Limiter<u64, CachedResponse> for ByteSizeLimiter {
	type KeyToInsert<'a> = u64;
	type LinkType = u32;

	fn is_over_the_limit(&self, _length: usize) -> bool {
		self.current_bytes > self.max_bytes
	}

	fn on_insert(
		&mut self,
		_length: usize,
		key: u64,
		value: CachedResponse,
	) -> Option<(u64, CachedResponse)> {
		if self.max_bytes == 0 {
			return None;
		}
		self.current_bytes = self.current_bytes.saturating_add(value.byte_size);
		if let Some(ref metrics) = self.metrics {
			metrics.entries.inc();
			metrics.size_bytes.set(self.current_bytes as u64);
		}
		Some((key, value))
	}

	fn on_replace(
		&mut self,
		_length: usize,
		_old_key: &mut u64,
		_new_key: u64,
		old_value: &mut CachedResponse,
		new_value: &mut CachedResponse,
	) -> bool {
		self.current_bytes = self.current_bytes.saturating_sub(old_value.byte_size);
		self.current_bytes = self.current_bytes.saturating_add(new_value.byte_size);
		if let Some(ref metrics) = self.metrics {
			metrics.size_bytes.set(self.current_bytes as u64);
		}
		true
	}

	fn on_removed(&mut self, _key: &mut u64, value: &mut CachedResponse) {
		self.current_bytes = self.current_bytes.saturating_sub(value.byte_size);
		log::trace!(
			target: "rpc_cache",
			"Cache eviction: freed={}B, remaining={}B",
			value.byte_size,
			self.current_bytes,
		);
		if let Some(ref metrics) = self.metrics {
			metrics.evictions.inc();
			metrics.entries.dec();
			metrics.size_bytes.set(self.current_bytes as u64);
		}
	}

	fn on_cleared(&mut self) {
		self.current_bytes = 0;
		if let Some(ref metrics) = self.metrics {
			metrics.size_bytes.set(0);
			metrics.entries.set(0);
		}
	}

	fn on_grow(&mut self, _new_memory_usage: usize) -> bool {
		true
	}
}

// --- Metrics ---

/// Prometheus metrics for the RPC cache.
#[derive(Debug, Clone)]
pub struct CacheMetrics {
	/// Cache hits by method.
	hits: CounterVec<U64>,
	/// Cache misses by method.
	misses: CounterVec<U64>,
	/// Current cache size in bytes.
	size_bytes: Gauge<U64>,
	/// Current number of cached entries.
	entries: Gauge<U64>,
	/// Total number of evictions.
	evictions: Counter<U64>,
}

impl CacheMetrics {
	/// Register cache metrics on the given prometheus registry.
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			hits: register(
				CounterVec::new(
					Opts::new("substrate_rpc_cache_hits_total", "Total RPC cache hits"),
					&["method"],
				)?,
				registry,
			)?,
			misses: register(
				CounterVec::new(
					Opts::new("substrate_rpc_cache_misses_total", "Total RPC cache misses"),
					&["method"],
				)?,
				registry,
			)?,
			size_bytes: register(
				Gauge::new("substrate_rpc_cache_size_bytes", "Current RPC cache size in bytes")?,
				registry,
			)?,
			entries: register(
				Gauge::new(
					"substrate_rpc_cache_entries",
					"Current number of entries in the RPC cache",
				)?,
				registry,
			)?,
			evictions: register(
				Counter::new(
					"substrate_rpc_cache_evictions_total",
					"Total number of RPC cache evictions",
				)?,
				registry,
			)?,
		})
	}

	fn on_hit(&self, method: &str) {
		self.hits.with_label_values(&[method]).inc();
	}

	fn on_miss(&self, method: &str) {
		self.misses.with_label_values(&[method]).inc();
	}
}

// --- Cache layer and middleware ---

type Cache = Arc<Mutex<LruMap<u64, CachedResponse, ByteSizeLimiter>>>;

/// Tower layer that wraps services with [`CacheMiddleware`].
#[derive(Clone)]
pub struct CacheLayer {
	cache: Cache,
	metrics: Option<CacheMetrics>,
}

impl CacheLayer {
	/// Create a new cache layer.
	///
	/// - `max_cache_size`: maximum cache size in bytes. Pass 0 to disable caching.
	/// - `metrics`: optional prometheus metrics.
	pub fn new(max_cache_size: usize, metrics: Option<CacheMetrics>) -> Self {
		let limiter = ByteSizeLimiter::new(max_cache_size, metrics.clone());
		let cache = Arc::new(Mutex::new(LruMap::new(limiter)));
		Self { cache, metrics }
	}
}

impl<S> tower::Layer<S> for CacheLayer {
	type Service = CacheMiddleware<S>;

	fn layer(&self, service: S) -> Self::Service {
		CacheMiddleware { service, cache: self.cache.clone(), metrics: self.metrics.clone() }
	}
}

/// RPC middleware that caches deterministic responses for block queries
/// with an explicit block hash.
pub struct CacheMiddleware<S> {
	service: S,
	cache: Cache,
	metrics: Option<CacheMetrics>,
}

impl<S: Clone> Clone for CacheMiddleware<S> {
	fn clone(&self) -> Self {
		Self {
			service: self.service.clone(),
			cache: self.cache.clone(),
			metrics: self.metrics.clone(),
		}
	}
}

impl<'a, S> RpcServiceT<'a> for CacheMiddleware<S>
where
	S: Send + Sync + RpcServiceT<'a> + Clone + 'static,
{
	type Future = BoxFuture<'a, MethodResponse>;

	fn call(&self, req: Request<'a>) -> Self::Future {
		let method = req.method_name();

		// Check if this method is cacheable.
		if !is_cacheable(method) {
			let service = self.service.clone();
			return async move { service.call(req).await }.boxed();
		}

		// Check for explicit block hash in params and normalize for cache key.
		let params = req.params();
		let params_str = params.as_str().unwrap_or("[]");
		let canonical_params = match check_block_hash_param(params_str) {
			Some(p) => p,
			None => {
				let service = self.service.clone();
				return async move { service.call(req).await }.boxed();
			},
		};

		// Compute cache key and check for hit.
		let key = cache_key(method, &canonical_params);
		{
			let mut cache = self.cache.lock();
			if let Some(cached) = cache.get(&key) {
				if cached.matches(method, &canonical_params) {
					let rp = response_from_cache(req.id(), &cached.result);
					if let Some(ref metrics) = self.metrics {
						metrics.on_hit(method);
					}
					log::trace!(
						target: "rpc_cache",
						"Cache hit for {} (key={:#x}, cached_size={}B)",
						method,
						key,
						cached.byte_size,
					);
					return async move { rp }.boxed();
				}
			}
		}

		// Cache miss — forward to inner service.
		let service = self.service.clone();
		let cache = self.cache.clone();
		let metrics = self.metrics.clone();
		let method_name = method.to_string();
		let canonical_owned = canonical_params.into_owned();

		async move {
			let rp = service.call(req).await;

			// Only cache successful responses.
			if rp.is_success() {
				if let Some(result) = extract_result(rp.as_result()) {
					let cached =
						CachedResponse::new(method_name.clone(), canonical_owned, result);
					let byte_size = cached.byte_size;
					let mut cache_guard = cache.lock();
					cache_guard.insert(key, cached);
					let entries = cache_guard.len();
					let total_bytes = cache_guard.limiter().current_bytes;
					drop(cache_guard);
					log::trace!(
						target: "rpc_cache",
						"Cache miss for {} (key={:#x}, response_size={}B, total_entries={}, total_size={}B)",
						method_name,
						key,
						byte_size,
						entries,
						total_bytes,
					);
					if let Some(ref metrics) = metrics {
						metrics.on_miss(&method_name);
					}
				}
			}

			rp
		}
		.boxed()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tower::Layer;

	/// Per-entry struct overhead added by `CachedResponse::new`.
	const OVERHEAD: usize = std::mem::size_of::<CachedResponse>();

	/// Helper to build a `CachedResponse` with empty method/params for limiter tests.
	///
	/// Generates a JSON string value whose `RawValue` representation is exactly
	/// `json_len` bytes (e.g. `json_len=7` → `"aaaaa"` which is 5 chars + 2 quotes).
	fn cached_response(json_len: usize) -> CachedResponse {
		assert!(json_len >= 2, "minimum json_len is 2 for an empty JSON string");
		let fill = "a".repeat(json_len - 2);
		let json = format!(r#""{}""#, fill);
		let raw: Box<serde_json::value::RawValue> = serde_json::from_str(&json).unwrap();
		debug_assert_eq!(raw.get().len(), json_len);
		CachedResponse::new(String::new(), String::new(), raw)
	}

	// --- is_cacheable tests ---

	#[test]
	fn cacheable_methods() {
		assert!(is_cacheable("state_call"));
		assert!(is_cacheable("state_callAt"));
		assert!(is_cacheable("state_getStorage"));
		assert!(is_cacheable("state_getRuntimeVersion"));
		assert!(is_cacheable("state_getMetadata"));
		assert!(is_cacheable("state_getKeysPaged"));
		assert!(is_cacheable("childstate_getStorage"));
		assert!(is_cacheable("childstate_getKeysPaged"));
	}

	#[test]
	fn non_cacheable_methods() {
		assert!(!is_cacheable("system_health"));
		assert!(!is_cacheable("chain_getBlock"));
		assert!(!is_cacheable("chain_getHeader"));
		assert!(!is_cacheable("chain_getBlockHash"));
		assert!(!is_cacheable("author_submitExtrinsic"));
		assert!(!is_cacheable("state_subscribeRuntimeVersion"));
		assert!(!is_cacheable("state_unsubscribeRuntimeVersion"));
		assert!(!is_cacheable("state_subscribeStorage"));
		assert!(!is_cacheable("state_unsubscribeStorage"));
	}

	// --- is_block_hash tests ---

	#[test]
	fn is_block_hash_valid_lowercase() {
		assert!(is_block_hash(HASH_66));
	}

	#[test]
	fn is_block_hash_valid_uppercase() {
		assert!(is_block_hash(
			"0xABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890"
		));
	}

	#[test]
	fn is_block_hash_valid_mixed_case() {
		assert!(is_block_hash(
			"0xaBcDeF1234567890abcdef1234567890ABCDEF1234567890abcdef1234567890"
		));
	}

	#[test]
	fn is_block_hash_rejects_too_short() {
		assert!(!is_block_hash("0xabcd"));
	}

	#[test]
	fn is_block_hash_rejects_too_long() {
		let long = format!("0x{}", "a".repeat(65));
		assert!(!is_block_hash(&long));
	}

	#[test]
	fn is_block_hash_rejects_missing_prefix() {
		let no_prefix = "a".repeat(66);
		assert!(!is_block_hash(&no_prefix));
	}

	#[test]
	fn is_block_hash_rejects_non_hex() {
		// 'g' is not a hex character.
		let bad = format!("0x{}g{}", "a".repeat(31), "b".repeat(32));
		assert_eq!(bad.len(), 66);
		assert!(!is_block_hash(&bad));
	}

	#[test]
	fn is_block_hash_rejects_empty() {
		assert!(!is_block_hash(""));
	}

	// --- check_block_hash_param tests ---

	const HASH_66: &str = "0xf1212766e424bdc1f8f1d2c11ee236cff70ad77cad64d82b7823acc1e682a815";

	#[test]
	fn detects_block_hash_as_last_param() {
		let params = format!(r#"["ReviveApi_eth_block", "0x", "{}"]"#, HASH_66);
		assert!(check_block_hash_param(&params).is_some());

		let params = format!(r#"["{}"]"#, HASH_66);
		assert!(check_block_hash_param(&params).is_some());
	}

	#[test]
	fn rejects_when_hash_omitted() {
		assert!(check_block_hash_param(r#"["ReviveApi_eth_block", "0x"]"#).is_none());
	}

	#[test]
	fn rejects_when_null() {
		assert!(check_block_hash_param(r#"["method", "0x", null]"#).is_none());
	}

	#[test]
	fn rejects_when_empty_params() {
		assert!(check_block_hash_param(r#"[]"#).is_none());
	}

	#[test]
	fn rejects_when_wrong_length() {
		assert!(check_block_hash_param(r#"["method", "0xdata"]"#).is_none());
		assert!(check_block_hash_param(r#"["0x"]"#).is_none());
	}

	#[test]
	fn rejects_non_hex_characters_in_hash() {
		// 0x + 64 chars, but 'zz' are not hex.
		let bad_hash = format!("0xzz{}", "a".repeat(62));
		assert_eq!(bad_hash.len(), 66);
		let params = format!(r#"["{}"]"#, bad_hash);
		assert!(check_block_hash_param(&params).is_none());
	}

	#[test]
	fn rejects_invalid_json() {
		assert!(check_block_hash_param("not json").is_none());
	}

	#[test]
	fn normalizes_whitespace() {
		let p1 = format!(r#"["a",  "0x",  "{}"]"#, HASH_66);
		let p2 = format!(r#"["a","0x","{}"]"#, HASH_66);
		let n1 = check_block_hash_param(&p1).unwrap();
		let n2 = check_block_hash_param(&p2).unwrap();
		assert_eq!(n1, n2);
		assert_eq!(cache_key("state_call", &n1), cache_key("state_call", &n2));
	}

	#[test]
	fn compact_params_return_borrowed() {
		let params = format!(r#"["a","0x","{}"]"#, HASH_66);
		let result = check_block_hash_param(&params).unwrap();
		assert!(matches!(result, Cow::Borrowed(_)));
	}

	// --- Helper: extract_result ---

	#[test]
	fn extract_result_works() {
		let response = r#"{"jsonrpc":"2.0","result":"0x1234","id":1}"#;
		let result = extract_result(response).unwrap();
		assert_eq!(result.get(), r#""0x1234""#);
	}

	#[test]
	fn extract_result_complex() {
		let response = r#"{"jsonrpc":"2.0","result":{"key":"val","num":42},"id":1}"#;
		let result = extract_result(response).unwrap();
		assert_eq!(result.get(), r#"{"key":"val","num":42}"#);
	}

	#[test]
	fn extract_result_error_response() {
		let response = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid"},"id":1}"#;
		assert!(extract_result(response).is_none());
	}

	// --- response_from_cache ---

	fn raw_value(json: &str) -> Box<serde_json::value::RawValue> {
		serde_json::from_str(json).unwrap()
	}

	#[test]
	fn response_from_cache_simple() {
		let raw = raw_value(r#""0x1234""#);
		let rp = response_from_cache(Id::Number(42), &raw);
		assert!(rp.is_success());
		let result_str = rp.as_result();
		assert!(result_str.contains(r#""result":"0x1234""#));
		assert!(result_str.contains(r#""id":42"#));
	}

	#[test]
	fn response_from_cache_different_id() {
		let raw = raw_value(r#""0xaaa""#);
		let rp1 = response_from_cache(Id::Number(1), &raw);
		let rp2 = response_from_cache(Id::Number(99), &raw);
		assert!(rp1.as_result().contains(r#""id":1"#));
		assert!(rp2.as_result().contains(r#""id":99"#));
	}

	// --- Byte-size limiter ---

	#[test]
	fn eviction_on_byte_limit() {
		// Budget fits one 60-byte entry but not two.
		let limiter = ByteSizeLimiter::new(60 + OVERHEAD + 1, None);
		let mut cache = LruMap::new(limiter);

		// Insert entries that exceed the limit.
		cache.insert(1, cached_response(60));
		cache.insert(2, cached_response(60));

		// First entry should have been evicted.
		assert!(cache.get(&1).is_none());
		assert!(cache.get(&2).is_some());
	}

	#[test]
	fn size_tracking_accurate() {
		let limiter = ByteSizeLimiter::new(10000, None);
		let mut cache = LruMap::new(limiter);

		cache.insert(1, cached_response(100));
		cache.insert(2, cached_response(200));

		assert_eq!(cache.limiter().current_bytes, 300 + 2 * OVERHEAD);
	}

	#[test]
	fn zero_size_cache_rejects_all() {
		let limiter = ByteSizeLimiter::new(0, None);
		let mut cache = LruMap::new(limiter);

		cache.insert(1, cached_response(10));
		assert!(cache.get(&1).is_none());
	}

	#[test]
	fn single_entry_larger_than_budget_evicts_immediately() {
		// Budget fits one small entry but not the large one.
		let limiter = ByteSizeLimiter::new(10 + OVERHEAD + 1, None);
		let mut cache = LruMap::new(limiter);

		// Insert a small entry first.
		cache.insert(1, cached_response(10));
		assert!(cache.get(&1).is_some());

		// Insert an entry larger than the entire budget — should evict everything
		// (including itself) to satisfy the byte limit.
		cache.insert(2, cached_response(100));
		assert!(cache.get(&1).is_none());
		assert!(cache.get(&2).is_none());
		assert_eq!(cache.limiter().current_bytes, 0);
	}

	#[test]
	fn on_replace_updates_size_tracking() {
		let limiter = ByteSizeLimiter::new(10000, None);
		let mut cache = LruMap::new(limiter);

		// Insert an entry, then replace it with a larger one under the same key.
		cache.insert(1, cached_response(10));
		assert_eq!(cache.limiter().current_bytes, 10 + OVERHEAD);

		cache.insert(1, cached_response(50));
		assert_eq!(cache.limiter().current_bytes, 50 + OVERHEAD);
		assert_eq!(cache.len(), 1);
	}

	#[test]
	fn on_replace_shrinks_size_tracking() {
		let limiter = ByteSizeLimiter::new(10000, None);
		let mut cache = LruMap::new(limiter);

		// Insert a large entry, then replace with a smaller one.
		cache.insert(1, cached_response(500));
		assert_eq!(cache.limiter().current_bytes, 500 + OVERHEAD);

		cache.insert(1, cached_response(50));
		assert_eq!(cache.limiter().current_bytes, 50 + OVERHEAD);
		assert_eq!(cache.len(), 1);
	}

	// --- Cache key ---

	#[test]
	fn cache_key_deterministic() {
		let k1 = cache_key("state_call", r#"["a","b","0xhash"]"#);
		let k2 = cache_key("state_call", r#"["a","b","0xhash"]"#);
		assert_eq!(k1, k2);
	}

	#[test]
	fn cache_key_different_for_different_params() {
		let k1 = cache_key("state_call", r#"["a","b","0xhash1"]"#);
		let k2 = cache_key("state_call", r#"["a","b","0xhash2"]"#);
		assert_ne!(k1, k2);
	}

	#[test]
	fn cache_key_different_for_different_methods() {
		let k1 = cache_key("state_call", r#"["a"]"#);
		let k2 = cache_key("state_getStorage", r#"["a"]"#);
		assert_ne!(k1, k2);
	}

	// --- CacheMiddleware integration tests ---

	/// Mock RPC service that counts calls and returns a fixed response.
	#[derive(Clone)]
	struct MockService {
		call_count: Arc<std::sync::atomic::AtomicUsize>,
	}

	impl MockService {
		fn new() -> Self {
			Self { call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)) }
		}

		fn call_count(&self) -> usize {
			self.call_count.load(std::sync::atomic::Ordering::SeqCst)
		}
	}

	impl<'a> RpcServiceT<'a> for MockService {
		type Future = BoxFuture<'a, MethodResponse>;

		fn call(&self, req: Request<'a>) -> Self::Future {
			self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			let id = req.id().into_owned();
			async move {
				MethodResponse::response(
					id,
					ResponsePayload::success_borrowed(&"0xresult"),
					usize::MAX,
				)
			}
			.boxed()
		}
	}

	/// Mock service that returns an error.
	#[derive(Clone)]
	struct MockErrorService;

	impl<'a> RpcServiceT<'a> for MockErrorService {
		type Future = BoxFuture<'a, MethodResponse>;

		fn call(&self, req: Request<'a>) -> Self::Future {
			let id = req.id().into_owned();
			async move {
				MethodResponse::error(
					id,
					jsonrpsee::types::ErrorObject::owned(-32000, "some error", None::<()>),
				)
			}
			.boxed()
		}
	}

	fn make_request(method: &str, params: &str) -> String {
		format!(r#"{{"jsonrpc":"2.0","method":"{}","params":{},"id":1}}"#, method, params)
	}

	fn make_cache_layer(max_size: usize) -> CacheLayer {
		CacheLayer::new(max_size, None)
	}

	#[tokio::test]
	async fn cache_hit_returns_cached_response() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, HASH_66));

		// First call — cache miss.
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		let rp1 = svc.call(req).await;
		assert!(rp1.is_success());
		assert_eq!(mock.call_count(), 1);

		// Second call — cache hit.
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		let rp2 = svc.call(req).await;
		assert!(rp2.is_success());
		assert_eq!(mock.call_count(), 1); // inner service not called again
	}

	#[tokio::test]
	async fn non_allowlisted_method_bypasses_cache() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("system_health", r#"[]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}

	#[tokio::test]
	async fn missing_block_hash_bypasses_cache() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		// state_call with only 2 params (no block hash).
		let req_json = make_request("state_call", r#"["method","0xdata"]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}

	#[tokio::test]
	async fn null_block_hash_bypasses_cache() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", r#"["method","0xdata",null]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}

	#[tokio::test]
	async fn error_response_not_cached() {
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(MockErrorService);

		let req_json = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, HASH_66));

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		let rp = svc.call(req).await;
		assert!(rp.is_error());

		// Check cache is empty.
		let cache = layer.cache.lock();
		assert_eq!(cache.len(), 0);
	}

	#[tokio::test]
	async fn concurrent_cache_access() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, HASH_66));

		// Prime the cache.
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		assert_eq!(mock.call_count(), 1);

		// Spawn many concurrent requests that should all hit the cache.
		let mut handles = Vec::new();
		for _ in 0..50 {
			let svc = svc.clone();
			let json = req_json.clone();
			handles.push(tokio::spawn(async move {
				let req = serde_json::from_str::<Request>(&json).unwrap();
				let rp = svc.call(req).await;
				assert!(rp.is_success());
			}));
		}
		for h in handles {
			h.await.unwrap();
		}

		// Inner service should still have been called only once (the initial miss).
		assert_eq!(mock.call_count(), 1);
	}

	#[tokio::test]
	async fn different_block_hashes_cached_separately() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024);
		let svc = layer.layer(mock.clone());

		let hash_a = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
		let hash_b = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
		let req_a = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, hash_a));
		let req_b = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, hash_b));

		// Miss for hash_a.
		let req = serde_json::from_str::<Request>(&req_a).unwrap();
		svc.call(req).await;
		assert_eq!(mock.call_count(), 1);

		// Miss for hash_b.
		let req = serde_json::from_str::<Request>(&req_b).unwrap();
		svc.call(req).await;
		assert_eq!(mock.call_count(), 2);

		// Hit for hash_a.
		let req = serde_json::from_str::<Request>(&req_a).unwrap();
		svc.call(req).await;
		assert_eq!(mock.call_count(), 2);

		// Hit for hash_b.
		let req = serde_json::from_str::<Request>(&req_b).unwrap();
		svc.call(req).await;
		assert_eq!(mock.call_count(), 2);

		assert_eq!(layer.cache.lock().len(), 2);
	}

	#[tokio::test]
	async fn cache_disabled_when_zero_size() {
		let mock = MockService::new();
		let layer = make_cache_layer(0);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", &format!(r#"["method","0xdata","{}"]"#, HASH_66));

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}
}

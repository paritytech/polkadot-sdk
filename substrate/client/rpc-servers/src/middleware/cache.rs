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

// TODO: Consider resolving "latest" (missing block hash) to the current best block hash
// so that repeated queries targeting the same "latest" block can hit the cache within the
// same block production interval (~6s). This would require the middleware to have access
// to the blockchain backend.

use std::{
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
fn is_cacheable(method: &str) -> bool {
	method.starts_with("state_") || method.starts_with("childstate_")
}

/// Parse params and check whether the last element is a block hash.
///
/// Returns the canonically serialized params string (whitespace-normalized) on success,
/// or `None` if the params don't contain an explicit block hash.
///
/// A block hash is a 66-character hex string (`0x` + 64 hex chars = 32 bytes).
fn parse_and_check_params(params: &str) -> Option<String> {
	let parsed: serde_json::Value = serde_json::from_str(params).ok()?;
	let arr = parsed.as_array().filter(|a| !a.is_empty())?;
	match arr.last().and_then(|v| v.as_str()) {
		Some(s) if s.len() == 66 && s.starts_with("0x") => {},
		_ => return None,
	}
	// Re-serialize to get a canonical (whitespace-normalized) form for hashing.
	Some(serde_json::to_string(&parsed).expect("re-serialization of valid JSON cannot fail"))
}

/// Compute a cache key from the method name and canonical params.
fn cache_key(method: &str, canonical_params: &str) -> u64 {
	let mut hasher = DefaultHasher::new();
	method.hash(&mut hasher);
	canonical_params.hash(&mut hasher);
	hasher.finish()
}

/// A cached RPC response. Stores the JSON `"result"` field value (not the full envelope).
struct CachedResponse {
	/// The raw JSON of the `"result"` field from the JSON-RPC response.
	result_json: String,
	/// Byte size of `result_json` for tracking purposes.
	byte_size: usize,
}

impl CachedResponse {
	fn new(result_json: String) -> Self {
		let byte_size = result_json.len();
		Self { result_json, byte_size }
	}
}

/// Extract the `"result"` field from a JSON-RPC response string.
fn extract_result_field(response_json: &str) -> Option<String> {
	#[derive(serde::Deserialize)]
	struct Envelope<'a> {
		#[serde(borrow)]
		result: &'a serde_json::value::RawValue,
	}

	let envelope: Envelope = serde_json::from_str(response_json).ok()?;
	Some(envelope.result.get().to_string())
}

/// Reconstruct a [`MethodResponse`] from a cached result payload and a request ID.
///
/// Parses the cached JSON fragment into a `serde_json::Value` and re-serializes it into
/// a proper JSON-RPC response with the given request ID.
fn response_from_cache(id: Id<'_>, result_json: &str) -> Option<MethodResponse> {
	let value: serde_json::Value = serde_json::from_str(result_json).ok()?;
	Some(MethodResponse::response(id, ResponsePayload::success(value), usize::MAX))
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
		_new_value: &mut CachedResponse,
	) -> bool {
		self.current_bytes = self.current_bytes.saturating_sub(old_value.byte_size);
		self.current_bytes = self.current_bytes.saturating_add(_new_value.byte_size);
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
		log::info!(
			target: "rpc_cache",
			"RPC cache initialized: max_size={}MB, metrics={}",
			max_cache_size / (1024 * 1024),
			if metrics.is_some() { "enabled" } else { "disabled" },
		);
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

		// Parse params, check for explicit block hash, and normalize for cache key.
		let params = req.params();
		let params_str = params.as_str().unwrap_or("[]");
		let canonical_params = match parse_and_check_params(params_str) {
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
				if let Some(rp) = response_from_cache(req.id(), &cached.result_json) {
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

		async move {
			let rp = service.call(req).await;

			// Only cache successful responses.
			if rp.is_success() {
				if let Some(result_json) = extract_result_field(rp.as_result()) {
					let cached = CachedResponse::new(result_json);
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
				}
			}

			if let Some(ref metrics) = metrics {
				metrics.on_miss(&method_name);
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
	}

	// --- parse_and_check_params tests ---

	const HASH_66: &str = "0xf1212766e424bdc1f8f1d2c11ee236cff70ad77cad64d82b7823acc1e682a815";

	#[test]
	fn parses_when_last_param_is_hash() {
		let params = format!(r#"["ReviveApi_eth_block", "0x", "{}"]"#, HASH_66);
		assert!(parse_and_check_params(&params).is_some());

		let params = format!(r#"["{}"]"#, HASH_66);
		assert!(parse_and_check_params(&params).is_some());
	}

	#[test]
	fn rejects_when_hash_omitted() {
		assert!(parse_and_check_params(r#"["ReviveApi_eth_block", "0x"]"#).is_none());
	}

	#[test]
	fn rejects_when_null() {
		assert!(parse_and_check_params(r#"["method", "0x", null]"#).is_none());
	}

	#[test]
	fn rejects_when_empty_params() {
		assert!(parse_and_check_params(r#"[]"#).is_none());
	}

	#[test]
	fn rejects_when_wrong_length() {
		assert!(parse_and_check_params(r#"["method", "0xdata"]"#).is_none());
		assert!(parse_and_check_params(r#"["0x"]"#).is_none());
	}

	#[test]
	fn normalizes_whitespace() {
		let p1 = format!(r#"["a",  "0x",  "{}"]"#, HASH_66);
		let p2 = format!(r#"["a","0x","{}"]"#, HASH_66);
		let n1 = parse_and_check_params(&p1).unwrap();
		let n2 = parse_and_check_params(&p2).unwrap();
		assert_eq!(n1, n2);
		assert_eq!(cache_key("state_call", &n1), cache_key("state_call", &n2));
	}

	// --- Helper: extract_result_field ---

	#[test]
	fn extract_result_field_works() {
		let response = r#"{"jsonrpc":"2.0","result":"0x1234","id":1}"#;
		let result = extract_result_field(response).unwrap();
		assert_eq!(result, r#""0x1234""#);
	}

	#[test]
	fn extract_result_field_complex_result() {
		let response = r#"{"jsonrpc":"2.0","result":{"key":"val","num":42},"id":1}"#;
		let result = extract_result_field(response).unwrap();
		assert_eq!(result, r#"{"key":"val","num":42}"#);
	}

	#[test]
	fn extract_result_field_error_response() {
		let response = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid"},"id":1}"#;
		let result = extract_result_field(response);
		assert!(result.is_none());
	}

	// --- response_from_cache ---

	#[test]
	fn response_from_cache_simple() {
		let rp = response_from_cache(Id::Number(42), r#""0x1234""#).unwrap();
		assert!(rp.is_success());
		let result_str = rp.as_result();
		assert!(result_str.contains(r#""result":"0x1234""#));
		assert!(result_str.contains(r#""id":42"#));
	}

	#[test]
	fn response_from_cache_different_id() {
		let rp1 = response_from_cache(Id::Number(1), r#""0xaaa""#).unwrap();
		let rp2 = response_from_cache(Id::Number(99), r#""0xaaa""#).unwrap();
		assert!(rp1.as_result().contains(r#""id":1"#));
		assert!(rp2.as_result().contains(r#""id":99"#));
	}

	// --- Byte-size limiter ---

	#[test]
	fn eviction_on_byte_limit() {
		let limiter = ByteSizeLimiter::new(100, None);
		let mut cache = LruMap::new(limiter);

		// Insert entries that exceed the limit.
		cache.insert(1, CachedResponse::new("a".repeat(60)));
		cache.insert(2, CachedResponse::new("b".repeat(60)));

		// First entry should have been evicted.
		assert!(cache.get(&1).is_none());
		assert!(cache.get(&2).is_some());
	}

	#[test]
	fn size_tracking_accurate() {
		let limiter = ByteSizeLimiter::new(1000, None);
		let mut cache = LruMap::new(limiter);

		cache.insert(1, CachedResponse::new("a".repeat(100)));
		cache.insert(2, CachedResponse::new("b".repeat(200)));

		assert_eq!(cache.limiter().current_bytes, 300);
	}

	#[test]
	fn zero_size_cache_rejects_all() {
		let limiter = ByteSizeLimiter::new(0, None);
		let mut cache = LruMap::new(limiter);

		cache.insert(1, CachedResponse::new("data".to_string()));
		assert!(cache.get(&1).is_none());
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

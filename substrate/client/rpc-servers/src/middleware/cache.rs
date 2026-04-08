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

//! RPC middleware for caching deterministic responses to finalized block queries.
//!
//! Caches the result payload of RPC responses for methods in the allowlist when the
//! request targets a finalized block. Cache hits skip both execution and serialization
//! of the inner handler.

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

/// A reference to a block, either by hash or by number.
///
/// Used by the finalization check closure to determine whether a block is finalized.
#[derive(Debug, Clone)]
pub enum BlockRef {
	/// A hex-encoded block hash extracted from RPC params.
	Hash(String),
	/// A block number (used by `chain_getBlockHash`).
	Number(u64),
}

/// Describes how to extract the block reference from a cacheable RPC method's params.
#[derive(Debug, Clone, Copy)]
enum BlockRefKind {
	/// The param at the given index is a hex-encoded block hash.
	HashAt(usize),
	/// The param at the given index is a block number.
	NumberAt(usize),
}

/// Returns the block ref extraction rule for a cacheable method, or `None` if not cacheable.
fn cacheable_method(method: &str) -> Option<BlockRefKind> {
	match method {
		"state_call" | "state_callAt" => Some(BlockRefKind::HashAt(2)),
		"state_getStorage" | "state_getStorageAt" => Some(BlockRefKind::HashAt(1)),
		"state_getRuntimeVersion" => Some(BlockRefKind::HashAt(0)),
		"chain_getBlock" => Some(BlockRefKind::HashAt(0)),
		"chain_getHeader" => Some(BlockRefKind::HashAt(0)),
		"chain_getBlockHash" => Some(BlockRefKind::NumberAt(0)),
		_ => None,
	}
}

/// Extract a [`BlockRef`] from the JSON-RPC request params based on the method's extraction rule.
fn extract_block_ref(params: &str, kind: BlockRefKind) -> Option<BlockRef> {
	let parsed: serde_json::Value = serde_json::from_str(params).ok()?;
	let arr = parsed.as_array()?;

	match kind {
		BlockRefKind::HashAt(idx) => {
			let val = arr.get(idx)?;
			let hash = val.as_str()?;
			if hash.is_empty() {
				return None;
			}
			Some(BlockRef::Hash(hash.to_string()))
		},
		BlockRefKind::NumberAt(idx) => {
			let val = arr.get(idx)?;
			let n = val.as_u64()?;
			Some(BlockRef::Number(n))
		},
	}
}

/// Compute a cache key from the method name and params.
fn cache_key(method: &str, params: &str) -> u64 {
	let mut hasher = DefaultHasher::new();
	method.hash(&mut hasher);
	params.hash(&mut hasher);
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
		log::debug!(
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

type IsFinalized = Arc<dyn Fn(BlockRef) -> bool + Send + Sync>;
type Cache = Arc<Mutex<LruMap<u64, CachedResponse, ByteSizeLimiter>>>;

/// Tower layer that wraps services with [`CacheMiddleware`].
#[derive(Clone)]
pub struct CacheLayer {
	cache: Cache,
	is_finalized: IsFinalized,
	metrics: Option<CacheMetrics>,
}

impl CacheLayer {
	/// Create a new cache layer.
	///
	/// - `max_cache_size`: maximum cache size in bytes. Pass 0 to disable caching.
	/// - `is_finalized`: closure that checks if a block ref is finalized.
	/// - `metrics`: optional prometheus metrics.
	pub fn new(
		max_cache_size: usize,
		is_finalized: IsFinalized,
		metrics: Option<CacheMetrics>,
	) -> Self {
		log::info!(
			target: "rpc_cache",
			"RPC cache initialized: max_size={}MB, metrics={}",
			max_cache_size / (1024 * 1024),
			if metrics.is_some() { "enabled" } else { "disabled" },
		);
		let limiter = ByteSizeLimiter::new(max_cache_size, metrics.clone());
		let cache = Arc::new(Mutex::new(LruMap::new(limiter)));
		Self { cache, is_finalized, metrics }
	}
}

impl<S> tower::Layer<S> for CacheLayer {
	type Service = CacheMiddleware<S>;

	fn layer(&self, service: S) -> Self::Service {
		CacheMiddleware {
			service,
			cache: self.cache.clone(),
			is_finalized: self.is_finalized.clone(),
			metrics: self.metrics.clone(),
		}
	}
}

/// RPC middleware that caches deterministic responses for finalized block queries.
pub struct CacheMiddleware<S> {
	service: S,
	cache: Cache,
	is_finalized: IsFinalized,
	metrics: Option<CacheMetrics>,
}

impl<S: Clone> Clone for CacheMiddleware<S> {
	fn clone(&self) -> Self {
		Self {
			service: self.service.clone(),
			cache: self.cache.clone(),
			is_finalized: self.is_finalized.clone(),
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

		// Check if this method is cacheable and extract block ref rule.
		let block_ref_kind = match cacheable_method(method) {
			Some(kind) => kind,
			None => {
				let service = self.service.clone();
				return async move { service.call(req).await }.boxed();
			},
		};

		// Extract block ref from params.
		let params = req.params();
		let params_str = params.as_str().unwrap_or("[]");
		let block_ref = match extract_block_ref(params_str, block_ref_kind) {
			Some(br) => br,
			None => {
				let service = self.service.clone();
				return async move { service.call(req).await }.boxed();
			},
		};

		// Check if the block is finalized.
		if !(self.is_finalized)(block_ref) {
			log::trace!(
				target: "rpc_cache",
				"Skipping cache for {} (block not finalized)",
				method,
			);
			let service = self.service.clone();
			return async move { service.call(req).await }.boxed();
		}

		// Compute cache key and check for hit.
		let key = cache_key(method, params_str);
		{
			let mut cache = self.cache.lock();
			if let Some(cached) = cache.get(&key) {
				if let Some(rp) = response_from_cache(req.id(), &cached.result_json) {
					if let Some(ref metrics) = self.metrics {
						metrics.on_hit(method);
					}
					log::debug!(
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
					log::debug!(
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

	// --- Block ref extraction tests ---

	#[test]
	fn extract_block_hash_state_call() {
		let params = r#"["method_name", "0xdata", "0xabc123"]"#;
		let result = extract_block_ref(params, BlockRefKind::HashAt(2));
		match result {
			Some(BlockRef::Hash(h)) => assert_eq!(h, "0xabc123"),
			_ => panic!("expected BlockRef::Hash"),
		}
	}

	#[test]
	fn extract_block_hash_state_call_missing() {
		let params = r#"["method_name", "0xdata"]"#;
		let result = extract_block_ref(params, BlockRefKind::HashAt(2));
		assert!(result.is_none());
	}

	#[test]
	fn extract_block_hash_state_call_null() {
		let params = r#"["method_name", "0xdata", null]"#;
		let result = extract_block_ref(params, BlockRefKind::HashAt(2));
		assert!(result.is_none());
	}

	#[test]
	fn extract_block_hash_state_get_storage() {
		let params = r#"["0xkey", "0xblockhash"]"#;
		let result = extract_block_ref(params, BlockRefKind::HashAt(1));
		match result {
			Some(BlockRef::Hash(h)) => assert_eq!(h, "0xblockhash"),
			_ => panic!("expected BlockRef::Hash"),
		}
	}

	#[test]
	fn extract_block_hash_chain_get_block_hash() {
		let params = r#"[42]"#;
		let result = extract_block_ref(params, BlockRefKind::NumberAt(0));
		match result {
			Some(BlockRef::Number(n)) => assert_eq!(n, 42),
			_ => panic!("expected BlockRef::Number"),
		}
	}

	#[test]
	fn extract_block_hash_get_runtime_version() {
		let params = r#"["0xabc123"]"#;
		let result = extract_block_ref(params, BlockRefKind::HashAt(0));
		match result {
			Some(BlockRef::Hash(h)) => assert_eq!(h, "0xabc123"),
			_ => panic!("expected BlockRef::Hash"),
		}
	}

	#[test]
	fn extract_block_hash_non_cacheable() {
		assert!(cacheable_method("system_health").is_none());
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

	fn always_finalized(_: BlockRef) -> bool {
		true
	}

	fn never_finalized(_: BlockRef) -> bool {
		false
	}

	fn make_cache_layer(max_size: usize, is_finalized: fn(BlockRef) -> bool) -> CacheLayer {
		CacheLayer::new(max_size, Arc::new(is_finalized), None)
	}

	#[tokio::test]
	async fn cache_hit_returns_cached_response() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024, always_finalized);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", r#"["method","0xdata","0xfinalized_hash"]"#);

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
		let layer = make_cache_layer(1024 * 1024, always_finalized);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("system_health", r#"[]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}

	#[tokio::test]
	async fn missing_block_ref_bypasses_cache() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024, always_finalized);
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
	async fn non_finalized_block_bypasses_cache() {
		let mock = MockService::new();
		let layer = make_cache_layer(1024 * 1024, never_finalized);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", r#"["method","0xdata","0xhash"]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}

	#[tokio::test]
	async fn error_response_not_cached() {
		let layer = make_cache_layer(1024 * 1024, always_finalized);
		let svc = layer.layer(MockErrorService);

		let req_json = make_request("state_call", r#"["method","0xdata","0xhash"]"#);

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
		let layer = make_cache_layer(0, always_finalized);
		let svc = layer.layer(mock.clone());

		let req_json = make_request("state_call", r#"["method","0xdata","0xhash"]"#);

		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;
		let req = serde_json::from_str::<Request>(&req_json).unwrap();
		svc.call(req).await;

		assert_eq!(mock.call_count(), 2);
	}
}

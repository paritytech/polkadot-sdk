// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! A JSON-RPC proxy in front of a JAM node that drops the first `submitWorkPackage` per distinct
//! work-package hash and forwards every later submission byte-exact.
//!
//! The resubmission test points the collators at this proxy instead of at the JAM node, so the
//! first submission never reaches a guarantor and the collator's resend does. It is a typed
//! proxy: it implements jsonrpsee 0.26's `ClientT`/`SubscriptionClientT`, and polkajam's blanket
//! `RpcClient`/`Node`/`RpcServer` impls turn that into the whole JIP-2 surface and a served
//! `into_rpc()`, subscriptions included, with no hand-written RPC methods.

use jam_std_common::{Node, RpcServer, VersionedParameters};
use jam_types::{AnyBytes, CoreIndex, WorkPackageHash};
use jsonrpsee_jam::{
	core::{
		client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
		params::BatchRequestBuilder,
		traits::ToRpcParams,
		DeserializeOwned, JsonRawValue,
	},
	server::{Server, ServerConfig, ServerHandle},
	ws_client::{WsClient, WsClientBuilder},
};
use std::{
	collections::{hash_map::Entry, HashMap},
	fmt,
	sync::{
		atomic::{AtomicU32, Ordering},
		Arc, Mutex,
	},
	time::Instant,
};

/// JAM RPC bodies carry whole work packages; 32 MiB clears the node's own limits by a wide margin.
const MAX_RPC_BODY_BYTES: u32 = 32 * 1024 * 1024;

/// Pre-serialized JSON-RPC params, so a request can be forwarded byte-exact.
#[derive(Debug)]
pub struct RawParams(pub Option<Box<JsonRawValue>>);

impl ToRpcParams for RawParams {
	fn to_rpc_params(self) -> Result<Option<Box<JsonRawValue>>, serde_json::Error> {
		Ok(self.0)
	}
}

/// The decision [`Attempts::record`] makes for one work-package hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
	/// The first sighting: swallow the submission.
	Drop,
	/// A resubmission: forward it, carrying the attempt number.
	Forward { attempt: u32 },
}

/// What the proxy has seen of one work-package hash.
#[derive(Clone, Debug)]
pub struct Attempt {
	/// The work-package hash, blake2b-256 of the encoded package.
	pub hash: [u8; 32],
	pub count: u32,
	pub first_seen: Instant,
	pub last_seen: Instant,
	/// The core named by the first submission.
	pub core: CoreIndex,
	/// Length in bytes of the work package.
	pub package_len: usize,
}

/// Per-hash submission bookkeeping: the first submission of each hash is dropped.
#[derive(Debug, Default)]
pub struct Attempts {
	by_hash: HashMap<[u8; 32], Attempt>,
}

impl Attempts {
	/// Record one submission of `hash`. The first is dropped, every later one is forwarded.
	pub fn record(&mut self, hash: [u8; 32], core: CoreIndex, package_len: usize) -> Verdict {
		let now = Instant::now();
		match self.by_hash.entry(hash) {
			Entry::Occupied(mut occupied) => {
				let attempt = occupied.get_mut();
				attempt.count += 1;
				attempt.last_seen = now;
				Verdict::Forward { attempt: attempt.count }
			},
			Entry::Vacant(vacant) => {
				vacant.insert(Attempt {
					hash,
					count: 1,
					first_seen: now,
					last_seen: now,
					core,
					package_len,
				});
				Verdict::Drop
			},
		}
	}

	/// Every tracked hash, oldest first.
	pub fn snapshot(&self) -> Vec<Attempt> {
		let mut attempts: Vec<Attempt> = self.by_hash.values().cloned().collect();
		attempts.sort_by_key(|attempt| attempt.first_seen);
		attempts
	}
}

/// A JAM node client that swallows the first `submitWorkPackage` of every distinct package.
pub struct DroppingProxy {
	upstream: WsClient,
	attempts: Arc<Mutex<Attempts>>,
	upstream_errors: Arc<AtomicU32>,
}

impl fmt::Debug for DroppingProxy {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("DroppingProxy").finish()
	}
}

impl ClientT for DroppingProxy {
	async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
	where
		Params: ToRpcParams + Send,
	{
		self.upstream.notification(method, params).await
	}

	async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
	where
		R: DeserializeOwned,
		Params: ToRpcParams + Send,
	{
		if method != "submitWorkPackage" {
			return self.upstream.request(method, params).await;
		}

		let raw = params.to_rpc_params()?;
		let json = raw.as_ref().map(|raw| raw.get()).unwrap_or("[]");
		let Ok((core, package, _extrinsics)) =
			serde_json::from_str::<(CoreIndex, AnyBytes, Vec<AnyBytes>)>(json)
		else {
			// An unparseable body is not a package we can reason about; forward it untouched.
			return self.upstream.request(method, RawParams(raw)).await;
		};

		let hash = jam_std_common::hash_raw(package.0.as_ref());
		let package_len = package.0.len();
		// Bound first so the lock guard is dropped before the forward await below.
		let verdict = self.attempts.lock().unwrap().record(hash, core, package_len);
		match verdict {
			Verdict::Drop => {
				log::info!(
					"swallowed the first submission wp_hash={:?} core={} package_len={}",
					WorkPackageHash(hash),
					core,
					package_len,
				);
				serde_json::from_str::<R>("null").map_err(Error::ParseError)
			},
			Verdict::Forward { attempt } => {
				log::info!(
					"forwarding attempt {} wp_hash={:?} core={} package_len={}",
					attempt,
					WorkPackageHash(hash),
					core,
					package_len,
				);
				let result = self.upstream.request::<R, _>(method, RawParams(raw)).await;
				if result.is_err() {
					self.upstream_errors.fetch_add(1, Ordering::Relaxed);
				}
				result
			},
		}
	}

	async fn batch_request<'a, R>(
		&self,
		batch: BatchRequestBuilder<'a>,
	) -> Result<BatchResponse<'a, R>, Error>
	where
		R: DeserializeOwned + fmt::Debug + 'a,
	{
		self.upstream.batch_request(batch).await
	}
}

impl SubscriptionClientT for DroppingProxy {
	async fn subscribe<'a, Notif, Params>(
		&self,
		subscribe_method: &'a str,
		params: Params,
		unsubscribe_method: &'a str,
	) -> Result<Subscription<Notif>, Error>
	where
		Params: ToRpcParams + Send,
		Notif: DeserializeOwned,
	{
		self.upstream.subscribe(subscribe_method, params, unsubscribe_method).await
	}

	async fn subscribe_to_method<Notif>(&self, method: &str) -> Result<Subscription<Notif>, Error>
	where
		Notif: DeserializeOwned,
	{
		self.upstream.subscribe_to_method(method).await
	}
}

/// A running [`DroppingProxy`] RPC server bound to `127.0.0.1:0`.
///
/// The proxy holds one upstream connection and never reconnects: the resubmission test's node
/// runs for the whole test and never restarts, so a dropped upstream connection is a test
/// failure, not a state to recover from.
pub struct ProxyServer {
	url: String,
	handle: Option<ServerHandle>,
	attempts: Arc<Mutex<Attempts>>,
	upstream_errors: Arc<AtomicU32>,
}

impl ProxyServer {
	/// Connect to `upstream_url`, apply the chain's protocol parameters and start serving.
	///
	/// Decoding a work package reads process-global protocol bounds, so fetching and applying
	/// the upstream's parameters has to happen before the server can accept a `submitWorkPackage`.
	pub async fn serve(upstream_url: &str) -> anyhow::Result<Self> {
		Self::serve_on(upstream_url, "127.0.0.1:0").await
	}

	/// Like [`Self::serve`], but bound to `listen_addr` instead of an ephemeral loopback port.
	pub async fn serve_on(upstream_url: &str, listen_addr: &str) -> anyhow::Result<Self> {
		let upstream = WsClientBuilder::default()
			.max_request_size(MAX_RPC_BODY_BYTES)
			.max_response_size(MAX_RPC_BODY_BYTES)
			.build(upstream_url)
			.await?;

		let VersionedParameters::V1(parameters) =
			Node::parameters(&upstream).await.map_err(|error| {
				anyhow::anyhow!("Unable to fetch the JAM chain parameters: {error}")
			})?;
		parameters
			.apply()
			.map_err(|error| anyhow::anyhow!("Invalid JAM chain parameters: {error}"))?;

		let attempts = Arc::new(Mutex::new(Attempts::default()));
		let upstream_errors = Arc::new(AtomicU32::new(0));
		let proxy = DroppingProxy {
			upstream,
			attempts: attempts.clone(),
			upstream_errors: upstream_errors.clone(),
		};

		let server = Server::builder()
			.set_config(
				ServerConfig::builder()
					.max_request_body_size(MAX_RPC_BODY_BYTES)
					.max_response_body_size(MAX_RPC_BODY_BYTES)
					.build(),
			)
			.build(listen_addr)
			.await?;
		let url = format!("ws://{}", server.local_addr()?);
		let handle = server.start(proxy.into_rpc());

		Ok(Self { url, handle: Some(handle), attempts, upstream_errors })
	}

	/// Serve on `listen_addr`, retrying until `upstream_url` answers, or return the last error once
	/// `timeout` has passed.
	///
	/// The sdk's single-call `spawn_fn` builds and starts the whole network at once, so a
	/// collator's `--jam-rpc-urls` and the proxy's listen address are fixed before the JAM node
	/// exists. This lets the proxy start first and connect upstream as soon as `jam-or` is up.
	pub async fn serve_when_ready(
		upstream_url: &str,
		listen_addr: &str,
		timeout: std::time::Duration,
	) -> anyhow::Result<Self> {
		let deadline = Instant::now() + timeout;
		loop {
			match Self::serve_on(upstream_url, listen_addr).await {
				Ok(server) => return Ok(server),
				Err(error) => {
					if Instant::now() >= deadline {
						return Err(error);
					}
					log::info!("proxy: upstream {upstream_url} is not up yet ({error}); retrying");
					tokio::time::sleep(std::time::Duration::from_millis(200)).await;
				},
			}
		}
	}

	/// The `ws://` URL the proxy is listening on.
	pub fn url(&self) -> String {
		self.url.clone()
	}

	/// Every work-package hash the proxy has seen, oldest first.
	pub fn snapshot(&self) -> Vec<Attempt> {
		self.attempts.lock().unwrap().snapshot()
	}

	/// How many forwarded submissions the upstream rejected.
	pub fn upstream_errors(&self) -> u32 {
		self.upstream_errors.load(Ordering::Relaxed)
	}

	/// A human-readable summary of what the proxy did, for test failure output.
	pub fn describe(&self) -> String {
		let attempts = self.snapshot();
		if attempts.is_empty() {
			return "proxy saw no submitWorkPackage submissions".to_string();
		}
		let mut summary = format!("proxy saw {} work package hash(es):", attempts.len());
		for attempt in attempts {
			summary.push_str(&format!(
				"\n  wp_hash={:?} core={} submission_count={} package_len={} first_seen={:?} last_seen={:?}",
				WorkPackageHash(attempt.hash),
				attempt.core,
				attempt.count,
				attempt.package_len,
				attempt.first_seen,
				attempt.last_seen,
			));
		}
		summary
	}

	/// Stop serving and wait for the server to finish.
	pub async fn shutdown(mut self) {
		if let Some(handle) = self.handle.take() {
			let _ = handle.stop();
			handle.stopped().await;
		}
	}
}

impl Drop for ProxyServer {
	fn drop(&mut self) {
		if let Some(handle) = self.handle.take() {
			let _ = handle.stop();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use jam_std_common::{BlockDesc, RpcClient};
	use jam_types::{HeaderHash, ProtocolParameters};
	use jsonrpsee_jam::{
		core::RpcResult, rpc_params, ws_client::WsClientBuilder, RpcModule, SubscriptionMessage,
	};
	use std::{sync::atomic::AtomicUsize, time::Duration};

	fn test_server_config() -> ServerConfig {
		ServerConfig::builder()
			.max_request_body_size(MAX_RPC_BODY_BYTES)
			.max_response_body_size(MAX_RPC_BODY_BYTES)
			.build()
	}

	async fn start_stub_server(module: RpcModule<()>) -> (String, ServerHandle) {
		let server = Server::builder()
			.set_config(test_server_config())
			.build("127.0.0.1:0")
			.await
			.expect("stub server binds");
		let url = format!("ws://{}", server.local_addr().expect("stub server has an address"));
		let handle = server.start(module);
		(url, handle)
	}

	async fn stub_parameters() -> RpcResult<VersionedParameters> {
		Ok(VersionedParameters::V1(ProtocolParameters::tiny()))
	}

	async fn stub_best_block() -> RpcResult<BlockDesc> {
		Ok(BlockDesc { header_hash: HeaderHash([3u8; 32]), slot: 42 })
	}

	async fn count_submission(calls: Arc<AtomicUsize>) -> RpcResult<()> {
		calls.fetch_add(1, Ordering::SeqCst);
		Ok(())
	}

	#[test]
	fn attempts_record_drops_first_and_forwards_the_rest() {
		let mut attempts = Attempts::default();
		let hash = [7u8; 32];

		assert_eq!(attempts.record(hash, 0, 100), Verdict::Drop);
		assert_eq!(attempts.record(hash, 0, 100), Verdict::Forward { attempt: 2 });
		assert_eq!(attempts.record(hash, 0, 100), Verdict::Forward { attempt: 3 });

		let other = [9u8; 32];
		assert_eq!(attempts.record(other, 1, 200), Verdict::Drop);
		assert_eq!(attempts.record(other, 1, 200), Verdict::Forward { attempt: 2 });

		let snapshot = attempts.snapshot();
		assert_eq!(snapshot.len(), 2);
		let first = snapshot.iter().find(|attempt| attempt.hash == hash).expect("first hash");
		assert_eq!(first.count, 3);
		assert_eq!(first.core, 0);
		assert_eq!(first.package_len, 100);
		let second = snapshot.iter().find(|attempt| attempt.hash == other).expect("second hash");
		assert_eq!(second.count, 2);
	}

	#[tokio::test]
	async fn drops_the_first_submit_work_package_and_forwards_later_ones() {
		let calls = Arc::new(AtomicUsize::new(0));
		let calls_in = calls.clone();
		let mut module = RpcModule::new(());
		module
			.register_async_method("submitWorkPackage", move |_params, _ctx, _ext| {
				count_submission(calls_in.clone())
			})
			.expect("registering submitWorkPackage succeeds");

		let (url, handle) = start_stub_server(module).await;
		let upstream = WsClientBuilder::default().build(&url).await.expect("connect to stub");
		let proxy = DroppingProxy {
			upstream,
			attempts: Arc::new(Mutex::new(Attempts::default())),
			upstream_errors: Arc::new(AtomicU32::new(0)),
		};

		let package = || AnyBytes(b"the same package bytes".to_vec().into());
		let submit = || {
			proxy.request::<(), _>(
				"submitWorkPackage",
				rpc_params![0u16, package(), Vec::<AnyBytes>::new()],
			)
		};

		submit().await.expect("the swallowed submission still answers successfully");
		submit().await.expect("the resubmission is forwarded");

		assert_eq!(calls.load(Ordering::SeqCst), 1, "the stub saw only the resubmission");
		assert_eq!(
			proxy.attempts.lock().unwrap().snapshot()[0].count,
			2,
			"the proxy tracked two submissions of the one hash"
		);

		let _ = handle.stop();
		handle.stopped().await;
	}

	#[tokio::test]
	async fn relays_requests_and_subscriptions_through_the_served_proxy() {
		let mut module = RpcModule::new(());
		module
			.register_async_method("parameters", |_params, _ctx, _ext| stub_parameters())
			.expect("registering parameters succeeds");
		module
			.register_async_method("bestBlock", |_params, _ctx, _ext| stub_best_block())
			.expect("registering bestBlock succeeds");
		module
			.register_subscription(
				"subscribeBestBlock",
				"bestBlock",
				"unsubscribeBestBlock",
				|_params, pending, _ctx, _ext| async move {
					let Ok(sink) = pending.accept().await else {
						return;
					};
					let block = BlockDesc { header_hash: HeaderHash([5u8; 32]), slot: 43 };
					if let Ok(message) =
						SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), &block)
					{
						let _ = sink.send(message).await;
					}
					sink.closed().await;
				},
			)
			.expect("registering subscribeBestBlock succeeds");

		let (upstream_url, upstream_handle) = start_stub_server(module).await;
		let proxy = ProxyServer::serve(&upstream_url).await.expect("the proxy serves");

		let client = WsClientBuilder::default()
			.build(proxy.url())
			.await
			.expect("connect to the proxy");
		let best = RpcClient::best_block(&client).await.expect("bestBlock relays");
		assert_eq!(best, BlockDesc { header_hash: HeaderHash([3u8; 32]), slot: 42 });

		let mut subscription = RpcClient::subscribe_best_block(&client)
			.await
			.expect("subscribeBestBlock relays");
		let update = tokio::time::timeout(Duration::from_secs(10), subscription.next())
			.await
			.expect("the relayed subscription yields in time")
			.expect("the relayed subscription is open")
			.expect("the relayed subscription item is a block");
		assert_eq!(update, BlockDesc { header_hash: HeaderHash([5u8; 32]), slot: 43 });

		drop(subscription);
		drop(client);
		tokio::time::timeout(Duration::from_secs(10), proxy.shutdown())
			.await
			.expect("the proxy shuts down");
		let _ = upstream_handle.stop();
		tokio::time::timeout(Duration::from_secs(10), upstream_handle.stopped())
			.await
			.expect("the stub shuts down");
	}
}

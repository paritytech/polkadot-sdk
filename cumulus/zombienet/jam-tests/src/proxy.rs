// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! A JSON-RPC proxy in front of a JAM node that drops `submitWorkPackage` calls according to a
//! [`DropPolicy`] and forwards the rest byte-exact.
//!
//! The resubmission tests point collators at this proxy instead of at the JAM node.
//! [`DropPolicy::Random`] drops a seeded random share of the submissions, so the parachain blocks
//! a dropped package carried only land once a collator puts them into a new package;
//! [`DropPolicy::Everything`] drops every submission. The proxy decodes each package's PoV to the
//! parachain blocks it carries: a forwarded package carrying a block of an earlier dropped one is
//! a [`Resubmission`], which a test awaits through [`ProxyServer::subscribe_resubmissions`].
//!
//! It is a typed proxy: it implements jsonrpsee 0.26's `ClientT`/`SubscriptionClientT`, and
//! polkajam's blanket `RpcClient`/`Node`/`RpcServer` impls turn that into the whole JIP-2 surface
//! and a served `into_rpc()`, subscriptions included, with no hand-written RPC methods. Only the
//! node's `header` method, which that vendored surface predates, is registered on top and relayed
//! raw.

use codec::Decode;
use cumulus_primitives_core::ParachainBlockData;
use jam_std_common::{Node, RpcServer, VersionedParameters};
use jam_types::{AnyBytes, CoreIndex, WorkPackageHash};
use jsonrpsee_jam::{
	core::{
		client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
		params::BatchRequestBuilder,
		traits::ToRpcParams,
		DeserializeOwned, JsonRawValue, RpcResult,
	},
	server::{Server, ServerConfig, ServerHandle},
	types::{
		error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
		ErrorObject,
	},
	ws_client::{WsClient, WsClientBuilder},
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sp_core::H256;
use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	fmt,
	sync::{
		atomic::{AtomicU32, Ordering},
		Arc, Mutex,
	},
	time::{Duration, Instant},
};
use tokio::sync::watch;

/// JAM RPC bodies carry whole work packages; 32 MiB clears the node's own limits by a wide margin.
const MAX_RPC_BODY_BYTES: u32 = 32 * 1024 * 1024;

/// The block type a PoV is decoded as to name its parachain blocks: `u32` numbers and BlakeTwo256
/// hashes, as every para these tests run uses, and extrinsics the proxy never looks into.
type OpaqueParaBlock = generic::Block<generic::Header<u32, BlakeTwo256>, OpaqueExtrinsic>;

/// Pre-serialized JSON-RPC params, so a request can be forwarded byte-exact.
#[derive(Debug)]
pub struct RawParams(pub Option<Box<JsonRawValue>>);

impl ToRpcParams for RawParams {
	fn to_rpc_params(self) -> Result<Option<Box<JsonRawValue>>, serde_json::Error> {
		Ok(self.0)
	}
}

/// The decision [`Attempts::record`] makes for one submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
	/// Swallow the submission; it never reaches the JAM node.
	Drop,
	/// Forward it, carrying how often its work-package hash has been submitted so far.
	Forward { attempt: u32 },
}

/// How [`Attempts::record`] decides which submissions to drop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropPolicy {
	/// Drop each submission independently with `probability`, drawn from an RNG seeded with
	/// `seed`, so the same seed and the same order of submissions give the same verdicts. `0.0`
	/// forwards every submission and `1.0` drops every one.
	Random { probability: f64, seed: u64 },
	/// Drop every sighting of every work-package hash; nothing is ever forwarded.
	Everything,
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
	/// `(hash, number)` of every parachain block the package's PoV carries; empty if the PoV does
	/// not decode.
	pub blocks: Vec<([u8; 32], u32)>,
}

/// A forwarded submission carrying parachain blocks that an earlier, dropped submission carried.
#[derive(Clone, Debug)]
pub struct Resubmission {
	/// The work-package hash of the forwarded submission.
	pub wp_hash: [u8; 32],
	/// `(hash, number)` of the previously dropped blocks among the ones it carries.
	pub blocks: Vec<([u8; 32], u32)>,
	/// When the proxy forwarded it.
	pub at: Instant,
}

/// Per-hash submission bookkeeping, governed by a [`DropPolicy`].
#[derive(Debug)]
pub struct Attempts {
	policy: DropPolicy,
	/// The source of [`DropPolicy::Random`]'s draws.
	rng: StdRng,
	by_hash: HashMap<[u8; 32], Attempt>,
	/// How many [`Self::record`] calls returned [`Verdict::Forward`].
	forwarded: usize,
	/// Hashes of the parachain blocks that dropped submissions carried.
	dropped_blocks: HashSet<[u8; 32]>,
	/// Every [`Resubmission`], oldest first.
	resubmissions: Vec<Resubmission>,
	/// Publishes `resubmissions.len()` whenever it grows.
	resubmission_count: watch::Sender<usize>,
}

impl Attempts {
	/// A ledger that applies `policy`.
	pub fn new(policy: DropPolicy) -> Self {
		let seed = match policy {
			DropPolicy::Random { seed, .. } => seed,
			DropPolicy::Everything => 0,
		};
		Self {
			policy,
			rng: StdRng::seed_from_u64(seed),
			by_hash: HashMap::new(),
			forwarded: 0,
			dropped_blocks: HashSet::new(),
			resubmissions: Vec::new(),
			resubmission_count: watch::Sender::new(0),
		}
	}

	/// Record one submission of `hash`, whose PoV carries `blocks`, and return the verdict.
	///
	/// [`DropPolicy::Random`] draws once per call and [`DropPolicy::Everything`] drops every call;
	/// either way the sighting is counted in [`Attempt::count`]. A dropped submission marks its
	/// blocks as dropped, and a forwarded one carrying any dropped block is recorded as a
	/// [`Resubmission`] of those blocks.
	pub fn record(
		&mut self,
		hash: [u8; 32],
		core: CoreIndex,
		package_len: usize,
		blocks: Vec<([u8; 32], u32)>,
	) -> Verdict {
		let now = Instant::now();
		let count = match self.by_hash.entry(hash) {
			Entry::Occupied(mut occupied) => {
				let attempt = occupied.get_mut();
				attempt.count += 1;
				attempt.last_seen = now;
				attempt.count
			},
			Entry::Vacant(vacant) => {
				vacant.insert(Attempt {
					hash,
					count: 1,
					first_seen: now,
					last_seen: now,
					core,
					package_len,
					blocks: blocks.clone(),
				});
				1
			},
		};

		let dropped = match self.policy {
			DropPolicy::Random { probability, .. } => self.rng.gen::<f64>() < probability,
			DropPolicy::Everything => true,
		};
		if dropped {
			self.dropped_blocks.extend(blocks.into_iter().map(|(block_hash, _)| block_hash));
			return Verdict::Drop;
		}

		self.forwarded += 1;
		let resubmitted: Vec<([u8; 32], u32)> = blocks
			.into_iter()
			.filter(|(block_hash, _)| self.dropped_blocks.contains(block_hash))
			.collect();
		if !resubmitted.is_empty() {
			log::info!(
				"resubmission #{} wp_hash={:?} blocks={}",
				self.resubmissions.len() + 1,
				WorkPackageHash(hash),
				format_blocks(&resubmitted),
			);
			self.resubmissions
				.push(Resubmission { wp_hash: hash, blocks: resubmitted, at: now });
			self.resubmission_count.send_replace(self.resubmissions.len());
		}
		Verdict::Forward { attempt: count }
	}

	/// How many [`Self::record`] calls returned [`Verdict::Forward`].
	pub fn forwarded_count(&self) -> usize {
		self.forwarded
	}

	/// Every tracked hash, oldest first.
	pub fn snapshot(&self) -> Vec<Attempt> {
		let mut attempts: Vec<Attempt> = self.by_hash.values().cloned().collect();
		attempts.sort_by_key(|attempt| attempt.first_seen);
		attempts
	}

	/// Every [`Resubmission`], oldest first.
	pub fn resubmissions(&self) -> &[Resubmission] {
		&self.resubmissions
	}

	/// How many [`Resubmission`]s were recorded.
	pub fn resubmission_count(&self) -> usize {
		self.resubmissions.len()
	}

	/// [`Self::resubmission_count`], updated whenever a [`Resubmission`] is recorded.
	pub fn subscribe_resubmissions(&self) -> watch::Receiver<usize> {
		self.resubmission_count.subscribe()
	}
}

/// `(hash, number)` of every parachain block in `pov`, the SCALE-encoded [`ParachainBlockData`] a
/// collator submits as a package's first extrinsic; empty if `pov` does not decode as one.
fn pov_blocks(pov: &[u8]) -> Vec<([u8; 32], u32)> {
	let Ok(data) = ParachainBlockData::<OpaqueParaBlock>::decode(&mut &pov[..]) else {
		return Vec::new();
	};
	data.blocks()
		.iter()
		.map(|block| (block.header.hash().into(), block.header.number))
		.collect()
}

/// `[#number 0xhash, ...]`, for logs and [`ProxyServer::describe`].
fn format_blocks(blocks: &[([u8; 32], u32)]) -> String {
	let blocks: Vec<String> = blocks
		.iter()
		.map(|(hash, number)| format!("#{number} {:?}", H256::from(*hash)))
		.collect();
	format!("[{}]", blocks.join(", "))
}

/// A JAM node client that swallows `submitWorkPackage` calls as its [`Attempts`] ledger decides.
pub struct DroppingProxy {
	upstream: Arc<WsClient>,
	attempts: Arc<Mutex<Attempts>>,
	upstream_errors: Arc<AtomicU32>,
	policy: DropPolicy,
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
		let Ok((core, package, extrinsics)) =
			serde_json::from_str::<(CoreIndex, AnyBytes, Vec<AnyBytes>)>(json)
		else {
			// An unparseable body is not a package we can reason about; forward it untouched.
			return self.upstream.request(method, RawParams(raw)).await;
		};

		let hash = jam_std_common::hash_raw(package.0.as_ref());
		let package_len = package.0.len();
		let blocks = extrinsics.first().map(|pov| pov_blocks(pov.0.as_ref())).unwrap_or_default();
		let blocks_summary = format_blocks(&blocks);
		// Bound first so the lock guard is dropped before the forward await below.
		let verdict = self.attempts.lock().unwrap().record(hash, core, package_len, blocks);
		match verdict {
			Verdict::Drop => {
				log::info!(
					"swallowed a submission wp_hash={:?} core={} package_len={} blocks={} \
					 policy={:?}",
					WorkPackageHash(hash),
					core,
					package_len,
					blocks_summary,
					self.policy,
				);
				serde_json::from_str::<R>("null").map_err(Error::ParseError)
			},
			Verdict::Forward { attempt } => {
				log::info!(
					"forwarding attempt {} wp_hash={:?} core={} package_len={} blocks={}",
					attempt,
					WorkPackageHash(hash),
					core,
					package_len,
					blocks_summary,
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
	/// Connect to `upstream_url`, apply the chain's protocol parameters and start serving on
	/// `listen_addr`, retrying until the upstream answers or `timeout` has passed.
	///
	/// Decoding a work package reads process-global protocol bounds, so fetching and applying
	/// the upstream's parameters has to happen before the server can accept a `submitWorkPackage`.
	///
	/// The sdk's single-call `spawn_fn` builds and starts the whole network at once, so a
	/// collator's `--jam-rpc-urls` and the proxy's listen address are fixed before the JAM node
	/// exists. This lets the proxy start first and connect upstream as soon as `jam-or` is up. A
	/// `listen_addr` ending in `:0` binds an ephemeral port, and [`Self::url`] reports the port the
	/// OS picked.
	pub async fn serve(
		policy: DropPolicy,
		upstream_url: &str,
		listen_addr: &str,
		timeout: Duration,
	) -> anyhow::Result<Self> {
		match policy {
			DropPolicy::Random { probability, seed } => {
				log::info!("proxy: dropping at random probability={probability} seed={seed}")
			},
			DropPolicy::Everything => log::info!("proxy: dropping every submission"),
		}
		let deadline = Instant::now() + timeout;
		loop {
			match Self::serve_once(policy, upstream_url, listen_addr).await {
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

	/// Connect to `upstream_url`, apply the chain's protocol parameters and start serving once on
	/// `listen_addr`.
	async fn serve_once(
		policy: DropPolicy,
		upstream_url: &str,
		listen_addr: &str,
	) -> anyhow::Result<Self> {
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

		let upstream = Arc::new(upstream);
		let attempts = Arc::new(Mutex::new(Attempts::new(policy)));
		let upstream_errors = Arc::new(AtomicU32::new(0));
		let proxy = DroppingProxy {
			upstream: upstream.clone(),
			attempts: attempts.clone(),
			upstream_errors: upstream_errors.clone(),
			policy,
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
		let mut module = proxy.into_rpc();
		// The vendored `RpcServer` surface predates the node's `header` method.
		module
			.register_async_method("header", move |params, _, _| {
				relay_header(upstream.clone(), params)
			})
			.map_err(|error| anyhow::anyhow!("Unable to register the `header` relay: {error}"))?;
		let handle = server.start(module);

		Ok(Self { url, handle: Some(handle), attempts, upstream_errors })
	}

	/// The `ws://` URL the proxy is listening on.
	pub fn url(&self) -> String {
		self.url.clone()
	}

	/// Every work-package hash the proxy has seen, oldest first.
	pub fn snapshot(&self) -> Vec<Attempt> {
		self.attempts.lock().unwrap().snapshot()
	}

	/// How many sightings were actually forwarded upstream, i.e. how many [`Verdict::Forward`]
	/// verdicts [`Attempts::record`] returned.
	pub fn forwarded_count(&self) -> usize {
		self.attempts.lock().unwrap().forwarded_count()
	}

	/// How many forwarded submissions the upstream rejected.
	pub fn upstream_errors(&self) -> u32 {
		self.upstream_errors.load(Ordering::Relaxed)
	}

	/// Every forwarded submission that carried a previously dropped parachain block, oldest first.
	pub fn resubmissions(&self) -> Vec<Resubmission> {
		self.attempts.lock().unwrap().resubmissions().to_vec()
	}

	/// How many [`Self::resubmissions`] there are.
	pub fn resubmission_count(&self) -> usize {
		self.attempts.lock().unwrap().resubmission_count()
	}

	/// [`Self::resubmission_count`], updated on every new resubmission, so a test can await a count
	/// instead of polling for it.
	pub fn subscribe_resubmissions(&self) -> watch::Receiver<usize> {
		self.attempts.lock().unwrap().subscribe_resubmissions()
	}

	/// A human-readable summary of what the proxy did, for test failure output.
	pub fn describe(&self) -> String {
		let attempts = self.snapshot();
		if attempts.is_empty() {
			return "proxy saw no submitWorkPackage submissions".to_string();
		}
		let mut summary = format!(
			"proxy saw {} work package hash(es) and {} resubmission(s):",
			attempts.len(),
			self.resubmission_count(),
		);
		for attempt in attempts {
			summary.push_str(&format!(
				"\n  wp_hash={:?} core={} submission_count={} package_len={} blocks={} \
				 first_seen={:?} last_seen={:?}",
				WorkPackageHash(attempt.hash),
				attempt.core,
				attempt.count,
				attempt.package_len,
				format_blocks(&attempt.blocks),
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

/// Forward a `header` call to `upstream` with its params untouched and hand back the raw result.
///
/// An error the upstream answers with, such as "Method not found" from a node without `header`,
/// reaches the caller unchanged.
async fn relay_header(
	upstream: Arc<WsClient>,
	params: jsonrpsee_jam::types::Params<'static>,
) -> RpcResult<Box<JsonRawValue>> {
	let raw = params
		.as_str()
		.map(|json| JsonRawValue::from_string(json.to_owned()))
		.transpose()
		.map_err(|error| ErrorObject::owned(INVALID_PARAMS_CODE, error.to_string(), None::<()>))?;
	upstream.request("header", RawParams(raw)).await.map_err(|error| match error {
		Error::Call(error) => error,
		error => ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_primitives_core::SchedulingProof;
	use jam_std_common::{BlockDesc, RpcClient};
	use jam_types::{HeaderHash, ProtocolParameters};
	use jsonrpsee_jam::{
		core::RpcResult, rpc_params, types::Params, ws_client::WsClientBuilder, RpcModule,
		SubscriptionMessage,
	};
	use std::{sync::atomic::AtomicUsize, time::Duration};

	/// Forwards every submission: no draw from `0.0..1.0` is below `0.0`.
	const FORWARD_ALL: DropPolicy = DropPolicy::Random { probability: 0.0, seed: 0 };
	/// How long a test waits for whatever it awaits.
	const WAIT: Duration = Duration::from_secs(10);
	/// The block the stub node serves a header for.
	const KNOWN_HEADER: HeaderHash = HeaderHash([1u8; 32]);
	/// The encoded header the stub node serves for [`KNOWN_HEADER`].
	const STUB_HEADER: &[u8] = b"the encoded header";
	/// The error code the stub node answers `header` with for any other block.
	const BLOCK_UNAVAILABLE_CODE: i32 = 1;

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

	async fn stub_header(params: Params<'static>) -> RpcResult<AnyBytes> {
		let hash: HeaderHash = params.one()?;
		if hash != KNOWN_HEADER {
			return Err(ErrorObject::owned(
				BLOCK_UNAVAILABLE_CODE,
				"block unavailable",
				None::<()>,
			));
		}
		Ok(AnyBytes(STUB_HEADER.to_vec().into()))
	}

	/// Parachain block `number` on top of `parent`, with an Aura digest and one opaque extrinsic.
	fn para_block(number: u32, parent: [u8; 32]) -> OpaqueParaBlock {
		let header = generic::Header {
			parent_hash: parent.into(),
			number,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: generic::Digest {
				logs: vec![generic::DigestItem::PreRuntime(*b"aura", u64::from(number).encode())],
			},
		};
		generic::Block {
			header,
			extrinsics: vec![OpaqueExtrinsic::from_blob(vec![number as u8; 4])],
		}
	}

	/// `(hash, number)` of `block`, the way the proxy names parachain blocks.
	fn block_id(block: &OpaqueParaBlock) -> ([u8; 32], u32) {
		(block.header.hash().into(), block.header.number)
	}

	/// A PoV shaped like the collator's: a V4 [`ParachainBlockData`] carrying the parent header and
	/// one additional-data slot per block.
	fn pov(blocks: Vec<OpaqueParaBlock>) -> Vec<u8> {
		let parent_header = para_block(0, [0u8; 32]).header.encode();
		let additional_data = vec![None; blocks.len()];
		ParachainBlockData::new_with_parent_header(
			blocks,
			// An empty `CompactProof`; the proxy never reads the proof.
			Decode::decode(&mut &[0u8][..]).expect("an empty proof decodes"),
			SchedulingProof::empty(),
			additional_data,
			parent_header,
			Vec::new(),
		)
		.encode()
	}

	/// Submit the package `package`, whose PoV carries `blocks`, through `proxy`.
	async fn submit(
		proxy: &DroppingProxy,
		package: &[u8],
		blocks: Vec<OpaqueParaBlock>,
	) -> Result<(), Error> {
		let package = AnyBytes(package.to_vec().into());
		let extrinsic = AnyBytes(pov(blocks).into());
		proxy
			.request("submitWorkPackage", rpc_params![0u16, package, vec![extrinsic]])
			.await
	}

	/// The verdicts on `count` submissions of distinct packages under `policy`.
	fn verdicts(policy: DropPolicy, count: u8) -> Vec<Verdict> {
		let mut attempts = Attempts::new(policy);
		(0..count).map(|i| attempts.record([i; 32], 0, 100, Vec::new())).collect()
	}

	#[test]
	fn random_policy_at_probability_zero_forwards_everything() {
		let mut attempts = Attempts::new(DropPolicy::Random { probability: 0.0, seed: 7 });
		for i in 0..64u8 {
			assert_eq!(
				attempts.record([i; 32], 0, 100, Vec::new()),
				Verdict::Forward { attempt: 1 }
			);
		}
		assert_eq!(
			attempts.record([0u8; 32], 0, 100, Vec::new()),
			Verdict::Forward { attempt: 2 },
			"a repeated hash counts its attempts"
		);
		assert_eq!(attempts.forwarded_count(), 65);
	}

	#[test]
	fn random_policy_at_probability_one_drops_everything() {
		let mut attempts = Attempts::new(DropPolicy::Random { probability: 1.0, seed: 7 });
		for i in 0..64u8 {
			assert_eq!(attempts.record([i; 32], 0, 100, Vec::new()), Verdict::Drop);
		}
		assert_eq!(attempts.forwarded_count(), 0);
	}

	#[test]
	fn random_policy_is_deterministic_for_a_fixed_seed() {
		let policy = |seed| DropPolicy::Random { probability: 0.5, seed };
		let seeded = verdicts(policy(42), 64);
		assert_eq!(seeded, verdicts(policy(42), 64), "the same seed gives the same verdicts");
		assert_ne!(seeded, verdicts(policy(43), 64), "another seed gives other verdicts");
		assert!(
			seeded.contains(&Verdict::Drop) && seeded.contains(&Verdict::Forward { attempt: 1 }),
			"a coin flip per submission both drops and forwards: {seeded:?}"
		);
	}

	#[test]
	fn everything_policy_drops_every_sighting_and_counts_them() {
		let mut attempts = Attempts::new(DropPolicy::Everything);
		let hash = [11u8; 32];

		assert_eq!(attempts.record(hash, 0, 100, Vec::new()), Verdict::Drop);
		assert_eq!(attempts.record(hash, 0, 100, Vec::new()), Verdict::Drop);
		assert_eq!(attempts.forwarded_count(), 0);

		let snapshot = attempts.snapshot();
		assert_eq!(snapshot.len(), 1);
		assert_eq!(snapshot[0].count, 2);
		assert_eq!(snapshot[0].package_len, 100);
	}

	#[test]
	fn a_forwarded_package_carrying_a_dropped_block_is_a_resubmission() {
		let x = ([1u8; 32], 5);
		let y = ([2u8; 32], 6);
		let unrelated = ([3u8; 32], 7);
		let mut attempts = Attempts::new(DropPolicy::Everything);
		let mut count = attempts.subscribe_resubmissions();

		assert_eq!(attempts.record([10u8; 32], 0, 100, vec![x]), Verdict::Drop);
		attempts.policy = FORWARD_ALL;

		assert_eq!(
			attempts.record([11u8; 32], 0, 100, vec![unrelated]),
			Verdict::Forward { attempt: 1 }
		);
		assert_eq!(attempts.resubmission_count(), 0, "a package without a dropped block is none");
		assert!(!count.has_changed().expect("the ledger keeps the sender"));

		assert_eq!(
			attempts.record([12u8; 32], 1, 200, vec![x, y]),
			Verdict::Forward { attempt: 1 }
		);
		assert_eq!(attempts.resubmission_count(), 1);
		let resubmission = &attempts.resubmissions()[0];
		assert_eq!(resubmission.wp_hash, [12u8; 32]);
		assert_eq!(resubmission.blocks, vec![x], "only the dropped block counts as resubmitted");
		assert!(count.has_changed().expect("the ledger keeps the sender"));
		assert_eq!(*count.borrow_and_update(), 1);
	}

	#[test]
	fn pov_blocks_names_every_block_of_a_pov() {
		let first = para_block(7, [9u8; 32]);
		let second = para_block(8, first.header.hash().into());
		assert_eq!(
			pov_blocks(&pov(vec![first.clone(), second.clone()])),
			vec![block_id(&first), block_id(&second)]
		);
		assert_eq!(pov_blocks(b"not a pov"), Vec::new(), "an undecodable PoV names no block");
	}

	#[tokio::test]
	async fn the_proxy_announces_a_resubmission_of_a_dropped_block() {
		let calls = Arc::new(AtomicUsize::new(0));
		let calls_in = calls.clone();
		let mut module = RpcModule::new(());
		module
			.register_async_method("parameters", |_params, _ctx, _ext| stub_parameters())
			.expect("registering parameters succeeds");
		module
			.register_async_method("submitWorkPackage", move |_params, _ctx, _ext| {
				count_submission(calls_in.clone())
			})
			.expect("registering submitWorkPackage succeeds");
		let (upstream_url, upstream_handle) = start_stub_server(module).await;

		let server = ProxyServer::serve(DropPolicy::Everything, &upstream_url, "127.0.0.1:0", WAIT)
			.await
			.expect("the proxy serves");
		// Submit through a proxy client sharing the served ledger: the served `submitWorkPackage`
		// first checks that the package decodes as a work package, which these are not.
		let upstream =
			WsClientBuilder::default().build(&upstream_url).await.expect("connect to stub");
		let proxy = DroppingProxy {
			upstream: Arc::new(upstream),
			attempts: server.attempts.clone(),
			upstream_errors: server.upstream_errors.clone(),
			policy: DropPolicy::Everything,
		};
		let mut resubmissions = server.subscribe_resubmissions();
		let announced = tokio::spawn(async move {
			resubmissions.wait_for(|count| *count >= 1).await.map(|count| *count)
		});

		let block = para_block(3, [4u8; 32]);
		submit(&proxy, b"package a", vec![block.clone()])
			.await
			.expect("the swallowed submission still answers successfully");
		assert_eq!(
			calls.load(Ordering::SeqCst),
			0,
			"the dropped submission never reached the stub"
		);

		server.attempts.lock().unwrap().policy = FORWARD_ALL;
		submit(&proxy, b"package b", vec![block.clone()])
			.await
			.expect("the resubmission is forwarded");

		let count = tokio::time::timeout(WAIT, announced)
			.await
			.expect("the resubmission is announced in time")
			.expect("the waiter does not panic")
			.expect("the ledger keeps the sender");
		assert_eq!(count, 1);
		assert_eq!(calls.load(Ordering::SeqCst), 1, "only the resubmission reached the stub");
		assert_eq!(server.resubmission_count(), 1);
		let resubmissions = server.resubmissions();
		assert_eq!(resubmissions[0].wp_hash, jam_std_common::hash_raw(b"package b"));
		assert_eq!(resubmissions[0].blocks, vec![block_id(&block)]);
		assert_eq!(server.snapshot()[0].blocks, vec![block_id(&block)]);

		drop(proxy);
		tokio::time::timeout(WAIT, server.shutdown())
			.await
			.expect("the proxy shuts down");
		let _ = upstream_handle.stop();
		tokio::time::timeout(WAIT, upstream_handle.stopped())
			.await
			.expect("the stub shuts down");
	}

	#[tokio::test]
	async fn relays_header_to_the_upstream() {
		let mut module = RpcModule::new(());
		module
			.register_async_method("parameters", |_params, _ctx, _ext| stub_parameters())
			.expect("registering parameters succeeds");
		module
			.register_async_method("header", |params, _ctx, _ext| stub_header(params))
			.expect("registering header succeeds");
		let (upstream_url, upstream_handle) = start_stub_server(module).await;
		let proxy = ProxyServer::serve(DropPolicy::Everything, &upstream_url, "127.0.0.1:0", WAIT)
			.await
			.expect("the proxy serves");
		let client = WsClientBuilder::default()
			.build(proxy.url())
			.await
			.expect("connect to the proxy");

		let header: AnyBytes = client
			.request("header", rpc_params![KNOWN_HEADER])
			.await
			.expect("header relays");
		assert_eq!(&header.0[..], STUB_HEADER);

		let error = client
			.request::<AnyBytes, _>("header", rpc_params![HeaderHash([2u8; 32])])
			.await
			.expect_err("the stub has no such block");
		let Error::Call(error) = error else {
			panic!("expected the stub's own error, got {error:?}");
		};
		assert_eq!(error.code(), BLOCK_UNAVAILABLE_CODE);

		drop(client);
		tokio::time::timeout(WAIT, proxy.shutdown())
			.await
			.expect("the proxy shuts down");
		let _ = upstream_handle.stop();
		tokio::time::timeout(WAIT, upstream_handle.stopped())
			.await
			.expect("the stub shuts down");
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
		let proxy = ProxyServer::serve(
			DropPolicy::Everything,
			&upstream_url,
			"127.0.0.1:0",
			Duration::from_secs(10),
		)
		.await
		.expect("the proxy serves");

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

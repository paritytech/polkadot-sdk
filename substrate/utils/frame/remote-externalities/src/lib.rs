// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Remote Externalities
//!
//! An equivalent of `sp_io::TestExternalities` that can load its state from a remote substrate
//! based chain, or a local state snapshot file.

mod logging;

use codec::{Compact, Decode, Encode};
use indicatif::{ProgressBar, ProgressStyle};
use jsonrpsee::{
	core::params::ArrayParams,
	ws_client::{WsClient, WsClientBuilder},
};
use log::*;
use serde::de::DeserializeOwned;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{
		well_known_keys::{is_default_child_storage_key, DEFAULT_CHILD_STORAGE_KEY_PREFIX},
		ChildInfo, ChildType, PrefixedStorageKey, StorageData, StorageKey,
	},
};
use sp_runtime::{
	traits::{Block as BlockT, HashingFor, Header},
	StateVersion,
};
use sp_state_machine::TestExternalities;
use std::{
	collections::VecDeque,
	fs,
	ops::{Deref, DerefMut},
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc, Mutex,
	},
	time::Duration,
};
use substrate_rpc_client::{rpc_params, BatchRequestBuilder, ChainApi, ClientT, StateApi};
use tokio::{sync::Semaphore, time::sleep};

type Result<T, E = &'static str> = std::result::Result<T, E>;

type KeyValue = (StorageKey, StorageData);
type TopKeyValues = Vec<KeyValue>;
type ChildKeyValues = Vec<(ChildInfo, Vec<KeyValue>)>;
type SnapshotVersion = Compact<u16>;

/// A versioned WebSocket client returned by `ConnectionManager::get()`.
///
/// Always contains a usable client. The version is used to detect if the client
/// has been recreated by another worker.
struct VersionedClient {
	ws_client: Arc<WsClient>,
	version: u64,
}

impl Deref for VersionedClient {
	type Target = WsClient;
	fn deref(&self) -> &Self::Target {
		&self.ws_client
	}
}

/// Represents a range of keys to fetch from the remote node.
#[derive(Debug, Clone)]
struct KeyRange {
	/// The starting key of this range (inclusive).
	start_key: StorageKey,
	/// The ending key of this range (exclusive), or None for open-ended range.
	end_key: Option<StorageKey>,
	/// The common prefix for this range.
	prefix: StorageKey,
}

impl KeyRange {
	fn new(start_key: StorageKey, end_key: Option<StorageKey>, prefix: StorageKey) -> Self {
		Self { start_key, end_key, prefix }
	}
}

/// Work queue for distributing key ranges to workers.
type WorkQueue = Arc<Mutex<VecDeque<KeyRange>>>;

/// Manages WebSocket client connections for parallel workers.
#[derive(Clone)]
struct ConnectionManager {
	clients: Vec<Arc<tokio::sync::Mutex<Client>>>,
}

impl ConnectionManager {
	fn new(clients: Vec<Arc<tokio::sync::Mutex<Client>>>) -> Result<Self> {
		if clients.is_empty() {
			return Err("At least one client must be provided");
		}

		Ok(Self { clients })
	}

	fn num_clients(&self) -> usize {
		self.clients.len()
	}

	/// Get a usable client for a specific worker.
	/// Distributes workers across available clients.
	async fn get(&self, worker_index: usize) -> VersionedClient {
		let client_index = worker_index % self.clients.len();
		let client = self.clients[client_index].lock().await;
		VersionedClient { ws_client: client.inner.clone(), version: client.version }
	}

	/// Called when a request fails. Triggers client recreation if version matches.
	async fn recreate_client(&self, worker_index: usize, failed: &VersionedClient) {
		let client_index = worker_index % self.clients.len();
		let mut client = self.clients[client_index].lock().await;
		let _ = client.recreate(failed.version).await;
	}
}

const LOG_TARGET: &str = "remote-ext";
const DEFAULT_HTTP_ENDPOINT: &str = "https://try-runtime.polkadot.io:443";
const SNAPSHOT_VERSION: SnapshotVersion = Compact(4);

/// The snapshot that we store on disk.
#[derive(Decode, Encode)]
struct Snapshot<B: BlockT> {
	snapshot_version: SnapshotVersion,
	state_version: StateVersion,
	// <Vec<Key, (Value, MemoryDbRefCount)>>
	raw_storage: Vec<(Vec<u8>, (Vec<u8>, i32))>,
	// The storage root of the state. This may vary from the storage root in the header, if not the
	// entire state was fetched.
	storage_root: B::Hash,
	header: B::Header,
}

impl<B: BlockT> Snapshot<B> {
	pub fn new(
		state_version: StateVersion,
		raw_storage: Vec<(Vec<u8>, (Vec<u8>, i32))>,
		storage_root: B::Hash,
		header: B::Header,
	) -> Self {
		Self {
			snapshot_version: SNAPSHOT_VERSION,
			state_version,
			raw_storage,
			storage_root,
			header,
		}
	}

	fn load(path: &PathBuf) -> Result<Snapshot<B>> {
		let bytes = fs::read(path).map_err(|_| "fs::read failed.")?;
		// The first item in the SCALE encoded struct bytes is the snapshot version. We decode and
		// check that first, before proceeding to decode the rest of the snapshot.
		let snapshot_version = SnapshotVersion::decode(&mut &*bytes)
			.map_err(|_| "Failed to decode snapshot version")?;

		if snapshot_version != SNAPSHOT_VERSION {
			return Err("Unsupported snapshot version detected. Please create a new snapshot.")
		}

		Decode::decode(&mut &*bytes).map_err(|_| "Decode failed")
	}
}

/// An externalities that acts exactly the same as [`sp_io::TestExternalities`] but has a few extra
/// bits and pieces to it, and can be loaded remotely.
pub struct RemoteExternalities<B: BlockT> {
	/// The inner externalities.
	pub inner_ext: TestExternalities<HashingFor<B>>,
	/// The block header which we created this externality env.
	pub header: B::Header,
}

impl<B: BlockT> Deref for RemoteExternalities<B> {
	type Target = TestExternalities<HashingFor<B>>;
	fn deref(&self) -> &Self::Target {
		&self.inner_ext
	}
}

impl<B: BlockT> DerefMut for RemoteExternalities<B> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.inner_ext
	}
}

/// The execution mode.
#[derive(Clone)]
pub enum Mode<H> {
	/// Online. Potentially writes to a snapshot file.
	Online(OnlineConfig<H>),
	/// Offline. Uses a state snapshot file and needs not any client config.
	Offline(OfflineConfig),
	/// Prefer using a snapshot file if it exists, else use a remote server.
	OfflineOrElseOnline(OfflineConfig, OnlineConfig<H>),
}

impl<H> Default for Mode<H> {
	fn default() -> Self {
		Mode::Online(OnlineConfig::default())
	}
}

/// Configuration of the offline execution.
///
/// A state snapshot config must be present.
#[derive(Clone)]
pub struct OfflineConfig {
	/// The configuration of the state snapshot file to use. It must be present.
	pub state_snapshot: SnapshotConfig,
}

/// A WebSocket client with version tracking for reconnection.
#[derive(Debug, Clone)]
pub struct Client {
	inner: Arc<WsClient>,
	version: u64,
	uri: String,
}

impl Client {
	/// Create a WebSocket client for the given URI.
	async fn create_ws_client(uri: &str) -> std::result::Result<WsClient, String> {
		debug!(target: LOG_TARGET, "initializing remote client to {:?}", uri);

		WsClientBuilder::default()
			.max_request_size(u32::MAX)
			.max_response_size(u32::MAX)
			.request_timeout(std::time::Duration::from_secs(60 * 5))
			.build(uri)
			.await
			.map_err(|e| format!("{e:?}"))
	}

	/// Create a new Client from a URI.
	///
	/// Returns `None` if the initial connection fails.
	pub async fn new(uri: impl Into<String>) -> Option<Self> {
		let uri = uri.into();
		match Self::create_ws_client(&uri).await {
			Ok(ws_client) => Some(Self { inner: Arc::new(ws_client), version: 0, uri }),
			Err(e) => {
				warn!(target: LOG_TARGET, "Connection to {uri} failed: {e}. Ignoring this URI.");
				None
			},
		}
	}

	/// Recreate the WebSocket client using the stored URI if the version matches.
	async fn recreate(&mut self, expected_version: u64) -> std::result::Result<(), String> {
		// Only recreate if version matches (prevents redundant reconnections)
		if self.version > expected_version {
			return Ok(());
		}

		debug!(target: LOG_TARGET, "Recreating client for `{}`", self.uri);
		let ws_client = Self::create_ws_client(&self.uri).await?;
		self.inner = Arc::new(ws_client);
		self.version = expected_version + 1;
		Ok(())
	}
}

/// Configuration of the online execution.
///
/// A state snapshot config may be present and will be written to in that case.
#[derive(Clone)]
pub struct OnlineConfig<H> {
	/// The block hash at which to get the runtime state. Will be latest finalized head if not
	/// provided.
	pub at: Option<H>,
	/// An optional state snapshot file to WRITE to, not for reading. Not written if set to `None`.
	pub state_snapshot: Option<SnapshotConfig>,
	/// The pallets to scrape. These values are hashed and added to `hashed_prefix`.
	pub pallets: Vec<String>,
	/// Transport URIs. Can be a single URI or multiple for load distribution.
	pub transport_uris: Vec<String>,
	/// Lookout for child-keys, and scrape them as well if set to true.
	pub child_trie: bool,
	/// Storage entry key prefixes to be injected into the externalities. The *hashed* prefix must
	/// be given.
	pub hashed_prefixes: Vec<Vec<u8>>,
	/// Storage entry keys to be injected into the externalities. The *hashed* key must be given.
	pub hashed_keys: Vec<Vec<u8>>,
}

impl<H: Clone> OnlineConfig<H> {
	fn at_expected(&self) -> H {
		self.at.clone().expect("block at must be initialized; qed")
	}
}

impl<H> Default for OnlineConfig<H> {
	fn default() -> Self {
		Self {
			transport_uris: vec![DEFAULT_HTTP_ENDPOINT.to_owned()],
			child_trie: true,
			at: None,
			state_snapshot: None,
			pallets: Default::default(),
			hashed_keys: Default::default(),
			hashed_prefixes: Default::default(),
		}
	}
}

impl<H> From<String> for OnlineConfig<H> {
	fn from(uri: String) -> Self {
		Self { transport_uris: vec![uri], ..Default::default() }
	}
}

/// Configuration of the state snapshot.
#[derive(Clone)]
pub struct SnapshotConfig {
	/// The path to the snapshot file.
	pub path: PathBuf,
}

impl SnapshotConfig {
	pub fn new<P: Into<PathBuf>>(path: P) -> Self {
		Self { path: path.into() }
	}
}

impl From<String> for SnapshotConfig {
	fn from(s: String) -> Self {
		Self::new(s)
	}
}

impl Default for SnapshotConfig {
	fn default() -> Self {
		Self { path: Path::new("SNAPSHOT").into() }
	}
}

/// Builder for remote-externalities.
#[derive(Clone)]
pub struct Builder<B: BlockT> {
	/// Custom key-pairs to be injected into the final externalities. The *hashed* keys and values
	/// must be given.
	hashed_key_values: Vec<KeyValue>,
	/// The keys that will be excluded from the final externality. The *hashed* key must be given.
	hashed_blacklist: Vec<Vec<u8>>,
	/// Connectivity mode, online or offline.
	mode: Mode<B::Hash>,
	/// If provided, overwrite the state version with this. Otherwise, the state_version of the
	/// remote node is used. All cache files also store their state version.
	///
	/// Overwrite only with care.
	overwrite_state_version: Option<StateVersion>,
	/// Connection manager for RPC clients (initialized during `init_remote_client`).
	conn_manager: Option<ConnectionManager>,
}

impl<B: BlockT> Default for Builder<B> {
	fn default() -> Self {
		Self {
			mode: Default::default(),
			hashed_key_values: Default::default(),
			hashed_blacklist: Default::default(),
			overwrite_state_version: None,
			conn_manager: None,
		}
	}
}

// Mode methods
impl<B: BlockT> Builder<B> {
	fn as_online(&self) -> &OnlineConfig<B::Hash> {
		match &self.mode {
			Mode::Online(config) => config,
			Mode::OfflineOrElseOnline(_, config) => config,
			_ => panic!("Unexpected mode: Online"),
		}
	}

	fn as_online_mut(&mut self) -> &mut OnlineConfig<B::Hash> {
		match &mut self.mode {
			Mode::Online(config) => config,
			Mode::OfflineOrElseOnline(_, config) => config,
			_ => panic!("Unexpected mode: Online"),
		}
	}

	fn conn_manager(&self) -> Result<&ConnectionManager> {
		self.conn_manager.as_ref().ok_or("connection manager must be initialized; qed")
	}

	/// Return rpc (ws) client.
	async fn rpc_client(&self) -> Result<Arc<WsClient>> {
		let conn_manager = self.conn_manager()?;
		Ok(conn_manager.get(0).await.ws_client)
	}
}

// RPC methods
impl<B: BlockT> Builder<B>
where
	B::Hash: DeserializeOwned,
	B::Header: DeserializeOwned,
{
	const PARALLEL_REQUESTS: usize = 24;
	// nodes by default will not return more than 1000 keys per request
	const DEFAULT_KEY_DOWNLOAD_PAGE: u32 = 1000;

	async fn rpc_get_storage(
		&self,
		key: StorageKey,
		maybe_at: Option<B::Hash>,
	) -> Result<Option<StorageData>> {
		trace!(target: LOG_TARGET, "rpc: get_storage");
		let client = self.rpc_client().await?;
		client.storage(key, maybe_at).await.map_err(|e| {
			error!(target: LOG_TARGET, "Error = {e:?}");
			"rpc get_storage failed."
		})
	}

	/// Get the latest finalized head.
	async fn rpc_get_head(&self) -> Result<B::Hash> {
		trace!(target: LOG_TARGET, "rpc: finalized_head");

		let client = self.rpc_client().await?;
		// sadly this pretty much unreadable...
		ChainApi::<(), _, B::Header, ()>::finalized_head(client.as_ref())
			.await
			.map_err(|e| {
				error!(target: LOG_TARGET, "Error = {e:?}");
				"rpc finalized_head failed."
			})
	}

	/// Get a single page of keys using a specific client.
	async fn get_keys_single_page_with_client(
		&self,
		client: &WsClient,
		prefix: Option<StorageKey>,
		start_key: Option<StorageKey>,
		at: B::Hash,
	) -> Result<Vec<StorageKey>> {
		client
			.storage_keys_paged(prefix, Self::DEFAULT_KEY_DOWNLOAD_PAGE, start_key, Some(at))
			.await
			.map_err(|e| {
				error!(target: LOG_TARGET, "Error = {e:?}");
				"rpc get_keys failed"
			})
	}

	/// Generate start keys for parallel fetching, dividing the workload.
	/// Uses the same logic as the original gen_start_keys but returns KeyRange objects.
	fn gen_key_ranges(prefix: &StorageKey) -> Vec<KeyRange> {
		let prefix_bytes = prefix.as_ref().to_vec();
		let mut ranges = Vec::with_capacity(16);

		// Create 16 ranges by appending one nibble (4 bits) to the prefix
		// Since we work with bytes, we append a byte where the upper nibble is 0x0-0xF
		// This gives us: 0x00, 0x10, 0x20, ..., 0xF0
		for i in 0u8..16u8 {
			let mut start_key = prefix_bytes.clone();
			start_key.push(i << 4); // Shift nibble to upper 4 bits

			let end_key = if i < 15 {
				let mut end = prefix_bytes.clone();
				end.push((i + 1) << 4); // Next nibble
				Some(StorageKey(end))
			} else {
				None
			};

			ranges.push(KeyRange::new(StorageKey(start_key), end_key, prefix.clone()));
		}

		ranges
	}

	/// Initialize the work queue with ranges for each prefix.
	fn initialize_work_queue(prefixes: &[StorageKey]) -> WorkQueue {
		let mut queue = VecDeque::new();

		for prefix in prefixes {
			let ranges = Self::gen_key_ranges(prefix);
			queue.extend(ranges);
		}

		Arc::new(Mutex::new(queue))
	}

	/// Internal generic parallel key fetching that abstracts over the RPC method.
	///
	/// Takes a closure that fetches a single batch of keys, allowing this function
	/// to work for both top-level keys and child storage keys.
	async fn rpc_get_keys_parallel_internal<F, Fut>(
		&self,
		prefix: &StorageKey,
		block: B::Hash,
		parallel: usize,
		log_prefix: &str,
		fetch_batch: F,
	) -> Result<Vec<StorageKey>>
	where
		F: Fn(Arc<Self>, KeyRange, B::Hash, usize, Arc<WsClient>) -> Fut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		Fut: std::future::Future<Output = Result<(Vec<StorageKey>, bool)>> + Send + 'static,
	{
		// Initialize work queue with top-level 16 ranges for this prefix
		let work_queue = Self::initialize_work_queue(&[prefix.clone()]);
		let initial_ranges = work_queue.lock().unwrap().len();
		eprintln!("🔧 Initialized work queue with {} ranges for parallel fetching", initial_ranges);

		// Get connection manager for handling client recreation across multiple RPC providers
		let conn_manager = Arc::new(self.conn_manager()?.clone());
		eprintln!("🌐 Using {} RPC provider(s) for parallel fetching", conn_manager.num_clients());

		// Shared storage for all collected keys
		let all_keys: Arc<Mutex<Vec<StorageKey>>> = Arc::new(Mutex::new(Vec::new()));

		// Track progress logging (log every 10,000 keys)
		let last_logged_milestone = Arc::new(std::sync::atomic::AtomicUsize::new(0));

		// Semaphore to limit parallel workers
		let semaphore = Arc::new(tokio::sync::Semaphore::new(parallel));
		let builder = Arc::new(self.clone());

		// Track active workers
		let active_workers = Arc::new(std::sync::atomic::AtomicUsize::new(0));

		let mut handles = vec![];

		// Spawn worker tasks
		eprintln!("🚀 Spawning {parallel} parallel workers for key fetching");

		for worker_index in 0..parallel {
			let permit =
				semaphore.clone().acquire_owned().await.expect("semaphore should not be closed");

			let builder = builder.clone();
			let work_queue = work_queue.clone();
			let all_keys = all_keys.clone();
			let active_workers = active_workers.clone();
			let conn_manager = conn_manager.clone();
			let last_logged_milestone = last_logged_milestone.clone();
			let fetch_batch = fetch_batch.clone();
			let log_prefix = log_prefix.to_string();

			let handle = tokio::spawn(async move {
				let mut is_active = false; // Track whether this worker is counted as active

				loop {
					// Try to get work from the queue
					let maybe_range = {
						let mut queue = work_queue.lock().unwrap();
						queue.pop_front()
					};

					let range = match maybe_range {
						Some(r) => {
							// Got work - if we weren't active, become active now
							if !is_active {
								active_workers.fetch_add(1, Ordering::SeqCst);
								is_active = true;
							}
							r
						},
						None => {
							// No work available - if we were active, become idle now
							if is_active {
								active_workers.fetch_sub(1, Ordering::SeqCst);
								is_active = false;
							}

							// Small delay to allow other workers to potentially add more work
							sleep(Duration::from_millis(100)).await;

							// Check again if there's new work or if all workers are idle
							let queue_len = work_queue.lock().unwrap().len();
							let active = active_workers.load(Ordering::SeqCst);

							if queue_len == 0 && active == 0 {
								// No work and no active workers - we're done
								break;
							} else {
								// Either queue has work or other workers are still active - keep
								// waiting
								continue;
							}
						},
					};

					// Get the client for this worker
					let client = conn_manager.get(worker_index).await;

					// Process this range - fetch ONE batch using the provided closure
					match fetch_batch(
						builder.clone(),
						range.clone(),
						block,
						worker_index,
						client.ws_client.clone(),
					)
					.await
					{
						Ok((batch_keys, is_full_batch)) => {
							// Get last two keys for subdivision if available
							let last_two_keys = if batch_keys.len() >= 2 {
								Some((
									batch_keys[batch_keys.len() - 2].clone(),
									batch_keys[batch_keys.len() - 1].clone(),
								))
							} else {
								None
							};

							// Store the keys we found
							let total_keys = {
								let mut keys = all_keys.lock().unwrap();
								keys.extend(batch_keys);
								keys.len()
							};

							// Log progress every 10,000 keys
							const LOG_INTERVAL: usize = 10_000;
							let current_milestone = (total_keys / LOG_INTERVAL) * LOG_INTERVAL;
							let last_milestone = last_logged_milestone.load(Ordering::Relaxed);

							if current_milestone > last_milestone && current_milestone > 0 {
								if last_logged_milestone
									.compare_exchange(
										last_milestone,
										current_milestone,
										Ordering::SeqCst,
										Ordering::Relaxed,
									)
									.is_ok()
								{
									eprintln!(
										"📊 {log_prefix}: Scraped {total_keys} keys so far..."
									);
								}
							}

							// If we got a full batch, subdivide the remaining key space
							if is_full_batch {
								if let Some((second_last, last)) = last_two_keys {
									let new_ranges = Self::subdivide_remaining_range(
										&second_last,
										&last,
										range.end_key.as_ref(),
										&range.prefix,
									);

									if !new_ranges.is_empty() {
										debug!(
											target: LOG_TARGET,
											"Worker {worker_index}: subdividing remaining range after {:?} into {} new ranges",
											HexDisplay::from(&last),
											new_ranges.len()
										);

										let mut queue = work_queue.lock().unwrap();
										queue.extend(new_ranges);
									}
								}
							}

							// Small delay to avoid overwhelming the node
							sleep(Duration::from_millis(10)).await;
						},
						Err(e) => {
							warn!(
								target: LOG_TARGET,
								"Worker {worker_index} failed to fetch keys: {e:?}. Requeueing for retry..."
							);

							// Tell connection manager to recreate this client
							conn_manager.recreate_client(worker_index, &client).await;

							// Put the range back in the queue for retry
							{
								let mut queue = work_queue.lock().unwrap();
								queue.push_back(range);
							}

							// Wait to avoid hammering a potentially failing connection
							sleep(Duration::from_secs(1)).await;
						},
					}
				}

				drop(permit);
			});

			handles.push(handle);
		}

		// Wait for all workers to complete
		futures::future::join_all(handles).await;

		// Extract and return all keys
		let keys = all_keys.lock().unwrap().clone();
		eprintln!(
			"🎉 Parallel key fetching complete: {} total keys fetched by {} workers",
			keys.len(),
			parallel
		);

		Ok(keys)
	}

	/// Get keys with `prefix` at `block` with multiple requests in parallel.
	async fn rpc_get_keys_parallel(
		&self,
		prefix: &StorageKey,
		block: B::Hash,
		parallel: usize,
	) -> Result<Vec<StorageKey>> {
		self.rpc_get_keys_parallel_internal(
			prefix,
			block,
			parallel,
			"Top-level keys",
			|builder, range, block, worker_index, client| async move {
				builder.rpc_get_keys_single_batch(range, block, worker_index, &client).await
			},
		)
		.await
	}

	/// Get child keys with `prefix` at `block` with multiple requests in parallel.
	async fn rpc_child_get_keys_parallel(
		&self,
		prefixed_top_key: &StorageKey,
		prefix: &StorageKey,
		block: B::Hash,
		parallel: usize,
	) -> Result<Vec<StorageKey>> {
		let prefixed_top_key = prefixed_top_key.clone();
		self.rpc_get_keys_parallel_internal(
			prefix,
			block,
			parallel,
			"Child keys",
			move |builder, range, block, worker_index, client| {
				let prefixed_top_key = prefixed_top_key.clone();
				async move {
					builder
						.rpc_child_get_keys_single_batch(
							range,
							&prefixed_top_key,
							block,
							worker_index,
							&client,
						)
						.await
				}
			},
		)
		.await
	}

	/// Get ONE batch of keys from the given range at `block`.
	/// Returns the keys and whether the batch was full (indicating more keys may exist).
	///
	/// Note: This method handles connection errors by indicating a restart is needed.
	/// The caller should handle reconnection logic.
	async fn rpc_get_keys_single_batch(
		&self,
		range: KeyRange,
		block: B::Hash,
		worker_index: usize,
		client: &WsClient,
	) -> Result<(Vec<StorageKey>, bool)> {
		// Fetch a single page of keys with retry logic
		// Note: The retry logic in get_keys_single_page handles transient errors,
		// but connection errors need to be propagated up for reconnection
		let mut page = self
			.get_keys_single_page_with_client(
				client,
				Some(range.prefix.clone()),
				Some(range.start_key.clone()),
				block,
			)
			.await?;

		// Avoid duplicated keys across workloads - filter out keys beyond our range
		if let (Some(last), Some(end)) = (page.last(), &range.end_key) {
			if last >= end {
				page.retain(|key| key < end);
			}
		}

		let page_len = page.len();
		let is_full_batch = page_len == Self::DEFAULT_KEY_DOWNLOAD_PAGE as usize;

		debug!(
			target: LOG_TARGET,
			"Worker {worker_index}: fetched {} keys from range, full_batch={}",
			page_len,
			is_full_batch
		);

		Ok((page, is_full_batch))
	}

	/// Subdivide the key space AFTER the last_key into up to 16 new ranges.
	///
	/// Takes the last two keys from a batch to find where they diverge, then creates
	/// ranges based on incrementing the nibble at the divergence point.
	fn subdivide_remaining_range(
		second_last_key: &StorageKey,
		last_key: &StorageKey,
		end_key: Option<&StorageKey>,
		prefix: &StorageKey,
	) -> Vec<KeyRange> {
		let second_last_bytes = second_last_key.as_ref();
		let last_key_bytes = last_key.as_ref();

		// Find the first byte position where the two keys diverge
		let divergence_pos = second_last_bytes
			.iter()
			.zip(last_key_bytes.iter())
			.position(|(a, b)| a != b)
			.unwrap_or(second_last_bytes.len().min(last_key_bytes.len()));

		let mut subdivision_nibble = None;

		for subdivision_pos in divergence_pos..last_key_bytes.len() {
			let byte = last_key_bytes[subdivision_pos];
			let nibble = byte >> 4;

			if nibble < 15 {
				subdivision_nibble = Some((subdivision_pos, nibble));
				break;
			}
		}

		let mut ranges = Vec::new();

		// If we found a position where we can subdivide
		if let Some((pos, current_nibble)) = subdivision_nibble {
			let subdivision_prefix = &last_key_bytes[..pos];

			let mut end = subdivision_prefix.to_vec();
			end.push((current_nibble + 1) << 4);
			ranges.push(KeyRange::new(last_key.clone(), Some(StorageKey(end)), prefix.clone()));

			// Create ranges for each nibble from (current_nibble + 1) to 0xF
			for nibble in (current_nibble + 1)..16u8 {
				let mut start = subdivision_prefix.to_vec();
				start.push(nibble << 4);
				let start_key = StorageKey(start);

				// Check if this range starts at or after the end
				if end_key.map_or(false, |ek| start_key >= *ek) {
					break
				}

				let chunk_end_key = if nibble < 15 {
					let mut end = subdivision_prefix.to_vec();
					end.push((nibble + 1) << 4);
					let computed_end = StorageKey(end);

					Some(match end_key {
						Some(actual_end) if &computed_end > actual_end => actual_end.clone(),
						_ => computed_end,
					})
				} else {
					end_key.cloned()
				};

				ranges.push(KeyRange::new(start_key, chunk_end_key, prefix.clone()));
			}
		} else if end_key.as_ref().map_or(true, |ek| last_key <= ek) {
			// We are not yet past the end
			ranges.push(KeyRange::new(last_key.clone(), end_key.cloned(), prefix.clone()));
		}

		ranges
	}

	/// Fetches storage data from a node using a dynamic batch size.
	///
	/// This function adjusts the batch size on the fly to help prevent overwhelming the node with
	/// large batch requests, and stay within request size limits enforced by the node.
	///
	/// # Arguments
	///
	/// * `client` - An `Arc` wrapped `HttpClient` used for making the requests.
	/// * `payloads` - A vector of tuples containing a JSONRPC method name and `ArrayParams`
	///
	/// # Returns
	///
	/// Returns a `Result` with a vector of `Option<StorageData>`, where each element corresponds to
	/// the storage data for the given method and parameters. The result will be an `Err` with a
	/// `String` error message if the request fails.
	///
	/// # Errors
	///
	/// This function will return an error if:
	/// * The batch request fails and the batch size is less than 2.
	/// * There are invalid batch params.
	/// * There is an error in the batch response.
	///
	/// # Example
	///
	/// ```ignore
	/// use your_crate::{get_storage_data_dynamic_batch_size, HttpClient, ArrayParams};
	/// use std::sync::Arc;
	///
	/// async fn example() {
	///     let client = HttpClient::new();
	///     let payloads = vec![
	///         ("some_method".to_string(), ArrayParams::new(vec![])),
	///         ("another_method".to_string(), ArrayParams::new(vec![])),
	///     ];
	///     let initial_batch_size = 10;
	///
	///     let storage_data = get_storage_data_dynamic_batch_size(client, payloads, batch_size).await;
	///     match storage_data {
	///         Ok(data) => println!("Storage data: {:?}", data),
	///         Err(e) => eprintln!("Error fetching storage data: {}", e),
	///     }
	/// }
	/// ```
	async fn get_storage_data_dynamic_batch_size(
		conn_manager: &ConnectionManager,
		worker_index: usize,
		payloads: Vec<(String, ArrayParams)>,
		bar: &ProgressBar,
	) -> Vec<Option<StorageData>> {
		let mut all_data: Vec<Option<StorageData>> = vec![];
		let mut start_index = 0;
		let batch_size = 1000;
		let total_payloads = payloads.len();

		while start_index < total_payloads {
			debug!(
				target: LOG_TARGET,
				"Value worker {worker_index}: Remaining payloads: {} Batch request size: {batch_size}",
				total_payloads - start_index,
			);

			let end_index = usize::min(start_index + batch_size, total_payloads);
			let page = &payloads[start_index..end_index];

			// Build the batch request
			let mut batch = BatchRequestBuilder::new();
			for (method, params) in page.iter() {
				if batch.insert(method, params.clone()).is_err() {
					panic!("Invalid batch method and/or params; qed");
				}
			}

			let batch_response = loop {
				let client = conn_manager.get(worker_index).await;

				match client.batch_request::<Option<StorageData>>(batch.clone()).await {
					Ok(response) => break response,
					Err(e) => {
						warn!(
							target: LOG_TARGET,
							"Value worker {worker_index}: batch request failed: {e:?}. Retrying..."
						);
						conn_manager.recreate_client(worker_index, &client).await;
						sleep(Duration::from_secs(1)).await;
					},
				}
			};

			debug!(
				target: LOG_TARGET,
				"Value worker {worker_index}: Batch size: {}",
				end_index - start_index,
			);

			let batch_response_len = batch_response.len();
			for item in batch_response.into_iter() {
				match item {
					Ok(x) => all_data.push(x),
					Err(e) => {
						warn!(target: LOG_TARGET, "Value worker {worker_index}: batch item error: {}", e.message());
						all_data.push(None);
					},
				}
			}
			bar.inc(batch_response_len as u64);

			start_index = end_index;
		}

		all_data
	}

	/// Synonym of `getPairs` that uses paged queries to first get the keys, and then
	/// map them to values one by one.
	///
	/// This can work with public nodes. But, expect it to be darn slow.
	pub(crate) async fn rpc_get_pairs(
		&self,
		prefix: StorageKey,
		at: B::Hash,
		pending_ext: &mut TestExternalities<HashingFor<B>>,
	) -> Result<Vec<KeyValue>> {
		let keys = logging::with_elapsed_async(
			|| async {
				// TODO: We could start downloading when having collected the first batch of keys.
				// https://github.com/paritytech/polkadot-sdk/issues/2494
				let keys = self
					.rpc_get_keys_parallel(&prefix, at, Self::PARALLEL_REQUESTS)
					.await?
					.into_iter()
					.collect::<Vec<_>>();

				Ok(keys)
			},
			"Scraping keys...",
			|keys| format!("Found {} keys", keys.len()),
		)
		.await?;

		if keys.is_empty() {
			return Ok(Default::default())
		}

		let conn_manager = self.conn_manager()?;

		let payloads = keys
			.iter()
			.map(|key| ("state_getStorage".to_string(), rpc_params!(key, at)))
			.collect::<Vec<_>>();

		let bar = ProgressBar::new(payloads.len() as u64);
		bar.enable_steady_tick(Duration::from_secs(1));
		bar.set_message("Downloading key values".to_string());
		bar.set_style(
			ProgressStyle::with_template(
				"[{elapsed_precise}] {msg} {per_sec} [{wide_bar}] {pos}/{len} ({eta})",
			)
			.unwrap()
			.progress_chars("=>-"),
		);

		// Create batches of payloads for dynamic work distribution
		// Each batch is: (start_index, payloads)
		const BATCH_SIZE: usize = 1000;
		let mut batches: VecDeque<(usize, Vec<(String, ArrayParams)>)> = VecDeque::new();
		for (batch_index, chunk) in payloads.chunks(BATCH_SIZE).enumerate() {
			batches.push_back((batch_index * BATCH_SIZE, chunk.to_vec()));
		}

		eprintln!("🔧 Initialized {} batches for dynamic value fetching", batches.len());
		eprintln!("🚀 Spawning {} parallel workers for value fetching", Self::PARALLEL_REQUESTS);

		// Shared structures for dynamic work distribution
		let work_queue = Arc::new(Mutex::new(batches));
		let results: Arc<Mutex<Vec<Option<StorageData>>>> =
			Arc::new(Mutex::new(vec![None; payloads.len()]));
		let active_workers = Arc::new(AtomicUsize::new(0));
		let semaphore = Arc::new(Semaphore::new(Self::PARALLEL_REQUESTS));

		// Spawn worker tasks
		let mut handles = vec![];
		for worker_index in 0..Self::PARALLEL_REQUESTS {
			let work_queue = Arc::clone(&work_queue);
			let results = Arc::clone(&results);
			let active_workers = Arc::clone(&active_workers);
			let conn_manager = conn_manager.clone();
			let bar = bar.clone();
			let semaphore = Arc::clone(&semaphore);

			let handle = tokio::spawn(async move {
				let permit = semaphore.acquire().await.unwrap();
				let mut is_active = false;

				loop {
					// Try to get work from the queue
					let work = {
						let mut queue = work_queue.lock().unwrap();
						let work = queue.pop_front();

						// Track active workers
						if work.is_some() && !is_active {
							active_workers.fetch_add(1, Ordering::SeqCst);
							is_active = true;
						}

						work
					};

					match work {
						Some((start_index, batch)) => {
							debug!(
								target: LOG_TARGET,
								"Value worker {worker_index}: Processing batch starting at index {start_index} with {} payloads",
								batch.len()
							);

							let batch_results = Self::get_storage_data_dynamic_batch_size(
								&conn_manager,
								worker_index,
								batch,
								&bar,
							)
							.await;

							// Store results in the correct positions
							let mut results_lock = results.lock().unwrap();
							for (offset, result) in batch_results.into_iter().enumerate() {
								results_lock[start_index + offset] = result;
							}
							debug!(
								target: LOG_TARGET,
								"Value worker {worker_index}: Successfully processed batch at index {start_index}"
							);
						},
						None => {
							// No more work in queue
							// Check if any other workers are still active
							if is_active {
								active_workers.fetch_sub(1, Ordering::SeqCst);
								is_active = false;
							}

							let active = active_workers.load(Ordering::SeqCst);
							if active == 0 {
								// All workers idle and queue empty - we're done
								debug!(
									target: LOG_TARGET,
									"Value worker {worker_index}: No more work and no active workers. Exiting."
								);
								break
							}

							// Wait a bit and check again
							sleep(Duration::from_millis(100)).await;
						},
					}
				}

				// Cleanup
				if is_active {
					active_workers.fetch_sub(1, Ordering::SeqCst);
				}

				drop(permit);
			});

			handles.push(handle);
		}

		// Wait for all workers to complete
		futures::future::join_all(handles).await;

		// Extract results
		let storage_data = Arc::try_unwrap(results)
			.map(|mutex| mutex.into_inner().unwrap())
			.unwrap_or_else(|arc| arc.lock().unwrap().clone());

		bar.finish_with_message("✅ Downloaded key values");
		println!();

		// Check if we got responses for all submitted requests.
		assert_eq!(keys.len(), storage_data.len());

		let key_values = keys
			.iter()
			.zip(storage_data)
			.map(|(key, maybe_value)| match maybe_value {
				Some(data) => (key.clone(), data),
				None => {
					warn!(target: LOG_TARGET, "key {key:?} had none corresponding value.");
					let data = StorageData(vec![]);
					(key.clone(), data)
				},
			})
			.collect::<Vec<_>>();

		logging::with_elapsed(
			|| {
				pending_ext.batch_insert(key_values.clone().into_iter().filter_map(|(k, v)| {
					// Don't insert the child keys here, they need to be inserted separately with
					// all their data in the load_child_remote function.
					match is_default_child_storage_key(&k.0) {
						true => None,
						false => Some((k.0, v.0)),
					}
				}));

				Ok(())
			},
			"Inserting keys into DB...",
			|_| "Inserted keys into DB".into(),
		)
		.expect("must succeed; qed");

		Ok(key_values)
	}

	/// Get the values corresponding to `child_keys` at the given `prefixed_top_key`.
	pub(crate) async fn rpc_child_get_storage_paged(
		conn_manager: &ConnectionManager,
		worker_index: usize,
		prefixed_top_key: &StorageKey,
		child_keys: Vec<StorageKey>,
		at: B::Hash,
	) -> Result<Vec<KeyValue>> {
		let child_keys_len = child_keys.len();

		let payloads = child_keys
			.iter()
			.map(|key| {
				(
					"childstate_getStorage".to_string(),
					rpc_params![
						PrefixedStorageKey::new(prefixed_top_key.as_ref().to_vec()),
						key,
						at
					],
				)
			})
			.collect::<Vec<_>>();

		let bar = ProgressBar::new(payloads.len() as u64);
		let storage_data =
			Self::get_storage_data_dynamic_batch_size(conn_manager, worker_index, payloads, &bar)
				.await;

		assert_eq!(child_keys_len, storage_data.len());

		Ok(child_keys
			.iter()
			.zip(storage_data)
			.map(|(key, maybe_value)| match maybe_value {
				Some(v) => (key.clone(), v),
				None => {
					warn!(target: LOG_TARGET, "key {key:?} had no corresponding value.");
					(key.clone(), StorageData(vec![]))
				},
			})
			.collect::<Vec<_>>())
	}

	/// Get ONE batch of child keys from the given range at `block`.
	async fn rpc_child_get_keys_single_batch(
		&self,
		range: KeyRange,
		prefixed_top_key: &StorageKey,
		block: B::Hash,
		worker_index: usize,
		client: &WsClient,
	) -> Result<(Vec<StorageKey>, bool)> {
		let top_key = PrefixedStorageKey::new(prefixed_top_key.0.clone());

		let mut page = substrate_rpc_client::ChildStateApi::storage_keys_paged(
			client,
			top_key,
			Some(range.prefix.clone()),
			Self::DEFAULT_KEY_DOWNLOAD_PAGE,
			Some(range.start_key.clone()),
			Some(block),
		)
		.await
		.map_err(|e| {
			error!(target: LOG_TARGET, "Error = {e:?}");
			"rpc child_get_keys failed"
		})?;

		// Avoid duplicated keys across workloads
		if let (Some(last), Some(end)) = (page.last(), &range.end_key) {
			if last >= end {
				page.retain(|key| key < end);
			}
		}

		let page_len = page.len();
		let is_full_batch = page_len == Self::DEFAULT_KEY_DOWNLOAD_PAGE as usize;

		debug!(
			target: LOG_TARGET,
			"Worker {worker_index}: fetched {} child keys from range, full_batch={}",
			page_len,
			is_full_batch
		);

		Ok((page, is_full_batch))
	}
}

impl<B: BlockT> Builder<B>
where
	B::Hash: DeserializeOwned,
	B::Header: DeserializeOwned,
{
	/// Load all of the child keys from the remote config, given the already scraped list of top key
	/// pairs.
	///
	/// `top_kv` need not be only child-bearing top keys. It should be all of the top keys that are
	/// included thus far.
	///
	/// This function concurrently populates `pending_ext`. the return value is only for writing to
	/// cache, we can also optimize further.
	async fn load_child_remote(
		&self,
		top_kv: &[KeyValue],
		pending_ext: &mut TestExternalities<HashingFor<B>>,
	) -> Result<ChildKeyValues> {
		let child_roots = top_kv
			.iter()
			.filter(|(k, _)| is_default_child_storage_key(k.as_ref()))
			.map(|(k, _)| k.clone())
			.collect::<Vec<_>>();

		if child_roots.is_empty() {
			info!(target: LOG_TARGET, "👩‍👦 no child roots found to scrape");
			return Ok(Default::default())
		}

		info!(
			target: LOG_TARGET,
			"👩‍👦 scraping child-tree data from {} top keys",
			child_roots.len(),
		);

		let at = self.as_online().at_expected();

		let conn_manager = self.conn_manager()?;
		let mut child_kv = vec![];
		for (worker_index, prefixed_top_key) in child_roots.iter().enumerate() {
			let child_keys = self
				.rpc_child_get_keys_parallel(
					&prefixed_top_key,
					&StorageKey(vec![]),
					at,
					Self::PARALLEL_REQUESTS,
				)
				.await?;

			let child_kv_inner = Self::rpc_child_get_storage_paged(
				&conn_manager,
				worker_index,
				&prefixed_top_key,
				child_keys,
				at,
			)
			.await?;

			let prefixed_top_key = PrefixedStorageKey::new(prefixed_top_key.clone().0);
			let un_prefixed = match ChildType::from_prefixed_key(&prefixed_top_key) {
				Some((ChildType::ParentKeyId, storage_key)) => storage_key,
				None => {
					error!(target: LOG_TARGET, "invalid key: {prefixed_top_key:?}");
					return Err("Invalid child key")
				},
			};

			let info = ChildInfo::new_default(un_prefixed);
			let key_values =
				child_kv_inner.iter().cloned().map(|(k, v)| (k.0, v.0)).collect::<Vec<_>>();
			child_kv.push((info.clone(), child_kv_inner));
			for (k, v) in key_values {
				pending_ext.insert_child(info.clone(), k, v);
			}
		}

		Ok(child_kv)
	}

	/// Build `Self` from a network node denoted by `uri`.
	///
	/// This function concurrently populates `pending_ext`. the return value is only for writing to
	/// cache, we can also optimize further.
	async fn load_top_remote(
		&self,
		pending_ext: &mut TestExternalities<HashingFor<B>>,
	) -> Result<TopKeyValues> {
		let config = self.as_online();
		let at = self
			.as_online()
			.at
			.expect("online config must be initialized by this point; qed.");
		info!(target: LOG_TARGET, "scraping key-pairs from remote at block height {at:?}");

		let mut keys_and_values = Vec::new();
		for prefix in &config.hashed_prefixes {
			let now = std::time::Instant::now();
			let additional_key_values =
				self.rpc_get_pairs(StorageKey(prefix.to_vec()), at, pending_ext).await?;
			let elapsed = now.elapsed();
			info!(
				target: LOG_TARGET,
				"adding data for hashed prefix: {:?}, took {:.2}s",
				HexDisplay::from(prefix),
				elapsed.as_secs_f32()
			);
			keys_and_values.extend(additional_key_values);
		}

		for key in &config.hashed_keys {
			let key = StorageKey(key.to_vec());
			info!(
				target: LOG_TARGET,
				"adding data for hashed key: {:?}",
				HexDisplay::from(&key)
			);
			match self.rpc_get_storage(key.clone(), Some(at)).await? {
				Some(value) => {
					pending_ext.insert(key.clone().0, value.clone().0);
					keys_and_values.push((key, value));
				},
				None => {
					warn!(
						target: LOG_TARGET,
						"no data found for hashed key: {:?}",
						HexDisplay::from(&key)
					);
				},
			}
		}

		Ok(keys_and_values)
	}

	/// The entry point of execution, if `mode` is online.
	///
	/// Initializes the remote clients and sets the `at` field if not specified.
	async fn init_remote_client(&mut self) -> Result<()> {
		// First, create all clients from URIs, filtering out ones that fail to connect.
		let online_config = self.as_online();
		let mut clients = Vec::new();
		for uri in &online_config.transport_uris {
			if let Some(client) = Client::new(uri.clone()).await {
				clients.push(Arc::new(tokio::sync::Mutex::new(client)));
			}
		}
		self.conn_manager = Some(ConnectionManager::new(clients)?);

		// Then, if `at` is not set, set it.
		if self.as_online().at.is_none() {
			let at = self.rpc_get_head().await?;
			info!(
				target: LOG_TARGET,
				"since no at is provided, setting it to latest finalized head, {at:?}",
			);
			self.as_online_mut().at = Some(at);
		}

		// Then, a few transformation that we want to perform in the online config:
		let online_config = self.as_online_mut();
		online_config.pallets.iter().for_each(|p| {
			online_config
				.hashed_prefixes
				.push(sp_crypto_hashing::twox_128(p.as_bytes()).to_vec())
		});

		if online_config.child_trie {
			online_config.hashed_prefixes.push(DEFAULT_CHILD_STORAGE_KEY_PREFIX.to_vec());
		}

		// Finally, if by now, we have put any limitations on prefixes that we are interested in, we
		// download everything.
		if online_config
			.hashed_prefixes
			.iter()
			.filter(|p| *p != DEFAULT_CHILD_STORAGE_KEY_PREFIX)
			.count() == 0
		{
			info!(
				target: LOG_TARGET,
				"since no prefix is filtered, the data for all pallets will be downloaded"
			);
			online_config.hashed_prefixes.push(vec![]);
		}

		Ok(())
	}

	async fn load_header(&self) -> Result<B::Header> {
		let client = self.rpc_client().await?;
		let at = self.as_online().at_expected();
		ChainApi::<(), _, B::Header, ()>::header(client.as_ref(), Some(at))
			.await
			.map_err(|e| {
				error!(target: LOG_TARGET, "Error = {e:?}");
				"rpc header failed"
			})?
			.ok_or("Network returned None block header")
	}

	/// Load the data from a remote server. The main code path is calling into `load_top_remote` and
	/// `load_child_remote`.
	///
	/// Must be called after `init_remote_client`.
	async fn load_remote_and_maybe_save(&mut self) -> Result<TestExternalities<HashingFor<B>>> {
		let client = self.rpc_client().await?;
		let state_version = StateApi::<B::Hash>::runtime_version(client.as_ref(), None)
			.await
			.map_err(|e| {
				error!(target: LOG_TARGET, "Error = {e:?}");
				"rpc runtime_version failed."
			})
			.map(|v| v.state_version())?;
		let mut pending_ext = TestExternalities::new_with_code_and_state(
			Default::default(),
			Default::default(),
			self.overwrite_state_version.unwrap_or(state_version),
		);

		// Load data from the remote into `pending_ext`.
		let top_kv = self.load_top_remote(&mut pending_ext).await?;
		self.load_child_remote(&top_kv, &mut pending_ext).await?;

		// Verify that the computed storage root matches the one in the block header
		let header = self.load_header().await?;
		let expected_root = header.state_root();
		let (raw_storage, computed_root) = pending_ext.into_raw_snapshot();

		if &computed_root != expected_root {
			error!(
				target: LOG_TARGET,
				"State root mismatch! Expected: {:?}, Computed: {:?}",
				expected_root,
				computed_root
			);
			return Err("Downloaded state does not match the expected storage root");
		}

		info!(
			target: LOG_TARGET,
			"✅ Storage root verification successful: {:?}",
			computed_root
		);

		// If we need to save a snapshot, save the raw storage and root hash to the snapshot.
		if let Some(path) = self.as_online().state_snapshot.clone().map(|c| c.path) {
			let snapshot =
				Snapshot::<B>::new(state_version, raw_storage.clone(), computed_root, header);
			let encoded = snapshot.encode();
			info!(
				target: LOG_TARGET,
				"writing snapshot of {} bytes to {path:?}",
				encoded.len(),
			);
			std::fs::write(path, encoded).map_err(|_| "fs::write failed")?;
		}

		// Return the externalities (reconstructed from verified snapshot)
		Ok(TestExternalities::from_raw_snapshot(
			raw_storage,
			computed_root,
			self.overwrite_state_version.unwrap_or(state_version),
		))
	}

	async fn do_load_remote(&mut self) -> Result<RemoteExternalities<B>> {
		self.init_remote_client().await?;
		let inner_ext = self.load_remote_and_maybe_save().await?;
		Ok(RemoteExternalities { header: self.load_header().await?, inner_ext })
	}

	fn do_load_offline(&mut self, config: OfflineConfig) -> Result<RemoteExternalities<B>> {
		let (header, inner_ext) = logging::with_elapsed(
			|| {
				info!(target: LOG_TARGET, "Loading snapshot from {:?}", &config.state_snapshot.path);

				let Snapshot { header, state_version, raw_storage, storage_root, .. } =
					Snapshot::<B>::load(&config.state_snapshot.path)?;
				let inner_ext = TestExternalities::from_raw_snapshot(
					raw_storage,
					storage_root,
					self.overwrite_state_version.unwrap_or(state_version),
				);

				Ok((header, inner_ext))
			},
			"Loading snapshot...",
			|_| "Loaded snapshot".into(),
		)?;

		Ok(RemoteExternalities { inner_ext, header })
	}

	pub(crate) async fn pre_build(mut self) -> Result<RemoteExternalities<B>> {
		let mut ext = match self.mode.clone() {
			Mode::Offline(config) => self.do_load_offline(config)?,
			Mode::Online(_) => self.do_load_remote().await?,
			Mode::OfflineOrElseOnline(offline_config, _) => {
				match self.do_load_offline(offline_config) {
					Ok(x) => x,
					Err(_) => self.do_load_remote().await?,
				}
			},
		};

		// inject manual key values.
		if !self.hashed_key_values.is_empty() {
			info!(
				target: LOG_TARGET,
				"extending externalities with {} manually injected key-values",
				self.hashed_key_values.len()
			);
			ext.batch_insert(self.hashed_key_values.into_iter().map(|(k, v)| (k.0, v.0)));
		}

		// exclude manual key values.
		if !self.hashed_blacklist.is_empty() {
			info!(
				target: LOG_TARGET,
				"excluding externalities from {} keys",
				self.hashed_blacklist.len()
			);
			for k in self.hashed_blacklist {
				ext.execute_with(|| sp_io::storage::clear(&k));
			}
		}

		Ok(ext)
	}
}

// Public methods
impl<B: BlockT> Builder<B>
where
	B::Hash: DeserializeOwned,
	B::Header: DeserializeOwned,
{
	/// Create a new builder.
	pub fn new() -> Self {
		Default::default()
	}

	/// Inject a manual list of key and values to the storage.
	pub fn inject_hashed_key_value(mut self, injections: Vec<KeyValue>) -> Self {
		for i in injections {
			self.hashed_key_values.push(i.clone());
		}
		self
	}

	/// Blacklist this hashed key from the final externalities. This is treated as-is, and should be
	/// pre-hashed.
	pub fn blacklist_hashed_key(mut self, hashed: &[u8]) -> Self {
		self.hashed_blacklist.push(hashed.to_vec());
		self
	}

	/// Configure a state snapshot to be used.
	pub fn mode(mut self, mode: Mode<B::Hash>) -> Self {
		self.mode = mode;
		self
	}

	/// The state version to use.
	pub fn overwrite_state_version(mut self, version: StateVersion) -> Self {
		self.overwrite_state_version = Some(version);
		self
	}

	pub async fn build(self) -> Result<RemoteExternalities<B>> {
		let mut ext = self.pre_build().await?;
		ext.commit_all().unwrap();

		info!(
			target: LOG_TARGET,
			"initialized state externalities with storage root {:?} and state_version {:?}",
			ext.as_backend().root(),
			ext.state_version
		);

		Ok(ext)
	}
}

#[cfg(test)]
mod test_prelude {
	pub(crate) use super::*;
	pub(crate) use sp_runtime::testing::{Block as RawBlock, MockCallU64};
	pub(crate) type UncheckedXt = sp_runtime::testing::TestXt<MockCallU64, ()>;
	pub(crate) type Block = RawBlock<UncheckedXt>;

	pub(crate) fn init_logger() {
		sp_tracing::try_init_simple();
	}
}

#[cfg(test)]
mod tests {
	use super::test_prelude::*;

	#[tokio::test]
	async fn can_load_state_snapshot() {
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig {
				state_snapshot: SnapshotConfig::new("test_data/test.snap"),
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});
	}

	#[tokio::test]
	async fn can_exclude_from_snapshot() {
		init_logger();

		// get the first key from the snapshot file.
		let some_key = Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig {
				state_snapshot: SnapshotConfig::new("test_data/test.snap"),
			}))
			.build()
			.await
			.expect("Can't read state snapshot file")
			.execute_with(|| {
				let key =
					sp_io::storage::next_key(&[]).expect("some key must exist in the snapshot");
				assert!(sp_io::storage::get(&key).is_some());
				key
			});

		Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig {
				state_snapshot: SnapshotConfig::new("test_data/test.snap"),
			}))
			.blacklist_hashed_key(&some_key)
			.build()
			.await
			.expect("Can't read state snapshot file")
			.execute_with(|| assert!(sp_io::storage::get(&some_key).is_none()));
	}
}

#[cfg(all(test, feature = "remote-test"))]
mod remote_tests {
	use super::test_prelude::*;
	use std::{env, os::unix::fs::MetadataExt};

	fn endpoint() -> String {
		env::var("TEST_WS").unwrap_or_else(|_| DEFAULT_HTTP_ENDPOINT.to_string())
	}

	#[tokio::test]
	async fn state_version_is_kept_and_can_be_altered() {
		const CACHE: &'static str = "state_version_is_kept_and_can_be_altered";
		init_logger();

		// first, build a snapshot.
		let ext = Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned()],
				child_trie: false,
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				..Default::default()
			}))
			.build()
			.await
			.unwrap();

		// now re-create the same snapshot.
		let cached_ext = Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig { state_snapshot: SnapshotConfig::new(CACHE) }))
			.build()
			.await
			.unwrap();

		assert_eq!(ext.state_version, cached_ext.state_version);

		// now overwrite it
		let other = match ext.state_version {
			StateVersion::V0 => StateVersion::V1,
			StateVersion::V1 => StateVersion::V0,
		};
		let cached_ext = Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig { state_snapshot: SnapshotConfig::new(CACHE) }))
			.overwrite_state_version(other)
			.build()
			.await
			.unwrap();

		assert_eq!(cached_ext.state_version, other);
	}

	#[tokio::test]
	async fn snapshot_block_hash_works() {
		const CACHE: &'static str = "snapshot_block_hash_works";
		init_logger();

		// first, build a snapshot.
		let ext = Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned()],
				child_trie: false,
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				..Default::default()
			}))
			.build()
			.await
			.unwrap();

		// now re-create the same snapshot.
		let cached_ext = Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig { state_snapshot: SnapshotConfig::new(CACHE) }))
			.build()
			.await
			.unwrap();

		assert_eq!(ext.header.hash(), cached_ext.header.hash());
	}

	#[tokio::test]
	async fn child_keys_are_loaded() {
		const CACHE: &'static str = "snapshot_retains_storage";
		init_logger();

		// create an ext with children keys
		let mut child_ext = Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned()],
				child_trie: true,
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				..Default::default()
			}))
			.build()
			.await
			.unwrap();

		// create an ext without children keys
		let mut ext = Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned()],
				child_trie: false,
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				..Default::default()
			}))
			.build()
			.await
			.unwrap();

		// there should be more keys in the child ext.
		assert!(
			child_ext.as_backend().backend_storage().keys().len() >
				ext.as_backend().backend_storage().keys().len()
		);
	}

	#[tokio::test]
	async fn offline_else_online_works() {
		const CACHE: &'static str = "offline_else_online_works_data";
		init_logger();
		// this shows that in the second run, we use the remote and create a snapshot.
		Builder::<Block>::new()
			.mode(Mode::OfflineOrElseOnline(
				OfflineConfig { state_snapshot: SnapshotConfig::new(CACHE) },
				OnlineConfig {
					transport_uris: vec![endpoint().clone()],
					pallets: vec!["Proxy".to_owned()],
					child_trie: false,
					state_snapshot: Some(SnapshotConfig::new(CACHE)),
					..Default::default()
				},
			))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});

		// this shows that in the second run, we are not using the remote
		Builder::<Block>::new()
			.mode(Mode::OfflineOrElseOnline(
				OfflineConfig { state_snapshot: SnapshotConfig::new(CACHE) },
				OnlineConfig {
					transport_uris: vec!["ws://non-existent:666".to_owned()],
					..Default::default()
				},
			))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});

		let to_delete = std::fs::read_dir(Path::new("."))
			.unwrap()
			.into_iter()
			.map(|d| d.unwrap())
			.filter(|p| p.path().file_name().unwrap_or_default() == CACHE)
			.collect::<Vec<_>>();

		assert!(to_delete.len() == 1);
		std::fs::remove_file(to_delete[0].path()).unwrap();
	}

	#[tokio::test]
	async fn can_build_one_small_pallet() {
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned()],
				child_trie: false,
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});
	}

	#[tokio::test]
	async fn can_build_few_pallet() {
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Proxy".to_owned(), "Multisig".to_owned()],
				child_trie: false,
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn can_create_snapshot() {
		const CACHE: &'static str = "can_create_snapshot";
		init_logger();

		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				pallets: vec!["Proxy".to_owned()],
				child_trie: false,
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});

		let to_delete = std::fs::read_dir(Path::new("."))
			.unwrap()
			.into_iter()
			.map(|d| d.unwrap())
			.filter(|p| p.path().file_name().unwrap_or_default() == CACHE)
			.collect::<Vec<_>>();

		assert!(to_delete.len() == 1);
		let to_delete = to_delete.first().unwrap();
		assert!(std::fs::metadata(to_delete.path()).unwrap().size() > 1);
		std::fs::remove_file(to_delete.path()).unwrap();
	}

	#[tokio::test]
	async fn can_create_child_snapshot() {
		const CACHE: &'static str = "can_create_child_snapshot";
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				state_snapshot: Some(SnapshotConfig::new(CACHE)),
				pallets: vec!["Crowdloan".to_owned()],
				child_trie: true,
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});

		let to_delete = std::fs::read_dir(Path::new("."))
			.unwrap()
			.into_iter()
			.map(|d| d.unwrap())
			.filter(|p| p.path().file_name().unwrap_or_default() == CACHE)
			.collect::<Vec<_>>();

		assert!(to_delete.len() == 1);
		let to_delete = to_delete.first().unwrap();
		assert!(std::fs::metadata(to_delete.path()).unwrap().size() > 1);
		std::fs::remove_file(to_delete.path()).unwrap();
	}

	#[tokio::test]
	async fn can_build_big_pallet() {
		if std::option_env!("TEST_WS").is_none() {
			return
		}
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				pallets: vec!["Staking".to_owned()],
				child_trie: false,
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});
	}

	#[tokio::test]
	async fn can_fetch_all() {
		if std::option_env!("TEST_WS").is_none() {
			return
		}
		init_logger();
		Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: vec![endpoint().clone()],
				..Default::default()
			}))
			.build()
			.await
			.unwrap()
			.execute_with(|| {});
	}

	#[tokio::test]
	async fn can_fetch_in_parallel() {
		init_logger();

		let mut builder = Builder::<Block>::new().mode(Mode::Online(OnlineConfig {
			transport_uris: vec![endpoint().clone()],
			..Default::default()
		}));
		builder.init_remote_client().await.unwrap();

		let at = builder.as_online().at.unwrap();

		// Test with a specific prefix
		let prefix = StorageKey(vec![13]);
		let para = builder.rpc_get_keys_parallel(&prefix, at, 4).await.unwrap();
		assert!(!para.is_empty(), "Should fetch some keys with prefix");

		// Test with empty prefix (all keys)
		let prefix = StorageKey(vec![]);
		let para = builder.rpc_get_keys_parallel(&prefix, at, 8).await.unwrap();
		assert!(!para.is_empty(), "Should fetch some keys with empty prefix");
	}

	#[tokio::test]
	#[ignore] // This test takes a long time, run with --ignored
	async fn bridge_hub_polkadot_storage_root_matches() {
		init_logger();

		// Use multiple RPC providers for load distribution
		let endpoints = vec![
			"wss://bridge-hub-polkadot-rpc.n.dwellir.com",
			"wss://sys.ibp.network/bridgehub-polkadot",
			"wss://bridgehub-polkadot.api.onfinality.io/public",
			"wss://dot-rpc.stakeworld.io/bridgehub",
		];

		info!(target: LOG_TARGET, "Connecting to Bridge Hub Polkadot using {} RPC providers", endpoints.len());

		let mut ext = Builder::<Block>::new()
			.mode(Mode::Online(OnlineConfig {
				transport_uris: endpoints.into_iter().map(|e| e.to_owned()).collect(),
				child_trie: true,
				..Default::default()
			}))
			.build()
			.await
			.expect("Failed to build remote externalities");

		// Get the computed storage root from our downloaded state
		let backend = ext.as_backend();
		let computed_root = *backend.root();
		// Get the expected storage root from the block header
		let expected_root = ext.header.state_root;

		info!(
			target: LOG_TARGET,
			"Computed storage root: {:?}",
			computed_root
		);
		info!(
			target: LOG_TARGET,
			"Expected storage root (from header): {:?}",
			expected_root
		);

		// The storage roots must match exactly - this proves we downloaded all keys correctly
		assert_eq!(
			computed_root, expected_root,
			"Storage root mismatch! Computed: {:?}, Expected: {:?}. \
			This indicates that not all keys were fetched or there were duplicates.",
			computed_root, expected_root
		);

		// Verify we actually got some keys
		ext.execute_with(|| {
			let key_count = sp_io::storage::next_key(&[])
				.map(|first_key| {
					let mut count = 1;
					let mut current = first_key;
					while let Some(next) = sp_io::storage::next_key(&current) {
						count += 1;
						current = next;
					}
					count
				})
				.unwrap_or(0);

			info!(target: LOG_TARGET, "Total keys in state: {}", key_count);
			assert!(key_count > 0, "Should have fetched some keys");
		});

		info!(
			target: LOG_TARGET,
			"✅ Storage root verification successful! All keys were fetched correctly."
		);
	}
}

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

mod client;
mod config;
mod key_range;
mod logging;

pub use config::{Mode, OfflineConfig, OnlineConfig, SnapshotConfig};

use client::{Client, ConnectionManager, VersionedClient};
use codec::Encode;
use config::Snapshot;
#[cfg(all(test, feature = "remote-test"))]
use config::DEFAULT_HTTP_ENDPOINT;
use indicatif::{ProgressBar, ProgressStyle};
use jsonrpsee::{core::params::ArrayParams, ws_client::WsClient};
use key_range::{initialize_work_queue, subdivide_remaining_range, KeyRange};
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
	ops::{Deref, DerefMut},
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

const LOG_TARGET: &str = "remote-ext";

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
	const PARALLEL_REQUESTS_PER_CLIENT: usize = 4;

	fn parallel_requests(&self) -> usize {
		self.conn_manager
			.as_ref()
			.map(|cm| cm.num_clients() * Self::PARALLEL_REQUESTS_PER_CLIENT)
			.unwrap_or(Self::PARALLEL_REQUESTS_PER_CLIENT)
	}

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
		page_size: u32,
	) -> Result<Vec<StorageKey>> {
		client
			.storage_keys_paged(prefix, page_size, start_key, Some(at))
			.await
			.map_err(|e| {
				error!(target: LOG_TARGET, "Error = {e:?}");
				"rpc get_keys failed"
			})
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
		F: Fn(Arc<Self>, KeyRange, B::Hash, usize, Arc<WsClient>, u32) -> Fut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		Fut: std::future::Future<Output = Result<(Vec<StorageKey>, bool)>> + Send + 'static,
	{
		// Initialize work queue with top-level 16 ranges for this prefix
		let work_queue = initialize_work_queue(&[prefix.clone()]);
		let initial_ranges = work_queue.lock().unwrap().len();
		info!(
			target: LOG_TARGET,
			"🔧 Initialized work queue with {} ranges for parallel fetching", initial_ranges
		);

		// Get connection manager for handling client recreation across multiple RPC providers
		let conn_manager = Arc::new(self.conn_manager()?.clone());
		info!(
			target: LOG_TARGET,
			"🌐 Using {} RPC provider(s) for parallel fetching", conn_manager.num_clients()
		);

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
		info!(target: LOG_TARGET, "🚀 Spawning {parallel} parallel workers for key fetching");

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
						range.page_size,
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
									info!(
										target: LOG_TARGET,
										"📊 {log_prefix}: Scraped {total_keys} keys so far..."
									);
								}
							}

							// If we got a full batch, subdivide the remaining key space
							if is_full_batch {
								if let Some((second_last, last)) = last_two_keys {
									let new_ranges = subdivide_remaining_range(
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
							debug!(
								target: LOG_TARGET,
								"Worker {worker_index} failed to fetch keys: {e:?}"
							);

							// Put the range back in the queue with halved page size
							work_queue.lock().unwrap().push_back(range.with_halved_page_size());

							// Wait before recreating the client
							sleep(Duration::from_secs(15)).await;

							// Tell connection manager to recreate this client
							let _ = conn_manager.recreate_client(worker_index, &client).await;
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
		info!(
			target: LOG_TARGET,
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
			|builder, range, block, worker_index, client, page_size| async move {
				builder
					.rpc_get_keys_single_batch(range, block, worker_index, &client, page_size)
					.await
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
		page_size: u32,
	) -> Result<(Vec<StorageKey>, bool)> {
		let mut page = self
			.get_keys_single_page_with_client(
				client,
				Some(range.prefix.clone()),
				Some(range.start_key.clone()),
				block,
				page_size,
			)
			.await?;

		// Determine if this was a full batch BEFORE filtering
		let is_full_batch = page.len() == page_size as usize;

		// Avoid duplicated keys across workloads - filter out keys beyond our range
		if let (Some(last), Some(end)) = (page.last(), &range.end_key) {
			if last >= end {
				page.retain(|key| key < end);
			}
		}

		let page_len = page.len();

		debug!(
			target: LOG_TARGET,
			"Worker {worker_index}: fetched {} keys from range, full_batch={}",
			page_len,
			is_full_batch
		);

		Ok((page, is_full_batch))
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
		client: &VersionedClient,
		worker_index: usize,
		payloads: &[(String, ArrayParams)],
		bar: &ProgressBar,
		batch_size: usize,
	) -> std::result::Result<Vec<Option<StorageData>>, String> {
		let mut all_data: Vec<Option<StorageData>> = vec![];
		let mut start_index = 0;
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

			let batch_response = client
				.batch_request::<Option<StorageData>>(batch)
				.await
				.map_err(|e| format!("{e:?}"))?;

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

		Ok(all_data)
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
		let parallel = self.parallel_requests();
		let keys = logging::with_elapsed_async(
			|| async { self.rpc_get_keys_parallel(&prefix, at, parallel).await },
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
		// Each batch is: (start_index, payloads, batch_size)
		const BATCH_SIZE: usize = 1000;
		let mut batches: VecDeque<(usize, Vec<(String, ArrayParams)>, usize)> = VecDeque::new();
		for (batch_index, chunk) in payloads.chunks(BATCH_SIZE).enumerate() {
			batches.push_back((batch_index * BATCH_SIZE, chunk.to_vec(), BATCH_SIZE));
		}

		let parallel = self.parallel_requests();

		info!(target: LOG_TARGET, "🔧 Initialized {} batches for dynamic value fetching", batches.len());
		info!(target: LOG_TARGET, "🚀 Spawning {} parallel workers for value fetching", parallel);

		// Shared structures for dynamic work distribution
		let work_queue = Arc::new(Mutex::new(batches));
		let results: Arc<Mutex<Vec<Option<StorageData>>>> =
			Arc::new(Mutex::new(vec![None; payloads.len()]));
		let active_workers = Arc::new(AtomicUsize::new(0));
		let semaphore = Arc::new(Semaphore::new(parallel));

		// Spawn worker tasks
		let mut handles = vec![];
		for worker_index in 0..parallel {
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
						Some((start_index, batch, batch_size)) => {
							debug!(
								target: LOG_TARGET,
								"Value worker {worker_index}: Processing batch starting at index {start_index} with {} payloads",
								batch.len()
							);

							let client = conn_manager.get(worker_index).await;

							match Self::get_storage_data_dynamic_batch_size(
								&client,
								worker_index,
								&batch,
								&bar,
								batch_size,
							)
							.await
							{
								Ok(batch_results) => {
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
								Err(e) => {
									debug!(
										target: LOG_TARGET,
										"Value worker {worker_index}: batch request failed: {e:?}"
									);

									// Put batch back in queue with halved batch_size (minimum 10)
									let new_batch_size = (batch_size / 2).max(10);
									work_queue.lock().unwrap().push_back((
										start_index,
										batch,
										new_batch_size,
									));

									// Wait before recreating the client
									sleep(Duration::from_secs(15)).await;

									// Recreate the client
									let _ =
										conn_manager.recreate_client(worker_index, &client).await;
								},
							}
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

		// Filter out None values - keys without values should NOT be inserted
		// (inserting with empty value would change the trie structure)
		let key_values: Vec<_> = keys
			.iter()
			.zip(storage_data)
			.filter_map(|(key, maybe_value)| maybe_value.map(|data| (key.clone(), data)))
			.collect();

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

		let storage_data = {
			let mut batch_size = 1000usize;
			loop {
				let client = conn_manager.get(worker_index).await;

				match Self::get_storage_data_dynamic_batch_size(
					&client,
					worker_index,
					&payloads,
					&bar,
					batch_size,
				)
				.await
				{
					Ok(data) => break data,
					Err(e) => {
						debug!(
							target: LOG_TARGET,
							"Child storage worker {worker_index}: batch request failed: {e:?}"
						);
						// Halve the batch size on failure (minimum 10)
						batch_size = (batch_size / 2).max(10);
						sleep(Duration::from_secs(15)).await;
						let _ = conn_manager.recreate_client(worker_index, &client).await;
					},
				}
			}
		};

		assert_eq!(child_keys_len, storage_data.len());

		// Filter out None values - keys without values should NOT be inserted
		// (inserting with empty value would change the child trie structure)
		Ok(child_keys
			.iter()
			.zip(storage_data)
			.filter_map(|(key, maybe_value)| maybe_value.map(|v| (key.clone(), v)))
			.collect::<Vec<_>>())
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
	/// This function uses parallel workers to fetch child tries concurrently.
	/// Each worker handles complete child tries using simple sequential fetching.
	async fn load_child_remote(
		&self,
		top_kv: &[KeyValue],
		pending_ext: &mut TestExternalities<HashingFor<B>>,
	) -> Result<ChildKeyValues> {
		let child_roots: Vec<StorageKey> = top_kv
			.iter()
			.filter(|(k, _)| is_default_child_storage_key(k.as_ref()))
			.map(|(k, _)| k.clone())
			.collect();

		if child_roots.is_empty() {
			info!(target: LOG_TARGET, "👩‍👦 no child roots found to scrape");
			return Ok(Default::default())
		}

		info!(
			target: LOG_TARGET,
			"👩‍👦 scraping child-tree data from {} child tries using parallel workers",
			child_roots.len(),
		);

		let at = self.as_online().at_expected();
		let conn_manager = self.conn_manager()?;
		let parallel = self.parallel_requests();

		// Create a work queue with all child roots
		let work_queue =
			Arc::new(std::sync::Mutex::new(child_roots.into_iter().collect::<VecDeque<_>>()));

		// Results will be collected here
		let results: Arc<std::sync::Mutex<Vec<(ChildInfo, Vec<KeyValue>)>>> =
			Arc::new(std::sync::Mutex::new(Vec::new()));

		// Progress tracking
		let completed_count = Arc::new(AtomicUsize::new(0));
		let total_count = work_queue.lock().unwrap().len();

		// Spawn workers
		let mut handles = Vec::new();
		for worker_index in 0..parallel {
			let work_queue = Arc::clone(&work_queue);
			let results = Arc::clone(&results);
			let completed_count = Arc::clone(&completed_count);
			let conn_manager = conn_manager.clone();
			let at = at;

			let handle = tokio::spawn(async move {
				loop {
					// Get next child trie to process
					let prefixed_top_key = {
						let mut queue = work_queue.lock().unwrap();
						queue.pop_front()
					};

					let prefixed_top_key = match prefixed_top_key {
						Some(key) => key,
						None => break, // No more work
					};

					// Get the client for this worker
					let client = conn_manager.get(worker_index).await;

					// Fetch all keys for this child trie
					let top_key = PrefixedStorageKey::new(prefixed_top_key.0.clone());
					let prefix = StorageKey(vec![]);
					let page_size = 1000u32;
					let mut child_keys = Vec::new();
					let mut start_key: Option<StorageKey> = None;

					let keys_result: Result<()> = async {
						loop {
							let page = substrate_rpc_client::ChildStateApi::storage_keys_paged(
								client.ws_client.as_ref(),
								top_key.clone(),
								Some(prefix.clone()),
								page_size,
								start_key.clone(),
								Some(at),
							)
							.await
							.map_err(|e| {
								error!(target: LOG_TARGET, "Error = {e:?}");
								"rpc child_get_keys failed"
							})?;

							let is_full_batch = page.len() == page_size as usize;
							if let Some(last) = page.last() {
								start_key = Some(last.clone());
							}
							child_keys.extend(page);

							if !is_full_batch {
								break;
							}
						}
						Ok(())
					}
					.await;

					if let Err(e) = keys_result {
						error!(
							target: LOG_TARGET,
							"Worker {worker_index}: Failed to fetch child keys: {e:?}"
						);
						// Put work back in queue and retry after reconnect
						work_queue.lock().unwrap().push_back(prefixed_top_key);
						sleep(Duration::from_secs(5)).await;
						let _ = conn_manager.recreate_client(worker_index, &client).await;
						continue;
					}

					// Fetch values for all keys
					let child_kv_inner = match Self::rpc_child_get_storage_paged(
						&conn_manager,
						worker_index,
						&prefixed_top_key,
						child_keys,
						at,
					)
					.await
					{
						Ok(kv) => kv,
						Err(e) => {
							error!(
								target: LOG_TARGET,
								"Worker {worker_index}: Failed to fetch child values: {e:?}"
							);
							// Put work back in queue and retry
							work_queue.lock().unwrap().push_back(prefixed_top_key);
							sleep(Duration::from_secs(5)).await;
							continue;
						},
					};

					// Process result
					let prefixed_key = PrefixedStorageKey::new(prefixed_top_key.0.clone());
					let un_prefixed = match ChildType::from_prefixed_key(&prefixed_key) {
						Some((ChildType::ParentKeyId, storage_key)) => storage_key,
						None => {
							error!(target: LOG_TARGET, "invalid key: {prefixed_key:?}");
							continue;
						},
					};

					let info = ChildInfo::new_default(un_prefixed);
					results.lock().unwrap().push((info, child_kv_inner));

					// Log progress
					let done = completed_count.fetch_add(1, Ordering::SeqCst) + 1;
					if done % 100 == 0 || done == total_count {
						info!(
							target: LOG_TARGET,
							"👩‍👦 Child tries progress: {}/{} completed",
							done,
							total_count
						);
					}
				}
			});

			handles.push(handle);
		}

		// Wait for all workers
		futures::future::join_all(handles).await;

		// Extract results and populate pending_ext
		let child_kv_results = Arc::try_unwrap(results)
			.map(|mutex| mutex.into_inner().unwrap())
			.unwrap_or_else(|arc| arc.lock().unwrap().clone());

		let mut child_kv = Vec::new();
		for (info, kv_inner) in child_kv_results {
			let key_values: Vec<(Vec<u8>, Vec<u8>)> =
				kv_inner.iter().cloned().map(|(k, v)| (k.0, v.0)).collect();
			for (k, v) in key_values {
				pending_ext.insert_child(info.clone(), k, v);
			}
			child_kv.push((info, kv_inner));
		}

		info!(
			target: LOG_TARGET,
			"👩‍👦 Completed scraping {} child tries",
			child_kv.len()
		);

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

// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use codec::Encode;
use cumulus_pallet_parachain_system::validate_block::StorageAccessParams;
use log::{debug, info, warn};
use rand::prelude::*;
use sc_cli::{Error, Result};
use sc_client_api::{Backend as ClientBackend, StorageProvider, UsageProvider};
use sp_api::CallApiAt;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_state_machine::{backend::AsTrieBackend, Backend};
use std::{fmt::Debug, sync::Arc, time::Instant};

use super::{cmd::StorageCmd, get_wasm_module, MAX_BATCH_SIZE_FOR_BLOCK_VALIDATION};
use crate::shared::{new_rng, BenchRecord};

impl StorageCmd {
	/// Benchmarks the time it takes to read a single Storage item.
	/// Uses the latest state that is available for the given client.
	pub(crate) fn bench_read<B, BA, C>(
		&self,
		client: Arc<C>,
		_shared_trie_cache: Option<sp_trie::cache::SharedTrieCache<HashingFor<B>>>,
	) -> Result<BenchRecord>
	where
		C: UsageProvider<B> + StorageProvider<B, BA> + CallApiAt<B>,
		B: BlockT + Debug,
		BA: ClientBackend<B>,
		<<B as BlockT>::Header as HeaderT>::Number: From<u32>,
	{
		if self.params.on_block_validation &&
			self.params.batch_size > MAX_BATCH_SIZE_FOR_BLOCK_VALIDATION
		{
			warn!(
				"Batch size is too large. This may cause problems with runtime memory allocation. Better set batch size to {} or less.",
				MAX_BATCH_SIZE_FOR_BLOCK_VALIDATION
			);
		}
		let mut record = BenchRecord::default();
		let best_hash = client.usage_info().chain.best_hash;

		info!("Preparing keys from block {}", best_hash);
		// Load all keys and randomly shuffle them.
		let mut keys: Vec<_> = client.storage_keys(best_hash, None, None)?.collect();
		let (mut rng, _) = new_rng(None);
		keys.shuffle(&mut rng);

		let mut child_nodes = Vec::new();
		// Interesting part here:
		// Read all the keys in the database and measure the time it takes to access each.
		info!("Reading {} keys", keys.len());

		// Read using the same TrieBackend and recorder for up to `batch_size` keys.
		// This would allow us to measure the amortized cost of reading a key.
		let recorder = (!self.params.disable_pov_recorder).then(|| Default::default());
		let mut recorder_clone = recorder.clone();
		let mut state = client
			.state_at(best_hash)
			.map_err(|_err| Error::Input("State not found".into()))?;
		let mut as_trie_backend = state.as_trie_backend();
		let mut backend = sp_state_machine::TrieBackendBuilder::wrap(&as_trie_backend)
			.with_optional_recorder(recorder)
			.build();
		let mut read_in_batch = 0;
		let mut on_validation_batch = vec![];
		let mut on_validation_size = 0;
		let last_key = keys.last().unwrap();
		for key in keys.as_slice() {
			match (self.params.include_child_trees, self.is_child_key(key.clone().0)) {
				(true, Some(info)) => {
					// child tree key
					for ck in client.child_storage_keys(best_hash, info.clone(), None, None)? {
						child_nodes.push((ck, info.clone()));
					}
				},
				_ => {
					// regular key
					on_validation_batch.push((key.0.clone(), None));
					let start = Instant::now();
					let v = backend
						.storage(key.0.as_ref())
						.expect("Checked above to exist")
						.ok_or("Value unexpectedly empty")?;
					on_validation_size += v.len();
					if !self.params.on_block_validation {
						record.append(v.len(), start.elapsed())?;
					}
				},
			}
			read_in_batch += 1;
			let is_batch_full = read_in_batch >= self.params.batch_size || key == last_key;

			// Read keys on block validation
			if is_batch_full && self.params.on_block_validation {
				let root = backend.root();
				let storage_proof = recorder_clone
					.clone()
					.map(|r| r.drain_storage_proof())
					.expect("Storage proof must exist for block validation");
				debug!(
					"POV: len {:?} {:?}",
					storage_proof.len(),
					storage_proof.clone().encoded_compact_size::<HashingFor<B>>(*root)
				);

				debug!("validate_block with {} keys", on_validation_batch.len());
				let wasm_module = get_wasm_module();
				let mut instance = wasm_module.new_instance().unwrap();
				let compact = storage_proof.into_compact_proof::<HashingFor<B>>(*root).unwrap();
				let params =
					StorageAccessParams::<B>::new_read(*root, compact, on_validation_batch.clone());

				// Dry run to get the time it takes without storage access
				let dry_run_encoded = params.as_dry_run().encode();
				let dry_run_start = Instant::now();
				instance.call_export("validate_block", &dry_run_encoded).unwrap();
				let dry_run_elapsed = dry_run_start.elapsed();
				debug!("validate_block dry-run time {:?}", dry_run_elapsed);

				let encoded = params.encode();
				let start = Instant::now();
				instance.call_export("validate_block", &encoded).unwrap();
				let elapsed = start.elapsed();
				debug!("validate_block time {:?}", elapsed);
				record.append(
					on_validation_size / on_validation_batch.len(),
					std::time::Duration::from_nanos(
						(elapsed - dry_run_elapsed).as_nanos() as u64 /
							on_validation_batch.len() as u64,
					),
				)?;

				on_validation_batch = vec![];
				on_validation_size = 0;
			}

			// Reload recorder
			if is_batch_full {
				// Using a new recorder for every read vs using the same for the entire batch
				// produces significant different results. Since in the real use case we use a
				// single recorder per block, simulate the same behavior by creating a new
				// recorder every batch size, so that the amortized cost of reading a key is
				// measured in conditions closer to the real world.
				let recorder = (!self.params.disable_pov_recorder).then(|| Default::default());
				recorder_clone = recorder.clone();
				state = client
					.state_at(best_hash)
					.map_err(|_err| Error::Input("State not found".to_string()))?;
				as_trie_backend = state.as_trie_backend();
				backend = sp_state_machine::TrieBackendBuilder::wrap(&as_trie_backend)
					.with_optional_recorder(recorder)
					.build();
				read_in_batch = 0;
			}
		}

		if self.params.include_child_trees {
			child_nodes.shuffle(&mut rng);

			info!("Reading {} child keys", child_nodes.len());
			let (last_child_key, last_child_info) = child_nodes.last().unwrap();
			for (key, info) in child_nodes.as_slice() {
				on_validation_batch.push((key.0.clone(), Some(info.clone())));
				let start = Instant::now();
				let v = backend
					.child_storage(info, key.0.as_ref())
					.expect("Checked above to exist")
					.ok_or("Value unexpectedly empty")?;
				on_validation_size += v.len();
				if !self.params.on_block_validation {
					record.append(v.len(), start.elapsed())?;
				}
				read_in_batch += 1;
				let is_batch_full = read_in_batch >= self.params.batch_size ||
					(last_child_key == key && last_child_info == info);

				// Read child keys on block validation
				if is_batch_full && self.params.on_block_validation {
					let root = backend.root();
					let storage_proof = recorder_clone
						.clone()
						.map(|r| r.drain_storage_proof())
						.expect("Storage proof must exist for block validation");
					debug!(
						"POV: len {:?} {:?}",
						storage_proof.len(),
						storage_proof.clone().encoded_compact_size::<HashingFor<B>>(*root)
					);

					debug!("validate_block with {} keys", on_validation_batch.len());
					let wasm_module = get_wasm_module();
					let mut instance = wasm_module.new_instance().unwrap();
					let compact = storage_proof.into_compact_proof::<HashingFor<B>>(*root).unwrap();
					let params = StorageAccessParams::<B>::new_read(
						*root,
						compact,
						on_validation_batch.clone(),
					);

					// Dry run to get the time it takes without storage access
					let dry_run_encoded = params.as_dry_run().encode();
					let dry_run_start = Instant::now();
					instance.call_export("validate_block", &dry_run_encoded).unwrap();
					let dry_run_elapsed = dry_run_start.elapsed();
					debug!("validate_block dry-run time {:?}", dry_run_elapsed);

					let encoded = params.encode();
					let start = Instant::now();
					instance.call_export("validate_block", &encoded).unwrap();
					let elapsed = start.elapsed();
					debug!("validate_block time {:?}", elapsed);
					record.append(
						on_validation_size / on_validation_batch.len(),
						std::time::Duration::from_nanos(
							(elapsed - dry_run_elapsed).as_nanos() as u64 /
								on_validation_batch.len() as u64,
						),
					)?;

					on_validation_batch = vec![];
					on_validation_size = 0;
				}

				// Reload recorder
				if is_batch_full {
					// Using a new recorder for every read vs using the same for the entire batch
					// produces significant different results. Since in the real use case we use a
					// single recorder per block, simulate the same behavior by creating a new
					// recorder every batch size, so that the amortized cost of reading a key is
					// measured in conditions closer to the real world.
					let recorder = (!self.params.disable_pov_recorder).then(|| Default::default());
					state = client
						.state_at(best_hash)
						.map_err(|_err| Error::Input("State not found".to_string()))?;
					as_trie_backend = state.as_trie_backend();
					backend = sp_state_machine::TrieBackendBuilder::wrap(&as_trie_backend)
						.with_optional_recorder(recorder)
						.build();
					read_in_batch = 0;
				}
			}
		}

		Ok(record)
	}
}

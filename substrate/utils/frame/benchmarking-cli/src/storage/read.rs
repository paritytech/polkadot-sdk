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

use log::info;
use rand::prelude::*;
use sc_cli::{Error, Result};
use sc_client_api::{Backend as ClientBackend, StorageProvider, UsageProvider};
use sp_api::CallApiAt;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_state_machine::{backend::AsTrieBackend, Backend};
use std::{fmt::Debug, sync::Arc, time::Instant};

use super::cmd::StorageCmd;
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
		let recorder =
			if self.params.enable_pov_recorder { Some(Default::default()) } else { None };
		let mut recorder_clone = recorder.clone();
		let mut state = client
			.state_at(best_hash)
			.map_err(|_err| Error::Input("State not found".to_string()))?;
		let mut as_trie_backend = state.as_trie_backend();
		let mut backend = sp_state_machine::TrieBackendBuilder::wrap(&as_trie_backend)
			.with_optional_recorder(recorder)
			.build();
		let mut read_in_batch = 0;
		let mut on_validation_batch = vec![];
		let mut on_validation_size = 0;
		for key in keys.as_slice() {
			match (self.params.include_child_trees, self.is_child_key(key.clone().0)) {
				(true, Some(info)) => {
					// child tree key
					for ck in client.child_storage_keys(best_hash, info.clone(), None, None)? {
						child_nodes.push((ck.clone(), info.clone()));
					}
				},
				_ => {
					// regular key
					// let start = Instant::now();
					on_validation_batch.push(key.clone());
					let v = backend
						.storage(key.0.as_ref())
						.expect("Checked above to exist")
						.ok_or("Value unexpectedly empty")?;
					on_validation_size += v.len();
					// record.append(v.len(), start.elapsed())?;
				},
			}
			read_in_batch += 1;

			// Read keys on block validation
			if on_validation_batch.len() >= self.params.batch_size {
				let root = backend.root();
				let pov = recorder_clone.clone().map(|r| r.drain_storage_proof());
				info!(
					"POV: len {:?} {:?}",
					pov.as_ref().map(|p| p.len()),
					pov.clone().map(|p| p.encoded_compact_size::<HashingFor<B>>(*root))
				);

				if let Some(storage_proof) = pov {
					use codec::Encode;
					use cumulus_pallet_parachain_system::validate_block::StorageAccessParams;

					info!("validate_block with {} keys", on_validation_batch.len());
					let wasm_module = get_wasm_module();
					let mut instance = wasm_module.new_instance().unwrap();
					let compact = storage_proof.into_compact_proof::<HashingFor<B>>(*root).unwrap();

					let params: StorageAccessParams<B> = StorageAccessParams {
						state_root: *root,
						storage_proof: compact,
						keys: on_validation_batch.clone(),
					};
					let encoded = params.encode();
					let start = Instant::now();
					instance.call_export("validate_block", &encoded).unwrap();
					record.append(
						on_validation_size / on_validation_batch.len(),
						std::time::Duration::from_nanos(
							start.elapsed().as_nanos() as u64 / on_validation_batch.len() as u64,
						),
					)?;
				}
				on_validation_batch = vec![];
				on_validation_size = 0;
			}

			if read_in_batch >= self.params.batch_size {
				// Reset the TrieBackend for the next batch of reads.
				let recorder =
					if self.params.enable_pov_recorder { Some(Default::default()) } else { None };
				recorder_clone = recorder.clone();
				info!("recorder {:} clone {:}", recorder.is_some(), recorder_clone.is_some());
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
		// TODO: implement detecting and reading child keys in the runtime
		if self.params.include_child_trees {
			child_nodes.shuffle(&mut rng);

			info!("Reading {} child keys", child_nodes.len());
			for (key, info) in child_nodes.as_slice() {
				// let start = Instant::now();
				on_validation_batch.push(key.clone());
				let v = backend
					.child_storage(info, key.0.as_ref())
					.expect("Checked above to exist")
					.ok_or("Value unexpectedly empty")?;
				on_validation_size += v.len();
				// record.append(v.len(), start.elapsed())?;
				read_in_batch += 1;

				// Read child keys on block validation
				if on_validation_batch.len() >= self.params.batch_size {
					let root = backend.root();
					let pov = recorder_clone.clone().map(|r| r.drain_storage_proof());
					info!(
						"POV: len {:?} {:?}",
						pov.as_ref().map(|p| p.len()),
						pov.clone().map(|p| p.encoded_compact_size::<HashingFor<B>>(*root))
					);

					if let Some(storage_proof) = pov {
						use codec::Encode;
						use cumulus_pallet_parachain_system::validate_block::StorageAccessParams;

						info!("validate_block with {} keys", on_validation_batch.len());
						let wasm_module = get_wasm_module();
						let mut instance = wasm_module.new_instance().unwrap();
						let compact =
							storage_proof.into_compact_proof::<HashingFor<B>>(*root).unwrap();
						let params: StorageAccessParams<B> = StorageAccessParams {
							state_root: *root,
							storage_proof: compact,
							keys: on_validation_batch.clone(),
						};
						let encoded = params.encode();
						let start = Instant::now();
						instance.call_export("validate_block", &encoded).unwrap();
						record.append(
							on_validation_size / on_validation_batch.len(),
							std::time::Duration::from_nanos(
								start.elapsed().as_nanos() as u64 /
									on_validation_batch.len() as u64,
							),
						)?;
					}
					on_validation_batch = vec![];
					on_validation_size = 0;
				}

				if read_in_batch >= self.params.batch_size {
					// Reset the TrieBackend for the next batch of reads.
					let recorder = if self.params.enable_pov_recorder {
						Some(Default::default())
					} else {
						None
					};
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

		// Read rest of the keys which are less tham a batch size
		if !on_validation_batch.is_empty() {
			let root = backend.root();
			let pov = recorder_clone.clone().map(|r| r.drain_storage_proof());
			info!(
				"POV: len {:?} {:?}",
				pov.as_ref().map(|p| p.len()),
				pov.clone().map(|p| p.encoded_compact_size::<HashingFor<B>>(*root))
			);

			if let Some(storage_proof) = pov {
				use codec::Encode;
				use cumulus_pallet_parachain_system::validate_block::StorageAccessParams;

				info!("validate_block with {} keys", on_validation_batch.len());
				let wasm_module = get_wasm_module();
				let mut instance = wasm_module.new_instance().unwrap();
				let compact = storage_proof.into_compact_proof::<HashingFor<B>>(*root).unwrap();

				let params: StorageAccessParams<B> = StorageAccessParams {
					state_root: *root,
					storage_proof: compact,
					keys: on_validation_batch.clone(),
				};
				let encoded = params.encode();
				let start = Instant::now();
				instance.call_export("validate_block", &encoded).unwrap();
				record.append(
					on_validation_size / on_validation_batch.len(),
					std::time::Duration::from_nanos(
						start.elapsed().as_nanos() as u64 / on_validation_batch.len() as u64,
					),
				)?;
			}
		}

		Ok(record)
	}
}

fn get_wasm_module() -> Box<dyn sc_executor_common::wasm_runtime::WasmModule> {
	let blob = sc_executor_common::runtime_blob::RuntimeBlob::uncompress_if_needed(
		frame_storage_access_test_runtime::WASM_BINARY
			.expect("You need to build the WASM binaries to run the benchmark!"),
	)
	.unwrap();

	let config = sc_executor_wasmtime::Config {
		allow_missing_func_imports: true,
		cache_path: None,
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy: sc_executor::DEFAULT_HEAP_ALLOC_STRATEGY,
			instantiation_strategy: sc_executor::WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: true,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	};
	Box::new(
		sc_executor_wasmtime::create_runtime::<sp_io::SubstrateHostFunctions>(blob, config)
			.expect("Unable to create wasm module."),
	)
}

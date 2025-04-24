// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The actual implementation of the validate block functionality.

use super::{trie_cache, trie_recorder, MemoryOptimizedValidationParams};
use cumulus_primitives_core::{
	relay_chain::Hash as RHash, ParachainBlockData, PersistedValidationData,
};
use cumulus_primitives_parachain_inherent::ParachainInherentData;

use polkadot_parachain_primitives::primitives::{
	HeadData, RelayChainBlockNumber, ValidationResult,
};

use alloc::vec::Vec;
use codec::{Decode, Encode};

use cumulus_primitives_core::relay_chain::vstaging::{UMPSignal, UMP_SEPARATOR};
use frame_support::{
	traits::{ExecuteBlock, ExtrinsicCall, Get, IsSubType},
	BoundedVec,
};
use sp_core::storage::{ChildInfo, StateVersion};
use sp_externalities::{set_and_run_with_externalities, Externalities};
use sp_io::KillStorageResult;
use sp_runtime::traits::{Block as BlockT, ExtrinsicLike, HashingFor, Header as HeaderT};
use sp_state_machine::OverlayedChanges;
use sp_trie::ProofSizeProvider;
use trie_recorder::SizeOnlyRecorderProvider;

type Ext<'a, Block, Backend> = sp_state_machine::Ext<'a, HashingFor<Block>, Backend>;

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

// Recorder instance to be used during this validate_block call.
environmental::environmental!(recorder: trait ProofSizeProvider);

/// Validate the given parachain block.
///
/// This function is doing roughly the following:
///
/// 1. We decode the [`ParachainBlockData`] from the `block_data` in `params`.
///
/// 2. We are doing some security checks like checking that the `parent_head` in `params`
/// is the parent of the block we are going to check. We also ensure that the `set_validation_data`
/// inherent is present in the block and that the validation data matches the values in `params`.
///
/// 3. We construct the sparse in-memory database from the storage proof inside the block data and
/// then ensure that the storage root matches the storage root in the `parent_head`.
///
/// 4. We replace all the storage related host functions with functions inside the wasm blob.
/// This means instead of calling into the host, we will stay inside the wasm execution. This is
/// very important as the relay chain validator hasn't the state required to verify the block. But
/// we have the in-memory database that contains all the values from the state of the parachain
/// that we require to verify the block.
///
/// 5. We are going to run `check_inherents`. This is important to check stuff like the timestamp
/// matching the real world time.
///
/// 6. The last step is to execute the entire block in the machinery we just have setup. Executing
/// the blocks include running all transactions in the block against our in-memory database and
/// ensuring that the final storage root matches the storage root in the header of the block. In the
/// end we return back the [`ValidationResult`] with all the required information for the validator.
#[doc(hidden)]
#[allow(deprecated)]
pub fn validate_block<
	B: BlockT,
	E: ExecuteBlock<B>,
	PSC: crate::Config,
	CI: crate::CheckInherents<B>,
>(
	MemoryOptimizedValidationParams {
		block_data,
		parent_head: parachain_head,
		relay_parent_number,
		relay_parent_storage_root,
	}: MemoryOptimizedValidationParams,
) -> ValidationResult
where
	B::Extrinsic: ExtrinsicCall,
	<B::Extrinsic as ExtrinsicCall>::Call: IsSubType<crate::Call<PSC>>,
{
	let _guard = (
		// Replace storage calls with our own implementations
		sp_io::storage::host_read.replace_implementation(host_storage_read),
		sp_io::storage::host_set.replace_implementation(host_storage_set),
		sp_io::storage::host_get.replace_implementation(host_storage_get),
		sp_io::storage::host_exists.replace_implementation(host_storage_exists),
		sp_io::storage::host_clear.replace_implementation(host_storage_clear),
		sp_io::storage::host_root.replace_implementation(host_storage_root),
		sp_io::storage::host_clear_prefix.replace_implementation(host_storage_clear_prefix),
		sp_io::storage::host_append.replace_implementation(host_storage_append),
		sp_io::storage::host_next_key.replace_implementation(host_storage_next_key),
		sp_io::storage::host_start_transaction
			.replace_implementation(host_storage_start_transaction),
		sp_io::storage::host_rollback_transaction
			.replace_implementation(host_storage_rollback_transaction),
		sp_io::storage::host_commit_transaction
			.replace_implementation(host_storage_commit_transaction),
		sp_io::default_child_storage::host_get
			.replace_implementation(host_default_child_storage_get),
		sp_io::default_child_storage::host_read
			.replace_implementation(host_default_child_storage_read),
		sp_io::default_child_storage::host_set
			.replace_implementation(host_default_child_storage_set),
		sp_io::default_child_storage::host_clear
			.replace_implementation(host_default_child_storage_clear),
		sp_io::default_child_storage::host_storage_kill
			.replace_implementation(host_default_child_storage_storage_kill),
		sp_io::default_child_storage::host_exists
			.replace_implementation(host_default_child_storage_exists),
		sp_io::default_child_storage::host_clear_prefix
			.replace_implementation(host_default_child_storage_clear_prefix),
		sp_io::default_child_storage::host_root
			.replace_implementation(host_default_child_storage_root),
		sp_io::default_child_storage::host_next_key
			.replace_implementation(host_default_child_storage_next_key),
		sp_io::offchain_index::host_set.replace_implementation(host_offchain_index_set),
		sp_io::offchain_index::host_clear.replace_implementation(host_offchain_index_clear),
		cumulus_primitives_proof_size_hostfunction::storage_proof_size::host_storage_proof_size
			.replace_implementation(host_storage_proof_size),
	);

	let block_data = codec::decode_from_bytes::<ParachainBlockData<B>>(block_data)
		.expect("Invalid parachain block data");

	let mut parent_header =
		codec::decode_from_bytes::<B::Header>(parachain_head.clone()).expect("Invalid parent head");

	let (blocks, proof) = block_data.into_inner();

	assert_eq!(
		*blocks
			.first()
			.expect("BlockData should have at least one block")
			.header()
			.parent_hash(),
		parent_header.hash(),
		"Parachain head needs to be the parent of the first block"
	);

	let mut processed_downward_messages = 0;
	let mut upward_messages = BoundedVec::default();
	let mut upward_message_signals = Vec::<Vec<_>>::new();
	let mut horizontal_messages = BoundedVec::default();
	let mut hrmp_watermark = Default::default();
	let mut head_data = None;
	let mut new_validation_code = None;
	let num_blocks = blocks.len();

	// Create the db
	let db = match proof.to_memory_db(Some(parent_header.state_root())) {
		Ok((db, _)) => db,
		Err(_) => panic!("Compact proof decoding failure."),
	};

	core::mem::drop(proof);

	let cache_provider = trie_cache::CacheProvider::new();
	// We use the storage root of the `parent_head` to ensure that it is the correct root.
	// This is already being done above while creating the in-memory db, but let's be paranoid!!
	let backend = sp_state_machine::TrieBackendBuilder::new_with_cache(
		db,
		*parent_header.state_root(),
		cache_provider,
	)
	.build();

	// We use the same recorder when executing all blocks. So, each node only contributes once to
	// the total size of the storage proof. This recorder should only be used for `execute_block`.
	let mut execute_recorder = SizeOnlyRecorderProvider::default();
	// `backend` with the `execute_recorder`. As the `execute_recorder`, this should only be used
	// for `execute_block`.
	let execute_backend = sp_state_machine::TrieBackendBuilder::wrap(&backend)
		.with_recorder(execute_recorder.clone())
		.build();

	// We let all blocks contribute to the same overlay. Data written by a previous block will be
	// directly accessible without going to the db.
	let mut overlay = OverlayedChanges::default();

	for (block_index, block) in blocks.into_iter().enumerate() {
		parent_header = block.header().clone();
		let inherent_data = extract_parachain_inherent_data(&block);

		validate_validation_data(
			&inherent_data.validation_data,
			relay_parent_number,
			relay_parent_storage_root,
			&parachain_head,
		);

		// We don't need the recorder or the overlay in here.
		run_with_externalities_and_recorder::<B, _, _>(
			&backend,
			&mut Default::default(),
			&mut Default::default(),
			|| {
				let relay_chain_proof = crate::RelayChainStateProof::new(
					PSC::SelfParaId::get(),
					inherent_data.validation_data.relay_parent_storage_root,
					inherent_data.relay_chain_state.clone(),
				)
				.expect("Invalid relay chain state proof");

				#[allow(deprecated)]
				let res = CI::check_inherents(&block, &relay_chain_proof);

				if !res.ok() {
					if log::log_enabled!(log::Level::Error) {
						res.into_errors().for_each(|e| {
							log::error!("Checking inherent with identifier `{:?}` failed", e.0)
						});
					}

					panic!("Checking inherents failed");
				}
			},
		);

		run_with_externalities_and_recorder::<B, _, _>(
			&execute_backend,
			// Here is the only place where we want to use the recorder.
			// We want to ensure that we not accidentally read something from the proof, that was
			// not yet read and thus, alter the proof size. Otherwise we end up with mismatches in
			// later blocks.
			&mut execute_recorder,
			&mut overlay,
			|| {
				E::execute_block(block);
			},
		);

		run_with_externalities_and_recorder::<B, _, _>(
			&backend,
			&mut Default::default(),
			// We are only reading here, but need to know what the old block has written. Thus, we
			// are passing here the overlay.
			&mut overlay,
			|| {
				new_validation_code =
					new_validation_code.take().or(crate::NewValidationCode::<PSC>::get());

				let mut found_separator = false;
				crate::UpwardMessages::<PSC>::get()
					.into_iter()
					.filter_map(|m| {
						// Filter out the `UMP_SEPARATOR` and the `UMPSignals`.
						if cfg!(feature = "experimental-ump-signals") {
							if m == UMP_SEPARATOR {
								found_separator = true;
								None
							} else if found_separator {
								if upward_message_signals.iter().all(|s| *s != m) {
									upward_message_signals.push(m);
								}
								None
							} else {
								// No signal or separator
								Some(m)
							}
						} else {
							Some(m)
						}
					})
					.for_each(|m| {
						upward_messages.try_push(m)
							.expect(
								"Number of upward messages should not be greater than `MAX_UPWARD_MESSAGE_NUM`",
							)
					});

				processed_downward_messages += crate::ProcessedDownwardMessages::<PSC>::get();
				horizontal_messages.try_extend(crate::HrmpOutboundMessages::<PSC>::get().into_iter()).expect(
					"Number of horizontal messages should not be greater than `MAX_HORIZONTAL_MESSAGE_NUM`",
				);
				hrmp_watermark = crate::HrmpWatermark::<PSC>::get();

				if block_index + 1 == num_blocks {
					head_data = Some(
						crate::CustomValidationHeadData::<PSC>::get()
							.map_or_else(|| HeadData(parent_header.encode()), HeadData),
					);
				}
			},
		)
	}

	if !upward_message_signals.is_empty() {
		let mut selected_core = None;
		let mut approved_peer = None;

		upward_message_signals.iter().for_each(|s| {
			match UMPSignal::decode(&mut &s[..]).expect("Failed to decode `UMPSignal`") {
				UMPSignal::SelectCore(selector, offset) => match &selected_core {
					Some(selected_core) if *selected_core != (selector, offset) => {
						panic!(
							"All `SelectCore` signals need to select the same core: {selected_core:?} vs {:?}",
							(selector, offset),
						)
					},
					Some(_) => {},
					None => {
						selected_core = Some((selector, offset));
					},
				},
				UMPSignal::ApprovedPeer(new_approved_peer) => match &approved_peer {
					Some(approved_peer) if *approved_peer != new_approved_peer => {
						panic!(
							"All `ApprovedPeer` signals need to select the same peer_id: {new_approved_peer:?} vs {approved_peer:?}",
						)
					},
					Some(_) => {},
					None => {
						approved_peer = Some(new_approved_peer);
					},
				},
			}
		});

		upward_messages
			.try_push(UMP_SEPARATOR)
			.expect("UMPSignals does not fit in UMPMessages");
		upward_messages
			.try_extend(upward_message_signals.into_iter())
			.expect("UMPSignals does not fit in UMPMessages");
	}

	ValidationResult {
		head_data: head_data.expect("HeadData not set"),
		new_validation_code: new_validation_code.map(Into::into),
		upward_messages,
		processed_downward_messages,
		horizontal_messages,
		hrmp_watermark,
	}
}

/// Extract the [`ParachainInherentData`].
fn extract_parachain_inherent_data<B: BlockT, PSC: crate::Config>(
	block: &B,
) -> &ParachainInherentData
where
	B::Extrinsic: ExtrinsicCall,
	<B::Extrinsic as ExtrinsicCall>::Call: IsSubType<crate::Call<PSC>>,
{
	block
		.extrinsics()
		.iter()
		// Inherents are at the front of the block and are unsigned.
		.take_while(|e| e.is_bare())
		.filter_map(|e| e.call().is_sub_type())
		.find_map(|c| match c {
			crate::Call::set_validation_data { data: validation_data } => Some(validation_data),
			_ => None,
		})
		.expect("Could not find `set_validation_data` inherent")
}

/// Validate the given [`PersistedValidationData`] against the [`MemoryOptimizedValidationParams`].
fn validate_validation_data(
	validation_data: &PersistedValidationData,
	relay_parent_number: RelayChainBlockNumber,
	relay_parent_storage_root: RHash,
	parent_head: &[u8],
) {
	assert_eq!(parent_head, validation_data.parent_head.0, "Parent head doesn't match");
	assert_eq!(
		relay_parent_number, validation_data.relay_parent_number,
		"Relay parent number doesn't match",
	);
	assert_eq!(
		relay_parent_storage_root, validation_data.relay_parent_storage_root,
		"Relay parent storage root doesn't match",
	);
}

/// Run the given closure with the externalities and recorder set.
fn run_with_externalities_and_recorder<Block: BlockT, R, F: FnOnce() -> R>(
	backend: &impl sp_state_machine::Backend<HashingFor<Block>>,
	recorder: &mut SizeOnlyRecorderProvider<HashingFor<Block>>,
	overlay: &mut OverlayedChanges<HashingFor<Block>>,
	execute: F,
) -> R {
	let mut ext = Ext::<Block, _>::new(overlay, backend);

	recorder::using(recorder, || set_and_run_with_externalities(&mut ext, || execute()))
}

fn host_storage_read(key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
	match with_externalities(|ext| ext.storage(key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			Some(value.len() as u32)
		},
		None => None,
	}
}

fn host_storage_set(key: &[u8], value: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), Some(value.to_vec())))
}

fn host_storage_get(key: &[u8]) -> Option<bytes::Bytes> {
	with_externalities(|ext| ext.storage(key).map(|value| value.into()))
}

fn host_storage_exists(key: &[u8]) -> bool {
	with_externalities(|ext| ext.exists_storage(key))
}

fn host_storage_clear(key: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), None))
}

fn host_storage_proof_size() -> u64 {
	recorder::with(|rec| rec.estimate_encoded_size()).expect("Recorder is always set; qed") as _
}

fn host_storage_root(version: StateVersion) -> Vec<u8> {
	with_externalities(|ext| ext.storage_root(version))
}

fn host_storage_clear_prefix(prefix: &[u8], limit: Option<u32>) -> KillStorageResult {
	with_externalities(|ext| ext.clear_prefix(prefix, limit, None).into())
}

fn host_storage_append(key: &[u8], value: Vec<u8>) {
	with_externalities(|ext| ext.storage_append(key.to_vec(), value))
}

fn host_storage_next_key(key: &[u8]) -> Option<Vec<u8>> {
	with_externalities(|ext| ext.next_storage_key(key))
}

fn host_storage_start_transaction() {
	with_externalities(|ext| ext.storage_start_transaction())
}

fn host_storage_rollback_transaction() {
	with_externalities(|ext| ext.storage_rollback_transaction().ok())
		.expect("No open transaction that can be rolled back.");
}

fn host_storage_commit_transaction() {
	with_externalities(|ext| ext.storage_commit_transaction().ok())
		.expect("No open transaction that can be committed.");
}

fn host_default_child_storage_get(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.child_storage(&child_info, key))
}

fn host_default_child_storage_read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
) -> Option<u32> {
	let child_info = ChildInfo::new_default(storage_key);
	match with_externalities(|ext| ext.child_storage(&child_info, key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			Some(value.len() as u32)
		},
		None => None,
	}
}

fn host_default_child_storage_set(storage_key: &[u8], key: &[u8], value: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		ext.place_child_storage(&child_info, key.to_vec(), Some(value.to_vec()))
	})
}

fn host_default_child_storage_clear(storage_key: &[u8], key: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.place_child_storage(&child_info, key.to_vec(), None))
}

fn host_default_child_storage_storage_kill(
	storage_key: &[u8],
	limit: Option<u32>,
) -> KillStorageResult {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.kill_child_storage(&child_info, limit, None).into())
}

fn host_default_child_storage_exists(storage_key: &[u8], key: &[u8]) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

fn host_default_child_storage_clear_prefix(
	storage_key: &[u8],
	prefix: &[u8],
	limit: Option<u32>,
) -> KillStorageResult {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.clear_child_prefix(&child_info, prefix, limit, None).into())
}

fn host_default_child_storage_root(storage_key: &[u8], version: StateVersion) -> Vec<u8> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.child_storage_root(&child_info, version))
}

fn host_default_child_storage_next_key(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.next_child_storage_key(&child_info, key))
}

fn host_offchain_index_set(_key: &[u8], _value: &[u8]) {}

fn host_offchain_index_clear(_key: &[u8]) {}

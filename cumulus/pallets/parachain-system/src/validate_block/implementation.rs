// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! The actual implementation of the validate block functionality.

use frame_support::traits::ExecuteBlock;
use sp_runtime::traits::{Block as BlockT, HashFor, Header as HeaderT, NumberFor};

use sp_io::KillChildStorageResult;
use sp_std::prelude::*;

use hash_db::{HashDB, EMPTY_PREFIX};

use polkadot_parachain::primitives::{
	HeadData, ValidationCode, ValidationParams, ValidationResult,
};

use codec::{Decode, Encode};

use cumulus_primitives_core::{
	well_known_keys::{
		HRMP_OUTBOUND_MESSAGES, HRMP_WATERMARK, NEW_VALIDATION_CODE, PROCESSED_DOWNWARD_MESSAGES,
		UPWARD_MESSAGES,
	},
	OutboundHrmpMessage, UpwardMessage,
};
use sp_core::storage::ChildInfo;
use sp_externalities::{set_and_run_with_externalities, Externalities};
use sp_trie::MemoryDB;

type Ext<'a, B> = sp_state_machine::Ext<
	'a,
	HashFor<B>,
	NumberFor<B>,
	sp_state_machine::TrieBackend<MemoryDB<HashFor<B>>, HashFor<B>>,
>;

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

type ParachainSystem<PSC> = crate::Module::<PSC>;

/// Validate a given parachain block on a validator.
#[doc(hidden)]
pub fn validate_block<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>(
	params: ValidationParams,
) -> ValidationResult {
	let block_data =
		cumulus_primitives_core::ParachainBlockData::<B>::decode(&mut &params.block_data.0[..])
			.expect("Invalid parachain block data");

	let parent_head =
		B::Header::decode(&mut &params.parent_head.0[..]).expect("Invalid parent head");

	let (header, extrinsics, storage_proof) = block_data.deconstruct();

	let head_data = HeadData(header.encode());

	let block = B::new(header, extrinsics);
	assert!(
		parent_head.hash() == *block.header().parent_hash(),
		"Invalid parent hash",
	);

	let db = storage_proof.into_memory_db();
	let root = parent_head.state_root().clone();
	if !HashDB::<HashFor<B>, _>::contains(&db, &root, EMPTY_PREFIX) {
		panic!("Witness data does not contain given storage root.");
	}
	let backend = sp_state_machine::TrieBackend::new(db, root);
	let mut overlay = sp_state_machine::OverlayedChanges::default();
	let mut cache = Default::default();
	let mut ext = Ext::<B>::new(&mut overlay, &mut cache, &backend);

	let _guard = (
		// Replace storage calls with our own implementations
		sp_io::storage::host_read.replace_implementation(host_storage_read),
		sp_io::storage::host_set.replace_implementation(host_storage_set),
		sp_io::storage::host_get.replace_implementation(host_storage_get),
		sp_io::storage::host_exists.replace_implementation(host_storage_exists),
		sp_io::storage::host_clear.replace_implementation(host_storage_clear),
		sp_io::storage::host_root.replace_implementation(host_storage_root),
		sp_io::storage::host_clear_prefix.replace_implementation(host_storage_clear_prefix),
		sp_io::storage::host_changes_root.replace_implementation(host_storage_changes_root),
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
	);

	let validation_data = set_and_run_with_externalities(&mut ext, || {
		super::set_and_run_with_validation_params(params, || {
			E::execute_block(block);

			ParachainSystem::<PSC>::validation_data()
				.expect("`PersistedValidationData` should be set in every block!")
		})
	});

	// If in the course of block execution new validation code was set, insert
	// its scheduled upgrade so we can validate that block number later.
	let new_validation_code = overlay
		.storage(NEW_VALIDATION_CODE)
		.flatten()
		.map(|slice| slice.to_vec())
		.map(ValidationCode);

	// Extract potential upward messages from the storage.
	let upward_messages = match overlay.storage(UPWARD_MESSAGES).flatten() {
		Some(encoded) => Vec::<UpwardMessage>::decode(&mut &encoded[..])
			.expect("Upward messages vec is not correctly encoded in the storage!"),
		None => Vec::new(),
	};

	let processed_downward_messages = overlay
		.storage(PROCESSED_DOWNWARD_MESSAGES)
		.flatten()
		.map(|v| {
			Decode::decode(&mut &v[..])
				.expect("Processed downward message count is not correctly encoded in the storage")
		})
		.unwrap_or_default();

	let horizontal_messages = match overlay.storage(HRMP_OUTBOUND_MESSAGES).flatten() {
		Some(encoded) => Vec::<OutboundHrmpMessage>::decode(&mut &encoded[..])
			.expect("Outbound HRMP messages vec is not correctly encoded in the storage!"),
		None => Vec::new(),
	};

	let hrmp_watermark = overlay
		.storage(HRMP_WATERMARK)
		.flatten()
		.map(|v| Decode::decode(&mut &v[..]).expect("HRMP watermark is not encoded correctly"))
		.unwrap_or(validation_data.relay_parent_number);

	ValidationResult {
		head_data,
		new_validation_code,
		upward_messages,
		processed_downward_messages,
		horizontal_messages,
		hrmp_watermark,
	}
}

fn host_storage_read(key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
	match with_externalities(|ext| ext.storage(key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = sp_std::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			Some(value.len() as u32)
		}
		None => None,
	}
}

fn host_storage_set(key: &[u8], value: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), Some(value.to_vec())))
}

fn host_storage_get(key: &[u8]) -> Option<Vec<u8>> {
	with_externalities(|ext| ext.storage(key).clone())
}

fn host_storage_exists(key: &[u8]) -> bool {
	with_externalities(|ext| ext.exists_storage(key))
}

fn host_storage_clear(key: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), None))
}

fn host_storage_root() -> Vec<u8> {
	with_externalities(|ext| ext.storage_root())
}

fn host_storage_clear_prefix(prefix: &[u8]) {
	with_externalities(|ext| ext.clear_prefix(prefix))
}

fn host_storage_changes_root(parent_hash: &[u8]) -> Option<Vec<u8>> {
	with_externalities(|ext| ext.storage_changes_root(parent_hash).ok().flatten())
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
			let written = sp_std::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			Some(value.len() as u32)
		}
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
) -> KillChildStorageResult {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let (all_removed, num_removed) = ext.kill_child_storage(&child_info, limit);
		match all_removed {
			true => KillChildStorageResult::AllRemoved(num_removed),
			false => KillChildStorageResult::SomeRemaining(num_removed),
		}
	})
}

fn host_default_child_storage_exists(storage_key: &[u8], key: &[u8]) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

fn host_default_child_storage_clear_prefix(storage_key: &[u8], prefix: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.clear_child_prefix(&child_info, prefix))
}

fn host_default_child_storage_root(storage_key: &[u8]) -> Vec<u8> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.child_storage_root(&child_info))
}

fn host_default_child_storage_next_key(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.next_child_storage_key(&child_info, key))
}

fn host_offchain_index_set(_key: &[u8], _value: &[u8]) {}

fn host_offchain_index_clear(_key: &[u8]) {}

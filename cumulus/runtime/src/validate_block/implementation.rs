// Copyright 2019 Parity Technologies (UK) Ltd.
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

use frame_executive::ExecuteBlock;
use sp_runtime::traits::{Block as BlockT, HashFor, NumberFor, Header as HeaderT};

use sp_std::{boxed::Box, vec::Vec};

use hash_db::{HashDB, EMPTY_PREFIX};

use parachain::primitives::{HeadData, ValidationCode, ValidationParams, ValidationResult};

use codec::{Decode, Encode};

use cumulus_primitives::{
	well_known_keys::{
		NEW_VALIDATION_CODE, PROCESSED_DOWNWARD_MESSAGES, UPWARD_MESSAGES, VALIDATION_DATA,
	},
	UpwardMessage, ValidationData,
};
use sp_externalities::{set_and_run_with_externalities};
use sp_externalities::{Externalities, ExtensionStore, Error, Extension};
use sp_trie::MemoryDB;
use sp_std::{any::{TypeId, Any}};
use sp_core::storage::{ChildInfo, TrackedStorageKey};

type StorageValue = Vec<u8>;
type StorageKey = Vec<u8>;

type Ext<'a, B: BlockT> = sp_state_machine::Ext<
	'a,
	HashFor<B>,
	NumberFor<B>,
	sp_state_machine::TrieBackend<MemoryDB<HashFor<B>>, HashFor<B>>,
>;

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f)
		.expect("Environmental externalities not set.")
}

/// Implement `Encode` by forwarding the stored raw vec.
struct EncodeOpaqueValue(Vec<u8>);

impl Encode for EncodeOpaqueValue {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		f(&self.0)
	}
}

/// Validate a given parachain block on a validator.
#[doc(hidden)]
pub fn validate_block<B: BlockT, E: ExecuteBlock<B>>(params: ValidationParams) -> ValidationResult {
	let block_data = crate::ParachainBlockData::<B>::decode(&mut &params.block_data.0[..])
		.expect("Invalid parachain block data");

	let parent_head =
		B::Header::decode(&mut &params.parent_head.0[..]).expect("Invalid parent head");

	let head_data = HeadData(block_data.header.encode());

	let block = B::new(block_data.header, block_data.extrinsics);
	assert!(
		parent_head.hash() == *block.header().parent_hash(),
		"Invalid parent hash",
	);

	let db = block_data.storage_proof.into_memory_db();
	let root = parent_head.state_root().clone();
	if !HashDB::<HashFor<B>, _>::contains(&db, &root, EMPTY_PREFIX) {
		panic!("Witness data does not contain given storage root.");
	}
	let backend = sp_state_machine::TrieBackend::new(
		db,
		root,
	);
	let mut overlay = sp_state_machine::OverlayedChanges::default();
	let mut cache = Default::default();
	let mut ext = WitnessExt::<B> {
		inner: Ext::<B>::new(&mut overlay, &mut cache, &backend),
		params: &params,
	};

	let _guard = unsafe {
		(
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
			sp_io::storage::host_start_transaction.replace_implementation(host_storage_start_transaction),
			sp_io::storage::host_rollback_transaction.replace_implementation(
				host_storage_rollback_transaction
			),
			sp_io::storage::host_commit_transaction.replace_implementation(
				host_storage_commit_transaction
			),
			sp_io::default_child_storage::host_get.replace_implementation(host_default_child_storage_get),
			sp_io::default_child_storage::host_read.replace_implementation(host_default_child_storage_read),
			sp_io::default_child_storage::host_set.replace_implementation(host_default_child_storage_set),
			sp_io::default_child_storage::host_clear.replace_implementation(
				host_default_child_storage_clear
			),
			sp_io::default_child_storage::host_storage_kill.replace_implementation(
				host_default_child_storage_storage_kill
			),
			sp_io::default_child_storage::host_exists.replace_implementation(
				host_default_child_storage_exists
			),
			sp_io::default_child_storage::host_clear_prefix.replace_implementation(
				host_default_child_storage_clear_prefix
			),
			sp_io::default_child_storage::host_root.replace_implementation(host_default_child_storage_root),
			sp_io::default_child_storage::host_next_key.replace_implementation(host_default_child_storage_next_key),
		)
	};

	set_and_run_with_externalities(&mut ext, || {
		E::execute_block(block);
	});

	// If in the course of block execution new validation code was set, insert
	// its scheduled upgrade so we can validate that block number later.
	let new_validation_code = overlay.storage(NEW_VALIDATION_CODE).flatten()
		.map(|slice| slice.to_vec())
		.map(ValidationCode);

	// Extract potential upward messages from the storage.
	let upward_messages = match overlay.storage(UPWARD_MESSAGES).flatten() {
		Some(encoded) => Vec::<UpwardMessage>::decode(&mut &encoded[..])
			.expect("Upward messages vec is not correctly encoded in the storage!"),
		None => Vec::new(),
	};

	let processed_downward_messages = overlay.storage(PROCESSED_DOWNWARD_MESSAGES)
		.flatten()
		.map(|v|
			Decode::decode(&mut &v[..])
				.expect("Processed downward message count is not correctly encoded in the storage")
		)
		.unwrap_or_default();

	let validation_data: ValidationData = overlay.storage(VALIDATION_DATA).flatten()
			.and_then(|v| Decode::decode(&mut &v[..]).ok())
			.expect("`ValidationData` is required to be placed into the storage!");

	ValidationResult {
		head_data,
		new_validation_code,
		upward_messages,
		processed_downward_messages,
		//TODO!
		horizontal_messages: Vec::new(),
		//TODO!
		hrmp_watermark: validation_data.persisted.block_number,
	}
}

/// The storage implementation used when validating a block that is using the
/// witness data as source.
struct WitnessExt<'a, B: BlockT> {
	inner: Ext<'a, B>,
	params: &'a ValidationParams,
}

impl<'a, B: BlockT> WitnessExt<'a, B> {
	/// Checks that the encoded `ValidationData` in `data` is correct.
	///
	/// Should be removed with: https://github.com/paritytech/cumulus/issues/217
	/// When removed `WitnessExt` could also be removed.
	fn check_validation_data(&self, mut data: &[u8]) {
		let validation_data = ValidationData::decode(&mut data).expect("Invalid `ValidationData`");

		assert_eq!(
			self.params.parent_head,
			validation_data.persisted.parent_head
		);
		assert_eq!(
			self.params.relay_chain_height,
			validation_data.persisted.block_number
		);
		assert_eq!(
			self.params.hrmp_mqc_heads,
			validation_data.persisted.hrmp_mqc_heads
		);
	}
}

impl<'a, B: BlockT> Externalities for WitnessExt<'a, B> {
	fn storage(&self, key: &[u8]) -> Option<StorageValue> {
		self.inner.storage(key)
	}

	fn set_offchain_storage(&mut self, key: &[u8], value: Option<&[u8]>) {
		self.inner.set_offchain_storage(key, value)
	}

	fn storage_hash(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.inner.storage_hash(key)
	}

	fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Option<StorageValue> {
		self.inner.child_storage(child_info, key)
	}

	fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		self.inner.child_storage_hash(child_info, key)
	}

	fn exists_storage(&self, key: &[u8]) -> bool {
		self.inner.exists_storage(key)
	}

	fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> bool {
		self.inner.exists_child_storage(child_info, key)
	}

	fn next_storage_key(&self, key: &[u8]) -> Option<StorageKey> {
		self.inner.next_storage_key(key)
	}

	fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Option<StorageKey> {
		self.inner.next_child_storage_key(child_info, key)
	}

	fn place_storage(&mut self, key: StorageKey, value: Option<StorageValue>) {
		if let Some(value) = value.as_ref() {
			if key == VALIDATION_DATA {
				self.check_validation_data(value);
			}
		}

		self.inner.place_storage(key, value)
	}

	fn place_child_storage(
		&mut self,
		child_info: &ChildInfo,
		key: StorageKey,
		value: Option<StorageValue>,
	) {
		self.inner.place_child_storage(child_info, key, value)
	}

	fn kill_child_storage(
		&mut self,
		child_info: &ChildInfo,
	) {
		self.inner.kill_child_storage(child_info)
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		self.inner.clear_prefix(prefix)
	}

	fn clear_child_prefix(
		&mut self,
		child_info: &ChildInfo,
		prefix: &[u8],
	) {
		self.inner.clear_child_prefix(child_info, prefix)
	}

	fn storage_append(
		&mut self,
		key: Vec<u8>,
		value: Vec<u8>,
	) {
		self.inner.storage_append(key, value)
	}

	fn chain_id(&self) -> u64 {
		42
	}

	fn storage_root(&mut self) -> Vec<u8> {
		self.inner.storage_root()
	}

	fn child_storage_root(
		&mut self,
		child_info: &ChildInfo,
	) -> Vec<u8> {
		self.inner.child_storage_root(child_info)
	}

	fn storage_changes_root(&mut self, parent_hash: &[u8]) -> Result<Option<Vec<u8>>, ()> {
		self.inner.storage_changes_root(parent_hash)
	}

	fn storage_start_transaction(&mut self) {
		self.inner.storage_start_transaction()
	}

	fn storage_rollback_transaction(&mut self) -> Result<(), ()> {
		self.inner.storage_rollback_transaction()
	}

	fn storage_commit_transaction(&mut self) -> Result<(), ()> {
		self.inner.storage_commit_transaction()
	}

	fn wipe(&mut self) {
		self.inner.wipe()
	}

	fn commit(&mut self) {
		self.inner.commit()
	}

	fn read_write_count(&self) -> (u32, u32, u32, u32) {
		self.inner.read_write_count()
	}

	fn reset_read_write_count(&mut self) {
		self.inner.reset_read_write_count()
	}

	fn get_whitelist(&self) -> Vec<TrackedStorageKey> {
		self.inner.get_whitelist()
	}

	fn set_whitelist(&mut self, new: Vec<TrackedStorageKey>) {
		self.inner.set_whitelist(new)
	}
}

impl<'a, B: BlockT> ExtensionStore for WitnessExt<'a, B> {
	fn extension_by_type_id(&mut self, type_id: TypeId) -> Option<&mut dyn Any> {
		self.inner.extension_by_type_id(type_id)
	}

	fn register_extension_with_type_id(
		&mut self,
		type_id: TypeId,
		extension: Box<dyn Extension>,
	) -> Result<(), Error> {
		self.inner.register_extension_with_type_id(type_id, extension)
	}

	fn deregister_extension_by_type_id(
		&mut self,
		type_id: TypeId,
	) -> Result<(), Error> {
		self.inner.deregister_extension_by_type_id(type_id)
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

fn host_default_child_storage_set(
	storage_key: &[u8],
	key: &[u8],
	value: &[u8],
) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.place_child_storage(&child_info, key.to_vec(), Some(value.to_vec())))
}

fn host_default_child_storage_clear(
	storage_key: &[u8],
	key: &[u8],
) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.place_child_storage(&child_info, key.to_vec(), None))
}

fn host_default_child_storage_storage_kill(
	storage_key: &[u8],
) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.kill_child_storage(&child_info))
}

fn host_default_child_storage_exists(
	storage_key: &[u8],
	key: &[u8],
) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

fn host_default_child_storage_clear_prefix(
	storage_key: &[u8],
	prefix: &[u8],
) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.clear_child_prefix(&child_info, prefix))
}

fn host_default_child_storage_root(
	storage_key: &[u8],
) -> Vec<u8> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.child_storage_root(&child_info))
}

fn host_default_child_storage_next_key(
	storage_key: &[u8],
	key: &[u8],
) -> Option<Vec<u8>> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.next_child_storage_key(&child_info, key))
}

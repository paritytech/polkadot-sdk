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

use crate::WitnessData;
use frame_executive::ExecuteBlock;
use sp_runtime::traits::{Block as BlockT, HashFor, Header as HeaderT};

use sp_std::{boxed::Box, vec::Vec};
use sp_trie::{delta_trie_root, read_trie_value, Layout, MemoryDB};

use hash_db::{HashDB, EMPTY_PREFIX};

use trie_db::{TrieDB, TrieDBIterator};

use parachain::primitives::{HeadData, ValidationCode, ValidationParams, ValidationResult};

use codec::{Decode, Encode, EncodeAppend};

use cumulus_primitives::{
	validation_function_params::ValidationFunctionParams,
	well_known_keys::{
		NEW_VALIDATION_CODE, PROCESSED_DOWNWARD_MESSAGES, UPWARD_MESSAGES,
		VALIDATION_FUNCTION_PARAMS,
	},
	GenericUpwardMessage,
};

/// Stores the global [`Storage`] instance.
///
/// As wasm is always executed with one thread, this global varibale is safe!
static mut STORAGE: Option<Box<dyn Storage>> = None;

/// Runs the given `call` with the global storage and returns the result of the call.
///
/// # Panic
///
/// Panics if the [`STORAGE`] is not initialized.
fn with_storage<R>(call: impl FnOnce(&mut dyn Storage) -> R) -> R {
	let mut storage = unsafe {
		STORAGE.take().expect("`STORAGE` needs to be set before calling this function.")
	};

	let res = call(&mut *storage);

	unsafe {
		STORAGE = Some(storage);
	}

	res
}

/// Abstract the storage into a trait without `Block` generic.
trait Storage {
	/// Retrieve the value for the given key.
	fn get(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Insert the given key and value.
	fn insert(&mut self, key: &[u8], value: &[u8]);

	/// Remove key and value.
	fn remove(&mut self, key: &[u8]);

	/// Calculate the storage root.
	///
	/// Returns the SCALE encoded hash.
	fn storage_root(&mut self) -> Vec<u8>;

	/// Clear all keys that start with the given prefix.
	fn clear_prefix(&mut self, prefix: &[u8]);

	/// Append the value to the given key
	fn storage_append(&mut self, key: &[u8], value: Vec<u8>);
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
		"Invalid parent hash"
	);

	// make a copy for later use
	let validation_function_params = (&params).into();

	let storage_inner = WitnessStorage::<B>::new(
		block_data.witness_data,
		parent_head.state_root().clone(),
		validation_function_params,
	)
	.expect("Witness data and storage root always match; qed");

	let _guard = unsafe {
		STORAGE = Some(Box::new(storage_inner));
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
		)
	};

	E::execute_block(block);

	// If in the course of block execution new validation code was set, insert
	// its scheduled upgrade so we can validate that block number later.
	let new_validation_code = with_storage(|storage| storage.get(NEW_VALIDATION_CODE)).map(ValidationCode);
	if new_validation_code.is_some() && validation_function_params.code_upgrade_allowed.is_none() {
		panic!("Attempt to upgrade validation function when not permitted!");
	}

	// Extract potential upward messages from the storage.
	let upward_messages = match with_storage(|storage| storage.get(UPWARD_MESSAGES)) {
		Some(encoded) => Vec::<GenericUpwardMessage>::decode(&mut &encoded[..])
			.expect("Upward messages vec is not correctly encoded in the storage!"),
		None => Vec::new(),
	};

	let processed_downward_messages = with_storage(|storage| storage.get(PROCESSED_DOWNWARD_MESSAGES))
		.and_then(|v| Decode::decode(&mut &v[..]).ok())
		.unwrap_or_default();

	ValidationResult {
		head_data,
		new_validation_code,
		upward_messages,
		processed_downward_messages,
	}
}

/// The storage implementation used when validating a block that is using the
/// witness data as source.
struct WitnessStorage<B: BlockT> {
	witness_data: MemoryDB<HashFor<B>>,
	overlay: hashbrown::HashMap<Vec<u8>, Option<Vec<u8>>>,
	storage_root: B::Hash,
	params: ValidationFunctionParams,
}

impl<B: BlockT> WitnessStorage<B> {
	/// Initialize from the given witness data and storage root.
	///
	/// Returns an error if given storage root was not found in the witness data.
	fn new(
		data: WitnessData,
		storage_root: B::Hash,
		params: ValidationFunctionParams,
	) -> Result<Self, &'static str> {
		let mut db = MemoryDB::default();
		data.into_iter().for_each(|i| {
			db.insert(EMPTY_PREFIX, &i);
		});

		if !HashDB::contains(&db, &storage_root, EMPTY_PREFIX) {
			return Err("Witness data does not contain given storage root.");
		}

		Ok(Self {
			witness_data: db,
			overlay: Default::default(),
			storage_root,
			params,
		})
	}
}

impl<B: BlockT> Storage for WitnessStorage<B> {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		match key {
			VALIDATION_FUNCTION_PARAMS => Some(self.params.encode()),
			key => self
				.overlay
				.get(key)
				.cloned()
				.or_else(|| {
					read_trie_value::<Layout<HashFor<B>>, _>(
						&self.witness_data,
						&self.storage_root,
						key,
					)
					.ok()
				})
				.unwrap_or(None),
		}
	}

	fn insert(&mut self, key: &[u8], value: &[u8]) {
		self.overlay.insert(key.to_vec(), Some(value.to_vec()));
	}

	fn remove(&mut self, key: &[u8]) {
		self.overlay.insert(key.to_vec(), None);
	}

	fn storage_root(&mut self) -> Vec<u8> {
		let root = delta_trie_root::<Layout<HashFor<B>>, _, _, _, _, _>(
			&mut self.witness_data,
			self.storage_root.clone(),
			self.overlay
				.iter()
				.map(|(k, v)| (k.as_ref(), v.as_ref().map(|v| v.as_ref()))),
		)
		.expect("Calculates storage root");

		root.encode()
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		self.overlay.iter_mut().for_each(|(k, v)| {
			if k.starts_with(prefix) {
				*v = None;
			}
		});

		let trie = match TrieDB::<Layout<HashFor<B>>>::new(&self.witness_data, &self.storage_root) {
			Ok(r) => r,
			Err(_) => panic!(),
		};

		for x in TrieDBIterator::new_prefixed(&trie, prefix).expect("Creates trie iterator") {
			let (key, _) = x.expect("Iterating trie iterator");
			self.overlay.insert(key, None);
		}
	}

	fn storage_append(&mut self, key: &[u8], value: Vec<u8>) {
		let value_vec = sp_std::vec![EncodeOpaqueValue(value)];
		let current_value = self.overlay.entry(key.to_vec()).or_default();

		let item = current_value.take().unwrap_or_default();
		*current_value = Some(
			match Vec::<EncodeOpaqueValue>::append_or_new(item, &value_vec) {
				Ok(item) => item,
				Err(_) => value_vec.encode(),
			},
		);
	}
}

fn host_storage_read(key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
	match with_storage(|storage| storage.get(key)) {
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
	with_storage(|storage| storage.insert(key, value));
}

fn host_storage_get(key: &[u8]) -> Option<Vec<u8>> {
	with_storage(|storage| storage.get(key).clone())
}

fn host_storage_exists(key: &[u8]) -> bool {
	with_storage(|storage| storage.get(key).is_some())
}

fn host_storage_clear(key: &[u8]) {
	with_storage(|storage| storage.remove(key));
}

fn host_storage_root() -> Vec<u8> {
	with_storage(|storage| storage.storage_root())
}

fn host_storage_clear_prefix(prefix: &[u8]) {
	with_storage(|storage| storage.clear_prefix(prefix))
}

fn host_storage_changes_root(_: &[u8]) -> Option<Vec<u8>> {
	// TODO implement it properly
	None
}

fn host_storage_append(key: &[u8], value: Vec<u8>) {
	with_storage(|storage| storage.storage_append(key, value));
}

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
use sp_runtime::traits::{Block as BlockT, HashFor, Header as HeaderT};

use sp_std::{boxed::Box, collections::btree_map::BTreeMap, ops::Bound, vec::Vec};
use sp_trie::{delta_trie_root, read_trie_value, Layout, MemoryDB, StorageProof};

use hash_db::{HashDB, EMPTY_PREFIX};

use trie_db::{Trie, TrieDB, TrieDBIterator};

use parachain::primitives::{HeadData, ValidationCode, ValidationParams, ValidationResult};

use codec::{Decode, Encode, EncodeAppend};

use cumulus_primitives::{
	well_known_keys::{
		NEW_VALIDATION_CODE, PROCESSED_DOWNWARD_MESSAGES, UPWARD_MESSAGES, VALIDATION_DATA,
	},
	GenericUpwardMessage, ValidationData,
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
		STORAGE
			.take()
			.expect("`STORAGE` needs to be set before calling this function.")
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

	/// Retrieve the value for the given key only if modified.
	fn modified(&self, key: &[u8]) -> Option<Vec<u8>>;

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

	/// Get the next storage key after the given `key`.
	fn next_key(&self, key: &[u8]) -> Option<Vec<u8>>;
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

	let storage_inner = WitnessStorage::<B>::new(
		block_data.storage_proof,
		parent_head.state_root().clone(),
		params,
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
			sp_io::storage::host_next_key.replace_implementation(host_storage_next_key),
		)
	};

	E::execute_block(block);

	// If in the course of block execution new validation code was set, insert
	// its scheduled upgrade so we can validate that block number later.
	let new_validation_code =
		with_storage(|storage| storage.modified(NEW_VALIDATION_CODE)).map(ValidationCode);

	// Extract potential upward messages from the storage.
	let upward_messages = match with_storage(|storage| storage.modified(UPWARD_MESSAGES)) {
		Some(encoded) => Vec::<GenericUpwardMessage>::decode(&mut &encoded[..])
			.expect("Upward messages vec is not correctly encoded in the storage!"),
		None => Vec::new(),
	};

	let processed_downward_messages =
		with_storage(|storage| storage.modified(PROCESSED_DOWNWARD_MESSAGES))
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
	overlay: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
	storage_root: B::Hash,
	validation_params: ValidationParams,
}

impl<B: BlockT> WitnessStorage<B> {
	/// Initialize from the given witness data and storage root.
	///
	/// Returns an error if given storage root was not found in the witness data.
	fn new(
		storage_proof: StorageProof,
		storage_root: B::Hash,
		validation_params: ValidationParams,
	) -> Result<Self, &'static str> {
		let db = storage_proof.into_memory_db();

		if !HashDB::contains(&db, &storage_root, EMPTY_PREFIX) {
			return Err("Witness data does not contain given storage root.");
		}

		Ok(Self {
			witness_data: db,
			overlay: Default::default(),
			storage_root,
			validation_params,
		})
	}

	/// Find the next storage key after the given `key` in the trie.
	fn trie_next_key(&self, key: &[u8]) -> Option<Vec<u8>> {
		let trie = TrieDB::<Layout<HashFor<B>>>::new(&self.witness_data, &self.storage_root)
			.expect("Creates next storage key `TrieDB`");
		let mut iter = trie.iter().expect("Creates trie iterator");

		// The key just after the one given in input, basically `key++0`.
		// Note: We are sure this is the next key if:
		// * size of key has no limit (i.e. we can always add 0 to the path),
		// * and no keys can be inserted between `key` and `key++0` (this is ensured by sp-io).
		let mut potential_next_key = Vec::with_capacity(key.len() + 1);
		potential_next_key.extend_from_slice(key);
		potential_next_key.push(0);

		iter.seek(&potential_next_key).expect("Seek trie iterator");

		let next_element = iter.next();

		if let Some(next_element) = next_element {
			let (next_key, _) = next_element.expect("Extracts next key");
			Some(next_key)
		} else {
			None
		}
	}

	/// Find the next storage key after the given `key` in the overlay.
	fn overlay_next_key(&self, key: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
		let range = (Bound::Excluded(key), Bound::Unbounded);
		self.overlay
			.range::<[u8], _>(range)
			.next()
			.map(|(k, v)| (&k[..], v.as_deref()))
	}

	/// Checks that the encoded `ValidationData` in `data` is correct.
	///
	/// Should be removed with: https://github.com/paritytech/cumulus/issues/217
	fn check_validation_data(&self, mut data: &[u8]) {
		let validation_data = ValidationData::decode(&mut data).expect("Invalid `ValidationData`");

		assert_eq!(
			self.validation_params.parent_head,
			validation_data.persisted.parent_head
		);
		assert_eq!(
			self.validation_params.relay_chain_height,
			validation_data.persisted.block_number
		);
		assert_eq!(
			self.validation_params.hrmp_mqc_heads,
			validation_data.persisted.hrmp_mqc_heads
		);
	}
}

impl<B: BlockT> Storage for WitnessStorage<B> {
	fn modified(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.overlay.get(key).cloned().unwrap_or(None)
	}

	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.overlay.get(key).cloned().unwrap_or_else(|| {
			read_trie_value::<Layout<HashFor<B>>, _>(&self.witness_data, &self.storage_root, key)
				.expect("Reading key from trie.")
		})
	}

	fn insert(&mut self, key: &[u8], value: &[u8]) {
		if key == VALIDATION_DATA {
			self.check_validation_data(value);
		}

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
			if let Ok((key, _)) = x {
				self.overlay.insert(key, None);
			}
		}
	}

	fn storage_append(&mut self, key: &[u8], value: Vec<u8>) {
		let value_vec = sp_std::vec![EncodeOpaqueValue(value)];

		let overlay = &mut self.overlay;
		let witness_data = &self.witness_data;
		let storage_root = &self.storage_root;

		let current_value = overlay.entry(key.to_vec()).or_insert_with(|| {
			read_trie_value::<Layout<HashFor<B>>, _>(witness_data, storage_root, key)
				.ok()
				.flatten()
		});

		let item = current_value.take().unwrap_or_default();
		*current_value = Some(
			match Vec::<EncodeOpaqueValue>::append_or_new(item, &value_vec) {
				Ok(item) => item,
				Err(_) => value_vec.encode(),
			},
		);
	}

	fn next_key(&self, key: &[u8]) -> Option<Vec<u8>> {
		let next_trie_key = self.trie_next_key(key);
		let next_overlay_key = self.overlay_next_key(key);

		match (next_trie_key, next_overlay_key) {
			(Some(backend_key), Some(overlay_key)) if &backend_key[..] < overlay_key.0 => {
				Some(backend_key)
			}
			(backend_key, None) => backend_key,
			(_, Some(overlay_key)) => {
				if overlay_key.1.is_some() {
					Some(overlay_key.0.to_vec())
				} else {
					self.next_key(&overlay_key.0)
				}
			}
		}
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

fn host_storage_next_key(key: &[u8]) -> Option<Vec<u8>> {
	with_storage(|storage| storage.next_key(key))
}

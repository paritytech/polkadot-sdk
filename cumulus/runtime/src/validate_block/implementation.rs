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
use sp_runtime::traits::{Block as BlockT, HasherFor, Header as HeaderT};

use sp_trie::{delta_trie_root, read_trie_value, Layout, MemoryDB};

use sp_std::{boxed::Box, vec::Vec};

use hash_db::{HashDB, EMPTY_PREFIX};

use trie_db::{Trie, TrieDB};

use parachain::{ValidationParams, ValidationResult};

use codec::{Decode, Encode};

/// Stores the global [`Storage`] instance.
///
/// As wasm is always executed with one thread, this global varibale is safe!
static mut STORAGE: Option<Box<dyn Storage>> = None;

/// Returns a mutable reference to the [`Storage`] implementation.
///
/// # Panic
///
/// Panics if the [`STORAGE`] is not initialized.
fn storage() -> &'static mut dyn Storage {
	unsafe {
		&mut **STORAGE
			.as_mut()
			.expect("`STORAGE` needs to be set before calling this function.")
	}
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
}

/// Validate a given parachain block on a validator.
#[doc(hidden)]
pub fn validate_block<B: BlockT, E: ExecuteBlock<B>>(params: ValidationParams) -> ValidationResult {
	let block_data = crate::ParachainBlockData::<B>::decode(&mut &params.block_data[..])
		.expect("Invalid parachain block data");

	let parent_head = B::Header::decode(&mut &params.parent_head[..]).expect("Invalid parent head");
	// TODO: Use correct head data
	let head_data = block_data.header.encode();

	// TODO: Add `PolkadotInherent`.
	let block = B::new(block_data.header, block_data.extrinsics);
	assert!(
		parent_head.hash() == *block.header().parent_hash(),
		"Invalid parent hash"
	);

	let storage = WitnessStorage::<B>::new(
		block_data.witness_data,
		block_data.witness_data_storage_root,
	)
	.expect("Witness data and storage root always match; qed");

	let _guard = unsafe {
		STORAGE = Some(Box::new(storage));
		(
			// Replace storage calls with our own implementations
			sp_io::storage::host_read.replace_implementation(host_storage_read),
			sp_io::storage::host_set.replace_implementation(host_storage_set),
			sp_io::storage::host_get.replace_implementation(host_storage_get),
			sp_io::storage::host_exists.replace_implementation(host_storage_exists),
			sp_io::storage::host_clear.replace_implementation(host_storage_clear),
			sp_io::storage::host_root.replace_implementation(host_storage_root),
			sp_io::storage::host_clear_prefix.replace_implementation(host_clear_prefix),
		)
	};

	E::execute_block(block);

	ValidationResult { head_data }
}

/// The storage implementation used when validating a block that is using the
/// witness data as source.
struct WitnessStorage<B: BlockT> {
	witness_data: MemoryDB<HasherFor<B>>,
	overlay: hashbrown::HashMap<Vec<u8>, Option<Vec<u8>>>,
	storage_root: B::Hash,
}

impl<B: BlockT> WitnessStorage<B> {
	/// Initialize from the given witness data and storage root.
	///
	/// Returns an error if given storage root was not found in the witness data.
	fn new(data: WitnessData, storage_root: B::Hash) -> Result<Self, &'static str> {
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
		})
	}
}

/// TODO: `TrieError` should implement `Debug` on `no_std`
fn unwrap_trie_error<R, T, E>(result: Result<R, Box<trie_db::TrieError<T, E>>>) -> R {
	match result {
		Ok(r) => r,
		Err(error) => match *error {
			trie_db::TrieError::InvalidStateRoot(_) => panic!("trie_db: Invalid state root"),
			trie_db::TrieError::IncompleteDatabase(_) => panic!("trie_db: IncompleteDatabase"),
			trie_db::TrieError::DecoderError(_, _) => panic!("trie_db: DecodeError"),
			trie_db::TrieError::InvalidHash(_, _) => panic!("trie_db: InvalidHash"),
			trie_db::TrieError::ValueAtIncompleteKey(_, _) => {
				panic!("trie_db: ValueAtIncompleteKey")
			}
		},
	}
}

impl<B: BlockT> Storage for WitnessStorage<B> {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.overlay
			.get(key)
			.cloned()
			.or_else(|| {
				read_trie_value::<Layout<HasherFor<B>>, _>(
					&self.witness_data,
					&self.storage_root,
					key,
				)
				.ok()
			})
			.unwrap_or(None)
	}

	fn insert(&mut self, key: &[u8], value: &[u8]) {
		self.overlay.insert(key.to_vec(), Some(value.to_vec()));
	}

	fn remove(&mut self, key: &[u8]) {
		self.overlay.insert(key.to_vec(), None);
	}

	fn storage_root(&mut self) -> Vec<u8> {
		let root = unwrap_trie_error(delta_trie_root::<Layout<HasherFor<B>>, _, _, _, _>(
			&mut self.witness_data,
			self.storage_root.clone(),
			self.overlay.drain(),
		));

		root.encode()
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		self.overlay.iter_mut().for_each(|(k, v)| {
			if k.starts_with(prefix) {
				*v = None;
			}
		});

		let trie = match TrieDB::<Layout<HasherFor<B>>>::new(&self.witness_data, &self.storage_root)
		{
			Ok(r) => r,
			Err(_) => panic!(),
		};

		let mut iter = unwrap_trie_error(trie.iter());
		unwrap_trie_error(iter.seek(prefix));

		for x in iter {
			let (key, _) = unwrap_trie_error(x);

			if !key.starts_with(prefix) {
				break;
			}

			self.overlay.insert(key, None);
		}
	}
}

fn host_storage_read(key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
	match storage().get(key) {
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
	storage().insert(key, value);
}

fn host_storage_get(key: &[u8]) -> Option<Vec<u8>> {
	storage().get(key).clone()
}

fn host_storage_exists(key: &[u8]) -> bool {
	storage().get(key).is_some()
}

fn host_storage_clear(key: &[u8]) {
	storage().remove(key);
}

fn host_storage_root() -> Vec<u8> {
	storage().storage_root()
}

fn host_clear_prefix(prefix: &[u8]) {
	storage().clear_prefix(prefix)
}

// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Logic for working with storage proofs.

#[cfg(feature = "test-helpers")]
use sp_trie::{recorder_ext::RecorderExt, Recorder, TrieDBBuilder, TrieError, TrieHash};
#[cfg(feature = "test-helpers")]
use trie_db::{Trie, TrieConfiguration, TrieDBMut};

#[cfg(feature = "std")]
pub use frame_proofs_primitives::proving::craft_valid_storage_proof;
pub use frame_proofs_primitives::proving::{
	raw_storage_proof_size, RawStorageProof, StorageProofChecker, StorageProofError,
};

/// Storage values size requirements.
///
/// This is currently used by benchmarks when generating storage proofs.
#[cfg(feature = "test-helpers")]
#[derive(Clone, Copy, Debug, Default)]
pub struct UnverifiedStorageProofParams {
	/// Expected storage proof size in bytes.
	pub db_size: Option<u32>,
}

#[cfg(feature = "test-helpers")]
impl UnverifiedStorageProofParams {
	/// Make storage proof parameters that require proof of at least `db_size` bytes.
	pub fn from_db_size(db_size: u32) -> Self {
		Self { db_size: Some(db_size) }
	}
}

/// Add extra data to the storage value so that it'll be of given size.
#[cfg(feature = "test-helpers")]
pub fn grow_storage_value(mut value: Vec<u8>, params: &UnverifiedStorageProofParams) -> Vec<u8> {
	if let Some(db_size) = params.db_size {
		if db_size as usize > value.len() {
			value.extend(sp_std::iter::repeat(42u8).take(db_size as usize - value.len()));
		}
	}
	value
}

/// Insert values in the provided trie at common-prefix keys in order to inflate the resulting
/// storage proof.
///
/// This function can add at most 15 common-prefix keys per prefix nibble (4 bits).
/// Each such key adds about 33 bytes (a node) to the proof.
#[cfg(feature = "test-helpers")]
pub fn grow_storage_proof<L: TrieConfiguration>(
	trie: &mut TrieDBMut<L>,
	prefix: Vec<u8>,
	num_extra_nodes: usize,
) {
	use sp_trie::TrieMut;

	let mut added_nodes = 0;
	for i in 0..prefix.len() {
		let mut prefix = prefix[0..=i].to_vec();
		// 1 byte has 2 nibbles (4 bits each)
		let first_nibble = (prefix[i] & 0xf0) >> 4;
		let second_nibble = prefix[i] & 0x0f;

		// create branches at the 1st nibble
		for branch in 1..=15 {
			if added_nodes >= num_extra_nodes {
				return;
			}

			// create branches at the 1st nibble
			prefix[i] = (first_nibble.wrapping_add(branch) % 16) << 4;
			trie.insert(&prefix, &[0; 32])
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			added_nodes += 1;
		}

		// create branches at the 2nd nibble
		for branch in 1..=15 {
			if added_nodes >= num_extra_nodes {
				return;
			}

			prefix[i] = (first_nibble << 4) | (second_nibble.wrapping_add(branch) % 16);
			trie.insert(&prefix, &[0; 32])
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			added_nodes += 1;
		}
	}

	assert_eq!(added_nodes, num_extra_nodes)
}

/// Record all keys for a given root.
#[cfg(feature = "test-helpers")]
pub fn record_all_keys<L: TrieConfiguration, DB>(
	db: &DB,
	root: &TrieHash<L>,
) -> Result<RawStorageProof, sp_std::boxed::Box<TrieError<L>>>
where
	DB: hash_db::HashDBRef<L::Hash, trie_db::DBValue>,
{
	let mut recorder = Recorder::<L>::new();
	let trie = TrieDBBuilder::<L>::new(db, root).with_recorder(&mut recorder).build();
	for x in trie.iter()? {
		let (key, _) = x?;
		trie.get(&key)?;
	}

	Ok(recorder.into_raw_storage_proof())
}

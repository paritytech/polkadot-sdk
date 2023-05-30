// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

//! Logic for working with more efficient storage proofs.

use frame_support::PalletError;
use sp_core::{storage::TrackedStorageKey, RuntimeDebug};
use sp_runtime::{SaturatedConversion, StateVersion};
use sp_std::{default::Default, vec, vec::Vec};
use sp_trie::{
	generate_trie_proof, verify_trie_proof, LayoutV0, LayoutV1, StorageProof, TrieDBBuilder,
	TrieHash,
};

use codec::{Decode, Encode};
use hash_db::Hasher;
use scale_info::TypeInfo;
use trie_db::{DBValue, Trie};

use crate::{storage_proof::RawStorageProof, Size};

pub type RawStorageKey = Vec<u8>;

/// Errors that can occur when interacting with `UntrustedVecDb` and `TrustedVecDb`.
#[derive(Clone, Encode, Decode, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo)]
pub enum VecDbError {
	/// Call to `generate_trie_proof()` failed.
	UnableToGenerateTrieProof,
	/// Call to `verify_trie_proof()` failed.
	InvalidProof,
	/// The `Vec` entries weren't sorted as expected.
	UnsortedEntries,
	/// The provided key wasn't found.
	UnavailableKey,
	/// The value associated to the provided key is `None`.
	EmptyVal,
	/// Error decoding value associated to a provided key.
	DecodeError,
	/// At least one key in the `VecDb` wasn't read.
	UnusedKey,
}

/// Structure representing a key-value database stored as a sorted `Vec` of tuples.
///
/// The structure also contains a proof of the fact that the key-value tuples are actually present
/// in the chain storage.
#[derive(Clone, Default, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub struct UntrustedVecDb {
	proof: RawStorageProof,
	db: Vec<(RawStorageKey, Option<DBValue>)>,
}

impl UntrustedVecDb {
	/// Creates a new instance of `UntrustedVecDb`.
	pub fn try_new<H: Hasher>(
		read_proof: StorageProof,
		root: TrieHash<LayoutV1<H>>,
		mut keys: Vec<impl AsRef<[u8]> + Ord>,
	) -> Result<Self, VecDbError> {
		// It's ok to use `LayoutV1` in this function, no matter the actual underlying layout,
		// because we only perform read operations. When reading `LayoutV0` and `LayoutV1` lead to
		// the same result.
		let mem_db = read_proof.into_memory_db();
		let trie_db = TrieDBBuilder::<LayoutV1<H>>::new(&mem_db, &root).build();

		let trie_proof = generate_trie_proof::<LayoutV1<H>, _, _, _>(&mem_db, root, &keys)
			.map_err(|_| VecDbError::UnableToGenerateTrieProof)?;

		let mut entries = Vec::with_capacity(keys.len());
		keys.sort();
		for key in keys {
			let val = trie_db.get(key.as_ref()).map_err(|_| VecDbError::UnavailableKey)?;
			entries.push((key.as_ref().to_vec(), val));
		}

		Ok(Self { proof: trie_proof, db: entries })
	}

	/// Validates the contained `db` against the contained proof. If the `db` is valid, converts it
	/// into a `TrustedVecDb`.
	pub fn verify<H: Hasher>(
		mut self,
		state_version: StateVersion,
		state_root: &TrieHash<LayoutV1<H>>,
	) -> Result<TrustedVecDb, VecDbError> {
		// First we verify the proof for the `UntrustedVecDb`.
		// Note that `verify_trie_proof()` also checks for duplicate keys and unused nodes.
		match state_version {
			StateVersion::V0 =>
				verify_trie_proof::<LayoutV0<H>, _, _, _>(state_root, &self.proof, &self.db),
			StateVersion::V1 =>
				verify_trie_proof::<LayoutV1<H>, _, _, _>(state_root, &self.proof, &self.db),
		}
		.map_err(|_| VecDbError::InvalidProof)?;

		// Fill the `TrustedVecDb`
		let mut trusted_db = Vec::with_capacity(self.db.len());
		let mut iter = self.db.drain(..).peekable();
		while let Some((key, val)) = iter.next() {
			// Let's also make sure that the db is actually sorted.
			if let Some((next_key, _)) = iter.peek() {
				if next_key <= &key {
					return Err(VecDbError::UnsortedEntries)
				}
			}
			trusted_db.push((TrackedStorageKey::new(key), val))
		}
		Ok(TrustedVecDb { db: trusted_db })
	}
}

impl Size for UntrustedVecDb {
	fn size(&self) -> u32 {
		let proof_size = self.proof.iter().fold(0usize, |sum, node| sum.saturating_add(node.len()));
		let entries_size = self.db.iter().fold(0usize, |sum, (key, value)| {
			sum.saturating_add(key.len())
				.saturating_add(value.as_ref().unwrap_or(&vec![]).len())
		});

		proof_size.saturating_add(entries_size).saturated_into()
	}
}

/// Structure representing a key-value database stored as a sorted `Vec` of tuples.
pub struct TrustedVecDb {
	db: Vec<(TrackedStorageKey, Option<DBValue>)>,
}

impl TrustedVecDb {
	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns an error if the key doesn't exist.
	pub fn get(&mut self, key: &impl AsRef<[u8]>) -> Result<&Option<DBValue>, VecDbError> {
		let idx = self
			.db
			.binary_search_by(|(db_key, _)| db_key.key.as_slice().cmp(key.as_ref()))
			.map_err(|_| VecDbError::UnavailableKey)?;
		let (db_key, db_val) = self.db.get_mut(idx).ok_or(VecDbError::UnavailableKey)?;
		db_key.add_read();
		Ok(db_val)
	}

	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns an error if the key doesn't exist or if the value associated to it is `None`.
	pub fn get_and_decode_mandatory<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<D, VecDbError> {
		let val = self.get(key)?.as_ref().ok_or(VecDbError::EmptyVal)?;
		D::decode(&mut &val[..]).map_err(|_| VecDbError::DecodeError)
	}

	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns `None` if the key doesn't exist or if the value associated to it is `None`.
	pub fn get_and_decode_optional<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<Option<D>, VecDbError> {
		match self.get_and_decode_mandatory(key) {
			Ok(val) => Ok(Some(val)),
			Err(VecDbError::UnavailableKey | VecDbError::EmptyVal) => Ok(None),
			Err(e) => Err(e),
		}
	}

	/// Checks if each key was read.
	pub fn ensure_no_unused_keys(&self) -> Result<(), VecDbError> {
		for (key, _) in &self.db {
			if !key.has_been_read() {
				return Err(VecDbError::UnusedKey)
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;

	use sp_state_machine::{prove_read, InMemoryBackend};

	type Hasher = sp_core::Blake2Hasher;

	fn generate_untrusted_vec_db(
		entries: Vec<(RawStorageKey, Option<DBValue>)>,
	) -> (H256, Result<UntrustedVecDb, VecDbError>) {
		let keys: Vec<_> = entries.iter().map(|(key, _)| key.clone()).collect();
		let entries: Vec<_> =
			entries.iter().cloned().map(|(key, val)| (None, vec![(key, val)])).collect();
		let backend = InMemoryBackend::<Hasher>::from((entries, StateVersion::V1));
		let root = *backend.root();
		let read_proof = prove_read(backend, &keys).unwrap();

		(root, UntrustedVecDb::try_new::<Hasher>(read_proof, root, keys))
	}

	#[test]
	fn verify_succeeds_when_used_correctly() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key1".to_vec(), None),
			(b"key2".to_vec(), Some(b"val2".to_vec())),
		]);
		let db = maybe_db.unwrap();

		assert!(db.verify::<Hasher>(StateVersion::V1, &root).is_ok());
	}

	#[test]
	fn verify_fails_when_proof_contains_unneeded_nodes() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key1".to_vec(), Some(b"val1".to_vec().encode())),
			(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
		]);
		let mut db = maybe_db.unwrap();
		assert!(db.db.pop().is_some());

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(VecDbError::InvalidProof)
		));
	}

	#[test]
	fn verify_fails_when_db_contains_duplicate_nodes() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![(b"key".to_vec(), None)]);
		let mut db = maybe_db.unwrap();
		db.db.push((b"key".to_vec(), None));

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(VecDbError::InvalidProof)
		));
	}

	#[test]
	fn verify_fails_when_entries_are_not_sorted() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key1".to_vec(), Some(b"val1".to_vec().encode())),
			(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
		]);
		let mut db = maybe_db.unwrap();
		db.db.reverse();

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(VecDbError::UnsortedEntries)
		));
	}

	#[test]
	fn get_and_decode_mandatory_works() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key11".to_vec(), Some(b"val11".to_vec().encode())),
			(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
			(b"key1".to_vec(), None),
			(b"key15".to_vec(), Some(b"val15".to_vec())),
		]);
		let db = maybe_db.unwrap();
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();

		assert!(
			matches!(trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key11"), Ok(val) if val == b"val11".to_vec())
		);
		assert!(
			matches!(trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key2"), Ok(val) if val == b"val2".to_vec())
		);
		assert!(matches!(
			trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key1"),
			Err(VecDbError::EmptyVal)
		));
		assert!(matches!(
			trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key15"),
			Err(VecDbError::DecodeError)
		));
	}

	#[test]
	fn get_and_decode_optional_works() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key11".to_vec(), Some(b"val11".to_vec().encode())),
			(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
			(b"key1".to_vec(), None),
			(b"key15".to_vec(), Some(b"val15".to_vec())),
		]);
		let db = maybe_db.unwrap();
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();

		assert!(
			matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key11"), Ok(Some(val)) if val ==
		b"val11".to_vec())
		);
		assert!(
			matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key2"), Ok(Some(val)) if val == b"val2".to_vec())
		);
		assert!(matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key1"), Ok(None)));
		assert!(matches!(
			trusted_db.get_and_decode_optional::<Vec<u8>>(b"key15"),
			Err(VecDbError::DecodeError)
		));
	}

	#[test]
	fn ensure_no_unused_keys_works_correctly() {
		let (root, maybe_db) = generate_untrusted_vec_db(vec![
			(b"key1".to_vec(), None),
			(b"key2".to_vec(), Some(b"val2".to_vec())),
		]);
		let db = maybe_db.unwrap();
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();
		assert!(trusted_db.get(b"key1").is_ok());

		assert!(matches!(trusted_db.ensure_no_unused_keys(), Err(VecDbError::UnusedKey)));
	}
}

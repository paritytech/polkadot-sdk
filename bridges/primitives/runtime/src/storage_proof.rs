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

use frame_support::PalletError;
use sp_core::RuntimeDebug;
use sp_std::vec::Vec;
use sp_trie::{
	accessed_nodes_tracker::AccessedNodesTracker, read_trie_value, LayoutV1, MemoryDB, StorageProof,
};

use codec::{Decode, DecodeWithMemTracking, Encode};
use hash_db::{HashDB, Hasher, EMPTY_PREFIX};
use scale_info::TypeInfo;
#[cfg(feature = "test-helpers")]
use sp_trie::{recorder_ext::RecorderExt, Recorder, TrieDBBuilder, TrieError, TrieHash};
#[cfg(feature = "test-helpers")]
use trie_db::{Trie, TrieConfiguration, TrieDBMut};

/// Errors that can occur when interacting with `UnverifiedStorageProof` and `VerifiedStorageProof`.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo,
)]
pub enum StorageProofError {
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
	/// At least one key or node wasn't read.
	UnusedKey,

	/// Expected storage root is missing from the proof. (for non-compact proofs)
	StorageRootMismatch,
	/// Unable to reach expected storage value using provided trie nodes. (for non-compact proofs)
	StorageValueUnavailable,
	/// The proof contains duplicate nodes. (for non-compact proofs)
	DuplicateNodes,
}

impl From<sp_trie::StorageProofError> for StorageProofError {
	fn from(e: sp_trie::StorageProofError) -> Self {
		match e {
			sp_trie::StorageProofError::DuplicateNodes => StorageProofError::DuplicateNodes,
		}
	}
}

impl From<sp_trie::accessed_nodes_tracker::Error> for StorageProofError {
	fn from(e: sp_trie::accessed_nodes_tracker::Error) -> Self {
		match e {
			sp_trie::accessed_nodes_tracker::Error::UnusedNodes => StorageProofError::UnusedKey,
		}
	}
}

/// Raw storage proof type (just raw trie nodes).
pub type RawStorageProof = sp_trie::RawStorageProof;

/// Calculates size for `RawStorageProof`.
pub fn raw_storage_proof_size(raw_storage_proof: &RawStorageProof) -> usize {
	raw_storage_proof
		.iter()
		.fold(0usize, |sum, node| sum.saturating_add(node.len()))
}

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

/// This struct is used to read storage values from a subset of a Merklized database. The "proof"
/// is a subset of the nodes in the Merkle structure of the database, so that it provides
/// authentication against a known Merkle root as well as the values in the
/// database themselves.
pub struct StorageProofChecker<H>
where
	H: Hasher,
{
	root: H::Out,
	db: MemoryDB<H>,
	accessed_nodes_tracker: AccessedNodesTracker<H::Out>,
}

impl<H> StorageProofChecker<H>
where
	H: Hasher,
{
	/// Constructs a new storage proof checker.
	///
	/// This returns an error if the given proof is invalid with respect to the given root.
	pub fn new(root: H::Out, proof: RawStorageProof) -> Result<Self, StorageProofError> {
		let proof = StorageProof::new_with_duplicate_nodes_check(proof)?;

		let recorder = AccessedNodesTracker::new(proof.len());

		let db = proof.into_memory_db();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(StorageProofError::StorageRootMismatch)
		}

		Ok(StorageProofChecker { root, db, accessed_nodes_tracker: recorder })
	}

	/// Returns error if the proof has some nodes that are left intact by previous `read_value`
	/// calls.
	pub fn ensure_no_unused_nodes(self) -> Result<(), StorageProofError> {
		self.accessed_nodes_tracker.ensure_no_unused_nodes().map_err(Into::into)
	}

	/// Reads a value from the available subset of storage. If the value cannot be read due to an
	/// incomplete or otherwise invalid proof, this function returns an error.
	pub fn read_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageProofError> {
		// LayoutV1 or LayoutV0 is identical for proof that only read values.
		read_trie_value::<LayoutV1<H>, _>(
			&self.db,
			&self.root,
			key,
			Some(&mut self.accessed_nodes_tracker),
			None,
		)
		.map_err(|_| StorageProofError::StorageValueUnavailable)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns an error. If value is
	/// read, but decoding fails, this function returns an error.
	pub fn read_and_decode_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<Option<T>, StorageProofError> {
		self.read_value(key).and_then(|v| {
			v.map(|v| {
				T::decode(&mut &v[..]).map_err(|e| {
					log::warn!(target: "bridge-storage-proofs", "read_and_decode_value error: {e:?}");
					StorageProofError::DecodeError
				})
			})
			.transpose()
		})
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, or if the value is `None`, this function
	/// returns an error. If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_mandatory_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<T, StorageProofError> {
		self.read_and_decode_value(key)?.ok_or(StorageProofError::EmptyVal)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns `Ok(None)`.
	/// If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_opt_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<Option<T>, StorageProofError> {
		match self.read_and_decode_value(key) {
			Ok(outbound_lane_data) => Ok(outbound_lane_data),
			Err(StorageProofError::StorageValueUnavailable) => Ok(None),
			Err(e) => Err(e),
		}
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
				return
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
				return
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

/// Return valid storage proof and state root.
///
/// Note: This should only be used for **testing**.
#[cfg(feature = "std")]
pub fn craft_valid_storage_proof() -> (sp_core::H256, RawStorageProof) {
	use sp_state_machine::{backend::Backend, prove_read, InMemoryBackend};

	let state_version = sp_runtime::StateVersion::default();

	// construct storage proof
	let backend = <InMemoryBackend<sp_core::Blake2Hasher>>::from((
		sp_std::vec![
			(None, vec![(b"key1".to_vec(), Some(b"value1".to_vec()))]),
			(None, vec![(b"key2".to_vec(), Some(b"value2".to_vec()))]),
			(None, vec![(b"key3".to_vec(), Some(b"value3".to_vec()))]),
			(None, vec![(b"key4".to_vec(), Some((42u64, 42u32, 42u16, 42u8).encode()))]),
			// Value is too big to fit in a branch node
			(None, vec![(b"key11".to_vec(), Some(vec![0u8; 32]))]),
		],
		state_version,
	));
	let root = backend.storage_root(sp_std::iter::empty(), state_version).0;
	let proof =
		prove_read(backend, &[&b"key1"[..], &b"key2"[..], &b"key4"[..], &b"key22"[..]]).unwrap();

	(root, proof.into_nodes().into_iter().collect())
}

#[cfg(test)]
pub mod tests_for_storage_proof_checker {
	use super::*;
	use codec::Encode;

	#[test]
	fn storage_proof_check() {
		let (root, proof) = craft_valid_storage_proof();

		// check proof in runtime
		let mut checker =
			<StorageProofChecker<sp_core::Blake2Hasher>>::new(root, proof.clone()).unwrap();
		assert_eq!(checker.read_value(b"key1"), Ok(Some(b"value1".to_vec())));
		assert_eq!(checker.read_value(b"key2"), Ok(Some(b"value2".to_vec())));
		assert_eq!(checker.read_value(b"key4"), Ok(Some((42u64, 42u32, 42u16, 42u8).encode())));
		assert_eq!(
			checker.read_value(b"key11111"),
			Err(StorageProofError::StorageValueUnavailable)
		);
		assert_eq!(checker.read_value(b"key22"), Ok(None));
		assert_eq!(checker.read_and_decode_value(b"key4"), Ok(Some((42u64, 42u32, 42u16, 42u8))),);
		assert!(matches!(
			checker.read_and_decode_value::<[u8; 64]>(b"key4"),
			Err(StorageProofError::DecodeError),
		));

		// checking proof against invalid commitment fails
		assert_eq!(
			<StorageProofChecker<sp_core::Blake2Hasher>>::new(sp_core::H256::random(), proof).err(),
			Some(StorageProofError::StorageRootMismatch)
		);
	}

	#[test]
	fn proof_with_unused_items_is_rejected() {
		let (root, proof) = craft_valid_storage_proof();

		let mut checker =
			StorageProofChecker::<sp_core::Blake2Hasher>::new(root, proof.clone()).unwrap();
		checker.read_value(b"key1").unwrap().unwrap();
		checker.read_value(b"key2").unwrap();
		checker.read_value(b"key4").unwrap();
		checker.read_value(b"key22").unwrap();
		assert_eq!(checker.ensure_no_unused_nodes(), Ok(()));

		let checker = StorageProofChecker::<sp_core::Blake2Hasher>::new(root, proof).unwrap();
		assert_eq!(checker.ensure_no_unused_nodes(), Err(StorageProofError::UnusedKey));
	}
}

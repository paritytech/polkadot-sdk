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

//! Logic for checking Substrate storage proofs.

use crate::StrippableError;
use codec::{Decode, Encode};
use frame_support::PalletError;
use hash_db::{HashDB, Hasher, EMPTY_PREFIX};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec::Vec};
pub use sp_trie::RawStorageProof;
use sp_trie::{
	accessed_nodes_tracker::{AccessedNodesTracker, Error as AccessedNodesTrackerError},
	read_trie_value,
	recorder_ext::RecorderExt,
	LayoutV1, MemoryDB, Recorder, StorageProof, StorageProofError, Trie, TrieConfiguration,
	TrieDBBuilder, TrieError, TrieHash,
};

/// Storage proof size requirements.
///
/// This is currently used by benchmarks when generating storage proofs.
#[derive(Clone, Copy, Debug)]
pub enum ProofSize {
	/// The proof is expected to be minimal. If value size may be changed, then it is expected to
	/// have given size.
	Minimal(u32),
	/// The proof is expected to have at least given size and grow by increasing value that is
	/// stored in the trie.
	HasLargeLeaf(u32),
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
	pub fn new(root: H::Out, proof: RawStorageProof) -> Result<Self, Error> {
		let proof = StorageProof::new_with_duplicate_nodes_check(proof)
			.map_err(|e| Error::StorageProof(e.into()))?;

		let recorder = AccessedNodesTracker::new(proof.len());

		let db = proof.into_memory_db();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(Error::StorageRootMismatch)
		}

		Ok(StorageProofChecker { root, db, accessed_nodes_tracker: recorder })
	}

	/// Returns error if the proof has some nodes that are left intact by previous `read_value`
	/// calls.
	pub fn ensure_no_unused_nodes(self) -> Result<(), Error> {
		self.accessed_nodes_tracker
			.ensure_no_unused_nodes()
			.map_err(|e| Error::AccessedNodesTracker(e.into()))
	}

	/// Reads a value from the available subset of storage. If the value cannot be read due to an
	/// incomplete or otherwise invalid proof, this function returns an error.
	pub fn read_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
		// LayoutV1 or LayoutV0 is identical for proof that only read values.
		read_trie_value::<LayoutV1<H>, _>(
			&self.db,
			&self.root,
			key,
			Some(&mut self.accessed_nodes_tracker),
			None,
		)
		.map_err(|_| Error::StorageValueUnavailable)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns an error. If value is
	/// read, but decoding fails, this function returns an error.
	pub fn read_and_decode_value<T: Decode>(&mut self, key: &[u8]) -> Result<Option<T>, Error> {
		self.read_value(key).and_then(|v| {
			v.map(|v| T::decode(&mut &v[..]).map_err(|e| Error::StorageValueDecodeFailed(e.into())))
				.transpose()
		})
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, or if the value is `None`, this function
	/// returns an error. If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_mandatory_value<T: Decode>(&mut self, key: &[u8]) -> Result<T, Error> {
		self.read_and_decode_value(key)?.ok_or(Error::StorageValueEmpty)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns `Ok(None)`.
	/// If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_opt_value<T: Decode>(&mut self, key: &[u8]) -> Result<Option<T>, Error> {
		match self.read_and_decode_value(key) {
			Ok(outbound_lane_data) => Ok(outbound_lane_data),
			Err(Error::StorageValueUnavailable) => Ok(None),
			Err(e) => Err(e),
		}
	}
}

/// Storage proof related errors.
#[derive(Encode, Decode, Clone, Eq, PartialEq, PalletError, Debug, TypeInfo)]
pub enum Error {
	/// Error generated by the `AccessedNodesTrackerError`.
	AccessedNodesTracker(StrippableError<AccessedNodesTrackerError>),
	/// Error originating in the `storage_proof` module.
	StorageProof(StrippableError<StorageProofError>),
	/// Expected storage root is missing from the proof.
	StorageRootMismatch,
	/// Unable to reach expected storage value using provided trie nodes.
	StorageValueUnavailable,
	/// The storage value is `None`.
	StorageValueEmpty,
	/// Failed to decode storage value.
	StorageValueDecodeFailed(StrippableError<codec::Error>),
}

/// Return valid storage proof and state root.
///
/// NOTE: This should only be used for **testing**.
#[cfg(feature = "std")]
pub fn craft_valid_storage_proof() -> (sp_core::H256, RawStorageProof) {
	use sp_state_machine::{backend::Backend, prove_read, InMemoryBackend};

	let state_version = sp_runtime::StateVersion::default();

	// construct storage proof
	let backend = <InMemoryBackend<sp_core::Blake2Hasher>>::from((
		vec![
			(None, vec![(b"key1".to_vec(), Some(b"value1".to_vec()))]),
			(None, vec![(b"key2".to_vec(), Some(b"value2".to_vec()))]),
			(None, vec![(b"key3".to_vec(), Some(b"value3".to_vec()))]),
			(None, vec![(b"key4".to_vec(), Some((42u64, 42u32, 42u16, 42u8).encode()))]),
			// Value is too big to fit in a branch node
			(None, vec![(b"key11".to_vec(), Some(vec![0u8; 32]))]),
		],
		state_version,
	));
	let root = backend.storage_root(std::iter::empty(), state_version).0;
	let proof =
		prove_read(backend, &[&b"key1"[..], &b"key2"[..], &b"key4"[..], &b"key22"[..]]).unwrap();

	(root, proof.into_nodes().into_iter().collect())
}

/// Record all keys for a given root.
pub fn record_all_keys<L: TrieConfiguration, DB>(
	db: &DB,
	root: &TrieHash<L>,
) -> Result<RawStorageProof, Box<TrieError<L>>>
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

#[cfg(test)]
pub mod tests {
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
		assert_eq!(checker.read_value(b"key11111"), Err(Error::StorageValueUnavailable));
		assert_eq!(checker.read_value(b"key22"), Ok(None));
		assert_eq!(checker.read_and_decode_value(b"key4"), Ok(Some((42u64, 42u32, 42u16, 42u8))),);
		assert!(matches!(
			checker.read_and_decode_value::<[u8; 64]>(b"key4"),
			Err(Error::StorageValueDecodeFailed(_)),
		));

		// checking proof against invalid commitment fails
		assert_eq!(
			<StorageProofChecker<sp_core::Blake2Hasher>>::new(sp_core::H256::random(), proof).err(),
			Some(Error::StorageRootMismatch)
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
		assert_eq!(
			checker.ensure_no_unused_nodes(),
			Err(Error::AccessedNodesTracker(AccessedNodesTrackerError::UnusedNodes.into()))
		);
	}
}

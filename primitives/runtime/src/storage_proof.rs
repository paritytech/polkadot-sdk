// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use codec::Decode;
use hash_db::{HashDB, Hasher, EMPTY_PREFIX};
use sp_runtime::RuntimeDebug;
use sp_std::{boxed::Box, vec::Vec};
use sp_trie::{
	read_trie_value, LayoutV1, MemoryDB, Recorder, StorageProof, Trie, TrieConfiguration,
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
	/// The proof is expected to have at least given size and grow by increasing number of trie
	/// nodes included in the proof.
	HasExtraNodes(u32),
	/// The proof is expected to have at least given size and grow by increasing value that is
	/// stored in the trie.
	HasLargeLeaf(u32),
}

/// This struct is used to read storage values from a subset of a Merklized database. The "proof"
/// is a subset of the nodes in the Merkle structure of the database, so that it provides
/// authentication against a known Merkle root as well as the values in the database themselves.
pub struct StorageProofChecker<H>
where
	H: Hasher,
{
	root: H::Out,
	db: MemoryDB<H>,
}

impl<H> StorageProofChecker<H>
where
	H: Hasher,
{
	/// Constructs a new storage proof checker.
	///
	/// This returns an error if the given proof is invalid with respect to the given root.
	pub fn new(root: H::Out, proof: StorageProof) -> Result<Self, Error> {
		let db = proof.into_memory_db();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(Error::StorageRootMismatch)
		}

		let checker = StorageProofChecker { root, db };
		Ok(checker)
	}

	/// Reads a value from the available subset of storage. If the value cannot be read due to an
	/// incomplete or otherwise invalid proof, this function returns an error.
	pub fn read_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
		// LayoutV1 or LayoutV0 is identical for proof that only read values.
		read_trie_value::<LayoutV1<H>, _>(&self.db, &self.root, key, None, None)
			.map_err(|_| Error::StorageValueUnavailable)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns an error. If value is
	/// read, but decoding fails, this function returns an error.
	pub fn read_and_decode_value<T: Decode>(&self, key: &[u8]) -> Result<Option<T>, Error> {
		self.read_value(key).and_then(|v| {
			v.map(|v| T::decode(&mut &v[..]).map_err(Error::StorageValueDecodeFailed))
				.transpose()
		})
	}
}

#[derive(Eq, RuntimeDebug, PartialEq)]
pub enum Error {
	StorageRootMismatch,
	StorageValueUnavailable,
	StorageValueDecodeFailed(codec::Error),
}

/// Return valid storage proof and state root.
///
/// NOTE: This should only be used for **testing**.
#[cfg(feature = "std")]
pub fn craft_valid_storage_proof() -> (sp_core::H256, StorageProof) {
	use codec::Encode;
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

	(root, proof)
}

/// Record all keys for a given root.
pub fn record_all_keys<L: TrieConfiguration, DB>(
	db: &DB,
	root: &TrieHash<L>,
	recorder: &mut Recorder<L>,
) -> Result<(), Box<TrieError<L>>>
where
	DB: hash_db::HashDBRef<L::Hash, trie_db::DBValue>,
{
	let trie = TrieDBBuilder::<L>::new(db, root).with_recorder(recorder).build();
	for x in trie.iter()? {
		let (key, _) = x?;
		trie.get(&key)?;
	}

	Ok(())
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use codec::Encode;

	#[test]
	fn storage_proof_check() {
		let (root, proof) = craft_valid_storage_proof();

		// check proof in runtime
		let checker =
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
}

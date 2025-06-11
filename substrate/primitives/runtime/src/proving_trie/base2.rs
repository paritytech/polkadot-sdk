// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Types for a base-2 merkle tree used for checking and generating proofs within the
//! runtime. The `binary-merkle-tree` crate exposes all of these same functionality (and more), but
//! this library is designed to work more easily with runtime native types, which simply need to
//! implement `Encode`/`Decode`.

use super::{ProofToHashes, ProvingTrie, TrieError};
use crate::{Decode, DispatchError, Encode};
use alloc::{collections::BTreeMap, vec::Vec};
use binary_merkle_tree::{merkle_proof, merkle_root, MerkleProof};
use codec::MaxEncodedLen;

/// A helper structure for building a basic base-2 merkle trie and creating compact proofs for that
/// trie.
pub struct BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
{
	db: BTreeMap<Key, Value>,
	root: Hashing::Out,
	_phantom: core::marker::PhantomData<(Key, Value)>,
}

impl<Hashing, Key, Value> ProvingTrie<Hashing, Key, Value> for BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Hashing::Out: Encode + Decode,
	Key: Encode + Decode + Ord,
	Value: Encode + Decode + Clone,
{
	/// Create a new instance of a `ProvingTrie` using an iterator of key/value pairs.
	fn generate_for<I>(items: I) -> Result<Self, DispatchError>
	where
		I: IntoIterator<Item = (Key, Value)>,
	{
		let mut db = BTreeMap::default();
		for (key, value) in items.into_iter() {
			db.insert(key, value);
		}
		let root = merkle_root::<Hashing, _>(db.iter().map(|item| item.encode()));
		Ok(Self { db, root, _phantom: Default::default() })
	}

	/// Access the underlying trie root.
	fn root(&self) -> &Hashing::Out {
		&self.root
	}

	/// Query a value contained within the current trie. Returns `None` if the
	/// nodes within the current `db` are insufficient to query the item.
	fn query(&self, key: &Key) -> Option<Value> {
		self.db.get(&key).cloned()
	}

	/// Create a compact merkle proof needed to prove a single key and its value are in the trie.
	/// Returns an error if the nodes within the current `db` are insufficient to create a proof.
	fn create_proof(&self, key: &Key) -> Result<Vec<u8>, DispatchError> {
		let mut encoded = Vec::with_capacity(self.db.len());
		let mut found_index = None;

		// Find the index of our key, and encode the (key, value) pair.
		for (i, (k, v)) in self.db.iter().enumerate() {
			// If we found the key we are looking for, save it.
			if k == key {
				found_index = Some(i);
			}

			encoded.push((k, v).encode());
		}

		let index = found_index.ok_or(TrieError::IncompleteDatabase)?;
		let proof = merkle_proof::<Hashing, Vec<Vec<u8>>, Vec<u8>>(encoded, index as u32);
		Ok(proof.encode())
	}

	/// Verify the existence of `key` and `value` in a given trie root and proof.
	fn verify_proof(
		root: &Hashing::Out,
		proof: &[u8],
		key: &Key,
		value: &Value,
	) -> Result<(), DispatchError> {
		verify_proof::<Hashing, Key, Value>(root, proof, key, value)
	}
}

impl<Hashing, Key, Value> ProofToHashes for BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Hashing::Out: MaxEncodedLen + Decode,
	Key: Decode,
	Value: Decode,
{
	// Our proof is just raw bytes.
	type Proof = [u8];
	// This base 2 merkle trie includes a `proof` field which is a `Vec<Hash>`.
	// The length of this vector tells us the depth of the proof, and how many
	// hashes we need to calculate.
	fn proof_to_hashes(proof: &[u8]) -> Result<u32, DispatchError> {
		let decoded_proof: MerkleProof<Hashing::Out, Vec<u8>> =
			Decode::decode(&mut &proof[..]).map_err(|_| TrieError::IncompleteProof)?;
		let depth = decoded_proof.proof.len();
		Ok(depth as u32)
	}
}

/// Verify the existence of `key` and `value` in a given trie root and proof.
pub fn verify_proof<Hashing, Key, Value>(
	root: &Hashing::Out,
	proof: &[u8],
	key: &Key,
	value: &Value,
) -> Result<(), DispatchError>
where
	Hashing: sp_core::Hasher,
	Hashing::Out: Decode,
	Key: Encode + Decode,
	Value: Encode + Decode,
{
	let decoded_proof: MerkleProof<Hashing::Out, Vec<u8>> =
		Decode::decode(&mut &proof[..]).map_err(|_| TrieError::IncompleteProof)?;
	if *root != decoded_proof.root {
		return Err(TrieError::RootMismatch.into());
	}

	if (key, value).encode() != decoded_proof.leaf {
		return Err(TrieError::ValueMismatch.into());
	}

	if binary_merkle_tree::verify_proof::<Hashing, _, _>(
		&decoded_proof.root,
		decoded_proof.proof,
		decoded_proof.number_of_leaves,
		decoded_proof.leaf_index,
		&decoded_proof.leaf,
	) {
		Ok(())
	} else {
		Err(TrieError::IncompleteProof.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::traits::BlakeTwo256;
	use sp_core::H256;
	use std::collections::BTreeMap;

	// A trie which simulates a trie of accounts (u32) and balances (u128).
	type BalanceTrie = BasicProvingTrie<BlakeTwo256, u32, u128>;

	// The expected root hash for an empty trie.
	fn empty_root() -> H256 {
		let tree = BalanceTrie::generate_for(Vec::new()).unwrap();
		*tree.root()
	}

	fn create_balance_trie() -> BalanceTrie {
		// Create a map of users and their balances.
		let mut map = BTreeMap::<u32, u128>::new();
		for i in 0..100u32 {
			map.insert(i, i.into());
		}

		// Put items into the trie.
		let balance_trie = BalanceTrie::generate_for(map).unwrap();

		// Root is changed.
		let root = *balance_trie.root();
		assert!(root != empty_root());

		// Assert valid keys are queryable.
		assert_eq!(balance_trie.query(&6u32), Some(6u128));
		assert_eq!(balance_trie.query(&9u32), Some(9u128));
		assert_eq!(balance_trie.query(&69u32), Some(69u128));

		balance_trie
	}

	#[test]
	fn empty_trie_works() {
		let empty_trie = BalanceTrie::generate_for(Vec::new()).unwrap();
		assert_eq!(*empty_trie.root(), empty_root());
	}

	#[test]
	fn basic_end_to_end_single_value() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid key.
		let proof = balance_trie.create_proof(&6u32).unwrap();

		// Assert key is provable, all other keys are invalid.
		for i in 0..200u32 {
			if i == 6 {
				assert_eq!(
					verify_proof::<BlakeTwo256, _, _>(&root, &proof, &i, &u128::from(i)),
					Ok(())
				);
				// Wrong value is invalid.
				assert_eq!(
					verify_proof::<BlakeTwo256, _, _>(&root, &proof, &i, &u128::from(i + 1)),
					Err(TrieError::ValueMismatch.into())
				);
			} else {
				assert!(
					verify_proof::<BlakeTwo256, _, _>(&root, &proof, &i, &u128::from(i)).is_err()
				);
			}
		}
	}

	#[test]
	fn proof_fails_with_bad_data() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid key.
		let proof = balance_trie.create_proof(&6u32).unwrap();

		// Correct data verifies successfully
		assert_eq!(verify_proof::<BlakeTwo256, _, _>(&root, &proof, &6u32, &6u128), Ok(()));

		// Fail to verify proof with wrong root
		assert_eq!(
			verify_proof::<BlakeTwo256, _, _>(&Default::default(), &proof, &6u32, &6u128),
			Err(TrieError::RootMismatch.into())
		);

		// Fail to verify proof with wrong data
		assert_eq!(
			verify_proof::<BlakeTwo256, _, _>(&root, &[], &6u32, &6u128),
			Err(TrieError::IncompleteProof.into())
		);
	}

	// We make assumptions about the structure of the merkle proof in order to provide the
	// `proof_to_hashes` function. This test keeps those assumptions checked.
	#[test]
	fn assert_structure_of_merkle_proof() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();
		// Create a proof for a valid key.
		let proof = balance_trie.create_proof(&6u32).unwrap();
		let decoded_proof: MerkleProof<H256, Vec<u8>> = Decode::decode(&mut &proof[..]).unwrap();

		let constructed_proof = MerkleProof::<H256, Vec<u8>> {
			root,
			proof: decoded_proof.proof.clone(),
			number_of_leaves: 100,
			leaf_index: 6,
			leaf: (6u32, 6u128).encode(),
		};
		assert_eq!(constructed_proof, decoded_proof);
	}

	#[test]
	fn proof_to_hashes() {
		let mut i: u32 = 1;
		while i < 10_000_000 {
			let trie = BalanceTrie::generate_for((0..i).map(|i| (i, u128::from(i)))).unwrap();
			let proof = trie.create_proof(&0).unwrap();
			let hashes = BalanceTrie::proof_to_hashes(&proof).unwrap();
			let log2 = (i as f64).log2().ceil() as u32;

			assert_eq!(hashes, log2);
			i = i * 10;
		}
	}
}

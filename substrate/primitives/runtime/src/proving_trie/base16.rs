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

//! Types for a compact base-16 merkle trie used for checking and generating proofs within the
//! runtime. The `sp-trie` crate exposes all of these same functionality (and more), but this
//! library is designed to work more easily with runtime native types, which simply need to
//! implement `Encode`/`Decode`. It also exposes a runtime friendly `TrieError` type which can be
//! use inside of a FRAME Pallet.
//!
//! Proofs are created with latest substrate trie format (`LayoutV1`), and are not compatible with
//! proofs using `LayoutV0`.

use super::{ProofToHashes, ProvingTrie, TrieError};
use crate::{Decode, DispatchError, Encode};
use alloc::vec::Vec;
use codec::MaxEncodedLen;
use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV1},
	LayoutV1, MemoryDB, RandomState, Trie, TrieMut,
};

/// A helper structure for building a basic base-16 merkle trie and creating compact proofs for that
/// trie. Proofs are created with latest substrate trie format (`LayoutV1`), and are not compatible
/// with proofs using `LayoutV0`.
pub struct BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
{
	db: MemoryDB<Hashing>,
	root: Hashing::Out,
	_phantom: core::marker::PhantomData<(Key, Value)>,
}

impl<Hashing, Key, Value> BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
{
	/// Create a compact merkle proof needed to prove all `keys` and their values are in the trie.
	///
	/// When verifying the proof created by this function, you must include all of the keys and
	/// values of the proof, else the verifier will complain that extra nodes are provided in the
	/// proof that are not needed.
	pub fn create_multi_proof(&self, keys: &[Key]) -> Result<Vec<u8>, DispatchError> {
		sp_trie::generate_trie_proof::<LayoutV1<Hashing>, _, _, _>(
			&self.db,
			self.root,
			&keys.into_iter().map(|k| k.encode()).collect::<Vec<Vec<u8>>>(),
		)
		.map_err(|err| TrieError::from(*err).into())
		.map(|structured_proof| structured_proof.encode())
	}
}

impl<Hashing, Key, Value> ProvingTrie<Hashing, Key, Value> for BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode + Decode,
{
	/// Create a new instance of a `ProvingTrie` using an iterator of key/value pairs.
	fn generate_for<I>(items: I) -> Result<Self, DispatchError>
	where
		I: IntoIterator<Item = (Key, Value)>,
	{
		let mut db = MemoryDB::with_hasher(RandomState::default());
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilderV1::new(&mut db, &mut root).build();
			for (key, value) in items.into_iter() {
				key.using_encoded(|k| value.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(Self { db, root, _phantom: Default::default() })
	}

	/// Access the underlying trie root.
	fn root(&self) -> &Hashing::Out {
		&self.root
	}

	/// Query a value contained within the current trie. Returns `None` if the
	/// nodes within the current `MemoryDB` are insufficient to query the item.
	fn query(&self, key: &Key) -> Option<Value> {
		let trie = TrieDBBuilder::new(&self.db, &self.root).build();
		key.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| Value::decode(&mut &*raw).ok())
	}

	/// Create a compact merkle proof needed to prove a single key and its value are in the trie.
	fn create_proof(&self, key: &Key) -> Result<Vec<u8>, DispatchError> {
		sp_trie::generate_trie_proof::<LayoutV1<Hashing>, _, _, _>(
			&self.db,
			self.root,
			&[key.encode()],
		)
		.map_err(|err| TrieError::from(*err).into())
		.map(|structured_proof| structured_proof.encode())
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
	Hashing::Out: MaxEncodedLen,
{
	// Our proof is just raw bytes.
	type Proof = [u8];
	// This base 16 trie uses a raw proof of `Vec<Vec<u8>`, where the length of the first `Vec`
	// is the depth of the trie. We can use this to predict the number of hashes.
	fn proof_to_hashes(proof: &[u8]) -> Result<u32, DispatchError> {
		use codec::DecodeLength;
		let depth =
			<Vec<Vec<u8>> as DecodeLength>::len(proof).map_err(|_| TrieError::DecodeError)?;
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
	Key: Encode,
	Value: Encode,
{
	let structured_proof: Vec<Vec<u8>> =
		Decode::decode(&mut &proof[..]).map_err(|_| TrieError::DecodeError)?;
	sp_trie::verify_trie_proof::<LayoutV1<Hashing>, _, _, _>(
		&root,
		&structured_proof,
		&[(key.encode(), Some(value.encode()))],
	)
	.map_err(|err| TrieError::from(err).into())
}

/// Verify the existence of multiple `items` in a given trie root and proof.
pub fn verify_multi_proof<Hashing, Key, Value>(
	root: &Hashing::Out,
	proof: &[u8],
	items: &[(Key, Value)],
) -> Result<(), DispatchError>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode,
{
	let structured_proof: Vec<Vec<u8>> =
		Decode::decode(&mut &proof[..]).map_err(|_| TrieError::DecodeError)?;
	let items_encoded = items
		.into_iter()
		.map(|(key, value)| (key.encode(), Some(value.encode())))
		.collect::<Vec<(Vec<u8>, Option<Vec<u8>>)>>();

	sp_trie::verify_trie_proof::<LayoutV1<Hashing>, _, _, _>(
		&root,
		&structured_proof,
		&items_encoded,
	)
	.map_err(|err| TrieError::from(err).into())
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
		sp_trie::empty_trie_root::<LayoutV1<BlakeTwo256>>()
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
		// Invalid key returns none.
		assert_eq!(balance_trie.query(&6969u32), None);

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
					Err(TrieError::RootMismatch.into())
				);
			} else {
				assert!(
					verify_proof::<BlakeTwo256, _, _>(&root, &proof, &i, &u128::from(i)).is_err()
				);
			}
		}
	}

	#[test]
	fn basic_end_to_end_multi() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid and invalid key.
		let proof = balance_trie.create_multi_proof(&[6u32, 9u32, 69u32]).unwrap();
		let items = [(6u32, 6u128), (9u32, 9u128), (69u32, 69u128)];

		assert_eq!(verify_multi_proof::<BlakeTwo256, _, _>(&root, &proof, &items), Ok(()));
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

		// Crete a bad proof.
		let bad_proof = balance_trie.create_proof(&99u32).unwrap();

		// Fail to verify data with the wrong proof
		assert_eq!(
			verify_proof::<BlakeTwo256, _, _>(&root, &bad_proof, &6u32, &6u128),
			Err(TrieError::ExtraneousHashReference.into())
		);
	}

	#[test]
	fn proof_to_hashes() {
		let mut i: u32 = 1;
		// Compute log base 16 and round up
		let log16 = |x: u32| -> u32 {
			let x_f64 = x as f64;
			let log16_x = (x_f64.ln() / 16_f64.ln()).ceil();
			log16_x as u32
		};

		while i < 10_000_000 {
			let trie = BalanceTrie::generate_for((0..i).map(|i| (i, u128::from(i)))).unwrap();
			let proof = trie.create_proof(&0).unwrap();
			let hashes = BalanceTrie::proof_to_hashes(&proof).unwrap();
			let log16 = log16(i).max(1);

			assert_eq!(hashes, log16);
			i = i * 10;
		}
	}
}

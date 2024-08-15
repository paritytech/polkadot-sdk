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

//! Types for a simple merkle trie used for checking and generating proofs.

use crate::{Decode, DispatchError, Encode};

use sp_std::vec::Vec;
use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV0},
	LayoutV1, MemoryDB, Recorder, Trie, TrieMut, EMPTY_PREFIX,
};

type HashOf<Hashing> = <Hashing as sp_core::Hasher>::Out;

/// A basic trie implementation for checking and generating proofs for a key / value pair.
pub struct BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
{
	db: MemoryDB<Hashing>,
	root: HashOf<Hashing>,
	_phantom: core::marker::PhantomData<(Key, Value)>,
}

impl<Hashing, Key, Value> BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode + Decode,
{
	/// Create a new instance of a `ProvingTrie` using an iterator of key/value pairs.
	pub fn generate_for<I>(items: I) -> Result<Self, DispatchError>
	where
		I: IntoIterator<Item = (Key, Value)>,
	{
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilderV0::new(&mut db, &mut root).build();
			for (key, value) in items.into_iter() {
				key.using_encoded(|k| value.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(Self { db, root, _phantom: Default::default() })
	}

	/// Access the underlying trie root.
	pub fn root(&self) -> &HashOf<Hashing> {
		&self.root
	}

	/// Check a proof contained within the current memory-db. Returns `None` if the
	/// nodes within the current `MemoryDB` are insufficient to query the item.
	pub fn query(&self, key: Key) -> Option<Value> {
		let trie = TrieDBBuilder::new(&self.db, &self.root).build();
		key.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| Value::decode(&mut &*raw).ok())
	}

	/// Create the full verification data needed to prove all `keys` and their values in the trie.
	/// Returns `None` if the nodes within the current `MemoryDB` are insufficient to create a
	/// proof.
	pub fn create_proof(&self, keys: Vec<Key>) -> Option<Vec<Vec<u8>>> {
		let mut recorder = Recorder::<LayoutV1<Hashing>>::new();

		{
			let trie =
				TrieDBBuilder::new(&self.db, &self.root).with_recorder(&mut recorder).build();

			keys.iter()
				.map(|key| {
					key.using_encoded(|k| {
						trie.get(k).ok()?.and_then(|raw| Value::decode(&mut &*raw).ok())
					})
				})
				.collect::<Option<Vec<_>>>()?;
		}

		Some(recorder.drain().into_iter().map(|r| r.data).collect())
	}

	/// Create the full verification data needed to prove a single key and its value in the trie.
	/// Returns `None` if the nodes within the current `MemoryDB` are insufficient to create a
	/// proof.
	pub fn create_single_value_proof(&self, key: Key) -> Option<Vec<Vec<u8>>> {
		let mut recorder = Recorder::<LayoutV1<Hashing>>::new();

		{
			let trie =
				TrieDBBuilder::new(&self.db, &self.root).with_recorder(&mut recorder).build();

			key.using_encoded(|k| {
				trie.get(k).ok()?.and_then(|raw| Value::decode(&mut &*raw).ok())
			})?;
		}

		Some(recorder.drain().into_iter().map(|r| r.data).collect())
	}

	/// Create a new instance of `ProvingTrie` from raw nodes. Nodes can be generated using the
	/// `create_proof` function.
	pub fn from_nodes(root: HashOf<Hashing>, nodes: &[Vec<u8>]) -> Self {
		use sp_trie::HashDBT;

		let mut memory_db = MemoryDB::default();
		for node in nodes {
			HashDBT::insert(&mut memory_db, EMPTY_PREFIX, &node[..]);
		}

		Self { db: memory_db, root, _phantom: Default::default() }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::traits::BlakeTwo256;
	use sp_core::H256;
	use sp_std::{collections::btree_map::BTreeMap, str::FromStr};

	// A trie which simulates a trie of accounts (u32) and balances (u128).
	type BalanceTrie = BasicProvingTrie<BlakeTwo256, u32, u128>;

	// The expected root hash for an empty trie.
	fn empty_root() -> H256 {
		H256::from_str("0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314")
			.unwrap()
	}

	#[test]
	fn empty_trie_works() {
		let empty_trie = BalanceTrie::generate_for(Vec::new()).unwrap();
		assert_eq!(*empty_trie.root(), empty_root());
	}

	#[test]
	fn basic_end_to_end() {
		// Create a map of users and their balances.
		let mut map = BTreeMap::<u32, u128>::new();
		for i in 0..10u32 {
			map.insert(i, i.into());
		}

		// Put items into the trie.
		let balance_trie = BalanceTrie::generate_for(map).unwrap();

		// Root is changed.
		let root = *balance_trie.root();
		assert!(root != empty_root());

		// Assert valid key is queryable.
		assert_eq!(balance_trie.query(6u32), Some(6u128));
		assert_eq!(balance_trie.query(9u32), Some(9u128));
		// Invalid key returns none.
		assert_eq!(balance_trie.query(69u32), None);

		// Create a proof for a valid key.
		let proof = balance_trie.create_single_value_proof(6u32).unwrap();
		// Can't create proof for invalid key.
		assert_eq!(balance_trie.create_single_value_proof(69u32), None);

		// Create a new proving trie from the proof.
		let new_balance_trie = BalanceTrie::from_nodes(root, &proof);

		// Assert valid key is queryable.
		assert_eq!(new_balance_trie.query(6u32), Some(6u128));
	}
}

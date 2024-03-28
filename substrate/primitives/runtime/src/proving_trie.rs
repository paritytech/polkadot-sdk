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

//! A generic implementation of a simple merkle trie used for making and verifying proofs.

use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV0},
	LayoutV0, MemoryDB, Recorder, Trie, TrieMut, EMPTY_PREFIX,
};

use crate::{traits::HashOutput, Decode, Encode, KeyTypeId};

/// A trie instance for checking and generating proofs.
pub struct ProvingTrie<Hashing, Hash, Item>
where
	Hashing: sp_core::Hasher<Out = Hash>,
	Hash: HashOutput,
	Item: Encode + Decode,
{
	db: MemoryDB<Hashing>,
	root: Hash,
	_phantom: core::marker::PhantomData<Item>,
}

impl<Hashing, Hash, Item> ProvingTrie<Hashing, Hash, Item>
where
	Hashing: sp_core::Hasher<Out = Hash>,
	Hash: HashOutput,
	Item: Encode + Decode,
{
	/// Access the underlying trie root.
	pub fn root(&self) -> &Hash {
		&self.root
	}

	/// Check a proof contained within the current memory-db. Returns `None` if the
	/// nodes within the current `MemoryDB` are insufficient to query the item.
	pub fn query(&self, key_id: KeyTypeId, key_data: &[u8]) -> Option<Item> {
		let trie = TrieDBBuilder::new(&self.db, &self.root).build();
		let val_idx = (key_id, key_data)
			.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| u32::decode(&mut &*raw).ok())?;

		val_idx
			.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| Item::decode(&mut &*raw).ok())
	}

	/// Prove the full verification data for a given key and key ID.
	pub fn prove(&self, key_id: KeyTypeId, key_data: &[u8]) -> Option<Vec<Vec<u8>>> {
		let mut recorder = Recorder::<LayoutV0<Hashing>>::new();
		{
			let trie =
				TrieDBBuilder::new(&self.db, &self.root).with_recorder(&mut recorder).build();
			let val_idx = (key_id, key_data).using_encoded(|s| {
				trie.get(s).ok()?.and_then(|raw| u32::decode(&mut &*raw).ok())
			})?;

			val_idx.using_encoded(|s| {
				trie.get(s).ok()?.and_then(|raw| Item::decode(&mut &*raw).ok())
			})?;
		}

		Some(recorder.drain().into_iter().map(|r| r.data).collect())
	}

	/// Create a new instance of a `ProvingTrie` using a set of raw nodes.
	pub fn from_nodes(root: Hash, nodes: &[Vec<u8>]) -> Self {
		use sp_trie::HashDBT;

		let mut memory_db = MemoryDB::default();
		for node in nodes {
			HashDBT::insert(&mut memory_db, EMPTY_PREFIX, &node[..]);
		}

		ProvingTrie { db: memory_db, root, _phantom: Default::default() }
	}

	/// Create a new instance of a `ProvingTrie` using an iterator of items in the trie.
	pub fn generate_for<I>(items: I) -> Result<Self, &'static str>
	where
		I: IntoIterator<Item = Item>,
	{
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilderV0::new(&mut db, &mut root).build();
			for (i, item) in items.into_iter().enumerate() {
				let i = i as u32;

				// insert each item into the trie
				i.using_encoded(|k| item.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(ProvingTrie { db, root, _phantom: Default::default() })
	}
}

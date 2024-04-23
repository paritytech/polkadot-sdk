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

use sp_state_machine::TrieCacheProvider;
use sp_std::{
	boxed::Box,
	cell::{RefCell, RefMut},
	collections::btree_map::{BTreeMap, Entry},
};
use sp_trie::NodeCodec;
use trie_db::{node::NodeOwned, Hasher};

/// Special purpose trie cache implementation that is able to cache an unlimited number
/// of values. To be used in `validate_block` to serve values and nodes that
/// have already been loaded and decoded from the storage proof.
pub(crate) struct TrieCache<'a, H: Hasher> {
	node_cache: RefMut<'a, BTreeMap<H::Out, NodeOwned<H::Out>>>,
	value_cache: Option<RefMut<'a, BTreeMap<Box<[u8]>, trie_db::CachedValue<H::Out>>>>,
}

impl<'a, H: Hasher> trie_db::TrieCache<NodeCodec<H>> for TrieCache<'a, H> {
	fn lookup_value_for_key(&mut self, key: &[u8]) -> Option<&trie_db::CachedValue<H::Out>> {
		self.value_cache.as_ref().and_then(|cache| cache.get(key))
	}

	fn cache_value_for_key(&mut self, key: &[u8], value: trie_db::CachedValue<H::Out>) {
		self.value_cache.as_mut().and_then(|cache| cache.insert(key.into(), value));
	}

	fn get_or_insert_node(
		&mut self,
		hash: <NodeCodec<H> as trie_db::NodeCodec>::HashOut,
		fetch_node: &mut dyn FnMut() -> trie_db::Result<
			NodeOwned<H::Out>,
			H::Out,
			<NodeCodec<H> as trie_db::NodeCodec>::Error,
		>,
	) -> trie_db::Result<&NodeOwned<H::Out>, H::Out, <NodeCodec<H> as trie_db::NodeCodec>::Error> {
		match self.node_cache.entry(hash) {
			Entry::Occupied(entry) => Ok(entry.into_mut()),
			Entry::Vacant(entry) => Ok(entry.insert(fetch_node()?)),
		}
	}

	fn get_node(
		&mut self,
		hash: &H::Out,
	) -> Option<&NodeOwned<<NodeCodec<H> as trie_db::NodeCodec>::HashOut>> {
		self.node_cache.get(hash)
	}
}

/// Provider of [`TrieCache`] instances.
pub(crate) struct CacheProvider<H: Hasher> {
	node_cache: RefCell<BTreeMap<H::Out, NodeOwned<H::Out>>>,
	/// Cache: `storage_root` => `storage_key` => `value`.
	///
	/// One `block` can for example use multiple tries (child tries) and we need to distinguish the
	/// cached (`storage_key`, `value`) between them. For this we are using the `storage_root` to
	/// distinguish them (even if the storage root is the same for two child tries, it just means
	/// that both are exactly the same trie and there would happen no collision).
	value_cache: RefCell<BTreeMap<H::Out, BTreeMap<Box<[u8]>, trie_db::CachedValue<H::Out>>>>,
}

impl<H: Hasher> CacheProvider<H> {
	/// Constructs a new instance of [`CacheProvider`] with an uninitialized state
	/// and empty node and value caches.
	pub fn new() -> Self {
		CacheProvider { node_cache: Default::default(), value_cache: Default::default() }
	}
}

impl<H: Hasher> TrieCacheProvider<H> for CacheProvider<H> {
	type Cache<'a> = TrieCache<'a, H> where H: 'a;

	fn as_trie_db_cache(&self, storage_root: <H as Hasher>::Out) -> Self::Cache<'_> {
		TrieCache {
			value_cache: Some(RefMut::map(self.value_cache.borrow_mut(), |c| {
				c.entry(storage_root).or_default()
			})),
			node_cache: self.node_cache.borrow_mut(),
		}
	}

	fn as_trie_db_mut_cache(&self) -> Self::Cache<'_> {
		// This method is called when we calculate the storage root.
		// We are not interested in caching new values (as we would throw them away directly after a
		// block is validated) and thus, we don't pass any `value_cache`.
		TrieCache { value_cache: None, node_cache: self.node_cache.borrow_mut() }
	}

	fn merge<'a>(&'a self, _other: Self::Cache<'a>, _new_root: <H as Hasher>::Out) {
		// This is called to merge the `value_cache` from `as_trie_db_mut_cache`, which is not
		// activated, so we don't need to do anything here.
	}
}

// This is safe here since we are single-threaded in WASM
unsafe impl<H: Hasher> Send for CacheProvider<H> {}
unsafe impl<H: Hasher> Sync for CacheProvider<H> {}

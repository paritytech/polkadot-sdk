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

//! Functions for partial state.

use super::{LayoutV1 as Layout, MemoryDB};
use hash_db::{HashDB, Hasher, Prefix, EMPTY_PREFIX};
use sp_core::storage::{well_known_keys, ChildType, PrefixedStorageKey};
use std::collections::{hash_map::Entry, HashMap};
use trie_db::{
	node::{decode_hash, Node, NodeHandle, Value},
	NibbleSlice, NibbleVec, NodeCodec, TrieLayout,
};

/// One chunk of a block's state: the trie nodes of a single verified range proof, as state sync
/// receives them from a peer.
///
/// A range proof carries every node on the paths from the state root down to the keys it covers,
/// so a chunk is complete from the root down, every chunk contains the root, and adjacent chunks
/// share the nodes along their boundary. All chunks of one sync carry the same block and root.
pub struct PartialState<H: Hasher, BlockNumber: Default> {
	/// The block whose state is being synced; keys the walker that consumes the chunks.
	pub block_hash: H::Out,
	pub block_number: BlockNumber,
	/// The root the walk starts from.
	pub state_root: H::Out,
	/// The proof's nodes keyed by hash alone, without database prefixes.
	pub nodes: MemoryDB<H>,
}

/// A received trie node together with the position it has to be stored at.
///
/// `prefix` is the node's database prefix: its nibble path inside the trie, preceded by the child
/// trie keyspace for nodes of a child trie. It is the prefix `sp_state_machine` passes when it
/// reads the node back (`prefix.as_prefix()` gives the `hash_db` form), so a database layer
/// derives the key from `(hash, prefix)` exactly as it does for reads, with no intermediate
/// prefixed map.
pub struct PrefixedTrieNode<H: Hasher> {
	/// Node hash.
	pub hash: H::Out,
	/// Database prefix, see the struct docs.
	pub prefix: NibbleVec,
	/// Encoded node.
	pub encoded: Vec<u8>,
}

/// Walks a block's state trie as its chunks arrive.
///
/// Only the root is known at first. A received node is decoded to learn its children, which are
/// awaited in turn. `import` feeds a chunk and returns the nodes that became writable, each
/// exactly once, so a reference-counting database never sees a node twice. `from_storage`
/// recovers the walk from a database that already holds part of the state, which is why nothing
/// about a walker needs to be persisted.
pub struct PartialStateWalker<H: Hasher> {
	/// The frontier: nodes whose parent has been seen and which have not. Empty once the walk is
	/// complete, and small while it runs, since chunks arrive in key order.
	pub nodes: Vec<TrieNodeInfo<H>>,
}

impl<H: Hasher> PartialStateWalker<H> {
	/// A walker that awaits the root: nothing of the state has been seen yet.
	pub fn new(root: H::Out) -> Self {
		Self { nodes: vec![TrieNodeInfo::root(root)] }
	}

	/// A walker for a state that `storage` already holds in part: descend from the root through
	/// the present nodes and await the absent ones. `storage` reads a node by hash and database
	/// prefix. On an empty database this is one read and equals `new`.
	pub fn from_storage(
		root: H::Out,
		mut storage: impl FnMut(&H::Out, Prefix) -> Option<Vec<u8>>,
	) -> Result<Self, Error> {
		let mut walker = Self { nodes: Vec::new() };
		let mut queue = vec![TrieNodeInfo::root(root)];
		while let Some(info) = queue.pop() {
			match storage(&info.hash, info.db_prefix()?.as_prefix()) {
				Some(encoded) => queue.extend(info.children(&encoded)?),
				None => walker.nodes.push(info),
			}
		}
		Ok(walker)
	}

	/// Is trie traversal completed.
	pub fn is_completed(&self) -> bool {
		self.nodes.is_empty()
	}

	/// Feed a chunk; returns the nodes that became writable, with their database prefix.
	///
	/// A chunk is complete from the root down, so one pass consumes it: every awaited node it
	/// contains is written and its children are looked up in the same chunk. Nodes it repeats
	/// from earlier chunks are not awaited and are skipped. On `Err` the walker is unusable: drop
	/// it and rebuild it with `from_storage`.
	pub fn import<BlockNumber: Default>(
		&mut self,
		chunk: PartialState<H, BlockNumber>,
	) -> Result<Vec<PrefixedTrieNode<H>>, Error> {
		let mut written = Vec::new();
		// The frontier is taken, not cloned; nodes still awaited are pushed back as they are met.
		let mut queue = std::mem::take(&mut self.nodes);
		while let Some(info) = queue.pop() {
			// The chunk is keyed by hash alone: proof prefixes differ from database prefixes.
			match chunk.nodes.get(&info.hash, EMPTY_PREFIX) {
				Some(encoded) => {
					queue.extend(info.children(&encoded)?);
					written.push(PrefixedTrieNode {
						hash: info.hash,
						prefix: info.db_prefix()?,
						encoded,
					});
				},
				None => self.nodes.push(info),
			}
		}
		Ok(written)
	}
}

/// The walkers of the blocks whose state is being synced, keyed by block hash.
///
/// Shared by sc-client-db and the in-memory backend. A walker is built from the caller's storage
/// on its block's first chunk and dropped on any failure, so the map never holds a walker that is
/// out of step with the storage.
pub struct PartialStateWalkers<H: Hasher>(HashMap<H::Out, PartialStateWalker<H>>);

impl<H: Hasher> Default for PartialStateWalkers<H> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<H: Hasher> PartialStateWalkers<H> {
	/// Feed a chunk to the walker of its block, see [`PartialStateWalker::import`].
	///
	/// `storage` reads a node of the caller's database by hash and prefix; it builds the walker
	/// on the block's first chunk. On `Err` the block has no walker any more.
	pub fn import<BlockNumber: Default>(
		&mut self,
		chunk: PartialState<H, BlockNumber>,
		storage: impl FnMut(&H::Out, Prefix) -> Option<Vec<u8>>,
	) -> Result<Vec<PrefixedTrieNode<H>>, Error> {
		let block_hash = chunk.block_hash;
		let walker = match self.0.entry(block_hash) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => {
				entry.insert(PartialStateWalker::from_storage(chunk.state_root, storage)?)
			},
		};
		let result = walker.import(chunk);
		// A failed walker is dropped, not rolled back: the next chunk rebuilds it from storage.
		if result.is_err() {
			self.0.remove(&block_hash);
		}
		result
	}

	/// Forget the walker of `block_hash`, if any. Returns whether there was one.
	pub fn remove(&mut self, block_hash: &H::Out) -> bool {
		self.0.remove(block_hash).is_some()
	}
}

/// Error for partial state.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Error {
	HashSizeMismatch,
	TrieDecodeError,
	ChildStorageRootMustBeInlineValue,
	UnknownChildStorageType,
}

fn as_hash<H: Hasher>(input: &[u8]) -> Result<H::Out, Error> {
	decode_hash::<H>(input).ok_or_else(|| Error::HashSizeMismatch)
}

/// Describes position and role of trie node.
pub struct TrieNodeInfo<H: Hasher> {
	/// Trie prefix inside top or child trie.
	/// Used to detect child trie by ":child_storage:" prefix.
	/// Part of database key.
	pub trie_prefix: NibbleVec,
	/// Node hash.
	/// Part of database key.
	pub hash: H::Out,
	/// Is this detached value or trie node.
	pub is_detached_value: bool,
	/// ":child_storage:" prefixed key in parent trie for this child trie.
	/// Name after ":child_storage:default:" is part of database key.
	pub child_trie_key: Option<Vec<u8>>,
}

impl<H: Hasher> TrieNodeInfo<H> {
	/// Make info for root node.
	pub fn root(hash: H::Out) -> Self {
		Self { trie_prefix: NibbleVec::new(), hash, is_detached_value: false, child_trie_key: None }
	}

	/// Get info for node children: branches, detached value and child trie.
	pub fn children(&self, encoded: &[u8]) -> Result<Vec<TrieNodeInfo<H>>, Error> {
		// A detached value is a leaf of the walk: bytes, not a node.
		if self.is_detached_value {
			return Ok(vec![]);
		}
		let node = <Layout<H> as TrieLayout>::Codec::decode(&mut &encoded[..])
			.map_err(|_| Error::TrieDecodeError)?;
		let mut children = vec![];
		// The node's own partial key: appended to this node's path, it gives the position of the
		// value and of the children below.
		let partial = match &node {
			Node::Leaf(partial, _) |
			Node::Extension(partial, _) |
			Node::NibbledBranch(partial, _, _) => Some(NibbleVec::from(*partial)),
			_ => None,
		};
		match &node {
			Node::Leaf(_, value) |
			Node::Branch(_, Some(value)) |
			Node::NibbledBranch(_, _, Some(value)) => {
				let mut trie_prefix = self.trie_prefix.clone();
				if let Some(partial) = &partial {
					trie_prefix.append(partial);
				}
				let key = trie_prefix.as_prefix().0;
				// A value under a `:child_storage:` key of the top trie is a child trie root,
				// stored inline. The child trie is walked from it, under its own keyspace.
				if self.child_trie_key.is_none() && well_known_keys::is_child_storage_key(key) {
					let hash = match value {
						Value::Inline(hash) => hash,
						_ => return Err(Error::ChildStorageRootMustBeInlineValue),
					};
					children.push(TrieNodeInfo {
						trie_prefix: NibbleVec::new(),
						hash: as_hash::<H>(hash)?,
						is_detached_value: false,
						child_trie_key: Some(key.to_vec()),
					});
				// A value longer than 32 bytes lives in its own node under the same prefix.
				} else if let Value::Node(hash) = value {
					children.push(TrieNodeInfo {
						trie_prefix,
						hash: as_hash::<H>(hash)?,
						is_detached_value: true,
						child_trie_key: self.child_trie_key.clone(),
					});
				}
			},
			_ => {},
		}
		// Hashed children of a branch. Inline children are part of this node's bytes and, being
		// shorter than a hash, cannot have hashed descendants of their own.
		match &node {
			Node::Branch(branches, _) | Node::NibbledBranch(_, branches, _) => {
				for (i, branch) in branches.iter().enumerate() {
					if let Some(NodeHandle::Hash(hash)) = branch {
						let mut trie_prefix = self.trie_prefix.clone();
						if let Some(partial) = &partial {
							trie_prefix.append(partial);
						}
						trie_prefix.push(i as u8);
						children.push(TrieNodeInfo {
							trie_prefix,
							hash: as_hash::<H>(hash)?,
							is_detached_value: false,
							child_trie_key: self.child_trie_key.clone(),
						});
					}
				}
			},
			_ => {},
		}
		Ok(children)
	}

	/// Get prefix for database key: a child trie node sits under its child keyspace followed by
	/// its path in the child trie, a top trie node under its path alone.
	pub fn db_prefix(&self) -> Result<NibbleVec, Error> {
		let mut prefix = NibbleVec::new();
		if let Some(key) = &self.child_trie_key {
			match ChildType::from_prefixed_key(PrefixedStorageKey::new_ref(&key)) {
				Some((ChildType::ParentKeyId, key)) => {
					prefix.append(&NibbleVec::from(NibbleSlice::new(key)))
				},
				None => return Err(Error::UnknownChildStorageType),
			}
		}
		prefix.append(&self.trie_prefix);
		Ok(prefix)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		prefixed_key, KeySpacedDBMut, LayoutV1, PrefixedMemoryDB, TrieDBMutBuilder, TrieMut,
	};
	use sp_core::Blake2Hasher;
	use std::collections::BTreeSet;

	type Hash = <Blake2Hasher as Hasher>::Out;

	const CHILD_NAME: &[u8] = b"child";

	/// Builds a top trie with one child trie the way `sp_state_machine` lays them out in the
	/// database, and returns that prefixed database together with the state root.
	fn build_state() -> (PrefixedMemoryDB<Blake2Hasher>, Hash) {
		let mut db = PrefixedMemoryDB::<Blake2Hasher>::default();

		let mut child_root = Hash::default();
		{
			let mut keyspaced = KeySpacedDBMut::new(&mut db, CHILD_NAME);
			let mut trie =
				TrieDBMutBuilder::<LayoutV1<Blake2Hasher>>::new(&mut keyspaced, &mut child_root)
					.build();
			trie.insert(b"a", b"1").unwrap();
			// Longer than 32 bytes: stored as a detached value node under LayoutV1.
			trie.insert(b"b", &[7u8; 64]).unwrap();
			trie.commit();
		}

		let mut root = Hash::default();
		{
			let mut trie =
				TrieDBMutBuilder::<LayoutV1<Blake2Hasher>>::new(&mut db, &mut root).build();
			trie.insert(b"key0", &[0u8; 40]).unwrap();
			trie.insert(b"key1", b"short").unwrap();
			let mut child_key = well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX.to_vec();
			child_key.extend_from_slice(CHILD_NAME);
			trie.insert(&child_key, child_root.as_ref()).unwrap();
			trie.commit();
		}
		(db, root)
	}

	/// Keys under which `db` holds its nodes: the oracle for the walker's prefixes.
	fn db_keys(db: &PrefixedMemoryDB<Blake2Hasher>) -> BTreeSet<Vec<u8>> {
		db.clone()
			.drain()
			.into_iter()
			.filter(|(_, (_, rc))| *rc > 0)
			.map(|(key, _)| key)
			.collect()
	}

	fn all_nodes(db: &PrefixedMemoryDB<Blake2Hasher>) -> Vec<Vec<u8>> {
		db.clone()
			.drain()
			.into_values()
			.filter(|(_, rc)| *rc > 0)
			.map(|(encoded, _)| encoded)
			.collect()
	}

	/// The nodes as a state-sync chunk: a `MemoryDB` keyed by hash only, no prefixes.
	fn chunk(nodes: Vec<Vec<u8>>, root: Hash) -> PartialState<Blake2Hasher, u64> {
		let mut db = MemoryDB::<Blake2Hasher>::default();
		for encoded in nodes {
			db.insert(EMPTY_PREFIX, &encoded);
		}
		PartialState { block_hash: root, block_number: 0, state_root: root, nodes: db }
	}

	fn written_keys(nodes: &[PrefixedTrieNode<Blake2Hasher>]) -> BTreeSet<Vec<u8>> {
		nodes
			.iter()
			.map(|node| prefixed_key::<Blake2Hasher>(&node.hash, node.prefix.as_prefix()))
			.collect()
	}

	#[test]
	fn import_recovers_database_prefixes() {
		let (db, root) = build_state();
		let expected = db_keys(&db);

		let mut walker = PartialStateWalker::<Blake2Hasher>::new(root);
		let written = walker.import(chunk(all_nodes(&db), root)).unwrap();

		assert_eq!(written_keys(&written), expected);
		assert_eq!(written.len(), expected.len(), "every position is written exactly once");
		assert!(walker.is_completed());
	}

	#[test]
	fn import_resumes_across_chunks() {
		let (db, root) = build_state();
		let expected = db_keys(&db);
		let root_node = db.get(&root, EMPTY_PREFIX).unwrap();

		let mut walker = PartialStateWalker::<Blake2Hasher>::new(root);
		let mut written = walker.import(chunk(vec![root_node], root)).unwrap();
		assert_eq!(written.len(), 1);
		assert!(!walker.is_completed());

		// The second chunk re-sends the root, as a range proof does: it must not be written again.
		written.extend(walker.import(chunk(all_nodes(&db), root)).unwrap());

		assert_eq!(written_keys(&written), expected);
		assert_eq!(written.len(), expected.len(), "the root is written once");
		assert!(walker.is_completed());
	}

	/// Keys the walker still waits for, as `sp_state_machine` would read them.
	fn awaited_keys(walker: &PartialStateWalker<Blake2Hasher>) -> BTreeSet<Vec<u8>> {
		walker
			.nodes
			.iter()
			.map(|info| {
				prefixed_key::<Blake2Hasher>(&info.hash, info.db_prefix().unwrap().as_prefix())
			})
			.collect()
	}

	#[test]
	fn from_storage_awaits_what_the_database_lacks() {
		let (db, root) = build_state();
		let root_node = db.get(&root, EMPTY_PREFIX).unwrap();

		// A sync that wrote the root and stopped.
		let mut live = PartialStateWalker::<Blake2Hasher>::new(root);
		let written = live.import(chunk(vec![root_node], root)).unwrap();
		let mut partial_db = PrefixedMemoryDB::<Blake2Hasher>::default();
		for node in &written {
			partial_db.emplace(node.hash, node.prefix.as_prefix(), node.encoded.clone());
		}

		// Rebuilt from that database, the walker waits for exactly what the live one does.
		let mut rebuilt = PartialStateWalker::<Blake2Hasher>::from_storage(root, |hash, prefix| {
			partial_db.get(hash, prefix)
		})
		.unwrap();
		assert_eq!(awaited_keys(&rebuilt), awaited_keys(&live));

		// An empty database gives a fresh walker, a complete one a finished walker.
		let empty = PrefixedMemoryDB::<Blake2Hasher>::default();
		let fresh = PartialStateWalker::<Blake2Hasher>::from_storage(root, |hash, prefix| {
			empty.get(hash, prefix)
		})
		.unwrap();
		assert_eq!(awaited_keys(&fresh), awaited_keys(&PartialStateWalker::new(root)));
		let complete = PartialStateWalker::<Blake2Hasher>::from_storage(root, |hash, prefix| {
			db.get(hash, prefix)
		})
		.unwrap();
		assert!(complete.is_completed());

		// The rest of the state goes through the rebuilt walker without writing the root again.
		let rest = rebuilt.import(chunk(all_nodes(&db), root)).unwrap();
		let mut all = written_keys(&written);
		all.extend(written_keys(&rest));
		assert_eq!(all, db_keys(&db));
		assert_eq!(written.len() + rest.len(), db_keys(&db).len(), "no node is written twice");
		assert!(rebuilt.is_completed());
	}

	#[test]
	fn failed_import_drops_the_walker() {
		let (db, root) = build_state();
		let root_node = db.get(&root, EMPTY_PREFIX).unwrap();
		let no_storage = |_: &Hash, _: Prefix| None;

		let mut walkers = PartialStateWalkers::<Blake2Hasher>::default();
		walkers.import(chunk(vec![root_node.clone()], root), no_storage).unwrap();

		// Garbage under an awaited hash: decoding fails and the block's walker is gone.
		let mut live = PartialStateWalker::<Blake2Hasher>::new(root);
		live.import(chunk(vec![root_node], root)).unwrap();
		let awaited = live.nodes[0].hash;
		let mut corrupt = chunk(vec![], root);
		corrupt.nodes.emplace(awaited, EMPTY_PREFIX, vec![0xff; 3]);
		assert!(matches!(walkers.import(corrupt, no_storage), Err(Error::TrieDecodeError)));
		assert!(!walkers.remove(&root), "the walker was dropped with the error");

		// The next chunk builds a walker again; with no storage behind it, the root is rewritten.
		let written = walkers.import(chunk(all_nodes(&db), root), no_storage).unwrap();
		assert_eq!(written_keys(&written), db_keys(&db));
	}
}

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

//! Writing a state sync chunk into the database: which nodes to store, and under which prefix.

use super::{LayoutV1 as Layout, MemoryDB};
use hash_db::{HashDB, Hasher, Prefix, EMPTY_PREFIX};
use sp_core::storage::{well_known_keys, ChildType, PrefixedStorageKey};
use std::{cmp::Ordering, collections::HashMap};
use trie_db::{
	node::{decode_hash, Node, NodeHandle, Value},
	NibbleSlice, NibbleVec, NodeCodec, TrieLayout,
};

/// One chunk of a block's state: the nodes of one verified range proof, as state sync gets them
/// from a peer, plus the range the proof covers.
///
/// Keys are in nested sync order: top trie keys in order, with a child trie's keys right after
/// the child's own top trie key. A cursor in that order is a top trie key, optionally followed by
/// a key inside that child trie.
pub struct PartialState<H: Hasher> {
	/// The block whose state is being synced.
	pub block_hash: H::Out,
	/// Root of that block's state trie. Every chunk contains it, since a range proof carries all
	/// nodes from the root down to the keys it covers.
	pub state_root: H::Out,
	/// The proof's nodes, keyed by hash only, no database prefixes.
	pub nodes: MemoryDB<H>,
	/// Cursor the range was requested from, exclusive. Empty for the first chunk.
	pub start: Vec<Vec<u8>>,
	/// Cursor the range ends at, inclusive, where the next request starts. `None` on the final
	/// chunk, which means the state is complete.
	pub end: Option<Vec<Vec<u8>>>,
	/// Child tries whose content the proof does not carry because another key has the same root
	/// and the server sends a root's content only once per request. `(key, source key)` pairs:
	/// the writer copies the source's nodes.
	pub child_trie_copies: Vec<(Vec<u8>, Vec<u8>)>,
}

/// A node to write, with the database prefix it is stored under.
pub struct PrefixedTrieNode<H: Hasher> {
	/// Node hash.
	pub hash: H::Out,
	/// The node's path in its trie, preceded by the child trie keyspace for a child trie node.
	/// This is the same prefix `sp_state_machine` passes when it reads the node back
	/// (`prefix.as_prefix()` gives the `hash_db` form), so the database builds the key from
	/// `(hash, prefix)` exactly as it does for reads.
	pub prefix: NibbleVec,
	/// Encoded node.
	pub encoded: Vec<u8>,
}

/// The nodes of `chunk` to write, each with its database prefix. Nodes come from the chunk
/// itself, or from `storage` for a child trie that is a copy of one an earlier chunk wrote.
/// Across a whole download, every node position is written exactly once.
pub fn chunk_nodes<H: Hasher>(
	chunk: &PartialState<H>,
	mut storage: impl FnMut(&H::Out, Prefix) -> Option<Vec<u8>>,
) -> Result<Vec<PrefixedTrieNode<H>>, Error> {
	let copies: HashMap<&[u8], &Vec<u8>> = chunk
		.child_trie_copies
		.iter()
		.map(|(key, source)| (key.as_slice(), source))
		.collect();
	let mut written = Vec::new();
	let mut stack = vec![TrieNodeInfo::<H>::root(chunk.state_root)];
	while let Some(info) = stack.pop() {
		// If this node belongs to a copied child trie, this is where the source trie holds it.
		let source = match info.child_trie_key.as_deref().and_then(|key| copies.get(key)) {
			Some(source) => Some(db_prefix(Some(source), &info.trie_prefix)?),
			None => None,
		};
		// The chunk keys its nodes by hash alone, so look them up without a prefix. A node of a
		// copied child trie is not in the chunk at all: the server sends a root's content once per
		// request, and the source key comes first in key order. So we read it from what the
		// earlier chunk wrote under the source keyspace.
		let Some(encoded) = chunk
			.nodes
			.get(&info.hash, EMPTY_PREFIX)
			.or_else(|| storage(&info.hash, source?.as_prefix()))
		else {
			// Not in this chunk and not a copy. The range that covers it brings it.
			continue;
		};
		let (subtree, children) = info.expand(&encoded)?;
		let child_trie_key = info.child_trie_key.as_deref();
		// The range rule: a node is written by the chunk whose range contains the first key under
		// it. The ranges of a download partition the keys, so that is exactly one chunk, and each
		// position gets written once. That matters on parity-db, where a second write of the same
		// node counts as a second reference.
		match placement(child_trie_key, &subtree, &chunk.start) {
			// Everything under it comes before the range, so an earlier chunk wrote it. The proof
			// still carries such nodes because the server seeks to the range start.
			Placement::Before => continue,
			// An ancestor of `start`: the subtree starts at or before it and reaches past it. The
			// node itself was written before, but children may start inside the range.
			Placement::Ancestor => {
				stack.extend(children);
				continue;
			},
			Placement::After => {},
		}
		// Past the end of the range: the next chunk writes it.
		if chunk
			.end
			.as_deref()
			.is_some_and(|end| placement(child_trie_key, &subtree, end) == Placement::After)
		{
			continue;
		}
		written.push(PrefixedTrieNode { hash: info.hash, prefix: info.db_prefix()?, encoded });
		stack.extend(children);
	}
	Ok(written)
}

/// Where a node's subtree lies relative to a cursor, in nested sync order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
	/// Every key under the node comes before the cursor.
	Before,
	/// The cursor is under the node, or is the node's own key.
	Ancestor,
	/// Every key under the node comes after the cursor.
	After,
}

/// Where the subtree at `subtree` lies relative to `cursor`. `child_trie_key` is the top trie key
/// of the child trie the node belongs to, `None` for a top trie node.
fn placement(child_trie_key: Option<&[u8]>, subtree: &NibbleVec, cursor: &[Vec<u8>]) -> Placement {
	// An empty cursor is the start of the state: everything comes after it.
	let Some(top) = cursor.first() else { return Placement::After };
	// A top trie node only looks at the top key, even when the cursor goes on into a child trie:
	// all of that child's keys sit right after the top key anyway.
	let Some(child_trie_key) = child_trie_key else {
		return compare(subtree, &NibbleSlice::new(top));
	};
	// A child trie node: the top keys decide first. A cursor that stops at the child's own key
	// counts as after the whole child trie. The request that read that key went on into the child
	// trie, so a request starting at the key continues in the top trie.
	match (child_trie_key.cmp(top.as_slice()), cursor.get(1)) {
		(Ordering::Less, _) | (Ordering::Equal, None) => Placement::Before,
		(Ordering::Greater, _) => Placement::After,
		(Ordering::Equal, Some(child)) => compare(subtree, &NibbleSlice::new(child)),
	}
}

/// `subtree` against the nibbles of a key in the same trie.
fn compare(subtree: &NibbleVec, key: &NibbleSlice) -> Placement {
	for i in 0..subtree.len().min(key.len()) {
		match subtree.at(i).cmp(&key.at(i)) {
			Ordering::Less => return Placement::Before,
			Ordering::Greater => return Placement::After,
			Ordering::Equal => {},
		}
	}
	// Equal so far. Either the key goes on under the node (or ends right at it), which makes the
	// node an ancestor, or the node sits below the key and so comes after it.
	if subtree.len() <= key.len() {
		Placement::Ancestor
	} else {
		Placement::After
	}
}

/// What can go wrong while decoding a chunk's nodes.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Error {
	/// A node references a child or a value by a hash of the wrong size.
	HashSizeMismatch,
	/// A trie node does not decode under the layout.
	TrieDecodeError,
	/// A child trie root under a `:child_storage:` key is not an inline value.
	ChildStorageRootMustBeInlineValue,
	/// A `:child_storage:` key is of a child trie type that is not known.
	UnknownChildStorageType,
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::HashSizeMismatch => "a node references a hash of the wrong size",
			Self::TrieDecodeError => "a trie node does not decode",
			Self::ChildStorageRootMustBeInlineValue => "a child trie root is not an inline value",
			Self::UnknownChildStorageType => "a child trie key is of an unknown type",
		})
	}
}

impl std::error::Error for Error {}

fn as_hash<H: Hasher>(input: &[u8]) -> Result<H::Out, Error> {
	decode_hash::<H>(input).ok_or_else(|| Error::HashSizeMismatch)
}

/// A node's position in the trie, for the walk and for the database key.
pub struct TrieNodeInfo<H: Hasher> {
	/// Nibble path to the node inside its trie, top or child. Part of the database key.
	pub trie_prefix: NibbleVec,
	/// Node hash. Part of the database key.
	pub hash: H::Out,
	/// A detached value, stored under its key's prefix, rather than a trie node.
	pub is_detached_value: bool,
	/// For a child trie node, the `:child_storage:` key of that child trie in the top trie. The
	/// part after `:child_storage:default:` becomes the keyspace in the database key.
	pub child_trie_key: Option<Vec<u8>>,
}

impl<H: Hasher> TrieNodeInfo<H> {
	/// Info for the root of the top trie.
	pub fn root(hash: H::Out) -> Self {
		Self { trie_prefix: NibbleVec::new(), hash, is_detached_value: false, child_trie_key: None }
	}

	/// Decode the node. Returns its subtree prefix, which every key under it starts with, and the
	/// nodes to visit next: hashed branch children, a detached value, or a child trie root.
	pub fn expand(&self, encoded: &[u8]) -> Result<(NibbleVec, Vec<TrieNodeInfo<H>>), Error> {
		// A detached value is just bytes and has nothing below it. Its position is its key's.
		if self.is_detached_value {
			return Ok((self.trie_prefix.clone(), vec![]));
		}
		let node = <Layout<H> as TrieLayout>::Codec::decode(&mut &encoded[..])
			.map_err(|_| Error::TrieDecodeError)?;
		// The node's own partial key extends its path. Its value and its children sit under that.
		let mut subtree = self.trie_prefix.clone();
		match &node {
			Node::Leaf(partial, _) |
			Node::Extension(partial, _) |
			Node::NibbledBranch(partial, _, _) => subtree.append(&NibbleVec::from(*partial)),
			_ => {},
		}
		let mut children = vec![];
		match &node {
			Node::Leaf(_, value) |
			Node::Branch(_, Some(value)) |
			Node::NibbledBranch(_, _, Some(value)) => {
				let key = subtree.as_prefix().0;
				// A value at a `:child_storage:` key of the top trie is a child trie root, stored
				// inline. We walk the child trie from it, under its own keyspace.
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
				// A value longer than 32 bytes lives in a node of its own, under the same prefix.
				} else if let Value::Node(hash) = value {
					children.push(TrieNodeInfo {
						trie_prefix: subtree.clone(),
						hash: as_hash::<H>(hash)?,
						is_detached_value: true,
						child_trie_key: self.child_trie_key.clone(),
					});
				}
			},
			_ => {},
		}
		// Hashed children of a branch. Inline children are part of this node's bytes, and being
		// shorter than a hash they cannot have hashed descendants of their own.
		match &node {
			Node::Branch(branches, _) | Node::NibbledBranch(_, branches, _) => {
				for (i, branch) in branches.iter().enumerate() {
					if let Some(NodeHandle::Hash(hash)) = branch {
						let mut trie_prefix = subtree.clone();
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
		Ok((subtree, children))
	}

	/// The node's database prefix: the child keyspace followed by its path for a child trie node,
	/// its path alone for a top trie node.
	pub fn db_prefix(&self) -> Result<NibbleVec, Error> {
		db_prefix(self.child_trie_key.as_ref(), &self.trie_prefix)
	}
}

/// Database prefix of a node at `trie_prefix` in the child trie with top key `child_trie_key`,
/// or in the top trie for `None`. A free function because copying a child trie needs a node's
/// prefix under another keyspace than its own.
fn db_prefix(
	child_trie_key: Option<&Vec<u8>>,
	trie_prefix: &NibbleVec,
) -> Result<NibbleVec, Error> {
	let mut prefix = NibbleVec::new();
	if let Some(key) = child_trie_key {
		match ChildType::from_prefixed_key(PrefixedStorageKey::new_ref(key)) {
			Some((ChildType::ParentKeyId, key)) => {
				prefix.append(&NibbleVec::from(NibbleSlice::new(key)))
			},
			None => return Err(Error::UnknownChildStorageType),
		}
	}
	prefix.append(trie_prefix);
	Ok(prefix)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		KeySpacedDB, KeySpacedDBMut, LayoutV1, PrefixedMemoryDB, Recorder, Trie, TrieDBBuilder,
		TrieDBMutBuilder, TrieMut,
	};
	use sp_core::Blake2Hasher;
	use std::collections::BTreeSet;

	type Hash = <Blake2Hasher as Hasher>::Out;
	type Chunk = PartialState<Blake2Hasher>;

	/// Two child tries with the same content. Their top trie leaves are distinct nodes, one-letter
	/// names would give two identical leaves with a single hash.
	const TWINS: [&[u8]; 2] = [b"alpha", b"bravo"];
	/// The content of every child trie. `b`, `c` and `d` are longer than 32 bytes, so under
	/// LayoutV1 each is a detached value below its own leaf: three subtrees a range read can stop
	/// between. `a` is inline in the root.
	const CHILD_CONTENT: [(&[u8], &[u8]); 4] =
		[(b"a", b"1"), (b"b", &[7u8; 64]), (b"c", &[8u8; 64]), (b"d", &[9u8; 64])];

	/// The top trie key of the child trie called `name`.
	fn child_key(name: &[u8]) -> Vec<u8> {
		let mut key = well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX.to_vec();
		key.extend_from_slice(name);
		key
	}

	/// A top trie with one child trie per name in `children`, all with the same content, laid out
	/// in the database as `sp_state_machine` does it. Returns the database, the state root and the
	/// shared child root.
	fn build_state(children: &[&[u8]]) -> (PrefixedMemoryDB<Blake2Hasher>, Hash, Hash) {
		let mut db = PrefixedMemoryDB::<Blake2Hasher>::default();

		let mut child_root = Hash::default();
		for name in children {
			let mut keyspaced = KeySpacedDBMut::new(&mut db, name);
			let mut trie =
				TrieDBMutBuilder::<LayoutV1<Blake2Hasher>>::new(&mut keyspaced, &mut child_root)
					.build();
			for (key, value) in CHILD_CONTENT {
				trie.insert(key, value).unwrap();
			}
			trie.commit();
		}

		let mut root = Hash::default();
		{
			let mut trie =
				TrieDBMutBuilder::<LayoutV1<Blake2Hasher>>::new(&mut db, &mut root).build();
			trie.insert(b"key0", &[0u8; 40]).unwrap();
			trie.insert(b"key1", b"short").unwrap();
			for name in children {
				trie.insert(&child_key(name), child_root.as_ref()).unwrap();
			}
			trie.commit();
		}
		(db, root, child_root)
	}

	/// Keys under which `db` holds its nodes: the oracle for the prefixes and for writing each
	/// position once.
	fn db_keys(db: &PrefixedMemoryDB<Blake2Hasher>) -> BTreeSet<Vec<u8>> {
		db.clone()
			.drain()
			.into_iter()
			.filter(|(_, (_, rc))| *rc > 0)
			.map(|(key, _)| key)
			.collect()
	}

	/// The nodes a read of `keys` touches, as a range proof carries them: in the top trie for a
	/// `keyspace` of `None`, else in the child trie under `keyspace`, `root` being its root.
	fn read_nodes(
		db: &PrefixedMemoryDB<Blake2Hasher>,
		root: &Hash,
		keyspace: Option<&[u8]>,
		keys: &[&[u8]],
	) -> Vec<Vec<u8>> {
		let mut recorder = Recorder::<LayoutV1<Blake2Hasher>>::new();
		match keyspace {
			Some(keyspace) => {
				let keyspaced = KeySpacedDB::new(db, keyspace);
				let trie = TrieDBBuilder::<LayoutV1<Blake2Hasher>>::new(&keyspaced, root)
					.with_recorder(&mut recorder)
					.build();
				for key in keys {
					trie.get(key).unwrap();
				}
			},
			None => {
				let trie = TrieDBBuilder::<LayoutV1<Blake2Hasher>>::new(db, root)
					.with_recorder(&mut recorder)
					.build();
				for key in keys {
					trie.get(key).unwrap();
				}
			},
		}
		recorder.drain().into_iter().map(|record| record.data).collect()
	}

	/// The nodes as a state-sync chunk: a `MemoryDB` keyed by hash only, no prefixes.
	fn chunk(
		nodes: Vec<Vec<u8>>,
		root: Hash,
		start: &[&[u8]],
		end: Option<&[&[u8]]>,
		child_trie_copies: &[(&[u8], &[u8])],
	) -> Chunk {
		let mut db = MemoryDB::<Blake2Hasher>::default();
		for encoded in nodes {
			db.insert(EMPTY_PREFIX, &encoded);
		}
		let cursor = |keys: &[&[u8]]| keys.iter().map(|key| key.to_vec()).collect();
		PartialState {
			block_hash: root,
			state_root: root,
			nodes: db,
			start: cursor(start),
			end: end.map(cursor),
			child_trie_copies: child_trie_copies
				.iter()
				.map(|(key, source)| (key.to_vec(), source.to_vec()))
				.collect(),
		}
	}

	/// Writes what `chunk_nodes` returns into `storage`, as a backend does, and returns how many
	/// nodes it wrote.
	fn import(chunk: &Chunk, storage: &mut PrefixedMemoryDB<Blake2Hasher>) -> usize {
		let written = chunk_nodes(chunk, |hash, prefix| storage.get(hash, prefix)).unwrap();
		for node in &written {
			storage.emplace(node.hash, node.prefix.as_prefix(), node.encoded.clone());
		}
		written.len()
	}

	fn nibbles(key: &[u8]) -> NibbleVec {
		NibbleVec::from(NibbleSlice::new(key))
	}

	#[test]
	fn placement_follows_nested_sync_order() {
		let t = b"key".to_vec();
		let alpha = child_key(b"alpha");
		let c = b"c".to_vec();

		// Nothing read yet: everything is after.
		assert_eq!(placement(None, &nibbles(b"key"), &[]), Placement::After);
		assert_eq!(placement(Some(&alpha), &nibbles(b"c"), &[]), Placement::After);

		// Top trie nodes against a top trie key.
		assert_eq!(placement(None, &nibbles(b"ke"), &[t.clone()]), Placement::Ancestor);
		assert_eq!(placement(None, &nibbles(b"key"), &[t.clone()]), Placement::Ancestor);
		assert_eq!(placement(None, &nibbles(b"key0"), &[t.clone()]), Placement::After);
		assert_eq!(placement(None, &nibbles(b"kex"), &[t.clone()]), Placement::Before);
		assert_eq!(placement(None, &nibbles(b"kez"), &[t.clone()]), Placement::After);

		// Child trie nodes: the top keys decide first, and a cursor at the child's own key stands
		// after the whole child trie.
		assert_eq!(placement(Some(&alpha), &nibbles(b"c"), &[t.clone()]), Placement::Before);
		assert_eq!(placement(Some(&alpha), &nibbles(b"c"), &[child_key(b"a")]), Placement::After);
		assert_eq!(placement(Some(&alpha), &nibbles(b"c"), &[alpha.clone()]), Placement::Before);
		assert_eq!(
			placement(Some(&alpha), &nibbles(b"c"), &[alpha.clone(), c.clone()]),
			Placement::Ancestor
		);
		assert_eq!(
			placement(Some(&alpha), &nibbles(b"b"), &[alpha.clone(), c.clone()]),
			Placement::Before
		);
		assert_eq!(
			placement(Some(&alpha), &nibbles(b"d"), &[alpha.clone(), c.clone()]),
			Placement::After
		);
		// A top trie node against a cursor inside a child trie: the top key alone counts.
		assert_eq!(
			placement(None, &nibbles(b"key"), &[alpha.clone(), c.clone()]),
			Placement::After
		);
		assert_eq!(placement(None, &nibbles(b":child_storage:"), &[alpha, c]), Placement::Ancestor);
	}

	/// A range that ends at the copied child's key covers its content, and the copy is made.
	#[test]
	fn child_key_at_the_end_keeps_its_content() {
		let (db, root, child_root) = build_state(&TWINS);
		let [alpha, bravo] = TWINS;
		let (alpha_key, bravo_key) = (child_key(alpha), child_key(bravo));
		let mut storage = PrefixedMemoryDB::<Blake2Hasher>::default();

		let top_keys: [&[u8]; 4] = [&alpha_key, &bravo_key, b"key0", b"key1"];
		let child_keys: [&[u8]; 4] = [b"a", b"b", b"c", b"d"];
		let copies = [(bravo_key.as_slice(), alpha_key.as_slice())];
		let read = || {
			let mut nodes = read_nodes(&db, &root, None, &top_keys);
			nodes.extend(read_nodes(&db, &child_root, Some(alpha), &child_keys));
			nodes
		};
		let cut = chunk(read(), root, &[], Some(&[&bravo_key]), &copies);
		import(&cut, &mut storage);

		let under =
			|name: &[u8]| db_keys(&storage).into_iter().filter(|key| key.starts_with(name)).count();
		assert_eq!(under(alpha), under(bravo), "bravo got every node alpha has");
		assert!(under(bravo) > 0);

		// The same proof as one unbounded range writes every position. What the cut range left
		// out lies past bravo's key, in the top trie.
		let mut whole = PrefixedMemoryDB::<Blake2Hasher>::default();
		import(&chunk(read(), root, &[], None, &copies), &mut whole);
		assert_eq!(db_keys(&whole), db_keys(&db));
		let past_the_end: Vec<_> =
			db_keys(&whole).difference(&db_keys(&storage)).cloned().collect();
		assert!(!past_the_end.is_empty(), "the keys after bravo's are left to the next range");
		assert!(
			past_the_end
				.iter()
				.all(|key| !key.starts_with(alpha) && !key.starts_with(bravo)),
			"nothing of the twins is left out: {past_the_end:?}"
		);
	}
}

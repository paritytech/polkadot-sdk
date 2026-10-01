//! Functions for partial state.

use super::{LayoutV1 as Layout, MemoryDB};
use codec::{Decode, Encode, Input, Output};
use hash_db::{HashDB, Hasher, EMPTY_PREFIX};
use sp_storage::{well_known_keys, ChildType, PrefixedStorageKey};
use std::collections::HashMap;
use trie_db::{
	node::{decode_hash, Node, NodeHandle, Value},
	NibbleSlice, NibbleVec, NodeCodec, TrieLayout,
};

/// Subset of nodes allowing reading part of state trie.
/// This state belongs to block with specified hash and number.
pub struct PartialState<H: Hasher, BlockNumber: Default> {
	pub block_hash: H::Out,
	pub block_number: BlockNumber,
	pub state_root: H::Out,
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

/// Traverse trie nodes asynchronously.
///
/// In the beginnning, state root hash is only piece of information known.
/// When node with that hash is received, this node can be decoded to learn it's children node
/// hashes. This class stores list of not yet received nodes, starting from root node.
/// When node is received, it is removed from that list, and it's children are added to that list.
/// `PartialStatePrefixer::import` behaves like recursive coroutine, traversing trie, and waiting
/// for nodes to be received.
///
/// Used by database to make `import_partial_state` idempotent, preventing double increment if
/// reference counting database is used.
#[derive(Clone, Decode, Encode)]
pub struct PartialStatePrefixer<H: Hasher> {
	pub nodes: Vec<TrieNodeInfo<H>>,
}

impl<H: Hasher> PartialStatePrefixer<H> {
	/// Start trie traversal from root.
	pub fn new(root: H::Out) -> Self {
		Self { nodes: vec![TrieNodeInfo::root(root)] }
	}

	/// Is trie traversal completed.
	pub fn is_completed(&self) -> bool {
		self.nodes.is_empty()
	}

	/// Resume trie traversal using nodes from partial state.
	/// Returns every node that became writable, with its database prefix.
	pub fn import<BlockNumber: Default>(
		&mut self,
		partial_state: PartialState<H, BlockNumber>,
	) -> Result<Vec<PrefixedTrieNode<H>>, Error> {
		let mut result = Vec::new();
		// resume recursive trie traversal
		let mut nodes_queue = self.nodes.clone();
		let mut nodes_after: Vec<TrieNodeInfo<H>> = vec![];
		while let Some(info) = nodes_queue.pop() {
			// intentionally using empty prefix, MemoryDB doesn't use prefixes.
			// also prefixes are inconsistent between compact encoded proof and database,
			// database prepends child storage name to prefix,
			// compact encoded proof doesn't.
			if let Some(encoded) = partial_state.nodes.get(&info.hash, EMPTY_PREFIX) {
				// node received, traverse it's children
				nodes_queue.extend(info.children(&encoded)?);
				// write this node to db
				result.push(PrefixedTrieNode {
					hash: info.hash,
					prefix: info.db_prefix()?,
					encoded,
				});
			} else {
				// wait for this node to be received
				nodes_after.push(info);
			}
		}
		self.nodes = nodes_after;
		Ok(result)
	}
}

/// Reused by sc-client-db and in-mem backend.
#[derive(Clone)]
pub struct PartialStatePrefixerPerBlock<H: Hasher>(pub HashMap<H::Out, PartialStatePrefixer<H>>);

impl<H: Hasher> Default for PartialStatePrefixerPerBlock<H> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<H: Hasher> Encode for PartialStatePrefixerPerBlock<H>
where
	H::Out: Encode,
{
	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		self.0.iter().collect::<Vec<_>>().encode_to(dest);
	}
}

impl<H: Hasher> Decode for PartialStatePrefixerPerBlock<H>
where
	H::Out: Decode,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		Ok(Self(Vec::<(H::Out, PartialStatePrefixer<H>)>::decode(input)?.into_iter().collect()))
	}
}

impl<H: Hasher> PartialStatePrefixerPerBlock<H> {
	/// Feed a chunk to the walker of its block, see [`PartialStatePrefixer::import`].
	pub fn import<BlockNumber: Default>(
		&mut self,
		partial_state: PartialState<H, BlockNumber>,
	) -> Result<Vec<PrefixedTrieNode<H>>, Error> {
		self.0
			.entry(partial_state.block_hash)
			.or_insert_with(|| PartialStatePrefixer::new(partial_state.state_root))
			.import(partial_state)
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

impl<H: Hasher> Clone for TrieNodeInfo<H> {
	fn clone(&self) -> Self {
		Self {
			trie_prefix: self.trie_prefix.clone(),
			hash: self.hash.clone(),
			is_detached_value: self.is_detached_value.clone(),
			child_trie_key: self.child_trie_key.clone(),
		}
	}
}

impl<H: Hasher> TrieNodeInfo<H> {
	/// Make info for root node.
	pub fn root(hash: H::Out) -> Self {
		Self { trie_prefix: NibbleVec::new(), hash, is_detached_value: false, child_trie_key: None }
	}

	/// Get info for node children: branches, detached value and child trie.
	pub fn children(&self, encoded: &[u8]) -> Result<Vec<TrieNodeInfo<H>>, Error> {
		if self.is_detached_value {
			return Ok(vec![]);
		}
		let node = <Layout<H> as TrieLayout>::Codec::decode(&mut &encoded[..])
			.map_err(|_| Error::TrieDecodeError)?;
		let mut children = vec![];
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

	/// Get prefix for database key.
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

impl<H: Hasher> Encode for TrieNodeInfo<H>
where
	H::Out: Encode,
{
	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		let (prefix_bytes, prefix_last) = self.trie_prefix.as_prefix();
		prefix_bytes.encode_to(dest);
		prefix_last.encode_to(dest);

		self.hash.encode_to(dest);
		self.is_detached_value.encode_to(dest);
		self.child_trie_key.encode_to(dest);
	}
}

impl<H: Hasher> Decode for TrieNodeInfo<H>
where
	H::Out: Decode,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		let prefix_bytes = Vec::<u8>::decode(input)?;
		let prefix_last = Option::<u8>::decode(input)?;
		let mut trie_prefix = NibbleVec::from(NibbleSlice::new(&prefix_bytes));
		if let Some(last) = prefix_last {
			trie_prefix.push(last);
		}

		let hash = Decode::decode(input)?;
		let is_detached_value = Decode::decode(input)?;
		let child_trie_key = Decode::decode(input)?;
		Ok(Self { trie_prefix, hash, is_detached_value, child_trie_key })
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

		let mut walker = PartialStatePrefixer::<Blake2Hasher>::new(root);
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

		let mut walker = PartialStatePrefixer::<Blake2Hasher>::new(root);
		let mut written = walker.import(chunk(vec![root_node], root)).unwrap();
		assert_eq!(written.len(), 1);
		assert!(!walker.is_completed());

		// The second chunk re-sends the root, as a range proof does: it must not be written again.
		written.extend(walker.import(chunk(all_nodes(&db), root)).unwrap());

		assert_eq!(written_keys(&written), expected);
		assert_eq!(written.len(), expected.len(), "the root is written once");
		assert!(walker.is_completed());
	}
}

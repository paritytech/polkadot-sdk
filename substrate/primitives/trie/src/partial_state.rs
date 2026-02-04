//! Functions for partial state.

use codec::{Decode, Encode, Input, Output};
use hash_db::EMPTY_PREFIX;
use hash_db::HashDB;
use hash_db::Hasher;
use sp_storage::ChildType;
use sp_storage::PrefixedStorageKey;
use sp_storage::well_known_keys;
use std::collections::HashMap;
use super::LayoutV1 as Layout;
use super::MemoryDB;
use super::PrefixedMemoryDB;
use trie_db::NibbleSlice;
use trie_db::NibbleVec;
use trie_db::node::decode_hash;
use trie_db::node::Node;
use trie_db::node::NodeHandle;
use trie_db::node::Value;
use trie_db::NodeCodec;
use trie_db::TrieLayout;

/// Subset of nodes allowing reading part of state trie.
/// This state belongs to block with specified hash and number.
pub struct PartialState<H: Hasher, BlockNumber: Default> {
	pub block_hash: H::Out,
	pub block_number: BlockNumber,
	pub state_root: H::Out,
	pub nodes: MemoryDB<H>,
}

/// Database requires prefix to store trie nodes.
/// Prefixed partial state can be written to database.
pub type PrefixedPartialState<H> = PrefixedMemoryDB<H>;

/// Traverse trie nodes asynchronously.
///
/// In the beginnning, state root hash is only piece of information known.
/// When node with that hash is received, this node can be decoded to learn it's children node hashes.
/// This class stores list of not yet received nodes, starting from root node.
/// When node is received, it is removed from that list, and it's children are added to that list.
/// `PartialStatePrefixer::import` behaves like recursive coroutine, traversing trie, and waiting for nodes to be received.
///
/// Used by database to make `import_partial_state` idempotent, preventing double increment if reference counting database is used.
#[derive(Clone, Decode, Encode)]
pub struct PartialStatePrefixer<H: Hasher>{
	pub nodes: Vec<TrieNodeInfo<H>>,
}

impl<H: Hasher> PartialStatePrefixer<H> {
	/// Start trie traversal from root.
	pub fn new(root: H::Out) -> Self {
		Self {
			nodes: vec![TrieNodeInfo::root(root)],
		}
	}

	/// Is trie traversal completed.
	pub fn is_completed(&self) -> bool {
		self.nodes.is_empty()
	}

	/// Resume trie traversal using nodes from partial state.
	/// Returns prefixed partial state for nodes used during traversal.
	pub fn import<BlockNumber: Default>(&mut self, partial_state: PartialState<H, BlockNumber>) -> Result<PrefixedPartialState<H>, Error> {
			let mut result = PrefixedPartialState::new(&[]);
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
					result.emplace(info.hash, info.db_prefix()?.as_prefix(), encoded);
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
	H::Out : Encode
{
	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		self.0.iter().collect::<Vec<_>>().encode_to(dest);
	}
}

impl<H: Hasher> Decode for PartialStatePrefixerPerBlock<H>
where
	H::Out : Decode
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		Ok(Self(Vec::<(H::Out, PartialStatePrefixer<H>)>::decode(input)?.into_iter().collect()))
	}
}

impl<H: Hasher> PartialStatePrefixerPerBlock<H> {
	pub fn import<BlockNumber: Default>(&mut self, partial_state: PartialState<H, BlockNumber>) -> Result<PrefixedPartialState<H>, Error> {
		self.0.entry(partial_state.block_hash)
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
		Self {
			trie_prefix: NibbleVec::new(),
			hash,
			is_detached_value: false,
			child_trie_key: None,
		}
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
			Node::Leaf(partial, _)
			| Node::Extension(partial, _)
			| Node::NibbledBranch(partial, _, _) => Some(NibbleVec::from(*partial)),
			_ => None,
		};
		match &node {
			Node::Leaf(_, value)
			| Node::Branch(_, Some(value))
			| Node::NibbledBranch(_, _, Some(value)) => {
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
				Some((ChildType::ParentKeyId, key)) => prefix.append(&NibbleVec::from(NibbleSlice::new(key))),
				None => return Err(Error::UnknownChildStorageType),
			}
		}
		prefix.append(&self.trie_prefix);
		Ok(prefix)
	}
}

impl<H: Hasher> Encode for TrieNodeInfo<H>
where
	H::Out : Encode
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
	H::Out : Decode
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
		Ok(Self {
			trie_prefix,
			hash,
			is_detached_value,
			child_trie_key
		})
	}
}

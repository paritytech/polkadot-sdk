use alloc::vec;
use alloc::vec::Vec;
use codec::Decode;
use codec::Encode;
use hash_db::Hasher;
use sp_storage::well_known_keys;
use trie_db::node::Node;
use trie_db::node::NodeHandle;
use trie_db::node::Value;
use trie_db::NibbleSlice;
use trie_db::NibbleVec;
use trie_db::NodeCodec;
use trie_db::TrieLayout;

use crate::LayoutV1 as Layout;

/// Error associated with the `storage_proof` module.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ProposalError {
	HashSizeMismatch,
	ChildStorageRootMustBeInlineValue,
	TrieDecodeError,
}

fn as_hash<H: Hasher>(input: &[u8]) -> Result<H::Out, ProposalError> {
	let mut hash = H::Out::default();
	if input.len() != hash.as_mut().len() {
		return Err(ProposalError::HashSizeMismatch);
	}
	hash.as_mut().copy_from_slice(input);
	Ok(hash)
}

/// Client doesn't have `UnknownLeaf` nodes, so requests them from server.
/// Both client and server already have `KnownParent` nodes.
/// Client uses `KnownParent` nodes as merkle proof that `UnknownLeaf` are reachable from root.
#[derive(Encode, Decode)]
pub struct ClientProof<H> {
	pub hash: H,
	pub children: Vec<ClientProof<H>>,
}
impl<H> ClientProof<H> {
	pub fn is_leaf(&self) -> bool {
		self.children.is_empty()
	}
}
pub const CLIENT_PROOF: &[u8] = b"ClientProof";

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum TrieNodeChildKind {
	ChildTrie,
	Value,
	Branch,
}
#[derive(Clone)]
pub struct TrieNodeChild<H> {
	pub kind: TrieNodeChildKind,
	pub prefix: NibbleVec,
	pub hash: H,
}

pub fn get_trie_node_children<H: Hasher>(
	prefix: &NibbleVec,
	encoded: &[u8],
) -> Result<Vec<TrieNodeChild<H::Out>>, ProposalError> {
	let node = <Layout<H> as TrieLayout>::Codec::decode(&mut &encoded[..])
		.map_err(|_| ProposalError::TrieDecodeError)?;
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
			let mut prefix = prefix.clone();
			if let Some(partial) = &partial {
				prefix.append(partial);
			}
			let key = prefix.as_prefix().0;
			if well_known_keys::is_child_storage_key(key) {
				assert!(well_known_keys::is_default_child_storage_key(key));
				prefix = NibbleVec::from(NibbleSlice::new(
					&key[well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX.len()..],
				));
				let hash = match value {
					Value::Inline(hash) => hash,
					_ => return Err(ProposalError::ChildStorageRootMustBeInlineValue),
				};
				let hash = as_hash::<H>(hash)?;
				children.push(TrieNodeChild { kind: TrieNodeChildKind::ChildTrie, prefix, hash });
			} else if let Value::Node(hash) = value {
				let hash = as_hash::<H>(hash)?;
				children.push(TrieNodeChild { kind: TrieNodeChildKind::Value, prefix, hash });
			}
		},
		_ => {},
	}
	match &node {
		Node::Branch(branches, _) | Node::NibbledBranch(_, branches, _) => {
			for (i, branch) in branches.iter().enumerate() {
				if let Some(NodeHandle::Hash(hash)) = branch {
					let hash = as_hash::<H>(hash)?;
					let mut prefix = prefix.clone();
					if let Some(partial) = &partial {
						prefix.append(partial);
					}
					prefix.push(i as u8);
					children.push(TrieNodeChild { kind: TrieNodeChildKind::Branch, prefix, hash });
				}
			}
		},
		_ => {},
	}
	Ok(children)
}

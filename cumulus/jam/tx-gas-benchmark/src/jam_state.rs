// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! The JAM state a parachain block reads, in memory.
//!
//! A block reads three JAM state entries at its anchor: the auth pools and the authorizer code for
//! its seal check, and its own `ParaInfo` for the included head. They live in a Gray Paper binary
//! trie whose root is the refine context's `state_root`, and the block carries a [`StateProof`]
//! for them, exactly like a block the collator builds against a real JAM node.

use jam_types::Encode as _;
use parachain_service_core::{
	authorizer::AuthorizerHash,
	blake2_256,
	block_authorization::AUTH_POOLS_STATE_KEY,
	para_info_key, service_preimage_state_key, service_value_state_key,
	types::{HeadData, ParaId, ValidationCodeRef},
	ParaInfo, ProofNode, StateKey, StateProof, PARACHAIN_SERVICE_ID,
};

type Hash = [u8; 32];

/// The cores of a tiny JAM network and the entries of a full auth pool, as the auth pools look on
/// the network the measured numbers come from.
const CORES: usize = 2;
const AUTH_POOL_SIZE: usize = 8;

/// A JAM state root and the proof of every entry under it.
pub struct JamState {
	pub root: Hash,
	pub proof: StateProof,
}

/// What the JAM state at a block's anchor holds for that block.
pub struct Entries<'a> {
	pub para_id: u32,
	/// Hash of the included para head, the parent of the block.
	pub included_head: Hash,
	/// The para's validation code: its hash and length.
	pub validation_code: (Hash, u32),
	/// The authorizer that has to be in an auth pool.
	pub authorizer_hash: AuthorizerHash,
	/// The authorizer code, held as a preimage by the parachain service.
	pub authorizer_code: &'a [u8],
}

impl JamState {
	pub fn new(entries: Entries) -> Self {
		let pools = vec![vec![entries.authorizer_hash; AUTH_POOL_SIZE]; CORES]
			.iter()
			.flat_map(|pool| pool.encode())
			.collect::<Vec<_>>();
		let para_info = ParaInfo {
			head_data: HeadData::try_from(entries.included_head.to_vec())
				.expect("a 32 byte head is below the head size limit; qed"),
			validation_code: Some(ValidationCodeRef {
				hash: entries.validation_code.0.into(),
				len: entries.validation_code.1,
			}),
			announced_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		};
		let code_hash = blake2_256(entries.authorizer_code);

		Trie::new(vec![
			(AUTH_POOLS_STATE_KEY, pools),
			(
				service_preimage_state_key(PARACHAIN_SERVICE_ID, &code_hash),
				entries.authorizer_code.to_vec(),
			),
			(
				service_value_state_key(
					PARACHAIN_SERVICE_ID,
					&para_info_key(ParaId::from(entries.para_id)),
				),
				codec::Encode::encode(&para_info),
			),
		])
		.into_state()
	}
}

/// Gray Paper merklization of a full key/value set, able to prove every key. The same
/// construction `cumulus-jam-state-reader` tests its proof reader against.
struct Trie {
	entries: Vec<(StateKey, Vec<u8>)>,
	nodes: Vec<ProofNode>,
}

impl Trie {
	fn new(mut entries: Vec<(StateKey, Vec<u8>)>) -> Self {
		entries.sort_by_key(|(key, _)| *key);
		Trie { entries, nodes: Vec::new() }
	}

	fn into_state(mut self) -> JamState {
		let entries = self.entries.clone();
		let root = self.hash_subtree(0, &entries);
		let values = self.entries.into_iter().filter(|(_, value)| value.len() > 32).collect();
		JamState { root, proof: StateProof { nodes: self.nodes, values } }
	}

	fn hash_subtree(&mut self, depth: usize, entries: &[(StateKey, Vec<u8>)]) -> Hash {
		match entries {
			[] => [0; 32],
			[(key, value)] => self.push(leaf_node(key, value)),
			_ => {
				let (left, right): (Vec<_>, Vec<_>) =
					entries.iter().cloned().partition(|(key, _)| bit_at(key, depth) == 0);
				let left = self.hash_subtree(depth + 1, &left);
				let right = self.hash_subtree(depth + 1, &right);
				self.push(branch_node(&left, &right))
			},
		}
	}

	fn push(&mut self, node: ProofNode) -> Hash {
		let hash = blake2_256(&node);
		self.nodes.push(node);
		hash
	}
}

fn bit_at(key: &StateKey, depth: usize) -> u8 {
	(key[depth / 8] >> (7 - (depth % 8))) & 1
}

fn leaf_node(key: &StateKey, value: &[u8]) -> ProofNode {
	let mut node = [0u8; 64];
	node[1..32].copy_from_slice(key);
	if value.len() > 32 {
		node[0] = 0b1100_0000;
		node[32..].copy_from_slice(&blake2_256(value));
	} else {
		node[0] = 0b1000_0000 | value.len() as u8;
		node[32..32 + value.len()].copy_from_slice(value);
	}
	node
}

fn branch_node(left: &Hash, right: &Hash) -> ProofNode {
	let mut node = [0u8; 64];
	node[..32].copy_from_slice(left);
	node[32..].copy_from_slice(right);
	node[0] &= 0b0111_1111;
	node
}

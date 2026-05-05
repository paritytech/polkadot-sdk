// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Fixture utilities for tests and benchmarks.
//! Only compiled when `runtime-benchmarks` or `fixtures` feature is enabled.
//! Not included in release builds.

use frame_support::traits::Get;
use sp_std::prelude::*;

use crate::*;
use frame_support::BoundedVec;
use sp_std::vec;

/// Error when building a receipt proof from raw bytes (length or node size out of bounds).
#[derive(Clone, Debug)]
pub struct ReceiptProofBoundsError;

/// Trie node where `value` is either the RLP-encoded item we're
/// proving or an intermediate hash (refers to node with same name in Geth)
/// Proof verification should return `value`. `key` is an implementation
/// detail of the trie.
pub struct ShortNode {
	pub key: Vec<u8>,
	pub value: Vec<u8>,
}

impl rlp::Decodable for ShortNode {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		let mut iter = rlp.iter();

		let key: Vec<u8> = match iter.next() {
			Some(data) => data.as_val()?,
			None => return Err(rlp::DecoderError::Custom("Expected key bytes")),
		};

		let value: Vec<u8> = match iter.next() {
			Some(data) => data.as_val()?,
			None => return Err(rlp::DecoderError::Custom("Expected value bytes")),
		};

		Ok(Self { key, value })
	}
}

/// Fixture-only: RLP encoding for ShortNode. Not included in release; used when generating
/// receipt proof fixtures (see snowbridge-verification-primitives fixtures).
#[cfg(any(test, feature = "std", feature = "runtime-benchmarks", feature = "fixtures"))]
impl rlp::Encodable for ShortNode {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(2);
		s.append(&self.key);
		s.append(&self.value);
	}
}

// impl Node for ShortNode {
// 	fn contains_hash(&self, hash: H256) -> bool {
// 		self.value == hash.0
// 	}
// }

/// Build a valid RLP-encoded MPT node: the RLP encoding of a `ShortNode` with `key = [0x00]`
/// and `value` (padded with zeros). The returned byte length is **at most** `max_len`
/// (`result.len() <= max_len`).
pub fn short_node(value: &[u8], max_len: usize) -> Vec<u8> {
	// RLP(ShortNode): key [0x00] is 1 byte. Value string of L bytes:
	//   L<=55: 1+L bytes → total 3+L
	//   56<=L<=255: 2+L bytes → payload 3+L, list header 2 bytes → total 5+L
	//   L>=256: 3+L bytes (2-byte length) → payload 4+L, list header 3 bytes → total 7+L
	let data_len = if max_len < 3 {
		0
	} else if max_len <= 58 {
		(max_len - 3).min(55)
	} else if max_len <= 260 {
		// 59..260: total = 5+L, so L = max_len - 5 (L <= 255)
		max_len - 5
	} else {
		// max_len >= 261: total = 7+L so L = max_len - 7 (avoids overflow for L>=256)
		max_len - 7
	};
	let mut value_padded = value.to_vec();
	value_padded.resize(data_len, 0u8);
	let node = ShortNode { key: sp_std::vec![0x00], value: value_padded };
	let encoded = rlp::encode(&node).to_vec();
	debug_assert!(
		encoded.len() <= max_len,
		"short_node: encoded len {} > max_len {}",
		encoded.len(),
		max_len
	);
	encoded
}

/// Build a hash-chain receipt proof with [`MaxDepth`] nodes, each of length [`MaxNodeSize`].
/// Uses short RLP nodes for the chain; each node is padded to MaxNodeSize for worst-case
/// benchmarks.
pub fn build_hash_chain_proof<MaxNodeSize, MaxDepth>() -> Vec<Vec<u8>>
where
	MaxNodeSize: Get<u32>,
	MaxDepth: Get<u32>,
{
	use sp_core::hashing;
	let depth = MaxDepth::get() as usize;
	let node_size = MaxNodeSize::get() as usize;
	let leaf = short_node(&[0xde, 0xad, 0xbe, 0xef], node_size);
	let mut proof = vec![leaf.clone()];
	let mut hash = hashing::keccak_256(&leaf);
	for _ in 0..depth.saturating_sub(1) {
		let node = short_node(&hash, node_size);
		hash = hashing::keccak_256(&node);
		proof.push(node);
	}
	proof.reverse();
	proof
}

/// Build a [`ReceiptProof`] from a `Vec<Vec<u8>>`. Fails if length or any node size exceeds bounds.
pub fn try_receipt_proof_from_vec<MaxNodeSize, MaxDepth>(
	v: Vec<Vec<u8>>,
) -> Result<ReceiptProof<MaxNodeSize, MaxDepth>, ReceiptProofBoundsError>
where
	MaxNodeSize: Get<u32>,
	MaxDepth: Get<u32>,
{
	let max_node = MaxNodeSize::get() as usize;
	let max_depth = MaxDepth::get() as usize;
	if v.len() > max_depth {
		return Err(ReceiptProofBoundsError);
	}
	let inner: Result<Vec<ReceiptProofNode<MaxNodeSize>>, _> = v
		.into_iter()
		.map(|x| {
			if x.len() > max_node {
				Err(ReceiptProofBoundsError)
			} else {
				BoundedVec::try_from(x).map_err(|_| ReceiptProofBoundsError)
			}
		})
		.collect();
	let inner = inner?;
	BoundedVec::try_from(inner).map_err(|_| ReceiptProofBoundsError)
}

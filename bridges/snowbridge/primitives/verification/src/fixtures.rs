// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Fixture utilities for tests and benchmarks.
//! Only compiled when `runtime-benchmarks` feature is enabled.

use frame_support::traits::Get;
use sp_std::prelude::*;

use crate::*;
use frame_support::BoundedVec;
use sp_std::vec;

/// Error when building a receipt proof from raw bytes (length or node size out of bounds).
#[derive(Clone, Debug)]
pub struct ReceiptProofBoundsError;

/// Minimal RLP encoding for [0x00, value] where value length is < 56.
/// Used to build short MPT nodes for hash-chain receipt proofs in benchmarks.
pub fn short_node(value: &[u8]) -> Vec<u8> {
	let mut out = Vec::with_capacity(1 + 2 + 1 + value.len());
	let payload_len = 2 + 1 + value.len();
	out.push(0xc0 + payload_len as u8);
	out.push(0x81);
	out.push(0x00);
	out.push(0x80 + value.len() as u8);
	out.extend_from_slice(value);
	out
}

/// Build a hash-chain receipt proof with [`MaxDepth`] nodes, each bounded by [`MaxNodeSize`].
/// Uses short RLP nodes for the chain; nodes are padded to MaxNodeSize for worst-case benchmarks.
pub fn build_hash_chain_proof<MaxNodeSize, MaxDepth>() -> Vec<Vec<u8>>
where
	MaxNodeSize: Get<u32>,
	MaxDepth: Get<u32>,
{
	use sp_core::hashing;
	let depth = MaxDepth::get() as usize;
	let node_size = MaxNodeSize::get() as usize;
	let leaf = short_node(&[0xde, 0xad, 0xbe, 0xef]);
	let mut proof = vec![leaf.clone()];
	let mut hash = hashing::keccak_256(&leaf);
	for _ in 0..depth.saturating_sub(1) {
		let node = short_node(&hash);
		hash = hashing::keccak_256(&node);
		proof.push(node);
	}
	proof.reverse();
	// Pad each node to MaxNodeSize for worst-case bounds
	proof
		.into_iter()
		.map(|mut node| {
			node.resize(node_size, 0u8);
			node
		})
		.collect()
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

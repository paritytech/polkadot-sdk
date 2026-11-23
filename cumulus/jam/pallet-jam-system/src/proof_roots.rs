// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The checks of `jam_validate_block` that bind a candidate to JAM data the validators checked.
//!
//! A `JamParent` digest that contradicts the refine context or the anchor chain, an anchor chain
//! that does not link, and a carried JAM state proof rooted elsewhere are provable lies the
//! candidate's author can be slashed for; they come back as a [`RefineError`]. A block without a
//! `JamParent` digest or a carried proof that does not decode is malformed input and panics. No
//! host call is made here, so the checks run in host tests exactly as in the riscv entry point.

use alloc::vec::Vec;
use codec::Decode;
use cumulus_jam_primitives::{anchor_proof::verify_anchor_chain, RefineError};
use cumulus_primitives_core::{CumulusDigestItem, JamParent};
use parachain_service_core::StateProof;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, LazyBlock};

/// The JAM state root each block's carried JAM state proof is verified against, in block order.
///
/// Every block must carry a `JamParent` digest. With an empty `anchor_chain` (a package built at
/// authoring time) the digest must equal `context` exactly and the root is `context_state_root`.
/// Otherwise (a package resubmitting blocks whose own package was lost) the chain — the encoded
/// JAM headers from the context anchor back to the oldest anchor the blocks name, newest first —
/// is verified against the context by [`verify_anchor_chain`], which pins every header to its
/// child's `parent` hash. A block's anchors then only have to be in the chain at the claimed
/// slots, and its root is the posterior state root the chain records for its anchor.
///
/// Fails on an invalid chain or at the first block whose digest mismatches or is unproven, and
/// panics at the first block without a `JamParent` digest.
pub(crate) fn derive_jam_proof_roots<B: BlockT>(
	context: JamParent,
	context_state_root: [u8; 32],
	blocks: &[B::LazyBlock],
	anchor_chain: &[Vec<u8>],
) -> Result<Vec<[u8; 32]>, RefineError> {
	let proven = (!anchor_chain.is_empty())
		.then(|| {
			verify_anchor_chain(
				context.anchor,
				context.anchor_slot,
				context_state_root.into(),
				anchor_chain,
			)
		})
		.transpose()
		.map_err(|_| RefineError::AnchorChainInvalid)?;

	blocks
		.iter()
		.map(|block| {
			let Some(claimed) = CumulusDigestItem::find_jam_parent_info(block.header().digest())
			else {
				panic!("block has no JamParent digest")
			};

			let Some(proven) = &proven else {
				return if claimed == context {
					Ok(context_state_root)
				} else {
					Err(RefineError::JamParentMismatch)
				};
			};
			let root = proven
				.anchor(&claimed.anchor, claimed.anchor_slot)
				.map_err(|_| RefineError::JamParentUnproven)?;
			proven
				.lookup_anchor(&claimed.lookup_anchor, claimed.lookup_anchor_slot)
				.map_err(|_| RefineError::JamParentUnproven)?;
			Ok(root.0)
		})
		.collect()
}

/// The JAM state proof a block carries as its `JAM_PROOF_KEY` additional-data entry, which must be
/// rooted at `trusted_root`, the root [`derive_jam_proof_roots`] derived for the block.
///
/// Panics if the entry does not decode as `(anchor_state_root, proof)`.
pub(crate) fn carried_jam_proof(
	entry: &[u8],
	trusted_root: [u8; 32],
) -> Result<StateProof, RefineError> {
	let (carried_root, proof) = <([u8; 32], StateProof)>::decode(&mut &entry[..])
		.expect("jam-proof entry must decode as (anchor_state_root, proof)");
	if carried_root != trusted_root {
		return Err(RefineError::JamProofRootMismatch);
	}
	Ok(proof)
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_jam_primitives::anchor_proof::jam_header_hash;
	use sp_core::H256;
	use sp_runtime::{generic, traits::BlakeTwo256, Digest, DigestItem, OpaqueExtrinsic};

	type Header = generic::Header<u32, BlakeTwo256>;
	type Block = generic::Block<Header, OpaqueExtrinsic>;

	/// The posterior state root the refine context carries for its anchor.
	const CONTEXT_ROOT: [u8; 32] = [0xCC; 32];

	/// The slot of the newest JAM header of [`anchor_chain`], the context anchor.
	const NEWEST_SLOT: u32 = 12;

	/// The posterior state root the fixture gives the JAM block at `slot`.
	fn root(slot: u32) -> H256 {
		H256::repeat_byte(slot as u8)
	}

	/// An encoded JAM header: the 100-byte prefix `verify_anchor_chain` reads, then a tail byte.
	fn jam_header(parent: H256, parent_state_root: H256, slot: u32) -> Vec<u8> {
		let mut encoded = Vec::new();
		encoded.extend_from_slice(parent.as_bytes());
		encoded.extend_from_slice(parent_state_root.as_bytes());
		encoded.extend_from_slice(&[0; 32]);
		encoded.extend_from_slice(&slot.to_le_bytes());
		encoded.push(1);
		encoded
	}

	/// Three linked JAM headers at slots `10..=NEWEST_SLOT`, newest first; the header at `slot`
	/// records `root(slot - 1)` as its parent state root.
	fn anchor_chain() -> Vec<Vec<u8>> {
		let mut parent = H256::repeat_byte(0xEE);
		let mut headers = Vec::new();
		for slot in 10..=NEWEST_SLOT {
			let encoded = jam_header(parent, root(slot - 1), slot);
			parent = jam_header_hash(&encoded);
			headers.push(encoded);
		}
		headers.reverse();
		headers
	}

	/// The anchors naming `chain[anchor]` as anchor and `chain[lookup]` as lookup anchor.
	fn anchors(chain: &[Vec<u8>], anchor: usize, lookup: usize) -> JamParent {
		JamParent {
			anchor: jam_header_hash(&chain[anchor]),
			anchor_slot: NEWEST_SLOT - anchor as u32,
			lookup_anchor: jam_header_hash(&chain[lookup]),
			lookup_anchor_slot: NEWEST_SLOT - lookup as u32,
		}
	}

	/// The refine context: the newest header of the chain as anchor, its parent as lookup anchor.
	fn context() -> JamParent {
		anchors(&anchor_chain(), 0, 1)
	}

	/// A block whose digest holds `logs`.
	fn block_with(logs: Vec<DigestItem>) -> <Block as BlockT>::LazyBlock {
		let header = <Header as HeaderT>::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs },
		);
		generic::LazyBlock::new(header, Vec::new())
	}

	/// A block whose `JamParent` digest names `parent`.
	fn block(parent: JamParent) -> <Block as BlockT>::LazyBlock {
		let JamParent { anchor, anchor_slot, lookup_anchor, lookup_anchor_slot } = parent;
		block_with(vec![CumulusDigestItem::JamParent {
			anchor,
			anchor_slot,
			lookup_anchor,
			lookup_anchor_slot,
		}
		.to_digest_item()])
	}

	fn derive(
		blocks: &[<Block as BlockT>::LazyBlock],
		chain: &[Vec<u8>],
	) -> Result<Vec<[u8; 32]>, RefineError> {
		derive_jam_proof_roots::<Block>(context(), CONTEXT_ROOT, blocks, chain)
	}

	#[test]
	fn blocks_naming_the_context_get_its_state_root() {
		let blocks = [block(context()), block(context())];

		assert_eq!(derive(&blocks, &[]), Ok(vec![CONTEXT_ROOT; 2]));
	}

	#[test]
	fn resubmitted_blocks_get_their_anchors_state_root() {
		let chain = anchor_chain();
		let blocks = [block(anchors(&chain, 2, 2)), block(anchors(&chain, 1, 2)), block(context())];

		assert_eq!(
			derive(&blocks, &chain),
			Ok(vec![root(10).0, root(NEWEST_SLOT - 1).0, CONTEXT_ROOT])
		);
	}

	#[test]
	fn jam_parent_other_than_the_context_is_a_mismatch() {
		let chain = anchor_chain();
		let older = anchors(&chain, 1, 2);
		let wrong_slot = JamParent { anchor_slot: NEWEST_SLOT + 1, ..context() };

		// Anchors the anchor chain would prove do not pass without the chain.
		assert_eq!(derive(&[block(older)], &[]), Err(RefineError::JamParentMismatch));
		// The slots are part of the claim.
		assert_eq!(derive(&[block(wrong_slot)], &[]), Err(RefineError::JamParentMismatch));
		// One mismatching block fails the whole package.
		assert_eq!(
			derive(&[block(context()), block(older)], &[]),
			Err(RefineError::JamParentMismatch)
		);
	}

	#[test]
	fn anchors_the_chain_does_not_prove_are_unproven() {
		let chain = anchor_chain();
		let unknown = H256::repeat_byte(0x42);
		let unknown_anchor = JamParent { anchor: unknown, ..anchors(&chain, 1, 2) };
		let unknown_lookup = JamParent { lookup_anchor: unknown, ..anchors(&chain, 1, 2) };
		let wrong_slot = JamParent { anchor_slot: NEWEST_SLOT, ..anchors(&chain, 1, 2) };

		for claimed in [unknown_anchor, unknown_lookup, wrong_slot] {
			assert_eq!(
				derive(&[block(context()), block(claimed)], &chain),
				Err(RefineError::JamParentUnproven),
				"{claimed:?}"
			);
		}
	}

	#[test]
	fn corrupt_anchor_chain_is_invalid() {
		let blocks = [block(context())];

		// One byte of the middle header's extrinsic hash: its hash no longer links.
		let mut broken_link = anchor_chain();
		broken_link[1][64] ^= 1;
		// A header shorter than the prefix.
		let mut truncated = anchor_chain();
		truncated[2].truncate(99);
		// A chain that does not start at the context anchor.
		let not_from_the_context = anchor_chain()[1..].to_vec();

		for chain in [broken_link, truncated, not_from_the_context] {
			assert_eq!(derive(&blocks, &chain), Err(RefineError::AnchorChainInvalid));
		}
	}

	#[test]
	fn carried_proof_rooted_elsewhere_is_a_root_mismatch() {
		let proof = StateProof { nodes: Vec::new(), values: vec![([7; 31], vec![1, 2, 3])] };

		assert_eq!(
			carried_jam_proof(&(CONTEXT_ROOT, proof.clone()).encode(), CONTEXT_ROOT),
			Ok(proof.clone())
		);
		assert_eq!(
			carried_jam_proof(&(root(NEWEST_SLOT - 1).0, proof).encode(), CONTEXT_ROOT),
			Err(RefineError::JamProofRootMismatch)
		);
	}

	#[test]
	#[should_panic(expected = "block has no JamParent digest")]
	fn block_without_jam_parent_digest_panics() {
		let other_digest = DigestItem::PreRuntime(*b"aura", vec![0; 8]);
		let _ = derive(&[block(context()), block_with(vec![other_digest])], &[]);
	}

	#[test]
	#[should_panic(expected = "block has no JamParent digest")]
	fn block_with_empty_digest_panics() {
		let _ = derive(&[block_with(Vec::new())], &anchor_chain());
	}

	#[test]
	#[should_panic(expected = "jam-proof entry must decode")]
	fn undecodable_carried_proof_panics() {
		let _ = carried_jam_proof(&CONTEXT_ROOT, CONTEXT_ROOT);
	}
}

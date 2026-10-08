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

//! The JAM (riscv/PolkaVM) implementation of validate block.
//!
//! On JAM the PVF is invoked with *no arguments* and reads its inputs / writes its outputs
//! through the child-PVM host functions of the Parachain Service (spec §4.2). The validation
//! core is shared with the polkadot implementation via
//! [`super::validate_block_core::execute_blocks`]; this module is the host-call surface that
//! feeds that core and the host-call side effects that sink its outputs.
//!
//! # Where the validation inputs come from on JAM
//!
//! The PoV is work-item extrinsic 0: the collator's `build_pov` output, a SCALE-encoded
//! `ParachainBlockData::V4`. The work-item payload carries only the candidate's validation-code
//! hash (the `pov` field is empty) and is read by the parachain service, not here. On the relay
//! chain the validator knows the previous head and the relay-parent context from its own chain
//! state; on JAM the PVF has no chain state, so the parent header travels *untrusted* in the V4
//! `parent_header` field and is bound twice:
//!
//! - the shared core's `verify_blocks_form_chain` asserts `blocks[0].parent_hash ==
//!   parent_header.hash()`, so a candidate that declares a parent other than the parent of its own
//!   first block aborts;
//! - the parachain service binds it to the canonical chain: the head it stores is a header hash
//!   (declared through `host::set_head`), and its `accumulate` compares `blake2_256` of that stored
//!   head against the `parent_head_hash` declared here (`host::set_parent_head_hash`, the
//!   `blake2_256` of the parent header's hash), so a candidate built on a stale or forged parent
//!   never accumulates.
//!
//! There is no relay chain on JAM: the core runs with `relay_parent_storage_root = None`, so its
//! relay-proof reader and `validate_validation_data` re-check are never armed, and the trie
//! randomness seed is derived from the JAM refine context's `lookup_anchor` + block hashes
//! instead of relay-parent state.
//!
//! # Which JAM anchors a block may name
//!
//! Every block carries a `JamParent` digest naming the JAM `anchor` and `lookup_anchor` (with
//! their slots) it was built against; the runtime reads its slots from that digest, so this is
//! where the digest is bound to something the JAM validators checked. Which anchors pass depends
//! on the V4 `anchor_chain` the PoV carries:
//!
//! - empty (a package built at authoring time): every block's digest must equal the refine context
//!   exactly, and every block's carried JAM state proof is verified against the context's
//!   `state_root`;
//! - non-empty (a package resubmitting blocks whose own package was lost): it holds the encoded JAM
//!   headers from the context anchor back to the oldest anchor the blocks name, newest first.
//!   [`verify_anchor_chain`] pins every header to its child's `parent` hash, so a block's anchors
//!   are accepted when they are in the chain at the claimed slots, and its JAM state proof is
//!   verified against its own anchor's posterior state root as the chain records it.

use super::{
	bytes::Bytes,
	host_functions::{additional_data, jam_data},
	validate_block_core::{execute_blocks, SharedValidationInputs},
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_jam_state_reader::{JamProofReader, JAM_PROOF_KEY};
use cumulus_primitives_core::{
	jam_anchor_proof::verify_anchor_chain, CumulusDigestItem, JamParent, ParachainBlockData,
};
use frame_support::traits::{ExecuteBlock, Get};
use parachain_service_core::StateProof;
use sp_additional_data::{hash_value, AdditionalData, AdditionalDataFinalizer};
use sp_crypto_hashing::{blake2_128, blake2_256};
use sp_runtime::traits::{
	Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, LazyBlock,
};

/// Bounded, opaque error payloads the caller leaves on the failure report path when the PVF
/// aborts abnormally (spec §4.2 / `report_error`). Kept as static slices so no unbounded
/// allocation happens on the abort path; each message has a distinct byte length on purpose —
/// the `RefineLog::Opaque` digest carries the length, which is how an abort is pinned from the
/// logs without decoding the payload.
const ERR_POV_EXTRINSIC_MISSING: &[u8] = b"jam_validate_block:pov-extrinsic-missing";
const ERR_POV_DECODE_FAILED: &[u8] = b"jam_validate_block:pov-decode-failed";
const ERR_PARENT_HEADER_MISSING: &[u8] = b"jam_validate_block:v4-parent-header-missing";
const ERR_HEAD_DATA_MISSING: &[u8] = b"jam_validate_block:no-head-data";
const ERR_JAM_PARENT_MISMATCH: &[u8] = b"jam_validate_block:jam-parent-mismatch";
const ERR_JAM_PARENT_MISSING: &[u8] = b"jam_validate_block:jam-parent-missing";
const ERR_NO_DIGEST_AT_ALL: &[u8] = b"jam_validate_block:no-digest-at-all";
const ERR_ANCHOR_CHAIN_INVALID: &[u8] = b"jam_validate_block:anchor-chain-invalid";
const ERR_JAM_PARENT_UNPROVEN: &[u8] = b"jam_validate_block:jam-parent-unproven-by-chain";

/// The `AdditionalDataFinalizer` committing the carried JAM state proof under `JAM_PROOF_KEY`.
///
/// The commitment is `sp_additional_data::hash_value` of the exact bytes the `JAM_PROOF_KEY`
/// entry carries in the additional-data map, so the digest recomputed on refine matches the one
/// committed at authoring (the omni-node's `JamProofFinalizer`). Threaded through the
/// `additional_data::using` provider seam that `host_finalize_into` folds.
struct JamProofFinalizer {
	commitment: [u8; 32],
}

impl AdditionalDataFinalizer for JamProofFinalizer {
	fn finalize(&self) -> Option<[u8; 32]> {
		Some(self.commitment)
	}
}

/// The single entry point the Parachain Service's Refine (spawned child PVM) calls (spec §4.2).
///
/// Same validation as the relay-chain path — [`super::relay_chain_implementation::validate_block`]
/// — instantiated with the same concrete `B`/`E`/`PSC` by the runtime layer, but with the JAM
/// setup: the PoV is read from the child-PVM work-item `extrinsic(0)` host function and the
/// `ValidationResult` outputs are written via host side effects instead of returned.
#[allow(clippy::unused_unit)]
pub fn jam_validate_block<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>() {
	// 1. Read the PoV from work-item extrinsic 0. The Refine invokes the child PVM with a single
	// work item (index 0), and the collator sends the PoV as that item's first extrinsic; the
	// payload only carries the validation-code hash the parachain service reads.
	let Some(pov) = host::extrinsic(0) else { host::report_error(ERR_POV_EXTRINSIC_MISSING) };

	// 2. The PoV is itself a SCALE-encoded `ParachainBlockData::V4`. Decode it to reach the
	// parent header and blocks.
	let Ok(block_data) =
		codec::decode_from_bytes::<ParachainBlockData<B::LazyBlock>>(Bytes::from(pov))
	else {
		host::report_error(ERR_POV_DECODE_FAILED)
	};

	// 3. The parent header is untrusted V4 transport — this module establishes it, the shared
	// core binds it to the candidate (`verify_blocks_form_chain`), the service to the canonical
	// chain (accumulate) — so a pre-V4 PoV cannot be validated on JAM: abort.
	let Some(parent_header_bytes) = block_data.parent_header() else {
		host::report_error(ERR_PARENT_HEADER_MISSING)
	};
	// Owned copy: the header must outlive the move of `block_data` into the core below, as it
	// feeds both `set_parent_head_hash` and the core's `parent_head` input.
	let parent_header = parent_header_bytes.to_vec();
	// The block hashes feed the randomness seed below; grab them before `block_data` is moved
	// into the core.
	let blocks = block_data.blocks();
	let last_block_hash = blocks.last().map(|block| block.header().hash());

	// 4. Declare the parent head hash this candidate is built on, exactly once (mandatory). The
	// stored head is the parent's header hash and `accumulate` compares `blake2_256` of it with
	// this declaration; `verify_blocks_form_chain` binds the parent header to `blocks[0]`.
	host::set_parent_head_hash(&blake2_256(
		&<HashingFor<B> as HashT>::hash(&parent_header).encode(),
	));

	// 5. Seed the trie-hashmap randomness. The relay path seeds from
	// `relay_parent_storage_root` + block hashes; JAM has no relay state, so the refine
	// context's `lookup_anchor` (which the collator cannot find out ahead of time) plays the
	// relay root's role. The shared `refine_context` wrapper aborts if the host does not serve
	// the context: a work package always carries one.
	let context = host::refine_context();
	let randomness_seed = build_jam_seed::<B>(*context.lookup_anchor, blocks);

	// The collator asserts which JAM blocks each candidate block is anchored to by carrying a
	// `JamParent` digest in its header; this is where that assertion is checked against the real
	// refine context, so the claim is only ever as good as the context the JAM host serves here.
	// The slots are checked too: the runtime reads them out of this digest instead of the
	// collator-supplied refine context, so the refine check is what makes that source trustworthy.
	// A package built at authoring time carries an empty anchor chain and every block must name
	// the context exactly; a package resubmitting blocks whose own package was
	// lost carries one, and every block's anchors must be proven by it instead. Either way each
	// block gets the trusted JAM state root its carried JAM state proof is verified against: the
	// context's, or its proven anchor's.
	let context_anchors = JamParent {
		anchor: cumulus_primitives_core::relay_chain::Hash::from(*context.anchor),
		anchor_slot: context.anchor_slot,
		lookup_anchor: cumulus_primitives_core::relay_chain::Hash::from(*context.lookup_anchor),
		lookup_anchor_slot: context.lookup_anchor_slot,
	};
	let jam_proof_roots = derive_jam_proof_roots::<B>(
		context_anchors,
		*context.state_root,
		blocks,
		block_data.anchor_chain(),
	);

	// 6. Run the SAME validation core as the polkadot path. There is no V3 scheduling on JAM
	// (`None` skips the signature-override hook) and no relay proof/validation-data re-check
	// (`|_| {}`; `validate_validation_data` is relay-only). Each block's trusted JAM anchor state
	// root was derived above, so the `on_execute` hook below can verify the carried
	// `JAM_PROOF_KEY` proof against it. `BlockNumberProvider` reads (`JamSlotNumber`) take their
	// slot from the block's own `JamParent` digest, checked above, so nothing is threaded through
	// this core.

	// Everything the emission below needs must be read while the block-execution storage
	// host-function overrides are still installed. `execute_blocks` restores them on return, so a
	// storage read after it is a raw host call the refine host does not serve — and an unserved
	// host call traps the guest, failing the whole refine with no diagnostic. That is why the
	// para id travels with the emit slot: `SelfParaId` is `parachain_info`'s `ParachainId`, a
	// storage item, not a constant. `on_block_validated` runs inside that scope, once per block,
	// so the final block's values are captured here.
	let upgrade_emit: core::cell::Cell<
		Option<([u8; 32], u32, crate::jam::upgrade::EmitPhase, u32)>,
	> = core::cell::Cell::new(None);
	// `execute_blocks` runs the `on_execute` hook exactly once per block, in block order (one call
	// per iteration of its block loop), so a counter pairs each call with its block's root.
	let next_block = core::cell::Cell::new(0usize);
	execute_blocks::<B, E, PSC>(
		SharedValidationInputs::<B> {
			block_data,
			parent_head: Bytes::from(parent_header),
			randomness_seed,
		},
		None,
		&|_| {
			upgrade_emit.set(
				crate::PendingUpgradeEmit::<PSC>::get().map(|(hash, len, phase)| {
					(hash, len, phase, u32::from(PSC::SelfParaId::get()))
				}),
			)
		},
		// Arm the JAM proof reader + finalizer from the carried `JAM_PROOF_KEY` entry for
		// the duration of each block's execution, so `jam_state_read` and `finalize` are
		// served from the proof that travels with the block. The carried root is *ignored* —
		// reads verify against the trusted root derived for this block above (the refine
		// context's state root, or its proven anchor's), so a candidate that recorded its JAM
		// reads against a different root fails at the first read. A malformed blob, or a proof
		// that cannot authenticate a key, panics rather than serving `None`.
		&|additional_data: &Option<AdditionalData>, execute: &mut dyn FnMut()| {
			let block_index = next_block.get();
			next_block.set(block_index + 1);
			let jam_anchor_state_root = *jam_proof_roots
				.get(block_index)
				.expect("one root was derived per block and the hook runs once per block; qed");
			let Some((mut jam_reader, mut jam_finalizer)) = additional_data.as_ref().map(|map| {
				let proof_bytes = map
					.get(JAM_PROOF_KEY)
					.expect("additional data map (present) must contain the jam-proof entry");
				let (_, proof) = <([u8; 32], StateProof)>::decode(&mut &proof_bytes[..])
					.expect("jam-proof entry must decode as (anchor_state_root, proof)");
				(
					JamProofReader::new(jam_anchor_state_root, proof),
					JamProofFinalizer { commitment: hash_value(proof_bytes) },
				)
			}) else {
				return execute();
			};
			// The same entry arms both the digest finalizer (so `host_finalize_into` folds
			// it into the `DigestItem::AdditionalData` the collator committed at authoring)
			// and the state reader; threading one without the other recomputes an empty
			// digest, which `frame_executive`'s digest-count check rejects.
			additional_data::using(&mut jam_finalizer, || {
				jam_data::using(&mut jam_reader, execute)
			});
		},
	);

	// 7. Sink the result through host side effects (spec §4.2). The head is the last block's
	// header hash; `CustomValidationHeadData` is relay-only. An empty PoV already aborted in
	// `verify_blocks_form_chain`, so a missing last block means a core invariant broke.
	let Some(head) = last_block_hash else { host::report_error(ERR_HEAD_DATA_MISSING) };
	host::set_head(&head.encode());
	// Hash-only upgrade path: the runtime decides during execution and stashes the upward
	// message in `PendingUpgradeEmit`; the service host calls only exist here, after execution.
	if let Some((hash, len, phase, para_id)) = upgrade_emit.get() {
		// §5.2 step 1: a parachain must reference a code before it can announce it, so the
		// solicitation goes first. The service treats a repeat solicit as a no-op.
		if matches!(phase, crate::jam::upgrade::EmitPhase::Announcement) {
			host::solicit(
				parachain_service_core::upward_message::Target::Parachain(
					parachain_service_core::types::ParaId::from(para_id),
				),
				hash,
				len,
			);
		}
		host::request_code_upgrade(hash, len, phase.into());
	}
}

/// The JAM state root each block's carried JAM state proof is verified against, in block order.
///
/// Every block must carry a `JamParent` digest. With an empty `anchor_chain` (a package built at
/// authoring time) the digest must equal `context` exactly and the root is `context_state_root`.
/// Otherwise (a package resubmitting blocks whose own package was lost) the chain — the encoded
/// JAM headers from the context anchor back to the oldest anchor the blocks name, newest first —
/// is verified against the context by [`verify_anchor_chain`], which pins every
/// header to its child's `parent` hash. A block's anchors then only have to be in the chain at the
/// claimed slots, and its root is the posterior state root the chain records for its anchor.
///
/// Aborts the refine at the first block whose digest is missing, mismatching or unproven.
fn derive_jam_proof_roots<B: BlockT>(
	context: JamParent,
	context_state_root: [u8; 32],
	blocks: &[B::LazyBlock],
	anchor_chain: &[Vec<u8>],
) -> Vec<[u8; 32]> {
	let proven = (!anchor_chain.is_empty()).then(|| {
		let Ok(proven) = verify_anchor_chain(
			context.anchor,
			context.anchor_slot,
			context_state_root.into(),
			anchor_chain,
		) else {
			host::report_error(ERR_ANCHOR_CHAIN_INVALID)
		};
		proven
	});

	blocks
		.iter()
		.map(|block| {
			let digest = block.header().digest();
			let Some(claimed) = CumulusDigestItem::find_jam_parent_info(digest) else {
				if digest.logs.is_empty() {
					host::report_error(ERR_NO_DIGEST_AT_ALL)
				}
				host::report_error(ERR_JAM_PARENT_MISSING)
			};

			let Some(proven) = &proven else {
				if claimed != context {
					host::report_error(ERR_JAM_PARENT_MISMATCH)
				}
				return context_state_root;
			};
			let Ok(root) = proven.anchor(&claimed.anchor, claimed.anchor_slot) else {
				host::report_error(ERR_JAM_PARENT_UNPROVEN)
			};
			if proven
				.lookup_anchor(&claimed.lookup_anchor, claimed.lookup_anchor_slot)
				.is_err()
			{
				host::report_error(ERR_JAM_PARENT_UNPROVEN)
			}
			root.0
		})
		.collect()
}

/// Build the trie-hashmap randomness seed from the JAM refine context's `lookup_anchor` plus
/// every block hash — the JAM analogue of
/// [`super::relay_chain_implementation::build_seed_from_head_data`] (the lookup anchor stands in
/// for the relay-parent storage root). Mixing a context value the collator cannot fully predict
/// with the block hashes keeps the seed changing every block and hard to find out ahead of time.
fn build_jam_seed<B: BlockT>(lookup_anchor: [u8; 32], blocks: &[B::LazyBlock]) -> [u8; 16] {
	let mut bytes_to_hash =
		Vec::with_capacity(blocks.len() * size_of::<B::Hash>() + size_of::<[u8; 32]>());

	bytes_to_hash.extend_from_slice(lookup_anchor.as_ref());
	blocks.iter().for_each(|block| {
		bytes_to_hash.extend_from_slice(block.header().hash().as_ref());
	});

	blake2_128(&bytes_to_hash)
}

/// Child host calls of the Parachain Service's Refine (spec §4.3).
///
/// None of these are declared here: the parachain-service-native wrappers (indices 200-203) and
/// the JAM `fetch` surface (work package, refine context, work-item extrinsics) are re-exported
/// from `parachain_service_core::host` / `parachain_service_core::refine`, so this runtime and
/// the node drive the exact same ABI definitions instead of per-runtime copies of the raw
/// `fetch` import.
#[cfg(jam)]
pub mod host {
	// The parachain-service host functions (indices 200-203) are defined once, in
	// `parachain_service_core::host`, so the two guests cannot drift apart on the ABI.
	pub use parachain_service_core::host::{
		report_error, request_code_upgrade, set_head, set_parent_head_hash, solicit,
	};
	// The fetch-based JAM helpers (`Fetch::RefineContext`, `workitems[a].extrinsics[b]`, …) are the
	// same ones the service's own refine entry point drives from; re-exported through
	// `parachain_service_core::refine` rather than re-declared. `refine_context` decodes the
	// real type instead of reading a field at a hardcoded offset, so an upstream field
	// reordering is a decode failure instead of silently wrong randomness.
	pub use parachain_service_core::refine::{extrinsic, refine_context};
}

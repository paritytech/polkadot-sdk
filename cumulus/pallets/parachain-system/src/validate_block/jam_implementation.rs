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
//! The work-item payload is the SCALE-encoded [`ParachainCandidate`] the collator's `build_pov`
//! produced: a validation-code hash and the PoV, which is itself a SCALE-encoded
//! `ParachainBlockData::V4`. On the relay chain the validator knows the previous head and the
//! relay-parent context from its own chain state; on JAM the PVF has no chain state, so the
//! parent header travels *untrusted* in the V4 `parent_header` field and is bound twice:
//!
//! - the shared core's `verify_blocks_form_chain` asserts `blocks[0].parent_hash ==
//!   parent_header.hash()`, so a candidate that declares a parent other than the parent of its own
//!   first block aborts;
//! - the parachain service binds it to the canonical chain: its `accumulate` compares the
//!   `parent_head_hash` declared here (via `host::set_parent_head_hash`, a `blake2_256` of the
//!   encoded header) against the head it itself stored, so a candidate built on a stale or forged
//!   parent never accumulates.
//!
//! There is no relay chain on JAM: the core runs with `relay_parent_storage_root = None`, so its
//! relay-proof reader and `validate_validation_data` re-check are never armed, and the trie
//! randomness seed is derived from the JAM refine context's `lookup_anchor` + block hashes
//! instead of relay-parent state.

use super::{
	bytes::Bytes,
	host_functions::{additional_data, jam_data},
	validate_block_core::{execute_blocks, SharedValidationInputs},
};
use alloc::vec::Vec;
use codec::Decode;
use cumulus_jam_state_reader::{JamProofReader, JAM_PROOF_KEY};
use cumulus_primitives_core::{CumulusDigestItem, ParachainBlockData};
use frame_support::traits::{ExecuteBlock, Get};
use parachain_service_core::{candidate::ParachainCandidate, StateProof};
use sp_additional_data::{hash_value, AdditionalData, AdditionalDataFinalizer};
use sp_crypto_hashing::{blake2_128, blake2_256};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, LazyBlock};

/// Bounded, opaque error payloads the caller leaves on the failure report path when the PVF
/// aborts abnormally (spec §4.2 / `report_error`). Kept as static slices so no unbounded
/// allocation happens on the abort path; each message has a distinct byte length on purpose —
/// the `RefineLog::Opaque` digest carries the length, which is how an abort is pinned from the
/// logs without decoding the payload.
const ERR_PAYLOAD_NO_WORK_ITEM: &[u8] = b"jam_validate_block:no-work-item-payload@0";
const ERR_PAYLOAD_DECODE_FAILED: &[u8] = b"jam_validate_block:candidate-decode-failed";
const ERR_POV_DECODE_FAILED: &[u8] = b"jam_validate_block:pov-decode-failed";
const ERR_PARENT_HEADER_MISSING: &[u8] = b"jam_validate_block:v4-parent-header-missing";
const ERR_HEAD_DATA_MISSING: &[u8] = b"jam_validate_block:no-head-data";
const ERR_JAM_PARENT_MISMATCH: &[u8] = b"jam_validate_block:jam-parent-mismatch";
const ERR_JAM_PARENT_MISSING: &[u8] = b"jam_validate_block:jam-parent-missing";
const ERR_NO_DIGEST_AT_ALL: &[u8] = b"jam_validate_block:no-digest-at-all";

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
/// setup: the candidate is read from the child-PVM `work_item_payload` host function and the
/// `ValidationResult` outputs are written via host side effects instead of returned.
#[allow(clippy::unused_unit)]
pub fn jam_validate_block<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>() {
	// 1. Read the work-item payload. The Refine invokes the child PVM with a single work item
	// (index 0); its payload is the SCALE-encoded `ParachainCandidate` the collator assembled.
	let payload = match host::work_item_payload(0) {
		Some(payload) => payload,
		None => host::report_error(ERR_PAYLOAD_NO_WORK_ITEM),
	};

	// 2. Decode the candidate with the shared JAM facade type — one definition, owned by the
	// parachain service, that cannot silently drift from what the collator encodes.
	let Ok(candidate) = ParachainCandidate::decode(&mut &payload[..]) else {
		host::report_error(ERR_PAYLOAD_DECODE_FAILED)
	};

	// 3. The PoV is itself a SCALE-encoded `ParachainBlockData::V4`. Decode it to reach the
	// parent header and blocks.
	let Ok(block_data) =
		codec::decode_from_bytes::<ParachainBlockData<B::LazyBlock>>(Bytes::from(candidate.pov))
	else {
		host::report_error(ERR_POV_DECODE_FAILED)
	};

	// 4. The parent header is untrusted V4 transport — this module establishes it, the shared
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

	// 5. Declare the parent head hash this candidate is built on, exactly once (mandatory). The
	// service records it in the work digest and compares it against the head it stored at
	// accumulate, so it must stay an explicit `blake2_256` over the encoded header (NOT
	// `B::Hashing`): `accumulate` compares its stored `blake2_256(&head_data)` against this.
	host::set_parent_head_hash(&blake2_256(&parent_header));

	// 6. Seed the trie-hashmap randomness. The relay path seeds from
	// `relay_parent_storage_root` + block hashes; JAM has no relay state, so the refine
	// context's `lookup_anchor` (which the collator cannot find out ahead of time) plays the
	// relay root's role. The same context carries the trusted state root of the anchor block —
	// checked on-chain when the package is reported — which the core verifies the carried JAM
	// state proof against. The shared `refine_context` wrapper aborts if the host does not serve
	// the context: a work package always carries one.
	let context = host::refine_context();
	let randomness_seed = build_jam_seed::<B>(*context.lookup_anchor, blocks);
	let jam_anchor_state_root = *context.state_root;

	// The collator asserts which JAM block this candidate is anchored to by carrying a
	// `JamParent` digest in the header; this is where that assertion is checked against the real
	// refine context. A collator naming an anchor, or a slot, other than the one this work package
	// was actually refined against cannot produce a matching digest, so the claim is only ever as
	// good as the context the JAM host serves here. The slots are checked too: the runtime reads
	// them out of this digest instead of the collator-supplied refine context, so the refine check
	// is what makes that source trustworthy.
	let expected = (
		cumulus_primitives_core::relay_chain::Hash::from(*context.anchor),
		context.anchor_slot,
		cumulus_primitives_core::relay_chain::Hash::from(*context.lookup_anchor),
		context.lookup_anchor_slot,
	);
	let claimed = blocks
		.iter()
		.find_map(|block| CumulusDigestItem::find_jam_parent_info(block.header().digest()))
		.map(|parent| {
			(parent.anchor, parent.anchor_slot, parent.lookup_anchor, parent.lookup_anchor_slot)
		});
	if claimed.is_none() {
		if blocks.iter().all(|block| block.header().digest().logs.is_empty()) {
			host::report_error(ERR_NO_DIGEST_AT_ALL)
		}
		host::report_error(ERR_JAM_PARENT_MISSING)
	}
	if claimed != Some(expected) {
		host::report_error(ERR_JAM_PARENT_MISMATCH)
	}

	// 7. Run the SAME validation core as the polkadot path. There is no V3 scheduling on JAM
	// (`None` skips the signature-override hook) and no relay proof/validation-data re-check
	// (`|_| {}`; `validate_validation_data` is relay-only). The trusted JAM anchor state root is
	// sourced from the refine context, so the `on_execute` hook below can verify the carried
	// `JAM_PROOF_KEY` proof. `BlockNumberProvider` reads (`JamSlotNumber`) take their slot from
	// the block's own `JamParent` digest, checked above, so nothing is threaded through this core.

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
	let result = execute_blocks::<B, E, PSC>(
		SharedValidationInputs::<B> {
			block_data,
			parent_head: Bytes::from(parent_header),
			randomness_seed,
		},
		None,
		&|_| {
			upgrade_emit.set(crate::PendingUpgradeEmit::<PSC>::get().map(|(hash, len, phase)| {
				(hash, len, phase, u32::from(PSC::SelfParaId::get()))
			}))
		},
		// Arm the JAM proof reader + finalizer from the carried `JAM_PROOF_KEY` entry for
		// the duration of each block's execution, so `jam_state_read` and `finalize` are
		// served from the proof that travels with the block. The carried root is *ignored* —
		// reads verify against the trusted anchor root from the refine context, so a candidate
		// that recorded its JAM reads against a different root fails at the first read. A
		// malformed blob, or a proof that cannot authenticate a key, panics rather than serving
		// `None`.
		&|additional_data: &Option<AdditionalData>, execute: &mut dyn FnMut()| {
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

	// 8. Sink the result through host side effects (spec §4.2). `head_data` is set by the core
	// after the last block executes; a `None` here is impossible if any block ran
	// (`verify_blocks_form_chain` aborts on an empty PoV first), so it means a core invariant
	// broke, not a malformed candidate — abort loudly instead of declaring no head.
	let Some(head) = result.head_data else { host::report_error(ERR_HEAD_DATA_MISSING) };
	host::set_head(&head.0);
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
/// the JAM `fetch` surface (work package, refine context, work-item payloads) are re-exported
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
	// The fetch-based JAM helpers (`Fetch::RefineContext`, `workitems[a].payload`, …) are the
	// same ones the service's own refine entry point drives from; re-exported through
	// `parachain_service_core::refine` rather than re-declared. `refine_context` decodes the
	// real type instead of reading a field at a hardcoded offset, so an upstream field
	// reordering is a decode failure instead of silently wrong randomness.
	pub use parachain_service_core::refine::{refine_context, work_item_payload};
}

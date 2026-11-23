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
//! core is the one of `cumulus-pallet-parachain-system` ([`execute_blocks`]); this module is the
//! host-call surface that feeds that core and the host-call side effects that sink its outputs.
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
//! There is no relay chain on JAM: the JAM hooks run no `validate_validation_data` re-check, and
//! the trie randomness seed is derived from the JAM refine context's `lookup_anchor` + block
//! hashes instead of relay-parent state.
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
//!   [`verify_anchor_chain`](cumulus_jam_primitives::anchor_proof::verify_anchor_chain) pins every
//!   header to its child's `parent` hash, so a block's anchors are accepted when they are in the
//!   chain at the claimed slots, and its JAM state proof is verified against its own anchor's
//!   posterior state root as the chain records it.
//!
//! # How a candidate is rejected
//!
//! A candidate that lies about JAM data, so its author can be slashed, aborts the refine through
//! [`report`] with the [`RefineError`] the checks in [`crate::proof_roots`] return. Malformed input
//! (no PoV, a PoV that does not decode, a pre-V4 PoV, a block without a `JamParent` digest) panics,
//! which fails the whole refine without an entry in the para's log.

use crate::{
	host_overrides::{self, jam_data},
	proof_roots::{carried_jam_proof, derive_jam_proof_roots},
};
use alloc::vec::Vec;
use codec::Encode;
use core::marker::PhantomData;
use cumulus_jam_primitives::RefineError;
use cumulus_jam_state_reader::{JamProofFinalizer, JamProofReader, JAM_PROOF_KEY};
use cumulus_pallet_parachain_system::validate_block::{
	bytes::Bytes,
	decode_from_bytes,
	host_functions::additional_data,
	validate_block_core::{execute_blocks, SharedValidationInputs, ValidationHooks},
};
use cumulus_primitives_core::{JamParent, ParachainBlockData};
use frame_support::traits::{ExecuteBlock, Get};
use sp_additional_data::{hash_value, AdditionalData};
use sp_core::H256;
use sp_crypto_hashing::{blake2_128, blake2_256};
use sp_runtime::traits::{
	Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, LazyBlock,
};

/// The message id of the code-upgrade `Solicit` and `RequestCodeUpgrade`. The service logs their
/// outcome under it, but the upgrade path reads `ParaInfo` instead of those log entries, so the id
/// is never read back.
const CODE_UPGRADE_MESSAGE_ID: parachain_service_core::types::MessageId = codec::Compact(0);

/// The code-upgrade emission of the last block: code hash, code length, phase and the para id it
/// is sent for.
type UpgradeEmit = ([u8; 32], u32, crate::EmitPhase, u32);

/// The JAM [`ValidationHooks`]: arms the JAM proof reader and finalizer around each block and
/// captures the upgrade emission of the last block.
struct JamValidationHooks<T> {
	/// The trusted JAM state root each block's carried JAM state proof is verified against, in
	/// block order.
	jam_proof_roots: Vec<[u8; 32]>,
	upgrade_emit: Option<UpgradeEmit>,
	_config: PhantomData<T>,
}

impl<B: BlockT, T: crate::Config> ValidationHooks<B> for JamValidationHooks<T> {
	/// Arms the JAM proof reader + finalizer from the carried `JAM_PROOF_KEY` entry for the
	/// duration of the block's execution, so `jam_state_read` and `finalize` are served from the
	/// proof that travels with the block. The carried root must equal the trusted root derived for
	/// this block (the refine context's posterior state root, or its proven anchor's), else the
	/// block is reported, and reads verify against that trusted root. A malformed blob, or a proof
	/// that cannot authenticate a key, panics rather than serving `None`.
	fn execute_block(
		&mut self,
		index: usize,
		additional_data: Option<&AdditionalData>,
		execute: &mut dyn FnMut(),
	) {
		let jam_anchor_state_root = *self
			.jam_proof_roots
			.get(index)
			.expect("one root was derived per block of the PoV; qed");
		let Some(map) = additional_data else { return execute() };
		let proof_bytes = map
			.get(JAM_PROOF_KEY)
			.expect("additional data map (present) must contain the jam-proof entry");
		let proof = carried_jam_proof(proof_bytes, jam_anchor_state_root)
			.unwrap_or_else(|error| report(error));
		let mut jam_reader = JamProofReader::new(jam_anchor_state_root, proof);
		let mut jam_finalizer = JamProofFinalizer { commitment: hash_value(proof_bytes) };
		// The same entry arms both the digest finalizer (so `host_finalize_into` folds it into
		// the `DigestItem::AdditionalData` the collator committed at authoring) and the state
		// reader; threading one without the other recomputes an empty digest, which
		// `frame_executive`'s digest-count check rejects.
		additional_data::using(&mut jam_finalizer, || jam_data::using(&mut jam_reader, execute));
	}

	/// Everything the emission after validation needs is read here, while the block-execution
	/// storage host-function overrides are still installed. `execute_blocks` restores them on
	/// return, so a storage read after it is a raw host call the refine host does not serve, and
	/// an unserved host call traps the guest, failing the whole refine with no diagnostic. That is
	/// why the para id travels with the emit slot: `SelfParaId` is `parachain_info`'s
	/// `ParachainId`, a storage item, not a constant.
	fn on_block_executed(
		&mut self,
		_index: usize,
		is_last: bool,
		_header: &B::Header,
		_parent_head: &[u8],
	) {
		if is_last {
			self.upgrade_emit = crate::PendingUpgradeEmit::<T>::get()
				.map(|(hash, len, phase)| (hash, len, phase, u32::from(T::SelfParaId::get())));
		}
	}
}

/// The single entry point the Parachain Service's Refine (spawned child PVM) calls (spec §4.2).
///
/// The same validation core as the relay-chain `validate_block`, with the JAM setup: the PoV is
/// read from the child-PVM work-item `extrinsic(0)` host function and the outputs are written via
/// host side effects instead of returned. `register_jam_validate_block!` exports it.
pub fn jam_validate_block<B: BlockT, E: ExecuteBlock<B>, T: crate::Config>() {
	// 1. Read the PoV from work-item extrinsic 0. The Refine invokes the child PVM with a single
	// work item (index 0), and the collator sends the PoV as that item's first extrinsic; the
	// payload only carries the validation-code hash the parachain service reads.
	let pov = host::extrinsic(0).expect("refine passes the PoV as work-item extrinsic 0");

	// 2. The PoV is itself a SCALE-encoded `ParachainBlockData::V4`. Decode it to reach the
	// parent header and blocks.
	let block_data = decode_from_bytes::<ParachainBlockData<B::LazyBlock>>(Bytes::from(pov))
		.expect("the PoV is a SCALE-encoded ParachainBlockData");

	// 3. The parent header is untrusted V4 transport — this module establishes it, the shared
	// core binds it to the candidate (`verify_blocks_form_chain`), the service to the canonical
	// chain (accumulate) — so a pre-V4 PoV cannot be validated on JAM.
	let parent_header_bytes = block_data
		.parent_header()
		.expect("a JAM PoV is ParachainBlockData::V4, which carries the parent header");
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
	// context's, or its proven anchor's. A block whose anchors are neither is reported.
	let context_anchors = JamParent {
		anchor: H256::from(*context.anchor),
		anchor_slot: context.anchor_slot,
		lookup_anchor: H256::from(*context.lookup_anchor),
		lookup_anchor_slot: context.lookup_anchor_slot,
	};
	let jam_proof_roots = derive_jam_proof_roots::<B>(
		context_anchors,
		*context.state_root,
		blocks,
		block_data.anchor_chain(),
	)
	.unwrap_or_else(|error| report(error));

	// 6. Run the SAME validation core as the polkadot path. There is no V3 scheduling on JAM and
	// no relay validation-data re-check (`validate_validation_data` is relay-only), so the hooks
	// only arm the JAM proof reader around each block, verifying the carried `JAM_PROOF_KEY`
	// proof against the block's trusted JAM anchor state root derived above, and capture the
	// upgrade emission. `BlockNumberProvider` reads (`JamSlotNumber`) take their slot from the
	// block's own `JamParent` digest, checked above, so nothing is threaded through this core.
	// The core replaces the storage host functions every target shares; the JAM-shaped and
	// JAM-only ones are installed here, for the core's run.
	let mut hooks =
		JamValidationHooks::<T> { jam_proof_roots, upgrade_emit: None, _config: PhantomData };
	let jam_overrides = host_overrides::install_overrides();
	execute_blocks::<B, E>(
		SharedValidationInputs::<B> {
			block_data,
			parent_head: Bytes::from(parent_header),
			randomness_seed,
		},
		&<T as frame_system::Config>::Version::get(),
		&mut hooks,
	);
	drop(jam_overrides);

	// 7. Sink the result through host side effects (spec §4.2). The head is the last block's
	// header hash; `CustomValidationHeadData` is relay-only. An empty PoV already aborted in
	// `verify_blocks_form_chain`, so a missing last block means a core invariant broke.
	let head = last_block_hash.expect("verify_blocks_form_chain rejects an empty PoV; qed");
	host::set_head(&head.encode());
	// Hash-only upgrade path: the runtime decides during execution and stashes the upward
	// message in `PendingUpgradeEmit`; the service host calls only exist here, after execution.
	if let Some((hash, len, phase, para_id)) = hooks.upgrade_emit {
		// §5.2 step 1: a parachain must reference a code before it can announce it, so the
		// solicitation goes first. The service treats a repeat solicit as a no-op.
		if matches!(phase, crate::EmitPhase::Announcement) {
			host::solicit(
				CODE_UPGRADE_MESSAGE_ID,
				parachain_service_core::upward_message::Target::Parachain(
					parachain_service_core::types::ParaId::from(para_id),
				),
				hash,
				len,
			);
		}
		host::request_code_upgrade(CODE_UPGRADE_MESSAGE_ID, hash, len, phase.into());
	}
}

/// Abort the refine for a candidate that lies about JAM data, with the SCALE-encoded `error` as
/// the payload the service records in the para's log as `RefineLog::Opaque`.
fn report(error: RefineError) -> ! {
	host::report_error(&error.encode())
}

/// Build the trie-hashmap randomness seed from the JAM refine context's `lookup_anchor` plus
/// every block hash — the JAM analogue of the relay path's `build_seed_from_head_data` (the lookup
/// anchor stands in for the relay-parent storage root). Mixing a context value the collator cannot
/// fully predict with the block hashes keeps the seed changing every block and hard to find out
/// ahead of time.
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

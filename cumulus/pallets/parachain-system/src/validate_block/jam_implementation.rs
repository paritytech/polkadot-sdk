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
	validate_block_core::{execute_blocks, SharedValidationInputs},
};
use alloc::vec::Vec;
use codec::Decode;
use cumulus_primitives_core::ParachainBlockData;
use frame_support::traits::ExecuteBlock;
use parachain_service_interface::candidate::ParachainCandidate;
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
const ERR_REFINE_CONTEXT_UNAVAILABLE: &[u8] = b"jam_validate_block:refine-context-fetch-failed";
const ERR_HEAD_DATA_MISSING: &[u8] = b"jam_validate_block:no-head-data";

/// The single entry point the Parachain Service's Refine (spawned child PVM) calls (spec §4.2).
///
/// Same validation as the polkadot path — [`super::polkadot_implementation::validate_block`] —
/// instantiated with the same concrete `B`/`E`/`PSC` by the runtime layer, but with the JAM
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
	// relay root's role. The same context carries the trusted `state_root` of the anchor block —
	// checked on-chain when the package is reported — which the core verifies the carried JAM
	// state proof against.
	let Some(context) = host::refine_context() else {
		host::report_error(ERR_REFINE_CONTEXT_UNAVAILABLE)
	};
	let randomness_seed = build_jam_seed::<B>(*context.lookup_anchor, blocks);

	// 7. Run the SAME validation core as the polkadot path. There is no V3 scheduling on JAM
	// (`None` skips the signature-override hook) and no relay proof/validation-data re-check
	// (`|_| {}`; `validate_validation_data` is relay-only). The trusted JAM anchor state root is
	// sourced from the refine context, so the core can verify the carried `JAM_PROOF_KEY` proof.
	let result = execute_blocks::<B, E, PSC>(
		SharedValidationInputs::<B> {
			block_data,
			parent_head: Bytes::from(parent_header),
			randomness_seed,
			relay_parent_storage_root: None,
			jam_anchor_state_root: Some(*context.state_root),
		},
		None,
		&|_| {},
	);

	// 8. Sink the result through host side effects (spec §4.2). `head_data` is set by the core
	// after the last block executes; a `None` here is impossible if any block ran
	// (`verify_blocks_form_chain` aborts on an empty PoV first), so it means a core invariant
	// broke, not a malformed candidate — abort loudly instead of declaring no head.
	let Some(head) = result.head_data else { host::report_error(ERR_HEAD_DATA_MISSING) };
	host::set_head(&head.0);
	if let Some(code) = &result.new_validation_code {
		host::request_code_upgrade(blake2_256(&code), code.len() as u32);
	}
}

/// Build the trie-hashmap randomness seed from the JAM refine context's `lookup_anchor` plus
/// every block hash — the JAM analogue of
/// [`super::polkadot_implementation::build_seed_from_head_data`] (the lookup anchor stands in
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
/// Every import sits at a fixed index: those forwarding a JAM host call keep its Gray Paper
/// index, those native to the Parachain Service are numbered from 100 up.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod host {
	use alloc::{vec, vec::Vec};
	use codec::{Compact, Encode};
	use jam_codec::Decode as _;
	use jam_types::RefineContext;

	/// `fetch` selector for `workitems[a].payload` (Gray Paper).
	const FETCH_WORK_ITEM_PAYLOAD: u64 = 13;

	/// `fetch` selector for the work package's refine context (Gray Paper).
	const FETCH_REFINE_CONTEXT: u64 = 10;

	/// Gray Paper sentinel for "no such item".
	const NONE: u64 = u64::MAX;

	/// The subset of the service's `UpwardMessage` ABI this runtime emits. The SCALE variant
	/// index is positional, so the ordering has to match the spec's `enum UpwardMessage`.
	#[derive(Encode)]
	enum UpwardMessage {
		RequestCodeUpgrade { hash: [u8; 32], len: Compact<u32> },
	}

	#[polkavm_derive::polkavm_import]
	extern "C" {
		// --- JAM host functions, forwarded at their Gray Paper index ---
		#[polkavm_import(index = 2)]
		fn fetch_raw(out_ptr: u32, offset: u64, out_len: u64, kind: u64, a: u64, b: u64) -> u64;

		// --- Parachain Service host functions ---
		#[polkavm_import(index = 200)]
		fn set_parent_head_hash_raw(hash_ptr: u32);
		#[polkavm_import(index = 201)]
		fn set_head_raw(ptr: u32, len: u32);
		#[polkavm_import(index = 202)]
		fn send_upward_message_raw(ptr: u32, len: u32);
		#[polkavm_import(index = 203)]
		fn report_error_raw(ptr: u32, len: u32);
	}

	/// Declare the parent head hash this candidate was built on (called once).
	pub fn set_parent_head_hash(hash: &[u8; 32]) {
		unsafe { set_parent_head_hash_raw(hash.as_ptr() as u32) }
	}

	/// Declare the new head data this parachain block produced.
	pub fn set_head(head: &[u8]) {
		unsafe { set_head_raw(head.as_ptr() as u32, head.len() as u32) }
	}

	/// Signal a PVF code upgrade request (`hash` + encoded-code length).
	pub fn request_code_upgrade(hash: [u8; 32], len: u32) {
		send_upward_message(&UpwardMessage::RequestCodeUpgrade { hash, len: Compact(len) }.encode())
	}

	/// Append one upward message to the work digest.
	fn send_upward_message(msg: &[u8]) {
		unsafe { send_upward_message_raw(msg.as_ptr() as u32, msg.len() as u32) }
	}

	/// Abort the PVF with an opaque error payload; never returns.
	pub fn report_error(data: &[u8]) -> ! {
		unsafe { report_error_raw(data.as_ptr() as u32, data.len() as u32) }
		unreachable!("`report_error` aborts the PVF; qed")
	}

	/// The work package's refine context, decoded from the `fetch` host call.
	///
	/// `None` means the host did not serve it or the bytes did not decode, both of which are
	/// protocol drift: a work package always carries a context. Decoding the real type rather
	/// than reading a field at a hardcoded offset is what makes an upstream field reordering a
	/// decode failure instead of silently wrong randomness.
	pub fn refine_context() -> Option<RefineContext> {
		let bytes = fetch(FETCH_REFINE_CONTEXT, 0, 0)?;
		RefineContext::decode(&mut &bytes[..]).ok()
	}

	/// Fetch the payload of work item `index`; `None` if absent.
	///
	/// `fetch` writes at most `out_len` bytes and returns the item's *full* length, so a
	/// zero-capacity probe yields the size to allocate.
	pub fn work_item_payload(index: u32) -> Option<Vec<u8>> {
		fetch(FETCH_WORK_ITEM_PAYLOAD, index as u64, 0)
	}

	/// Fetch a `(kind, a, b)` value, probing for its length first (Gray Paper semantics).
	fn fetch(kind: u64, a: u64, b: u64) -> Option<Vec<u8>> {
		let fetch = |ptr: u32, len: u64| unsafe { fetch_raw(ptr, 0, len, kind, a, b) };

		let len = fetch(0, 0);
		if len == NONE {
			return None;
		}

		let mut buf = vec![0u8; len as usize];
		loop {
			let actual = fetch(buf.as_ptr() as u32, buf.len() as u64);
			if actual == NONE {
				return None;
			}
			let actual = actual as usize;
			if actual <= buf.len() {
				buf.truncate(actual);
				return Some(buf);
			}
			buf.resize(actual, 0);
		}
	}
}

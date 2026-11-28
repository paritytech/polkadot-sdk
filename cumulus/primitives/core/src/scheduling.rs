// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling types for low-latency parachain block production.
//!
//! V3 candidates separate the relay parent (execution context) from the scheduling parent (a recent
//! relay tip used for core assignment), so blocks can build on older relay parents while still
//! being scheduled off recent relay state.
//!
//! # Resubmission
//!
//! If a candidate isn't backed in time, another collator can resubmit it with a fresh
//! `scheduling_parent` — same `relay_parent`, no re-execution — by providing a
//! `signed_scheduling_info` proving it is the eligible author for the slot at
//! `internal_scheduling_parent`.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use polkadot_primitives::{ApprovedPeerId, CoreSelector, Header as RelayChainHeader, Slot};
use sp_runtime::traits::{BlakeTwo256, Hash as HashT};

/// Payload a collator signs to resubmit a candidate.
///
/// Binds the core selection and credited peer to an internal scheduling parent, preventing replay
/// across scheduling contexts.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingInfoPayload {
	/// Which core to use (indexes into the parachain's assigned cores).
	pub core_selector: CoreSelector,
	/// The claim queue offset.
	pub claim_queue_offset: u8,
	/// Peer ID credited for successful collation delivery.
	pub peer_id: ApprovedPeerId,
	/// The internal scheduling parent whose slot decides the eligible author that must sign this
	/// payload.
	pub internal_scheduling_parent: polkadot_primitives::Hash,
}

/// Signed scheduling info for candidate resubmission: a [`SchedulingInfoPayload`] plus the
/// collator's signature over it, proving eligibility for the slot at `internal_scheduling_parent`.
///
/// `claim_queue_offset` comes from the runtime's `relay_parent_offset`, not this struct, so the
/// collator cannot override it.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SignedSchedulingInfo {
	/// The scheduling information.
	pub payload: SchedulingInfoPayload,
	/// The eligible collator's signature over the SCALE-encoded [`SchedulingInfoPayload`].
	///
	/// A fixed 64-byte blob, decodable as either sr25519 or ed25519 (both are 64 bytes).
	pub signature: [u8; 64],
}

impl SchedulingInfoPayload {
	/// Create a new scheduling info payload.
	pub fn new(
		core_selector: CoreSelector,
		claim_queue_offset: u8,
		peer_id: ApprovedPeerId,
		internal_scheduling_parent: polkadot_primitives::Hash,
	) -> Self {
		Self { core_selector, claim_queue_offset, peer_id, internal_scheduling_parent }
	}
}

/// V3 scheduling proof included in the PoV.
///
/// Proves ancestry from `scheduling_parent` back to the internal scheduling parent; the PVF
/// validates it against the `relay_parent`/`scheduling_parent` in the candidate descriptor.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingProof {
	/// Relay chain headers chaining `scheduling_parent` backward: each header's `parent_hash` is
	/// the next header's hash. The first header hashes to the candidate's `scheduling_parent`, the
	/// last header's `parent_hash` is the internal scheduling parent. Length is the runtime's
	/// `RelayParentOffset`.
	pub header_chain: Vec<RelayChainHeader>,
	/// The header at `internal_scheduling_parent`; its hash must equal the internal scheduling
	/// parent derived from `header_chain` (the last header's `parent_hash`, or `scheduling_parent`
	/// if the chain is empty).
	pub internal_scheduling_parent_header: RelayChainHeader,
	/// Optional signed core-selection override:
	///
	/// - `None`, `relay_parent == internal_scheduling_parent`: initial submission; core selection
	///   comes from the block's UMP signals.
	/// - `Some`, `relay_parent == internal_scheduling_parent`: initial submission with an explicit
	///   (optional) core selection.
	/// - `Some`, `relay_parent != internal_scheduling_parent`: resubmission (required); the
	///   signature overrides the block's UMP signals and is verified against the eligible author
	///   for the slot at `internal_scheduling_parent`.
	pub signed_scheduling_info: Option<SignedSchedulingInfo>,
}

impl SchedulingProof {
	/// Create a new scheduling proof.
	pub fn new(
		header_chain: Vec<RelayChainHeader>,
		internal_scheduling_parent_header: RelayChainHeader,
		signed_scheduling_info: Option<SignedSchedulingInfo>,
	) -> Self {
		Self { header_chain, internal_scheduling_parent_header, signed_scheduling_info }
	}

	/// The scheduling parent hash: the first/newest header in `header_chain`, or
	/// `internal_scheduling_parent_header.hash()` when the chain is empty (they coincide at
	/// `relay_parent_offset = 0`).
	pub fn scheduling_parent(&self) -> polkadot_primitives::Hash {
		self.header_chain
			.first()
			.map(BlakeTwo256::hash_of)
			.unwrap_or_else(|| self.internal_scheduling_parent_header.hash())
	}
}

/// Verifier for V3 scheduling: reports whether V3 is enabled and verifies a candidate's
/// [`SignedSchedulingInfo`].
pub trait VerifySchedulingSignature {
	/// Whether V3 scheduling validation is enabled.
	const V3_SCHEDULING_ENABLED: bool;

	/// Verify `signed_info` against the author eligible at `relay_slot` (the internal scheduling
	/// parent's slot).
	fn verify(signed_info: &SignedSchedulingInfo, relay_slot: Slot) -> bool;
}

/// Default no-op wiring: V3 disabled, scheduling info accepted unconditionally. A real verifier
/// should also turn V3 on.
impl VerifySchedulingSignature for () {
	const V3_SCHEDULING_ENABLED: bool = false;

	fn verify(_signed_info: &SignedSchedulingInfo, _relay_slot: Slot) -> bool {
		true
	}
}

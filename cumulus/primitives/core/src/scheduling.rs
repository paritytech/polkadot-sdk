// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling types for low-latency parachain block production.
//!
//! V3 candidates separate the relay parent (execution context) from the scheduling
//! parent (a recent relay chain tip used for core assignment). This enables building
//! on older relay parents while still being scheduled based on recent relay state.
//!
//! # Resubmission
//!
//! When a candidate fails to get backed in time, a different collator can resubmit
//! it with a new `scheduling_parent` (fresh relay tip) without re-executing the blocks.
//! The `relay_parent` stays the same since the execution context hasn't changed.
//!
//! For resubmission, `signed_scheduling_info` must be provided. The resubmitting
//! collator signs the core selection, proving they are the eligible parachain author
//! for the slot derived from `internal_scheduling_parent`. The `internal_scheduling_parent`
//! is also bundled into the signed payload so the signature binds to this specific
//! scheduling chain and is not reusable on a different one.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use polkadot_primitives::{ApprovedPeerId, CoreSelector, Header as RelayChainHeader};
use sp_runtime::traits::{BlakeTwo256, Hash as HashT};

/// Payload signed by a collator for resubmission.
///
/// This binds the core selection to a specific internal scheduling parent,
/// preventing replay attacks across different scheduling contexts.
///
/// Note: `claim_queue_offset` is NOT included because it's derived from the
/// runtime's `relay_parent_offset` configuration - the collator cannot override it.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingInfoPayload {
	/// Which core to use (indexes into the parachain's assigned cores).
	pub core_selector: CoreSelector,
	/// The internal scheduling parent hash. Bundled into the signed payload as
	/// anti-replay binding so the signature is tied to this specific scheduling
	/// chain and cannot be replayed against another one. (The slot used for author
	/// lookup comes from `SchedulingProof::header_chain.last()`, not from this hash.)
	pub internal_scheduling_parent: polkadot_primitives::Hash,
}

/// Signed scheduling information for candidate resubmission.
///
/// When a collator resubmits a candidate (with a newer `scheduling_parent` but same
/// `relay_parent`), they must sign the core selection to prove eligibility for the
/// slot at `internal_scheduling_parent`.
///
/// The `claim_queue_offset` is derived from the runtime's `relay_parent_offset`
/// configuration and is not part of this struct - it cannot be overridden by the
/// collator.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SignedSchedulingInfo {
	/// Which core to use (indexes into the parachain's assigned cores).
	pub core_selector: CoreSelector,
	/// Peer ID to receive reputation credit for successful collation delivery.
	/// Overrides the peer ID from the block's commitments, allowing the
	/// resubmitting collator to receive reputation instead of the original
	/// block author who failed to deliver.
	pub peer_id: ApprovedPeerId,
	/// Signature by the eligible parachain Aura author for the slot at the oldest
	/// header in the scheduling proof's chain (= `header_chain.last()`). Signs
	/// `SchedulingInfoPayload(core_selector, internal_scheduling_parent)`.
	///
	/// The verifier derives the parachain slot from the BABE pre-digest of
	/// `SchedulingProof::header_chain.last()` and looks up the eligible Aura author
	/// from the parachain's authority set. This header is candidate-specific and
	/// tamper-proof: the chain-linkage check in `check_scheduling` proves it's the
	/// actual relay block `RelayParentOffset − 1` hops behind `scheduling_parent`.
	///
	/// The `internal_scheduling_parent` hash in the payload further binds the
	/// signature to this specific scheduling chain so it cannot be replayed against
	/// another one.
	///
	/// Stored as a fixed 64-byte blob so the verifier can decode it as either an sr25519
	/// or ed25519 signature, depending on the parachain's Aura authority crypto. Both
	/// schemes produce 64-byte signatures.
	pub signature: [u8; 64],
}

impl SchedulingInfoPayload {
	/// Create a new scheduling info payload.
	pub fn new(
		core_selector: CoreSelector,
		internal_scheduling_parent: polkadot_primitives::Hash,
	) -> Self {
		Self { core_selector, internal_scheduling_parent }
	}
}

/// V3 scheduling proof included in the POV.
///
/// Provides the ancestry from scheduling_parent back to the internal scheduling
/// parent. The PVF validates this against the relay_parent and scheduling_parent
/// from the candidate descriptor extension.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingProof {
	/// Relay chain headers proving ancestry from scheduling_parent backward.
	///
	/// Forms a chain where each header's parent_hash equals the next header's hash.
	/// The first header's hash must equal the candidate's scheduling_parent.
	/// The last header's parent_hash is the internal scheduling parent.
	/// Length is defined by the parachain runtime config (RelayParentOffset).
	pub header_chain: Vec<RelayChainHeader>,
	/// Signed scheduling info for core selection override.
	///
	/// - `None` with `relay_parent == internal_scheduling_parent`: Initial submission. Core
	///   selection comes from the parachain block's UMP signals.
	///
	/// - `Some` with `relay_parent == internal_scheduling_parent`: Initial submission with
	///   explicit core selection. This is optional but legal. Collators should refuse to
	///   acknowledge blocks with invalid scheduling info, so providing a signature is not required
	///   for initial submissions.
	///
	/// - `Some` with `relay_parent != internal_scheduling_parent`: Resubmission (required). The
	///   resubmitting collator signs the core selection, overriding the block's UMP signals.
	///   Signature is verified against the eligible author for the slot at
	///   `internal_scheduling_parent`.
	pub signed_scheduling_info: Option<SignedSchedulingInfo>,
}

impl SchedulingProof {
	/// Derive the scheduling parent hash from the header chain.
	///
	/// Returns `Some(hash)` if the header chain is non-empty (hash of the first/newest header),
	/// or `None` if the chain is empty (scheduling_parent == relay_parent).
	pub fn scheduling_parent(&self) -> Option<polkadot_primitives::Hash> {
		self.header_chain.first().map(BlakeTwo256::hash_of)
	}
}

/// Verifies a [`SignedSchedulingInfo`] against the parachain's eligible Aura author.
///
/// Wired into [`cumulus_pallet_parachain_system::Config`] (via an associated type) and
/// called from the PVF `validate_block` path. The default implementation in the runtime
/// composition is [`NoVerification`]; parachains that opt into V3 resubmission supply a
/// real implementation (e.g. `AuraSchedulingVerifier` from `cumulus-pallet-aura-ext`).
///
/// The verifier receives the candidate's `slot_anchor_header` — `header_chain.last()`,
/// the oldest header in the scheduling proof. It extracts the parachain slot from
/// that header's BABE pre-digest, looks up the eligible Aura author, and verifies the
/// 64-byte signature against [`SchedulingInfoPayload`]. The chain-linkage check in
/// `check_scheduling` already proves this header is tamper-proof.
pub trait VerifySchedulingSignature {
	/// Returns `true` if `signed_info.signature` is a valid signature over
	/// `SchedulingInfoPayload(signed_info.core_selector, internal_scheduling_parent)`
	/// by the parachain Aura author eligible at the slot of `slot_anchor_header`.
	fn verify(
		signed_info: &SignedSchedulingInfo,
		slot_anchor_header: &RelayChainHeader,
		internal_scheduling_parent: polkadot_primitives::Hash,
	) -> bool;
}

/// No-op verifier: always returns `true`.
///
/// Default for parachain runtimes that haven't opted into V3 resubmission verification.
/// Wiring a real verifier (e.g. `AuraSchedulingVerifier`) replaces this.
pub struct NoVerification;

impl VerifySchedulingSignature for NoVerification {
	fn verify(
		_signed_info: &SignedSchedulingInfo,
		_slot_anchor_header: &RelayChainHeader,
		_internal_scheduling_parent: polkadot_primitives::Hash,
	) -> bool {
		true
	}
}

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
//! collator signs the core selection, proving they are the eligible author for the
//! slot derived from the `internal_scheduling_parent`.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use polkadot_primitives::{CollatorId, CollatorSignature, CoreSelector, Header as RelayChainHeader};
use sp_runtime::traits::AppVerify;

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
    /// The internal scheduling parent whom's slot decides the 
    /// eligible block author that must sign the payload.
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
    /// Signature by the eligible collator for the slot at `internal_scheduling_parent`.
    /// Signs `SchedulingInfoPayload(core_selector, internal_scheduling_parent)`.
    pub signature: CollatorSignature,
}

impl SignedSchedulingInfo {
    /// Verify the signature against the expected collator.
    ///
    /// # Arguments
    /// * `expected_collator` - The collator ID that should have signed this
    /// * `internal_scheduling_parent` - The internal scheduling parent hash
    ///
    /// # Returns
    /// `true` if the signature is valid for the expected collator.
    pub fn verify(
        &self,
        expected_collator: &CollatorId,
        internal_scheduling_parent: polkadot_primitives::Hash,
    ) -> bool {
        let payload = SchedulingInfoPayload {
            core_selector: self.core_selector.clone(),
            internal_scheduling_parent,
        };
        let encoded = payload.encode();
        self.signature.verify(encoded.as_slice(), expected_collator)
    }
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
    /// - `None` with `relay_parent == internal_scheduling_parent`: Initial submission.
    ///   Core selection comes from the parachain block's UMP signals.
    ///
    /// - `Some` with `relay_parent == internal_scheduling_parent`: Initial submission with
    ///   explicit core selection. This is optional but legal. Collators should refuse to
    ///   acknowledge blocks with invalid scheduling info, so providing a signature is not
    ///   required for initial submissions.
    ///
    /// - `Some` with `relay_parent != internal_scheduling_parent`: Resubmission (required).
    ///   The resubmitting collator signs the core selection, overriding the block's UMP signals.
    ///   Signature is verified against the eligible author for the slot at
    ///   `internal_scheduling_parent`.
    pub signed_scheduling_info: Option<SignedSchedulingInfo>,
}

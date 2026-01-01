// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling types for low-latency parachain block production.
//!
//! V3 candidates separate the relay parent (execution context) from the scheduling
//! parent (a recent relay chain tip used for core assignment). This enables building
//! on older relay parents while still being scheduled based on recent relay state.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use polkadot_primitives::Header as RelayChainHeader;

/// V3 scheduling proof included in the POV.
///
/// Provides the ancestry from scheduling_parent back to the internal scheduling
/// parent. The PVF validates this against the relay_parent and scheduling_parent
/// from the candidate descriptor extension.
///
/// The core assignment (core_index, claim_queue_offset) is extracted from the
/// parachain block's UMP signals, not from this struct.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingProof {
    /// Relay chain headers proving ancestry from scheduling_parent backward.
    ///
    /// Forms a chain where each header's parent_hash equals the next header's hash.
    /// The first header's hash must equal the candidate's scheduling_parent.
    /// The last header's parent_hash is the internal scheduling parent.
    /// Length is defined by the parachain runtime config (RelayParentOffset).
    ///
    /// For initial submission (no re-submission), relay_parent should equal
    /// the internal_scheduling_parent (last header's parent_hash).
    pub header_chain: Vec<RelayChainHeader>,
}

// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Scheduling validation for V3 candidates.
//!
//! Validates the header chain from scheduling_parent to internal_scheduling_parent,
//! and verifies relay_parent is at or before internal_scheduling_parent.

use cumulus_primitives_core::SchedulingProof;
use sp_runtime::traits::{BlakeTwo256, Hash as HashT, Header as HeaderT};

/// Hash type for relay chain.
pub type RelayHash = polkadot_core_primitives::Hash;

/// Errors that can occur during scheduling validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulingValidationError {
    /// Header chain has wrong length.
    InvalidHeaderChainLength { expected: u32, actual: usize },
    /// Header chain does not form a valid chain.
    BrokenHeaderChain { index: usize },
    /// First header hash does not match scheduling_parent.
    SchedulingParentMismatch,
    /// relay_parent is not at or before internal_scheduling_parent.
    RelayParentNotAtOrBeforeInternalSchedulingParent,
}

/// Result of successful scheduling validation.
#[derive(Debug, Clone)]
pub struct SchedulingValidationResult {
    /// The internal scheduling parent (derived from header chain).
    pub internal_scheduling_parent: RelayHash,
}

/// Validate scheduling proof from the POV.
///
/// This function:
/// 1. Verifies the header chain has the expected fixed length
/// 2. Verifies headers form a valid chain starting at scheduling_parent
/// 3. Derives internal_scheduling_parent from the header chain
/// 4. Verifies relay_parent equals internal_scheduling_parent (for initial submission)
///
/// # Arguments
/// * `scheduling_proof` - The scheduling proof from POV (ParachainBlockData::V2)
/// * `relay_parent` - The relay parent from the candidate descriptor extension
/// * `scheduling_parent` - The scheduling parent from the candidate descriptor extension
/// * `expected_header_chain_length` - The fixed length expected by the parachain runtime
pub fn validate_scheduling(
    scheduling_proof: &SchedulingProof,
    relay_parent: RelayHash,
    scheduling_parent: RelayHash,
    expected_header_chain_length: u32,
) -> Result<SchedulingValidationResult, SchedulingValidationError> {
    let header_chain = &scheduling_proof.header_chain;

    // 1. Verify header chain length
    if header_chain.len() != expected_header_chain_length as usize {
        return Err(SchedulingValidationError::InvalidHeaderChainLength {
            expected: expected_header_chain_length,
            actual: header_chain.len(),
        });
    }

    // 2. Verify header chain forms a valid chain
    // First header's hash must equal scheduling_parent
    if !header_chain.is_empty() {
        let first_header_hash = BlakeTwo256::hash_of(&header_chain[0]);
        if first_header_hash != scheduling_parent {
            return Err(SchedulingValidationError::SchedulingParentMismatch);
        }
    }

    // Each header's parent_hash must match the hash of the next header
    for i in 0..header_chain.len().saturating_sub(1) {
        let current_parent = header_chain[i].parent_hash();
        let next_hash = BlakeTwo256::hash_of(&header_chain[i + 1]);
        if *current_parent != next_hash {
            return Err(SchedulingValidationError::BrokenHeaderChain { index: i });
        }
    }

    // 3. Derive internal_scheduling_parent
    // It's the parent_hash of the last (oldest) header in the chain
    let internal_scheduling_parent = if header_chain.is_empty() {
        // If header chain is empty (length 0), internal_scheduling_parent == scheduling_parent
        scheduling_parent
    } else {
        *header_chain.last().expect("checked non-empty").parent_hash()
    };

    // 4. For initial submission, relay_parent must equal internal_scheduling_parent
    // Re-submission support (relay_parent != internal_scheduling_parent) is future work
    if relay_parent != internal_scheduling_parent {
        return Err(SchedulingValidationError::RelayParentNotAtOrBeforeInternalSchedulingParent);
    }

    Ok(SchedulingValidationResult { internal_scheduling_parent })
}

#[cfg(test)]
mod tests {
    // TODO: Add tests for:
    // - Valid header chain with matching lengths
    // - Invalid header chain length
    // - Broken header chain
    // - relay_parent == internal_scheduling_parent (should pass)
    // - relay_parent != internal_scheduling_parent (should fail for now)
}

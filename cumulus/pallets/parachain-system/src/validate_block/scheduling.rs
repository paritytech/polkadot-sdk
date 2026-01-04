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
pub type RelayHash = sp_core::H256;

/// Errors that can occur during scheduling validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulingValidationError {
    /// Header chain has wrong length.
    InvalidHeaderChainLength { expected: u32, actual: usize },
    /// Header chain does not form a valid chain.
    BrokenHeaderChain { index: usize },
    /// First header hash does not match scheduling_parent.
    SchedulingParentMismatch,
    /// relay_parent is within the header chain but not at internal_scheduling_parent.
    /// For resubmission, relay_parent must be an ancestor of internal_scheduling_parent.
    RelayParentInHeaderChain,

    /// Resubmission is missing required signed_scheduling_info.
    /// When relay_parent != internal_scheduling_parent, the resubmitting collator must
    /// sign the core selection to prove slot eligibility.
    MissingSignedSchedulingInfo,
    /// Signature verification failed for resubmission.
    /// The signature does not match the expected eligible collator for the slot.
    InvalidSignature,
}

/// Result of successful scheduling validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingValidationResult {
    /// The internal scheduling parent (derived from header chain).
    pub internal_scheduling_parent: RelayHash,
    /// Whether this is a resubmission (relay_parent != internal_scheduling_parent).
    pub is_resubmission: bool,
}

/// Validate scheduling proof from the POV.
///
/// This function:
/// 1. Verifies the header chain has the expected fixed length
/// 2. Verifies headers form a valid chain starting at scheduling_parent
/// 3. Derives internal_scheduling_parent from the header chain
/// 4. Validates relay_parent position and signed_scheduling_info presence
///
/// # relay_parent validation
///
/// The relay_parent must either:
/// - Equal internal_scheduling_parent (initial submission, no signature required)
/// - Be an ancestor of internal_scheduling_parent (resubmission, signature required)
///
/// relay_parent must NOT be within the header chain itself (between scheduling_parent
/// and internal_scheduling_parent), as that would indicate an invalid resubmission.
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

    // 4. Validate relay_parent position
    // relay_parent must NOT be inside the header chain (it can equal internal_scheduling_parent
    // or be an ancestor of it, but not somewhere between scheduling_parent and
    // internal_scheduling_parent)
    for header in header_chain.iter() {
        let header_hash = BlakeTwo256::hash_of(header);
        if relay_parent == header_hash {
            return Err(SchedulingValidationError::RelayParentInHeaderChain);
        }
    }

    // 5. Validate signed_scheduling_info based on relay_parent position
    let is_initial_submission = relay_parent == internal_scheduling_parent;

    if !is_initial_submission {
        // Resubmission: relay_parent is an ancestor of internal_scheduling_parent.
        // The resubmitting collator must sign the core selection.
        if scheduling_proof.signed_scheduling_info.is_none() {
            return Err(SchedulingValidationError::MissingSignedSchedulingInfo);
        }
        // Signature verification is done separately after slot/authority lookup
    }
    // Note: For initial submission (relay_parent == internal_scheduling_parent),
    // signed_scheduling_info is optional. If absent, core selection comes from the
    // block's UMP signals. If present, signature verification is still performed.
    // Collators should refuse to acknowledge blocks with invalid scheduling info,
    // so providing signed_scheduling_info is not necessary but is legal.

    Ok(SchedulingValidationResult {
        internal_scheduling_parent,
        is_resubmission: !is_initial_submission,
    })
}

/// Verify the signature in signed_scheduling_info for a resubmission.
///
/// This should only be called after `validate_scheduling` returns successfully with
/// `is_resubmission: true`. The caller must provide the eligible collator derived
/// from the Aura authorities at the first block's state.
///
/// # Arguments
/// * `signed_scheduling_info` - The signed scheduling info from the proof
/// * `expected_collator` - The eligible collator for the slot (from `slot % authorities.len()`)
/// * `internal_scheduling_parent` - The internal scheduling parent hash
///
/// # Returns
/// `Ok(())` if the signature is valid, `Err(InvalidSignature)` otherwise.
pub fn verify_resubmission_signature(
    signed_scheduling_info: &cumulus_primitives_core::SignedSchedulingInfo,
    expected_collator: &cumulus_primitives_core::relay_chain::CollatorId,
    internal_scheduling_parent: RelayHash,
) -> Result<(), SchedulingValidationError> {
    if signed_scheduling_info.verify(expected_collator, internal_scheduling_parent) {
        Ok(())
    } else {
        Err(SchedulingValidationError::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;
    use cumulus_primitives_core::{
        relay_chain::CollatorSignature, CoreSelector, SchedulingProof, SignedSchedulingInfo,
    };
    use sp_core::crypto::UncheckedFrom;
    use sp_runtime::generic::Header;
    use sp_runtime::traits::BlakeTwo256;

    type RelayHeader = Header<u32, BlakeTwo256>;

    /// Creates a dummy signature for testing (not cryptographically valid).
    fn dummy_signature() -> CollatorSignature {
        CollatorSignature::unchecked_from([0u8; 64])
    }

    /// Creates a chain of headers where each header's parent_hash points to the next.
    /// Returns headers ordered newest-to-oldest (index 0 = newest = scheduling_parent).
    fn make_header_chain(len: usize) -> (Vec<RelayHeader>, RelayHash) {
        if len == 0 {
            // For empty chain, return arbitrary hash as the "relay_parent"
            return (vec![], RelayHash::repeat_byte(0x00));
        }

        let mut headers = Vec::with_capacity(len);

        // Build from oldest to newest, then reverse
        // Start with oldest header pointing to relay_parent
        let relay_parent = RelayHash::repeat_byte(0x42);
        let mut parent_hash = relay_parent;

        for i in 0..len {
            let header = RelayHeader::new(
                (i + 1) as u32,  // block number
                Default::default(),
                Default::default(),
                parent_hash,
                Default::default(),
            );
            parent_hash = BlakeTwo256::hash_of(&header);
            headers.push(header);
        }

        // Reverse so newest is first (matches expected ordering)
        headers.reverse();
        (headers, relay_parent)
    }

    // =========================================================================
    // Valid cases
    // =========================================================================

    #[test]
    fn valid_header_chain_length_3() {
        // Test: A valid 3-header chain should validate successfully.
        let (headers, relay_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        assert!(result.is_ok());
        // internal_scheduling_parent should equal relay_parent for valid chains
        assert_eq!(result.unwrap().internal_scheduling_parent, relay_parent);
    }

    #[test]
    fn valid_empty_header_chain() {
        // Test: Empty chain (offset=0) means scheduling_parent == relay_parent.
        let scheduling_parent = RelayHash::repeat_byte(0xAA);
        let relay_parent = scheduling_parent; // Must be equal for offset=0

        let proof = SchedulingProof { header_chain: vec![], signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().internal_scheduling_parent, scheduling_parent);
    }

    #[test]
    fn valid_single_header_chain() {
        // Test: Single header chain (offset=1).
        let (headers, relay_parent) = make_header_chain(1);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 1);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().internal_scheduling_parent, relay_parent);
    }

    // =========================================================================
    // Invalid length cases
    // =========================================================================

    #[test]
    fn reject_wrong_header_chain_length_too_short() {
        // Test: Chain shorter than expected should be rejected.
        let (headers, relay_parent) = make_header_chain(2);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        // Expect 3, but only 2 provided
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        assert_eq!(
            result,
            Err(SchedulingValidationError::InvalidHeaderChainLength {
                expected: 3,
                actual: 2
            })
        );
    }

    #[test]
    fn reject_wrong_header_chain_length_too_long() {
        // Test: Chain longer than expected should be rejected.
        let (headers, relay_parent) = make_header_chain(4);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        // Expect 3, but 4 provided
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        assert_eq!(
            result,
            Err(SchedulingValidationError::InvalidHeaderChainLength {
                expected: 3,
                actual: 4
            })
        );
    }

    // =========================================================================
    // Invalid scheduling_parent cases
    // =========================================================================

    #[test]
    fn reject_scheduling_parent_mismatch() {
        // Test: scheduling_parent must hash to the first header.
        let (headers, relay_parent) = make_header_chain(3);
        let wrong_scheduling_parent = RelayHash::repeat_byte(0xFF);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, wrong_scheduling_parent, 3);

        assert_eq!(result, Err(SchedulingValidationError::SchedulingParentMismatch));
    }

    // =========================================================================
    // Broken header chain cases
    // =========================================================================

    #[test]
    fn reject_broken_header_chain() {
        // Test: Headers must form a valid chain via parent_hash linkage.
        let (mut headers, relay_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        // Corrupt the middle header's parent_hash to break the chain
        headers[1] = RelayHeader::new(
            99,
            Default::default(),
            Default::default(),
            RelayHash::repeat_byte(0xDE), // Wrong parent hash
            Default::default(),
        );

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        // Chain breaks at index 0 (first header's parent doesn't match second header's hash)
        assert_eq!(result, Err(SchedulingValidationError::BrokenHeaderChain { index: 0 }));
    }

    // =========================================================================
    // relay_parent validation cases
    // =========================================================================

    #[test]
    fn reject_relay_parent_inside_header_chain() {
        // Test: relay_parent must not be one of the headers in the chain.
        // It should either equal internal_scheduling_parent or be an ancestor of it.
        let (headers, _correct_relay_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);
        // Use the middle header's hash as relay_parent (invalid)
        let relay_parent_in_chain = BlakeTwo256::hash_of(&headers[1]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent_in_chain, scheduling_parent, 3);

        assert_eq!(result, Err(SchedulingValidationError::RelayParentInHeaderChain));
    }

    // =========================================================================
    // Resubmission validation cases
    // =========================================================================

    #[test]
    fn initial_submission_allows_signed_scheduling_info() {
        // Test: Initial submission (relay_parent == internal_scheduling_parent) may
        // optionally include signed_scheduling_info. This is legal because collators
        // should refuse to acknowledge blocks with invalid scheduling info anyway.
        let (headers, relay_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let signed_info = SignedSchedulingInfo {
            core_selector: CoreSelector(0),

            signature: dummy_signature(),
        };

        let proof = SchedulingProof {
            header_chain: headers,
            signed_scheduling_info: Some(signed_info),
        };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        // Validation passes - signed_scheduling_info is optional for initial submission
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_resubmission);
    }

    #[test]
    fn reject_resubmission_without_signed_scheduling_info() {
        // Test: Resubmission (relay_parent != internal_scheduling_parent) requires
        // signed_scheduling_info to prove the resubmitting collator's eligibility.
        let (headers, _internal_scheduling_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);
        // Use an unrelated hash as relay_parent (simulates resubmission)
        let older_relay_parent = RelayHash::repeat_byte(0xBB);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, older_relay_parent, scheduling_parent, 3);

        assert_eq!(result, Err(SchedulingValidationError::MissingSignedSchedulingInfo));
    }

    #[test]
    fn valid_resubmission_with_signed_scheduling_info() {
        // Test: Resubmission with signed_scheduling_info passes validation
        // (signature verification happens separately).
        let (headers, internal_scheduling_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);
        // Use an unrelated hash as relay_parent (simulates resubmission where
        // relay_parent is an ancestor of internal_scheduling_parent)
        let older_relay_parent = RelayHash::repeat_byte(0xBB);

        let signed_info = SignedSchedulingInfo {
            core_selector: CoreSelector(0),

            signature: dummy_signature(),
        };

        let proof = SchedulingProof {
            header_chain: headers,
            signed_scheduling_info: Some(signed_info),
        };
        let result = validate_scheduling(&proof, older_relay_parent, scheduling_parent, 3);

        // Validation passes - signature verification is done separately
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.is_resubmission);
        assert_eq!(result.internal_scheduling_parent, internal_scheduling_parent);
    }

    #[test]
    fn initial_submission_is_not_resubmission() {
        // Test: Initial submission has is_resubmission = false
        let (headers, relay_parent) = make_header_chain(3);
        let scheduling_parent = BlakeTwo256::hash_of(&headers[0]);

        let proof = SchedulingProof { header_chain: headers, signed_scheduling_info: None };
        let result = validate_scheduling(&proof, relay_parent, scheduling_parent, 3);

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_resubmission);
        assert_eq!(result.internal_scheduling_parent, relay_parent);
    }

    // =========================================================================
    // Signature verification tests
    // =========================================================================

    #[test]
    fn verify_resubmission_signature_valid() {
        // Test: Valid signature from correct collator passes verification
        use cumulus_primitives_core::SchedulingInfoPayload;
        use sp_core::Pair;

        let internal_scheduling_parent = RelayHash::repeat_byte(0x42);

        // Create a keypair and derive the collator ID
        let keypair = sp_core::sr25519::Pair::from_seed(&[1u8; 32]);
        let collator_id: cumulus_primitives_core::relay_chain::CollatorId = keypair.public().into();

        // Create the payload and sign it
        let payload = SchedulingInfoPayload::new(CoreSelector(1), internal_scheduling_parent);
        let signature: CollatorSignature = keypair.sign(&payload.encode()).into();

        let signed_info = SignedSchedulingInfo {
            core_selector: CoreSelector(1),

            signature,
        };

        let result =
            verify_resubmission_signature(&signed_info, &collator_id, internal_scheduling_parent);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_resubmission_signature_wrong_collator() {
        // Test: Signature from wrong collator fails verification
        use cumulus_primitives_core::SchedulingInfoPayload;
        use sp_core::Pair;

        let internal_scheduling_parent = RelayHash::repeat_byte(0x42);

        // Create keypair for signing
        let signing_keypair = sp_core::sr25519::Pair::from_seed(&[1u8; 32]);

        // Create a different keypair for expected collator
        let expected_keypair = sp_core::sr25519::Pair::from_seed(&[2u8; 32]);
        let expected_collator: cumulus_primitives_core::relay_chain::CollatorId =
            expected_keypair.public().into();

        // Sign with the wrong key
        let payload = SchedulingInfoPayload::new(CoreSelector(1), internal_scheduling_parent);
        let signature: CollatorSignature = signing_keypair.sign(&payload.encode()).into();

        let signed_info = SignedSchedulingInfo {
            core_selector: CoreSelector(1),

            signature,
        };

        let result =
            verify_resubmission_signature(&signed_info, &expected_collator, internal_scheduling_parent);
        assert_eq!(result, Err(SchedulingValidationError::InvalidSignature));
    }

    #[test]
    fn verify_resubmission_signature_wrong_internal_scheduling_parent() {
        // Test: Signature for different internal_scheduling_parent fails verification
        use cumulus_primitives_core::SchedulingInfoPayload;
        use sp_core::Pair;

        let signed_isp = RelayHash::repeat_byte(0x42);
        let verify_isp = RelayHash::repeat_byte(0x43); // Different!

        let keypair = sp_core::sr25519::Pair::from_seed(&[1u8; 32]);
        let collator_id: cumulus_primitives_core::relay_chain::CollatorId = keypair.public().into();

        // Sign for one internal_scheduling_parent
        let payload = SchedulingInfoPayload::new(CoreSelector(1), signed_isp);
        let signature: CollatorSignature = keypair.sign(&payload.encode()).into();

        let signed_info = SignedSchedulingInfo {
            core_selector: CoreSelector(1),

            signature,
        };

        // Verify against a different internal_scheduling_parent
        let result = verify_resubmission_signature(&signed_info, &collator_id, verify_isp);
        assert_eq!(result, Err(SchedulingValidationError::InvalidSignature));
    }
}

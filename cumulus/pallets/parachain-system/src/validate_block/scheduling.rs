// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Scheduling validation for V3 candidates.
//!
//! Validates the header chain from scheduling_parent to internal_scheduling_parent,
//! and verifies relay_parent is at or before internal_scheduling_parent.

use cumulus_primitives_core::SchedulingProof;
use polkadot_parachain_primitives::primitives::ValidationParamsExtension;
use sp_runtime::traits::Header as HeaderT;

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
}

/// Result of successful scheduling validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingValidationResult {
	/// The internal scheduling parent (derived from header chain).
	pub internal_scheduling_parent: RelayHash,
	/// Whether this is a resubmission (relay_parent != internal_scheduling_parent).
	pub is_resubmission: bool,
}

/// Validate V3 scheduling based on runtime config and candidate extension.
///
/// Returns `None` for V1/V2 candidates, `Some(result)` for valid V3.
/// Panics on config/extension mismatches or validation failures.
pub fn validate_v3_scheduling(
	v3_enabled: bool,
	extension: &Option<ValidationParamsExtension>,
	scheduling_proof: Option<&SchedulingProof>,
	expected_header_chain_length: u32,
) -> Option<SchedulingValidationResult> {
	match (v3_enabled, extension) {
		(false, None) => {
			// V3 disabled and no extension: normal V1/V2 path
			None
		},
		(false, Some(_)) => {
			// V3 disabled but extension present: this should not happen
			// The relay chain should not send V3 candidates to parachains that have not enabled it
			panic!(
				"V3 extension present but V3 scheduling is disabled. \
                Ensure collators and runtime are in sync."
			);
		},
		(true, None) => {
			// V3 enabled but no extension: candidates must be V3
			panic!(
				"V3 scheduling is enabled but no V3 extension present. \
                Collators must provide V3 candidates when V3 is enabled."
			);
		},
		(true, Some(ValidationParamsExtension::V3 { relay_parent, scheduling_parent })) => {
			// V3 enabled and extension present: validate scheduling
			let scheduling_proof = scheduling_proof
				.expect("V3 candidates require ParachainBlockData::V2 with scheduling_proof");

			match check_scheduling(
				scheduling_proof,
				*relay_parent,
				*scheduling_parent,
				expected_header_chain_length,
			) {
				Ok(result) => Some(result),
				Err(e) => panic!("V3 scheduling validation failed: {:?}", e),
			}
		},
	}
}

/// Check the scheduling proof against the relay parent, scheduling parent,
/// and expected header chain length. Returns the internal scheduling parent
/// and whether this is a resubmission.
pub fn check_scheduling(
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
		let first_header_hash = header_chain[0].hash();
		if first_header_hash != scheduling_parent {
			return Err(SchedulingValidationError::SchedulingParentMismatch);
		}
	}

	// Each header's parent_hash must match the hash of the next header
	for i in 0..header_chain.len().saturating_sub(1) {
		let current_parent = header_chain[i].parent_hash();
		let next_hash = header_chain[i + 1].hash();
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
		let header_hash = header.hash();
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

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::{
		CoreSelector, SchedulingInfoPayload, SchedulingProof, SignedSchedulingInfo,
	};
	use sp_runtime::{generic::Header, traits::BlakeTwo256};

	type RelayHeader = Header<u32, BlakeTwo256>;

	/// Creates a dummy signature blob for testing (not cryptographically valid).
	fn dummy_signature() -> [u8; 64] {
		[0u8; 64]
	}

	/// Creates a chain of headers where each header's parent_hash points to the next, plus the
	/// relay header at `internal_scheduling_parent` (its hash equals the chain's last header's
	/// `parent_hash`, or `scheduling_parent` for an empty chain).
	///
	/// Returns:
	/// - chain headers ordered newest-to-oldest (index 0 = newest = scheduling_parent),
	/// - and the internal scheduling parent header.
	fn make_header_chain(len: usize) -> (Vec<RelayHeader>, RelayHeader) {
		let isp_header = RelayHeader::new(
			0u32,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		let relay_parent = isp_header.hash();

		if len == 0 {
			return (vec![], isp_header);
		}

		let mut headers = Vec::with_capacity(len);
		let mut parent_hash = relay_parent;

		for i in 0..len {
			let header = RelayHeader::new(
				(i + 1) as u32, // block number
				Default::default(),
				Default::default(),
				parent_hash,
				Default::default(),
			);
			parent_hash = header.hash();
			headers.push(header);
		}

		// Reverse so newest is first (matches expected ordering)
		headers.reverse();
		(headers, isp_header)
	}

	// =========================================================================
	// Valid cases
	// =========================================================================

	#[test]
	fn valid_header_chain_length_3() {
		// Test: A valid 3-header chain should validate successfully.
		let (headers, isp_header) = make_header_chain(3);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		assert!(result.is_ok());
		// internal_scheduling_parent should equal relay_parent for valid chains
		assert_eq!(result.unwrap().internal_scheduling_parent, relay_parent);
	}

	#[test]
	fn valid_empty_header_chain() {
		// Test: Empty chain (offset=0) means scheduling_parent == relay_parent.
		let (_, isp_header) = make_header_chain(0);
		let scheduling_parent = isp_header.hash();
		let relay_parent = scheduling_parent;

		let proof = SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 0);

		assert!(result.is_ok());
		assert_eq!(result.unwrap().internal_scheduling_parent, scheduling_parent);
	}

	#[test]
	fn valid_single_header_chain() {
		// Test: Single header chain (offset=1).
		let (headers, isp_header) = make_header_chain(1);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 1);

		assert!(result.is_ok());
		assert_eq!(result.unwrap().internal_scheduling_parent, relay_parent);
	}

	// =========================================================================
	// Invalid length cases
	// =========================================================================

	#[test]
	fn reject_wrong_header_chain_length_too_short() {
		// Test: Chain shorter than expected should be rejected.
		let (headers, isp_header) = make_header_chain(2);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		// Expect 3, but only 2 provided
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		assert_eq!(
			result,
			Err(SchedulingValidationError::InvalidHeaderChainLength { expected: 3, actual: 2 })
		);
	}

	#[test]
	fn reject_wrong_header_chain_length_too_long() {
		// Test: Chain longer than expected should be rejected.
		let (headers, isp_header) = make_header_chain(4);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		// Expect 3, but 4 provided
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		assert_eq!(
			result,
			Err(SchedulingValidationError::InvalidHeaderChainLength { expected: 3, actual: 4 })
		);
	}

	// =========================================================================
	// Invalid scheduling_parent cases
	// =========================================================================

	#[test]
	fn reject_scheduling_parent_mismatch() {
		// Test: scheduling_parent must hash to the first header.
		let (headers, isp_header) = make_header_chain(3);
		let relay_parent = isp_header.hash();
		let wrong_scheduling_parent = RelayHash::repeat_byte(0xFF);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, wrong_scheduling_parent, 3);

		assert_eq!(result, Err(SchedulingValidationError::SchedulingParentMismatch));
	}

	// =========================================================================
	// Broken header chain cases
	// =========================================================================

	#[test]
	fn reject_broken_header_chain() {
		// Test: Headers must form a valid chain via parent_hash linkage.
		let (mut headers, isp_header) = make_header_chain(3);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		// Corrupt the middle header's parent_hash to break the chain
		headers[1] = RelayHeader::new(
			99,
			Default::default(),
			Default::default(),
			RelayHash::repeat_byte(0xDE), // Wrong parent hash
			Default::default(),
		);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

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
		let (headers, isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		// Use the middle header's hash as relay_parent (invalid)
		let relay_parent_in_chain = headers[1].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent_in_chain, scheduling_parent, 3);

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
		let (headers, isp_header) = make_header_chain(3);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let signed_info = SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(
				CoreSelector(0),
				0,
				Default::default(),
				relay_parent,
			),
			signature: dummy_signature(),
		};

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: Some(signed_info),
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		// Validation passes - signed_scheduling_info is optional for initial submission
		assert!(result.is_ok());
		let result = result.unwrap();
		assert!(!result.is_resubmission);
	}

	#[test]
	fn reject_resubmission_without_signed_scheduling_info() {
		// Test: Resubmission (relay_parent != internal_scheduling_parent) requires
		// signed_scheduling_info to prove the resubmitting collator's eligibility.
		let (headers, isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		// Use an unrelated hash as relay_parent (simulates resubmission)
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, older_relay_parent, scheduling_parent, 3);

		assert_eq!(result, Err(SchedulingValidationError::MissingSignedSchedulingInfo));
	}

	#[test]
	fn valid_resubmission_with_signed_scheduling_info() {
		// Test: Resubmission with signed_scheduling_info passes validation
		// (signature verification happens separately).
		let (headers, isp_header) = make_header_chain(3);
		let internal_scheduling_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();
		// Use an unrelated hash as relay_parent (simulates resubmission where
		// relay_parent is an ancestor of internal_scheduling_parent)
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let signed_info = SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(
				CoreSelector(0),
				0,
				Default::default(),
				internal_scheduling_parent,
			),
			signature: dummy_signature(),
		};

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: Some(signed_info),
		};
		let result = check_scheduling(&proof, older_relay_parent, scheduling_parent, 3);

		// Validation passes - signature verification is done separately
		assert!(result.is_ok());
		let result = result.unwrap();
		assert!(result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, internal_scheduling_parent);
	}

	#[test]
	fn initial_submission_is_not_resubmission() {
		// Test: Initial submission has is_resubmission = false
		let (headers, isp_header) = make_header_chain(3);
		let relay_parent = isp_header.hash();
		let scheduling_parent = headers[0].hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		assert!(result.is_ok());
		let result = result.unwrap();
		assert!(!result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, relay_parent);
	}

	// =========================================================================
	// validate_v3_scheduling tests
	// =========================================================================

	/// Helper: builds a valid V3 extension and scheduling proof for a given header chain length.
	/// Returns (extension, proof, expected_result).
	fn make_v3_initial_submission(
		chain_len: u32,
	) -> (ValidationParamsExtension, SchedulingProof, SchedulingValidationResult) {
		let (headers, isp_header) = make_header_chain(chain_len as usize);
		let scheduling_parent =
			if headers.is_empty() { isp_header.hash() } else { headers[0].hash() };

		let extension =
			ValidationParamsExtension::V3 { relay_parent: isp_header.hash(), scheduling_parent };
		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};
		let expected = SchedulingValidationResult {
			internal_scheduling_parent: isp_header.hash(),
			is_resubmission: false,
		};
		(extension, proof, expected)
	}

	#[test]
	fn v3_disabled_no_extension_returns_none() {
		let result = validate_v3_scheduling(false, &None, None, 0);
		assert!(result.is_none());
	}

	#[test]
	#[should_panic(expected = "V3 extension present but V3 scheduling is disabled")]
	fn v3_disabled_with_extension_panics() {
		let ext = ValidationParamsExtension::V3 {
			relay_parent: RelayHash::default(),
			scheduling_parent: RelayHash::default(),
		};
		validate_v3_scheduling(false, &Some(ext), None, 0);
	}

	#[test]
	#[should_panic(expected = "V3 scheduling is enabled but no V3 extension present")]
	fn v3_enabled_no_extension_panics() {
		validate_v3_scheduling(true, &None, None, 0);
	}

	#[test]
	fn v3_enabled_valid_initial_submission() {
		let (ext, proof, expected) = make_v3_initial_submission(3);
		let result = validate_v3_scheduling(true, &Some(ext), Some(&proof), 3);
		assert_eq!(result, Some(expected));
	}

	#[test]
	fn v3_enabled_valid_empty_header_chain() {
		let (ext, proof, expected) = make_v3_initial_submission(0);
		let result = validate_v3_scheduling(true, &Some(ext), Some(&proof), 0);
		assert_eq!(result, Some(expected));
	}

	#[test]
	#[should_panic(expected = "V3 candidates require ParachainBlockData::V2 with scheduling_proof")]
	fn v3_enabled_missing_scheduling_proof_panics() {
		let (ext, _, _) = make_v3_initial_submission(3);
		// Pass None as scheduling_proof to simulate a V0/V1 POV
		validate_v3_scheduling(true, &Some(ext), None, 3);
	}

	#[test]
	#[should_panic(expected = "V3 scheduling validation failed")]
	fn v3_enabled_invalid_header_chain_length_panics() {
		let (ext, proof, _) = make_v3_initial_submission(3);
		// Expect 5 headers but proof only has 3
		validate_v3_scheduling(true, &Some(ext), Some(&proof), 5);
	}

	#[test]
	fn v3_enabled_valid_resubmission() {
		let (headers, isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		// Use an unrelated hash as relay_parent to simulate a resubmission
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let ext =
			ValidationParamsExtension::V3 { relay_parent: older_relay_parent, scheduling_parent };
		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: Some(SignedSchedulingInfo {
				payload: SchedulingInfoPayload::new(
					CoreSelector(0),
					0,
					Default::default(),
					isp_header.hash(),
				),
				signature: dummy_signature(),
			}),
		};

		let result = validate_v3_scheduling(true, &Some(ext), Some(&proof), 3);
		let result = result.expect("should succeed");
		assert!(result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, isp_header.hash());
	}

	#[test]
	#[should_panic(expected = "V3 scheduling validation failed")]
	fn v3_enabled_resubmission_without_signature_panics() {
		let (headers, isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let ext =
			ValidationParamsExtension::V3 { relay_parent: older_relay_parent, scheduling_parent };
		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header.clone(),
			signed_scheduling_info: None,
		};

		// Should panic because resubmission requires signed_scheduling_info
		validate_v3_scheduling(true, &Some(ext), Some(&proof), 3);
	}
}

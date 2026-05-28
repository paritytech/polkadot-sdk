// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Scheduling validation for V3 candidates.
//!
//! Validates the header chain from scheduling_parent to internal_scheduling_parent,
//! and verifies relay_parent is at or before internal_scheduling_parent.

use cumulus_primitives_core::{
	relay_chain::ApprovedPeerId, ClaimQueueOffset, CoreSelector, SchedulingProof,
	SignedSchedulingInfo,
};
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
	/// `internal_scheduling_parent_header` does not hash to the internal scheduling
	/// parent derived from the header chain (or `scheduling_parent` when the chain
	/// is empty). The PVF reads the BABE pre-digest from this header to derive the
	/// parachain slot used for author lookup; without the linkage check a collator
	/// could attach an unrelated header pointing the verifier at an arbitrary slot.
	InternalSchedulingParentHeaderMismatch,
	/// `signed_scheduling_info.payload.internal_scheduling_parent` does not match the
	/// internal scheduling parent derived from the proof. The signer must have signed
	/// over the same ISP the proof points to; rejecting the mismatch here prevents a
	/// signature meant for a different scheduling context from being reused.
	SignedSchedulingInfoIspMismatch,
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
/// Returns `None` for V1/V2 candidates, `Some(result)` for valid V3. Panics on
/// config/extension mismatches or chain-shape validation failures.
///
/// This function only validates the *shape* of the scheduling proof (header chain
/// linkage, relay-parent position, presence of `signed_scheduling_info` when
/// required, and that `internal_scheduling_parent_header` hashes to the derived
/// internal scheduling parent). Signature verification on `signed_scheduling_info`
/// is the caller's responsibility — see `validate_block` for the call site that
/// invokes `PSC::SchedulingSignatureVerifier` using the returned
/// `internal_scheduling_parent`.
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

/// Check the scheduling proof against the relay parent, scheduling parent, and
/// expected header chain length.
///
/// Two submission shapes are valid:
/// - **Initial submission** (`relay_parent == internal_scheduling_parent`):
///   `signed_scheduling_info` is optional. When absent, core selection comes from the block's UMP
///   signals; when present it is legal but unused here.
/// - **Resubmission** (`relay_parent` is an ancestor of `internal_scheduling_parent`):
///   `signed_scheduling_info` is required and its `payload.internal_scheduling_parent` must match
///   the derived ISP.
///
/// Returns the derived `internal_scheduling_parent` and a flag indicating which
/// shape matched. Signature verification on `signed_scheduling_info` is the
/// caller's responsibility — see `validate_block` for the call site that invokes
/// `PSC::SchedulingSignatureVerifier`.
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

	// 3. Derive internal_scheduling_parent. It's the parent_hash of the last (oldest)
	// header in the chain, or `scheduling_parent` itself when the chain is empty
	// (`RelayParentOffset = 0`).
	let internal_scheduling_parent = if header_chain.is_empty() {
		scheduling_parent
	} else {
		*header_chain.last().expect("checked non-empty; qed").parent_hash()
	};

	// 4. The internal_scheduling_parent_header carried in the proof must hash to the
	// internal_scheduling_parent we just derived. The PVF reads the BABE pre-digest
	// out of this header to derive the parachain slot used for author lookup; without
	// the linkage check a collator could attach an unrelated header pointing the
	// verifier at an arbitrary slot.
	if scheduling_proof.internal_scheduling_parent_header.hash() != internal_scheduling_parent {
		return Err(SchedulingValidationError::InternalSchedulingParentHeaderMismatch);
	}

	// 5. Validate relay_parent position. relay_parent must NOT be inside the header
	// chain — it either equals internal_scheduling_parent (initial submission) or is
	// an ancestor of it (resubmission), but never between scheduling_parent and
	// internal_scheduling_parent.
	for header in header_chain.iter() {
		let header_hash = header.hash();
		if relay_parent == header_hash {
			return Err(SchedulingValidationError::RelayParentInHeaderChain);
		}
	}

	// 6. Validate signed_scheduling_info based on relay_parent position.
	let is_initial_submission = relay_parent == internal_scheduling_parent;

	if !is_initial_submission {
		// Resubmission: relay_parent is an ancestor of internal_scheduling_parent.
		// The resubmitting collator must sign the core selection.
		if scheduling_proof.signed_scheduling_info.is_none() {
			return Err(SchedulingValidationError::MissingSignedSchedulingInfo);
		}
	}

	// 7. When signed_scheduling_info is present, its payload must commit to the same
	// ISP the proof points to.
	if let Some(signed_info) = &scheduling_proof.signed_scheduling_info {
		if signed_info.payload.internal_scheduling_parent != internal_scheduling_parent {
			return Err(SchedulingValidationError::SignedSchedulingInfoIspMismatch);
		}
	}

	Ok(SchedulingValidationResult {
		internal_scheduling_parent,
		is_resubmission: !is_initial_submission,
	})
}

/// Apply the resubmission override from a verified `SignedSchedulingInfo`: the
/// canonical `(core_selector, claim_queue_offset)` and `approved_peer` to emit as
/// the block's UMP signals are read directly from the signed payload, since the
/// resubmitting collator signed over all three.
pub fn apply_resubmission_override(
	signed_info: &SignedSchedulingInfo,
) -> ((CoreSelector, ClaimQueueOffset), ApprovedPeerId) {
	(
		(
			signed_info.payload.core_selector,
			ClaimQueueOffset(signed_info.payload.claim_queue_offset),
		),
		signed_info.payload.peer_id.clone(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::{
		CoreSelector, SchedulingInfoPayload, SchedulingProof, SignedSchedulingInfo,
	};
	use rstest::rstest;
	use sp_runtime::{generic::Header, traits::BlakeTwo256};

	type RelayHeader = Header<u32, BlakeTwo256>;

	/// Creates a dummy signature blob for testing (not cryptographically valid).
	fn dummy_signature() -> [u8; 64] {
		[0u8; 64]
	}

	/// Builds a `SignedSchedulingInfo` with the given core selector, ISP, and a dummy
	/// signature. `claim_queue_offset` and `peer_id` use default/zero values.
	///
	/// `check_scheduling` cross-checks `payload.internal_scheduling_parent` against the
	/// ISP derived from the proof, so callers must pass the ISP the proof points to (or
	/// a deliberately-mismatched value to exercise the rejection path).
	fn dummy_signed(core_selector: CoreSelector, isp: RelayHash) -> SignedSchedulingInfo {
		SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(core_selector, 0, Default::default(), isp),
			signature: dummy_signature(),
		}
	}

	/// Creates a chain of headers where each header's parent_hash points to the next,
	/// plus the relay header at `internal_scheduling_parent` (ISP). The ISP header's
	/// hash equals the chain's last header's `parent_hash`, or coincides with
	/// `scheduling_parent` when the chain is empty.
	///
	/// Returns the chain headers ordered newest-to-oldest (index 0 = newest =
	/// `scheduling_parent`) and the ISP header. Tests pick their own `relay_parent`:
	/// `isp_header.hash()` for initial submission, an unrelated hash for resubmission.
	fn make_header_chain(len: usize) -> (Vec<RelayHeader>, RelayHeader) {
		// Construct the ISP header first so we can derive its hash and build the chain
		// on top of it.
		let isp_header = RelayHeader::new(
			0u32,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);

		if len == 0 {
			return (vec![], isp_header);
		}

		let mut headers = Vec::with_capacity(len);
		let mut parent_hash = isp_header.hash();

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

		// Reverse so newest is first (matches expected ordering).
		headers.reverse();
		(headers, isp_header)
	}

	// =========================================================================
	// Valid cases
	// =========================================================================

	#[rstest]
	#[case::len_1(1)]
	#[case::len_3(3)]
	fn valid_non_empty_header_chain(#[case] len: usize) {
		// Valid N-header chain on initial submission (`relay_parent == ISP`): validation
		// passes, `internal_scheduling_parent == relay_parent`, and `is_resubmission`
		// is false. Length 0 is structurally different (no chain headers) and lives in
		// its own test.
		let (headers, isp_header) = make_header_chain(len);
		let scheduling_parent = headers[0].hash();
		let relay_parent = isp_header.hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, len as u32)
			.expect("valid chain should pass");
		assert_eq!(result.internal_scheduling_parent, relay_parent);
		assert!(!result.is_resubmission);
	}

	#[test]
	fn valid_empty_header_chain() {
		// Empty chain (offset=0) means scheduling_parent == relay_parent and the
		// ISP header must hash to scheduling_parent.
		let (_, isp_header) = make_header_chain(0);
		let scheduling_parent = isp_header.hash();
		let relay_parent = scheduling_parent; // Must be equal for offset=0

		let proof = SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 0)
			.expect("valid empty chain should pass");
		assert_eq!(result.internal_scheduling_parent, scheduling_parent);
		assert!(!result.is_resubmission);
	}

	// =========================================================================
	// Invalid length cases
	// =========================================================================

	#[rstest]
	#[case::too_short(2)]
	#[case::too_long(4)]
	fn reject_wrong_header_chain_length(#[case] actual: usize) {
		// Chain whose length doesn't match the expected (3) is rejected with
		// `InvalidHeaderChainLength`, both when too short and when too long.
		let (headers, isp_header) = make_header_chain(actual);
		let scheduling_parent = headers[0].hash();
		let relay_parent = isp_header.hash();

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);

		assert_eq!(
			result,
			Err(SchedulingValidationError::InvalidHeaderChainLength { expected: 3, actual })
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
			internal_scheduling_parent_header: isp_header,
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
		let scheduling_parent = headers[0].hash();
		let relay_parent = isp_header.hash();

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
			internal_scheduling_parent_header: isp_header,
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
			internal_scheduling_parent_header: isp_header,
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
		let scheduling_parent = headers[0].hash();
		let relay_parent = isp_header.hash();

		let signed_info = dummy_signed(CoreSelector(0), isp_header.hash());

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
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
			internal_scheduling_parent_header: isp_header,
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
		let scheduling_parent = headers[0].hash();
		let internal_scheduling_parent = isp_header.hash();
		// Use an unrelated hash as relay_parent (simulates resubmission where
		// relay_parent is an ancestor of internal_scheduling_parent)
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let signed_info = dummy_signed(CoreSelector(0), internal_scheduling_parent);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: Some(signed_info),
		};
		let result = check_scheduling(&proof, older_relay_parent, scheduling_parent, 3);

		// Validation passes - signature verification is done separately
		assert!(result.is_ok());
		let result = result.unwrap();
		assert!(result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, internal_scheduling_parent);
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
		let relay_parent = isp_header.hash();
		let scheduling_parent = if headers.is_empty() { relay_parent } else { headers[0].hash() };

		let extension = ValidationParamsExtension::V3 { relay_parent, scheduling_parent };
		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let expected = SchedulingValidationResult {
			internal_scheduling_parent: relay_parent,
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

	#[rstest]
	#[case::empty(0)]
	#[case::len_3(3)]
	fn v3_enabled_valid_initial_submission(#[case] chain_len: u32) {
		let (ext, proof, expected) = make_v3_initial_submission(chain_len);
		let result = validate_v3_scheduling(true, &Some(ext), Some(&proof), chain_len);
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
		let internal_scheduling_parent = isp_header.hash();
		// Use an unrelated hash as relay_parent to simulate a resubmission
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		let ext =
			ValidationParamsExtension::V3 { relay_parent: older_relay_parent, scheduling_parent };
		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: Some(dummy_signed(CoreSelector(0), internal_scheduling_parent)),
		};

		let result = validate_v3_scheduling(true, &Some(ext), Some(&proof), 3);
		let result = result.expect("should succeed");
		assert!(result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, internal_scheduling_parent);
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
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};

		// Should panic because resubmission requires signed_scheduling_info
		validate_v3_scheduling(true, &Some(ext), Some(&proof), 3);
	}

	#[test]
	fn empty_chain_with_signed_info_passes_when_relay_parent_matches() {
		// With an empty chain and `relay_parent == scheduling_parent`, the candidate
		// is an initial submission. An accompanying `signed_scheduling_info` is legal
		// (collators may refuse stale info, but `check_scheduling` doesn't forbid it).
		let (_, isp_header) = make_header_chain(0);
		let scheduling_parent = isp_header.hash();
		let relay_parent = scheduling_parent;
		let proof = SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: Some(dummy_signed(CoreSelector(0), scheduling_parent)),
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 0);
		assert!(result.is_ok());
		assert!(!result.unwrap().is_resubmission);
	}

	#[test]
	fn empty_chain_with_mismatched_relay_parent_is_resubmission() {
		// With `RelayParentOffset = 0` the header chain is always empty, for both
		// initial submissions and resubmissions. When `relay_parent != scheduling_parent`
		// the candidate is a resubmission: `internal_scheduling_parent` falls back to
		// `scheduling_parent`, and the linkage check (against the proof's ISP header)
		// is what ultimately rejects an inconsistent proof.
		let (_, isp_header) = make_header_chain(0);
		let scheduling_parent = isp_header.hash();
		let relay_parent = RelayHash::repeat_byte(0xBB);
		let proof = SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: Some(dummy_signed(CoreSelector(0), scheduling_parent)),
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 0).unwrap();
		assert!(result.is_resubmission);
		assert_eq!(result.internal_scheduling_parent, scheduling_parent);
	}

	#[test]
	fn empty_chain_resubmission_without_signed_info_is_rejected() {
		// Empty chain + `relay_parent != scheduling_parent` is treated as a resubmission;
		// without `signed_scheduling_info` we reject as we would for any other resubmission.
		let (_, isp_header) = make_header_chain(0);
		let scheduling_parent = isp_header.hash();
		let relay_parent = RelayHash::repeat_byte(0xBB);
		let proof = SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 0);
		assert_eq!(result, Err(SchedulingValidationError::MissingSignedSchedulingInfo));
	}

	#[test]
	fn reject_unlinked_internal_scheduling_parent_header() {
		// ISP header that does not hash to the derived internal_scheduling_parent must
		// be rejected: otherwise a collator could point the verifier at an arbitrary
		// slot to satisfy the author lookup.
		let (headers, real_isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		let relay_parent = real_isp_header.hash();
		// An unrelated header with a different block number → different hash.
		let unrelated_isp_header = RelayHeader::new(
			42u32,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: unrelated_isp_header,
			signed_scheduling_info: None,
		};
		let result = check_scheduling(&proof, relay_parent, scheduling_parent, 3);
		assert_eq!(result, Err(SchedulingValidationError::InternalSchedulingParentHeaderMismatch));
	}

	#[test]
	fn reject_signed_info_with_mismatched_isp() {
		// A signed payload whose `internal_scheduling_parent` doesn't match the ISP
		// derived from the proof must be rejected here, not just at signature-verifier
		// time. Without this, an eligible author could sign a payload claiming a stale
		// ISP and the verifier's signature check would still succeed over those bytes.
		let (headers, isp_header) = make_header_chain(3);
		let scheduling_parent = headers[0].hash();
		let older_relay_parent = RelayHash::repeat_byte(0xBB);

		// Payload commits to a different ISP than the proof carries.
		let wrong_isp = RelayHash::repeat_byte(0xCC);
		let signed_info = dummy_signed(CoreSelector(0), wrong_isp);

		let proof = SchedulingProof {
			header_chain: headers,
			internal_scheduling_parent_header: isp_header,
			signed_scheduling_info: Some(signed_info),
		};
		let result = check_scheduling(&proof, older_relay_parent, scheduling_parent, 3);
		assert_eq!(result, Err(SchedulingValidationError::SignedSchedulingInfoIspMismatch));
	}

	// =========================================================================
	// apply_resubmission_override tests
	// =========================================================================

	fn signed_with(
		core_selector: CoreSelector,
		claim_queue_offset: u8,
		peer_id: ApprovedPeerId,
	) -> SignedSchedulingInfo {
		SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(
				core_selector,
				claim_queue_offset,
				peer_id,
				Default::default(),
			),
			signature: [0u8; 64],
		}
	}

	fn peer(byte: u8) -> ApprovedPeerId {
		ApprovedPeerId::try_from(vec![byte; 4]).expect("4 bytes fits the bound; qed")
	}

	#[test]
	fn override_returns_all_fields_from_signed_payload() {
		// All three values — `core_selector`, `claim_queue_offset`, and `peer_id` — are
		// signed by the resubmitting collator, so the override sources every field from
		// the signed payload. Distinct values across the three return-value fields ensure
		// no field is silently sourced from the wrong place.
		let signed = signed_with(CoreSelector(7), 3, peer(0xAA));

		let ((selector, offset), peer_id) = apply_resubmission_override(&signed);

		assert_eq!(selector, CoreSelector(7), "core_selector must come from the signed payload");
		assert_eq!(offset, ClaimQueueOffset(3), "offset must come from the signed payload");
		assert_eq!(peer_id, peer(0xAA), "approved_peer must come from the signed payload");
	}
}

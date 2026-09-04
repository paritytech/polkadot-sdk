// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use assert_matches::assert_matches;
use polkadot_node_primitives::{BlockData, Collation, MaybeCompressedPoV};
use polkadot_primitives::{
	transpose_claim_queue, ClaimQueueOffset, CoreSelector, HeadData, OutboundHrmpMessage,
	UMPSignal, ValidationCode, ValidationCodeHash, UMP_SEPARATOR,
};
use polkadot_primitives_test_helpers::dummy_head_data;
use rstest::rstest;
use std::collections::{BTreeMap, VecDeque};

const PARA_ID: ParaId = ParaId::new(5);
const N_VALIDATORS: usize = 3;

fn pvd() -> PersistedValidationData {
	PersistedValidationData {
		parent_head: dummy_head_data(),
		relay_parent_number: 10,
		relay_parent_storage_root: Hash::repeat_byte(1),
		max_pov_size: 1024,
	}
}

fn validation_code_hash() -> ValidationCodeHash {
	Hash::repeat_byte(42).into()
}

fn collation(pov_size: usize) -> Collation {
	Collation {
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		new_validation_code: None,
		head_data: dummy_head_data(),
		proof_of_validity: MaybeCompressedPoV::Raw(PoV {
			block_data: BlockData(vec![0; pov_size]),
		}),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	}
}

/// A collation with the given UMP signals appended after the separator.
fn collation_with_signals(signals: &[UMPSignal]) -> Collation {
	let mut collation = collation(0);
	if !signals.is_empty() {
		collation.upward_messages.force_push(UMP_SEPARATOR);
		for signal in signals {
			collation.upward_messages.force_push(signal.encode());
		}
	}
	collation
}

fn segment_collation(collation: Collation, relay_parent: Hash) -> SegmentCollation {
	SegmentCollation {
		collation,
		relay_parent,
		validation_data: pvd(),
		validation_code_hash: validation_code_hash(),
		session_index: 1,
	}
}

/// A claim queue assigning every core in `cores` to [`PARA_ID`] at depth 0.
fn claim_queue(cores: &[u32]) -> TransposedClaimQueue {
	transpose_claim_queue(
		cores
			.iter()
			.map(|core| (CoreIndex(*core), VecDeque::from([PARA_ID])))
			.collect::<BTreeMap<_, _>>(),
	)
}

fn params(
	collations: Vec<SegmentCollation>,
	version: CandidateDescriptorVersion,
	scheduling_parent: Hash,
) -> SegmentToDistribute {
	let scheduling = match version {
		CandidateDescriptorVersion::V2 => {
			SchedulingContext::V2 { relay_parent: scheduling_parent, session: 7 }
		},
		CandidateDescriptorVersion::V3 => {
			SchedulingContext::V3 { scheduling_parent, scheduling_session: 7 }
		},
		// `SchedulingContext` cannot represent any other version, which is the point of the
		// type; tests only ask for V2 or V3; qed
		other => unreachable!("unrepresentable descriptor version in test: {other:?}"),
	};

	SegmentToDistribute { core_index: CoreIndex(0), scheduling, collations }
}

/// `build_segment` with the fixture para and validator count applied.
fn build_seg(
	segment: SegmentToDistribute,
	transposed_claim_queue: &TransposedClaimQueue,
) -> Result<Segment, Error> {
	build_segment(segment, PARA_ID, N_VALIDATORS, transposed_claim_queue)
}

#[test]
fn builds_v2_segment() {
	let relay_parent = Hash::repeat_byte(0);
	let collation =
		collation_with_signals(&[UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0))]);

	let segment = build_seg(
		params(
			vec![segment_collation(collation, relay_parent)],
			CandidateDescriptorVersion::V2,
			relay_parent,
		),
		&claim_queue(&[0]),
	)
	.unwrap();

	assert_matches!(segment, Segment::V2(entry) => {
		assert_eq!(entry.relay_parent, relay_parent);
		assert_eq!(entry.session_index, 7); // derived from SchedulingContext::V2 { session }
		assert_eq!(entry.validation_code_hash, validation_code_hash());
		assert_eq!(entry.persisted_validation_data_hash, pvd().hash());
		assert_eq!(entry.parent_head_data.hash(), dummy_head_data().hash());
		assert_eq!(entry.output_head_data_hash, dummy_head_data().hash());
		assert_eq!(entry.erasure_root, expected_erasure_root(&entry.pov));
	});
}

#[test]
// V2 session_index is always derived from SchedulingContext::V2 { session }, not from the
// collation, so a caller that supplies an inconsistent value still emits the correct session.
fn v2_session_index_derived_from_scheduling_context() {
	let relay_parent = Hash::repeat_byte(0);
	let collation =
		collation_with_signals(&[UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0))]);
	// Collation carries session_index 99, but the scheduling context says 7.
	let mut seg = segment_collation(collation, relay_parent);
	seg.session_index = 99;

	let segment = build_seg(
		SegmentToDistribute {
			core_index: CoreIndex(0),
			scheduling: SchedulingContext::V2 { relay_parent, session: 7 },
			collations: vec![seg],
		},
		&claim_queue(&[0]),
	)
	.unwrap();

	assert_matches!(segment, Segment::V2(entry) => {
		// Must be 7 (from context), not 99 (from collation).
		assert_eq!(entry.session_index, 7);
	});
}

/// The erasure root of the available data, computed independently of the code under test.
fn expected_erasure_root(pov: &PoV) -> Hash {
	let available_data = AvailableData { validation_data: pvd(), pov: Arc::new(pov.clone()) };
	let chunks = polkadot_erasure_coding::obtain_chunks_v1(N_VALIDATORS, &available_data).unwrap();
	polkadot_erasure_coding::branches(&chunks).root()
}

#[test]
fn builds_v3_segment_with_scheduling_parent() {
	let relay_parent = Hash::repeat_byte(0xAA);
	let scheduling_parent = Hash::repeat_byte(0xBB);
	let collations = (0..2)
		.map(|_| {
			segment_collation(
				collation_with_signals(&[UMPSignal::SelectCore(
					CoreSelector(0),
					ClaimQueueOffset(0),
				)]),
				relay_parent,
			)
		})
		.collect();

	let segment = build_seg(
		params(collations, CandidateDescriptorVersion::V3, scheduling_parent),
		&claim_queue(&[0]),
	)
	.unwrap();

	assert_matches!(segment, Segment::V3 { scheduling_parent: sp, scheduling_session, candidates } => {
		assert_eq!(sp, scheduling_parent);
		assert_eq!(scheduling_session, 7);
		assert_eq!(candidates.len(), 2);
		// The descriptor's relay parent is the execution context, distinct from the
		// scheduling context.
		assert!(candidates.iter().all(|entry| entry.relay_parent == relay_parent));
	});
}

#[test]
// A V3 candidate needs UMP signals, but an `ApprovedPeer` signal on its own is enough.
fn builds_v3_segment_with_only_approved_peer_signal() {
	let relay_parent = Hash::repeat_byte(0);
	let collation =
		collation_with_signals(&[UMPSignal::ApprovedPeer(vec![1, 2, 3, 4, 5].try_into().unwrap())]);

	assert_matches!(
		build_seg(
			params(
				vec![segment_collation(collation, relay_parent)],
				CandidateDescriptorVersion::V3,
				relay_parent,
			),
			&claim_queue(&[0]),
		),
		Ok(Segment::V3 { candidates, .. }) => assert_eq!(candidates.len(), 1)
	);
}

#[test]
// A V3 candidate without any UMP signal is rejected by the UMP signal checks.
fn rejects_v3_candidate_without_ump_signals() {
	let relay_parent = Hash::repeat_byte(0);

	assert_matches!(
		build_seg(
			params(
				vec![segment_collation(collation(0), relay_parent)],
				CandidateDescriptorVersion::V3,
				relay_parent,
			),
			&claim_queue(&[0]),
		),
		Err(Error::CandidateReceiptCheck(
			CommittedCandidateReceiptError::NoUMPSignalWithV3Descriptor
		))
	);
}

#[test]
// The core the candidate is submitted on must be assigned to the para in the claim queue.
fn rejects_core_index_not_assigned_to_para() {
	let relay_parent = Hash::repeat_byte(0);

	assert_matches!(
		build_seg(
			params(
				vec![segment_collation(collation(0), relay_parent)],
				CandidateDescriptorVersion::V2,
				relay_parent,
			),
			// The candidate is submitted on core 0, which is not assigned to the para.
			&claim_queue(&[1]),
		),
		Err(Error::CandidateReceiptCheck(_))
	);
}

#[test]
fn rejects_pov_exceeding_max_pov_size() {
	let relay_parent = Hash::repeat_byte(0);
	// A xorshift stream does not compress, so the size check cannot be satisfied by
	// compressing the PoV.
	let mut state = 0x2545_f491_4f6c_dd1du64;
	let block_data = (0..pvd().max_pov_size as usize * 4)
		.map(|_| {
			state ^= state << 13;
			state ^= state >> 7;
			state ^= state << 17;
			state as u8
		})
		.collect::<Vec<_>>();
	let mut collation = collation(0);
	collation.proof_of_validity =
		MaybeCompressedPoV::Raw(PoV { block_data: BlockData(block_data) });

	assert_matches!(
		build_seg(
			params(
				vec![segment_collation(collation, relay_parent)],
				CandidateDescriptorVersion::V2,
				relay_parent,
			),
			&claim_queue(&[0]),
		),
		Err(Error::POVSizeExceeded(_, max)) => assert_eq!(max, pvd().max_pov_size as usize)
	);
}

#[test]
fn rejects_empty_segment() {
	let relay_parent = Hash::repeat_byte(0);

	assert_matches!(
		build_seg(params(vec![], CandidateDescriptorVersion::V3, relay_parent), &claim_queue(&[0]),),
		Err(Error::InvalidSegmentSize(0))
	);
}

#[test]
fn rejects_v2_segment_with_multiple_collations() {
	let relay_parent = Hash::repeat_byte(0);
	let collations = (0..2)
		.map(|_| segment_collation(collation(0), relay_parent))
		.collect::<Vec<_>>();

	assert_matches!(
		build_seg(
			params(collations, CandidateDescriptorVersion::V2, relay_parent),
			&claim_queue(&[0]),
		),
		Err(Error::V2InvalidSegmentLength)
	);
}

#[test]
fn rejects_v3_segment_exceeding_max_segment_len() {
	let relay_parent = Hash::repeat_byte(0);
	let collations = (0..MAX_SEGMENT_LEN + 1)
		.map(|_| {
			segment_collation(
				collation_with_signals(&[UMPSignal::SelectCore(
					CoreSelector(0),
					ClaimQueueOffset(0),
				)]),
				relay_parent,
			)
		})
		.collect::<Vec<_>>();
	let len = collations.len();

	assert_matches!(
		build_seg(
			params(collations, CandidateDescriptorVersion::V3, relay_parent),
			&claim_queue(&[0]),
		),
		Err(Error::InvalidSegmentSize(reported)) => assert_eq!(reported, len)
	);
}

#[test]
// The unchecked builder accepts a core index that the UMP signal checks would reject.
fn unchecked_builder_skips_ump_signal_checks() {
	let relay_parent = Hash::repeat_byte(0);

	// The two builders must be given the *same* input for the comparison to mean anything, and
	// that input must be one the check actually rejects. A separator followed by an undecodable
	// signal fails inside `ump_signals()`, independently of the core index.
	let mut malformed = collation(0);
	malformed.upward_messages.force_push(UMP_SEPARATOR);
	malformed.upward_messages.force_push(vec![0xFF; 8]);

	let params = |collation| SegmentEntryParams {
		collation: segment_collation(collation, relay_parent),
		para_id: PARA_ID,
		core_index: CoreIndex(0),
		n_validators: N_VALIDATORS,
	};

	let checked = build_segment_entry(
		params(malformed.clone()),
		&claim_queue(&[0]),
		CandidateDescriptorVersion::V2,
	);
	assert_matches!(checked, Err(Error::CandidateReceiptCheck(_)));

	// Same collation, check skipped: it builds.
	let entry = build_segment_entry_without_ump_check(
		segment_collation(malformed, relay_parent),
		N_VALIDATORS,
	)
	.unwrap();
	assert_eq!(entry.relay_parent, relay_parent);
}

#[test]
// Every commitment field must reach the entry's `commitments_hash`. The other tests use default
// commitments, so dropping a field from the `CandidateCommitments` literal in `build_entry`, or
// transposing two of the same type, would not change any hash they assert on. In production that
// surfaces only as validators rejecting every candidate while the collator reports success.
fn commitments_hash_covers_every_field() {
	let relay_parent = Hash::repeat_byte(0);

	// Distinct, non-default values in all six fields.
	let mut collation = collation(0);
	collation.head_data = HeadData(vec![1, 2, 3]);
	collation.new_validation_code = Some(ValidationCode(vec![4, 5, 6]));
	collation.processed_downward_messages = 7;
	collation.hrmp_watermark = 9;
	collation
		.horizontal_messages
		.force_push(OutboundHrmpMessage { recipient: ParaId::from(11u32), data: vec![12] });
	// A real upward message, then the separator and the core selector the UMP check needs.
	collation.upward_messages.force_push(vec![13, 14]);
	collation.upward_messages.force_push(UMP_SEPARATOR);
	collation
		.upward_messages
		.force_push(UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode());

	let expected = CandidateCommitments {
		upward_messages: collation.upward_messages.clone(),
		horizontal_messages: collation.horizontal_messages.clone(),
		new_validation_code: collation.new_validation_code.clone(),
		head_data: collation.head_data.clone(),
		processed_downward_messages: collation.processed_downward_messages,
		hrmp_watermark: collation.hrmp_watermark,
	};

	let segment = build_seg(
		params(
			vec![segment_collation(collation, relay_parent)],
			CandidateDescriptorVersion::V2,
			relay_parent,
		),
		&claim_queue(&[0]),
	)
	.unwrap();

	assert_matches!(segment, Segment::V2(entry) => {
		assert_eq!(entry.commitments_hash, expected.hash());
		assert_eq!(entry.output_head_data_hash, expected.head_data.hash());
	});
}

#[rstest]
#[case(CandidateDescriptorVersion::V1)]
#[case(CandidateDescriptorVersion::Unknown(42))]
// `build_segment_entry` takes a raw descriptor version, so it keeps the explicit guard the
// removed subsystem had. Callers going through `build_segment` are protected by the type.
fn build_segment_entry_rejects_unsupported_descriptor_version(
	#[case] version: CandidateDescriptorVersion,
) {
	let relay_parent = Hash::repeat_byte(0);

	assert_matches!(
		build_segment_entry(
			SegmentEntryParams {
				collation: segment_collation(collation(0), relay_parent),
				para_id: PARA_ID,
				core_index: CoreIndex(0),
				n_validators: N_VALIDATORS,
			},
			&claim_queue(&[0]),
			version,
		),
		Err(Error::UnsupportedDescriptorVersion)
	);
}

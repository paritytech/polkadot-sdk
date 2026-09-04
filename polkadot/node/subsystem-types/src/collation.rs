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

//! Collator-side helpers for turning collations into the [`Segment`] that is handed to the
//! collator protocol via [`CollatorProtocolMessage::DistributeSegment`].
//!
//! Building an entry compresses the PoV, computes the erasure root and checks the UMP signals
//! against the claim queue. The candidate receipt itself is assembled by the receiver from the
//! entry fields.
//!
//! [`CollatorProtocolMessage::DistributeSegment`]: crate::messages::CollatorProtocolMessage::DistributeSegment

use crate::messages::{Segment, SegmentEntry};
use codec::Encode;
use polkadot_node_primitives::{AvailableData, PoV, SegmentCollation, MAX_SEGMENT_LEN};
use polkadot_primitives::{
	v9::parse_ump_signals_for_commitments, CandidateCommitments, CandidateDescriptorVersion,
	CommittedCandidateReceiptError, CoreIndex, Hash, Id as ParaId, PersistedValidationData,
	SessionIndex, TransposedClaimQueue,
};
use sp_runtime::{traits::ConstU32, BoundedVec};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Something that went wrong while building a segment.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Erasure coding of the available data failed.
	#[error(transparent)]
	Erasure(#[from] polkadot_erasure_coding::Error),
	/// The UMP signals are inconsistent with the descriptor fields.
	#[error("Candidate receipt check failed: {0}")]
	CandidateReceiptCheck(CommittedCandidateReceiptError),
	/// The compressed PoV does not fit into the para's `max_pov_size`.
	#[error("PoV size {0} exceeded maximum size of {1}")]
	POVSizeExceeded(usize, usize),
	/// The number of collations in the segment is not in the allowed range.
	#[error("Segment size {0} is not in allowed range")]
	InvalidSegmentSize(usize),
	/// A V2 segment carries more than one candidate.
	#[error("Segments consisting of V2 candidates should have exactly one entry.")]
	V2InvalidSegmentLength,
	/// Only V2 and V3 candidate descriptors can be built.
	#[error("Only V2 and V3 candidate descriptor versions can be built")]
	UnsupportedDescriptorVersion,
}

/// Everything needed to build a single [`SegmentEntry`].
pub struct SegmentEntryParams {
	/// The collation and the context it was built in.
	pub collation: SegmentCollation,
	/// The parachain the collation is for.
	pub para_id: ParaId,
	/// The core the resulting candidate is to be backed on.
	pub core_index: CoreIndex,
	/// The number of validators in the session, used for erasure coding.
	pub n_validators: usize,
}

/// Which relay block determined the core assignment for a segment.
///
/// The variant fixes the candidate descriptor version, so a V2 segment cannot carry a scheduling
/// parent foreign to its collations and a V3 segment cannot omit one.
///
/// This is the input mirror of [`Segment`]'s own V2/V3 split, and the same distinction the
/// descriptor encodes via `CandidateDescriptorV2::new`/`new_v3` and reads back through
/// `scheduling_session()`: for V2 the relay parent's session, for V3 an explicit one.
#[derive(Debug, Clone, Copy)]
pub enum SchedulingContext {
	/// V2 descriptors: the collations' own relay parent is the scheduling context.
	V2 {
		/// The relay parent the collations build on, which doubles as the scheduling parent.
		relay_parent: Hash,
		/// The session index at `relay_parent`.
		session: SessionIndex,
	},
	/// V3 descriptors: an explicit scheduling parent, which may differ from the relay parent.
	V3 {
		/// The scheduling parent shared by all collations in the segment.
		scheduling_parent: Hash,
		/// The session index at `scheduling_parent`.
		scheduling_session: SessionIndex,
	},
}

impl SchedulingContext {
	/// The relay block whose claim queue and validator set govern this segment. Read on every
	/// path, in both variants, so it is never a placeholder.
	pub fn anchor(&self) -> Hash {
		match self {
			Self::V2 { relay_parent, .. } => *relay_parent,
			Self::V3 { scheduling_parent, .. } => *scheduling_parent,
		}
	}

	/// The session index at [`Self::anchor`].
	pub fn session(&self) -> SessionIndex {
		match self {
			Self::V2 { session, .. } => *session,
			Self::V3 { scheduling_session, .. } => *scheduling_session,
		}
	}

	/// The candidate descriptor version this context implies.
	pub fn descriptor_version(&self) -> CandidateDescriptorVersion {
		match self {
			Self::V2 { .. } => CandidateDescriptorVersion::V2,
			Self::V3 { .. } => CandidateDescriptorVersion::V3,
		}
	}
}

/// A segment of collations sharing a scheduling context and a target core, ready to be built
/// into candidates.
pub struct SegmentToDistribute {
	/// The core every candidate in the segment is to be backed on.
	pub core_index: CoreIndex,
	/// The scheduling context shared by all collations in the segment.
	pub scheduling: SchedulingContext,
	/// The collations, in the order they should be distributed.
	pub collations: Vec<SegmentCollation>,
}

/// Build the [`Segment`] for a set of collations sharing a scheduling context and a core.
///
/// A `V2` segment must carry exactly one collation, a `V3` segment between one and
/// [`MAX_SEGMENT_LEN`] of them.
pub fn build_segment(
	segment: SegmentToDistribute,
	para_id: ParaId,
	n_validators: usize,
	transposed_claim_queue: &TransposedClaimQueue,
) -> Result<Segment, Error> {
	let SegmentToDistribute { core_index, scheduling, collations } = segment;
	let candidates_descriptor_version = scheduling.descriptor_version();

	let len = collations.len();
	if len == 0 {
		return Err(Error::InvalidSegmentSize(len));
	}
	if candidates_descriptor_version == CandidateDescriptorVersion::V2 && len > 1 {
		return Err(Error::V2InvalidSegmentLength);
	}

	let mut entries = Vec::with_capacity(len);
	for collation in collations {
		entries.push(build_segment_entry(
			SegmentEntryParams { collation, para_id, core_index, n_validators },
			transposed_claim_queue,
			candidates_descriptor_version,
		)?);
	}

	match candidates_descriptor_version {
		CandidateDescriptorVersion::V2 => {
			// Validated above to contain exactly one collation.
			let entry = entries.pop().ok_or(Error::InvalidSegmentSize(0))?;
			Ok(Segment::V2(entry))
		},
		CandidateDescriptorVersion::V3 => {
			let (scheduling_parent, scheduling_session) = match scheduling {
				SchedulingContext::V3 { scheduling_parent, scheduling_session } => {
					(scheduling_parent, scheduling_session)
				},
				// `descriptor_version()` returns V3 only for the V3 variant.
				SchedulingContext::V2 { .. } => return Err(Error::UnsupportedDescriptorVersion),
			};
			Ok(Segment::V3 {
				scheduling_parent,
				scheduling_session,
				candidates: BoundedVec::<SegmentEntry, ConstU32<MAX_SEGMENT_LEN>>::try_from(
					entries,
				)
				.map_err(|_| Error::InvalidSegmentSize(len))?,
			})
		},
		CandidateDescriptorVersion::V1 | CandidateDescriptorVersion::Unknown(_) => {
			Err(Error::UnsupportedDescriptorVersion)
		},
	}
}

/// Build a single [`SegmentEntry`] and check the collation's UMP signals against the claim queue.
pub fn build_segment_entry(
	params: SegmentEntryParams,
	transposed_claim_queue: &TransposedClaimQueue,
	candidates_descriptor_version: CandidateDescriptorVersion,
) -> Result<SegmentEntry, Error> {
	let SegmentEntryParams { collation, para_id, core_index, n_validators } = params;

	build_entry(
		collation,
		n_validators,
		Some(UmpSignalCheck {
			transposed_claim_queue,
			candidates_descriptor_version,
			para_id,
			core_index,
		}),
	)
}

/// Build a single [`SegmentEntry`] without checking the UMP signals.
///
/// The check enforces that the parachain selected the core the candidate is submitted on, so
/// skipping it can only produce candidates that validators reject. This exists solely for
/// malicious test collators; honest collators must use [`build_segment_entry`].
#[cfg(any(feature = "test-utils", test))]
pub fn build_segment_entry_without_ump_check(
	collation: SegmentCollation,
	n_validators: usize,
) -> Result<SegmentEntry, Error> {
	build_entry(collation, n_validators, None)
}

struct UmpSignalCheck<'a> {
	transposed_claim_queue: &'a TransposedClaimQueue,
	candidates_descriptor_version: CandidateDescriptorVersion,
	para_id: ParaId,
	core_index: CoreIndex,
}

fn build_entry(
	collation: SegmentCollation,
	n_validators: usize,
	ump_check: Option<UmpSignalCheck<'_>>,
) -> Result<SegmentEntry, Error> {
	let SegmentCollation {
		collation,
		relay_parent,
		validation_data,
		validation_code_hash,
		session_index,
	} = collation;

	let persisted_validation_data_hash = validation_data.hash();
	let parent_head_data = validation_data.parent_head.clone();

	// Apply compression to the block data.
	//
	// As long as `POV_BOMB_LIMIT` is at least `max_pov_size`, this ensures that honest collators
	// never produce an uncompressed PoV which starts with a compression magic number, which would
	// lead validators to reject the collation.
	let pov = collation.proof_of_validity.into_compressed();
	let encoded_size = pov.encoded_size();
	if encoded_size > validation_data.max_pov_size as usize {
		return Err(Error::POVSizeExceeded(encoded_size, validation_data.max_pov_size as usize));
	}

	let erasure_root = erasure_root(n_validators, validation_data, pov.clone())?;

	let commitments = CandidateCommitments {
		upward_messages: collation.upward_messages,
		horizontal_messages: collation.horizontal_messages,
		new_validation_code: collation.new_validation_code,
		head_data: collation.head_data,
		processed_downward_messages: collation.processed_downward_messages,
		hrmp_watermark: collation.hrmp_watermark,
	};

	if let Some(check) = ump_check {
		parse_ump_signals_for_commitments(
			&commitments,
			check.candidates_descriptor_version,
			check.transposed_claim_queue,
			check.para_id,
			check.core_index,
		)
		.map_err(Error::CandidateReceiptCheck)?;
	}

	Ok(SegmentEntry {
		relay_parent,
		session_index,
		validation_code_hash,
		persisted_validation_data_hash,
		erasure_root,
		commitments_hash: commitments.hash(),
		output_head_data_hash: commitments.head_data.hash(),
		pov,
		parent_head_data,
	})
}

fn erasure_root(
	n_validators: usize,
	persisted_validation: PersistedValidationData,
	pov: PoV,
) -> Result<Hash, Error> {
	let available_data =
		AvailableData { validation_data: persisted_validation, pov: Arc::new(pov) };

	let chunks = polkadot_erasure_coding::obtain_chunks_v1(n_validators, &available_data)?;
	Ok(polkadot_erasure_coding::branches(&chunks).root())
}

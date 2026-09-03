// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Collator-side helpers for turning collations into the [`Segment`] that is handed to the
//! collator protocol via [`CollatorProtocolMessage::DistributeSegment`].
//!
//! Building an entry compresses the PoV, computes the erasure root and checks the UMP signals
//! against the claim queue. The candidate receipt itself is assembled by the receiver from the
//! entry fields.
//!
//! [`Segment`]: polkadot_node_subsystem::messages::Segment
//! [`CollatorProtocolMessage::DistributeSegment`]: polkadot_node_subsystem::messages::CollatorProtocolMessage::DistributeSegment

use codec::Encode;
use polkadot_node_primitives::{AvailableData, PoV, SegmentCollation, MAX_SEGMENT_LEN};
use polkadot_node_subsystem::messages::{Segment, SegmentEntry};
use polkadot_primitives::{
	v9::parse_ump_signals_for_commitments, CandidateCommitments, CandidateDescriptorVersion,
	CommittedCandidateReceiptError, CoreIndex, Hash, Id as ParaId, PersistedValidationData,
	SessionIndex, TransposedClaimQueue,
};
use sp_core::{bounded::BoundedVec, ConstU32};
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

/// Everything needed to build a whole [`Segment`].
pub struct BuildSegmentParams {
	/// The parachain the collations are for.
	pub para_id: ParaId,
	/// The core every candidate in the segment is to be backed on.
	pub core_index: CoreIndex,
	/// The number of validators in the session, used for erasure coding.
	pub n_validators: usize,
	/// The scheduling parent shared by all collations in the segment. For V2 segments this is
	/// the collations' relay parent.
	pub scheduling_parent: Hash,
	/// The session index at the scheduling parent. Ignored for V2 segments.
	pub scheduling_session: SessionIndex,
	/// The descriptor version of the candidates, which also selects the segment shape.
	pub candidates_descriptor_version: CandidateDescriptorVersion,
	/// The collations, in the order they should be distributed.
	pub collations: Vec<SegmentCollation>,
}

/// Build the [`Segment`] for a set of collations sharing a scheduling parent and a core.
///
/// A `V2` segment must carry exactly one collation, a `V3` segment between one and
/// [`MAX_SEGMENT_LEN`] of them. Any other descriptor version is rejected.
pub fn build_segment(
	params: BuildSegmentParams,
	transposed_claim_queue: &TransposedClaimQueue,
) -> Result<Segment, Error> {
	let BuildSegmentParams {
		para_id,
		core_index,
		n_validators,
		scheduling_parent,
		scheduling_session,
		candidates_descriptor_version,
		collations,
	} = params;

	if !matches!(
		candidates_descriptor_version,
		CandidateDescriptorVersion::V2 | CandidateDescriptorVersion::V3
	) {
		return Err(Error::UnsupportedDescriptorVersion);
	}

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
		CandidateDescriptorVersion::V3 => Ok(Segment::V3 {
			scheduling_parent,
			scheduling_session,
			candidates: BoundedVec::<SegmentEntry, ConstU32<MAX_SEGMENT_LEN>>::try_from(entries)
				.map_err(|_| Error::InvalidSegmentSize(len))?,
		}),
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

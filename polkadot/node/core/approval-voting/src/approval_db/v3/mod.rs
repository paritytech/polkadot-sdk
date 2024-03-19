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

//! Version 3 of the DB schema.
//!
//! Version 3 modifies the `our_approval` format of `ApprovalEntry`
//! and adds a new field `pending_signatures` for `BlockEntry`

use parity_scale_codec::{Decode, Encode};
use polkadot_node_primitives::approval::v2::CandidateBitfield;
use polkadot_node_subsystem::SubsystemResult;
use polkadot_node_subsystem_util::database::{DBTransaction, Database};
use polkadot_overseer::SubsystemError;
use polkadot_primitives::{
	BlockNumber, CandidateHash, CandidateIndex, CandidateReceipt, CoreIndex, GroupIndex, Hash,
	SessionIndex, ValidatorIndex, ValidatorSignature,
};

use sp_consensus_slots::Slot;

use std::collections::BTreeMap;

use super::common::{block_entry_key, candidate_entry_key, load_decode, Config};

/// Re-export this structs as v3 since they did not change between v2 and v3.
pub use super::v2::{Bitfield, OurAssignment, Tick, TrancheEntry};

pub mod migration_helpers;

#[cfg(test)]
pub mod tests;

/// Metadata about our approval signature
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct OurApproval {
	/// The signature for the candidates hashes pointed by indices.
	pub signature: ValidatorSignature,
	/// The indices of the candidates signed in this approval.
	pub signed_candidates_indices: CandidateBitfield,
}

/// Metadata regarding approval of a particular candidate within the context of some
/// particular block.
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct ApprovalEntry {
	pub tranches: Vec<TrancheEntry>,
	pub backing_group: GroupIndex,
	pub our_assignment: Option<OurAssignment>,
	pub our_approval_sig: Option<OurApproval>,
	// `n_validators` bits.
	pub assigned_validators: Bitfield,
	pub approved: bool,
}

/// Metadata regarding approval of a particular candidate.
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct CandidateEntry {
	pub candidate: CandidateReceipt,
	pub session: SessionIndex,
	// Assignments are based on blocks, so we need to track assignments separately
	// based on the block we are looking at.
	pub block_assignments: BTreeMap<Hash, ApprovalEntry>,
	pub approvals: Bitfield,
}

/// Metadata regarding approval of a particular block, by way of approval of the
/// candidates contained within it.
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct BlockEntry {
	pub block_hash: Hash,
	pub block_number: BlockNumber,
	pub parent_hash: Hash,
	pub session: SessionIndex,
	pub slot: Slot,
	/// Random bytes derived from the VRF submitted within the block by the block
	/// author as a credential and used as input to approval assignment criteria.
	pub relay_vrf_story: [u8; 32],
	// The candidates included as-of this block and the index of the core they are
	// leaving. Sorted ascending by core index.
	pub candidates: Vec<(CoreIndex, CandidateHash)>,
	// A bitfield where the i'th bit corresponds to the i'th candidate in `candidates`.
	// The i'th bit is `true` iff the candidate has been approved in the context of this
	// block. The block can be considered approved if the bitfield has all bits set to `true`.
	pub approved_bitfield: Bitfield,
	pub children: Vec<Hash>,
	// A list of candidates we have checked, but didn't not sign and
	// advertise the vote yet.
	pub candidates_pending_signature: BTreeMap<CandidateIndex, CandidateSigningContext>,
	// Assignments we already distributed. A 1 bit means the candidate index for which
	// we already have sent out an assignment. We need this to avoid distributing
	// multiple core assignments more than once.
	pub distributed_assignments: Bitfield,
}

#[derive(Encode, Decode, Debug, Clone, PartialEq)]
/// Context needed for creating an approval signature for a given candidate.
pub struct CandidateSigningContext {
	/// The candidate hash, to be included in the signature.
	pub candidate_hash: CandidateHash,
	/// The latest tick we have to create and send the approval.
	pub sign_no_later_than_tick: Tick,
}

/// Load a candidate entry from the aux store in v2 format.
pub fn load_candidate_entry_v2(
	store: &dyn Database,
	config: &Config,
	candidate_hash: &CandidateHash,
) -> SubsystemResult<Option<super::v2::CandidateEntry>> {
	load_decode(store, config.col_approval_data, &candidate_entry_key(candidate_hash))
		.map(|u: Option<super::v2::CandidateEntry>| u.map(|v| v.into()))
		.map_err(|e| SubsystemError::with_origin("approval-voting", e))
}

/// Load a block entry from the aux store in v2 format.
pub fn load_block_entry_v2(
	store: &dyn Database,
	config: &Config,
	block_hash: &Hash,
) -> SubsystemResult<Option<super::v2::BlockEntry>> {
	load_decode(store, config.col_approval_data, &block_entry_key(block_hash))
		.map(|u: Option<super::v2::BlockEntry>| u.map(|v| v.into()))
		.map_err(|e| SubsystemError::with_origin("approval-voting", e))
}

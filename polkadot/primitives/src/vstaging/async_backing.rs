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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

/// A candidate pending availability.
#[derive(RuntimeDebug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct CandidatePendingAvailability<H = Hash, N = BlockNumber> {
	/// The hash of the candidate.
	pub candidate_hash: CandidateHash,
	/// The candidate's descriptor.
	pub descriptor: CandidateDescriptorV2<H>,
	/// The commitments of the candidate.
	pub commitments: CandidateCommitments,
	/// The candidate's relay parent's number.
	pub relay_parent_number: N,
	/// The maximum Proof-of-Validity size allowed, in bytes.
	pub max_pov_size: u32,
}

impl<H: Copy> From<CandidatePendingAvailability<H>>
	for crate::v8::async_backing::CandidatePendingAvailability<H>
{
	fn from(value: CandidatePendingAvailability<H>) -> Self {
		Self {
			candidate_hash: value.candidate_hash,
			descriptor: value.descriptor.into(),
			commitments: value.commitments,
			relay_parent_number: value.relay_parent_number,
			max_pov_size: value.max_pov_size,
		}
	}
}

/// Constraints on the actions that can be taken by a new parachain
/// block. These limitations are implicitly associated with some particular
/// parachain, which should be apparent from usage.
#[derive(RuntimeDebug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct Constraints<N = BlockNumber> {
	/// The minimum relay-parent number accepted under these constraints.
	pub min_relay_parent_number: N,
	/// The maximum Proof-of-Validity size allowed, in bytes.
	pub max_pov_size: u32,
	/// The maximum new validation code size allowed, in bytes.
	pub max_code_size: u32,
	/// The maximum head-data size, in bytes.
	pub max_head_data_size: u32,
	/// The amount of UMP messages remaining.
	pub ump_remaining: u32,
	/// The amount of UMP bytes remaining.
	pub ump_remaining_bytes: u32,
	/// The maximum number of UMP messages allowed per candidate.
	pub max_ump_num_per_candidate: u32,
	/// Remaining DMP queue. Only includes sent-at block numbers.
	pub dmp_remaining_messages: Vec<N>,
	/// The limitations of all registered inbound HRMP channels.
	pub hrmp_inbound: InboundHrmpLimitations<N>,
	/// The limitations of all registered outbound HRMP channels.
	pub hrmp_channels_out: Vec<(Id, OutboundHrmpChannelLimitations)>,
	/// The maximum number of HRMP messages allowed per candidate.
	pub max_hrmp_num_per_candidate: u32,
	/// The required parent head-data of the parachain.
	pub required_parent: HeadData,
	/// The expected validation-code-hash of this parachain.
	pub validation_code_hash: ValidationCodeHash,
	/// The code upgrade restriction signal as-of this parachain.
	pub upgrade_restriction: Option<UpgradeRestriction>,
	/// The future validation code hash, if any, and at what relay-parent
	/// number the upgrade would be minimally applied.
	pub future_validation_code: Option<(N, ValidationCodeHash)>,
}

/// The per-parachain state of the backing system, including
/// state-machine constraints and candidates pending availability.
#[derive(RuntimeDebug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct BackingState<H = Hash, N = BlockNumber> {
	/// The state-machine constraints of the parachain.
	pub constraints: crate::async_backing::Constraints<N>,
	/// The candidates pending availability. These should be ordered, i.e. they should form
	/// a sub-chain, where the first candidate builds on top of the required parent of the
	/// constraints and each subsequent builds on top of the previous head-data.
	pub pending_availability: Vec<CandidatePendingAvailability<H, N>>,
}

impl<H: Copy> From<BackingState<H>> for crate::v8::async_backing::BackingState<H> {
	fn from(value: BackingState<H>) -> Self {
		Self {
			constraints: value.constraints,
			pending_availability: value
				.pending_availability
				.into_iter()
				.map(|candidate| candidate.into())
				.collect::<Vec<_>>(),
		}
	}
}

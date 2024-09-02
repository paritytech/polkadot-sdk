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

/// The per-parachain state of the backing system, including
/// state-machine constraints and candidates pending availability.
#[derive(RuntimeDebug, Clone, PartialEq, Encode, Decode, TypeInfo)]
pub struct BackingState<H = Hash, N = BlockNumber> {
	/// The state-machine constraints of the parachain.
	pub constraints: Constraints<N>,
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

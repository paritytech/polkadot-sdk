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

//! Staging Primitives.

// Put any primitives used by staging APIs functions here
use super::{
	Balance, CoreIndex, CandidateCommitments, Hash, Id, ValidationCode, ValidationCodeHash,
	ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
};
use sp_std::prelude::*;

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_arithmetic::Perbill;
use sp_core::RuntimeDebug;

/// Scheduler configuration parameters. All coretime/ondemand parameters are here.
#[derive(
	RuntimeDebug,
	Copy,
	Clone,
	PartialEq,
	Encode,
	Decode,
	TypeInfo,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct SchedulerParams<BlockNumber> {
	/// How often parachain groups should be rotated across parachains.
	///
	/// Must be non-zero.
	pub group_rotation_frequency: BlockNumber,
	/// Availability timeout for a block on a core, measured in blocks.
	///
	/// This is the maximum amount of blocks after a core became occupied that validators have time
	/// to make the block available.
	///
	/// This value only has effect on group rotations. If backers backed something at the end of
	/// their rotation, the occupied core affects the backing group that comes afterwards. We limit
	/// the effect one backing group can have on the next to `paras_availability_period` blocks.
	///
	/// Within a group rotation there is no timeout as backers are only affecting themselves.
	///
	/// Must be at least 1. With a value of 1, the previous group will not be able to negatively
	/// affect the following group at the expense of a tight availability timeline at group
	/// rotation boundaries.
	pub paras_availability_period: BlockNumber,
	/// The maximum number of validators to have per core.
	///
	/// `None` means no maximum.
	pub max_validators_per_core: Option<u32>,
	/// The amount of blocks ahead to schedule paras.
	pub lookahead: u32,
	/// How many cores are managed by the coretime chain.
	pub num_cores: u32,
	/// The max number of times a claim can time out in availability.
	pub max_availability_timeouts: u32,
	/// The maximum queue size of the pay as you go module.
	pub on_demand_queue_max_size: u32,
	/// The target utilization of the spot price queue in percentages.
	pub on_demand_target_queue_utilization: Perbill,
	/// How quickly the fee rises in reaction to increased utilization.
	/// The lower the number the slower the increase.
	pub on_demand_fee_variability: Perbill,
	/// The minimum amount needed to claim a slot in the spot pricing queue.
	pub on_demand_base_fee: Balance,
	/// The number of blocks a claim stays in the scheduler's claim queue before getting cleared.
	/// This number should go reasonably higher than the number of blocks in the async backing
	/// lookahead.
	pub ttl: BlockNumber,
}

impl<BlockNumber: Default + From<u32>> Default for SchedulerParams<BlockNumber> {
	fn default() -> Self {
		Self {
			group_rotation_frequency: 1u32.into(),
			paras_availability_period: 1u32.into(),
			max_validators_per_core: Default::default(),
			lookahead: 1,
			num_cores: Default::default(),
			max_availability_timeouts: Default::default(),
			on_demand_queue_max_size: ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
			on_demand_target_queue_utilization: Perbill::from_percent(25),
			on_demand_fee_variability: Perbill::from_percent(3),
			on_demand_base_fee: 10_000_000u128,
			ttl: 5u32.into(),
		}
	}
}

/// A unique descriptor of the candidate receipt.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct CandidateDescriptor<H = Hash> {
	/// The ID of the para this is a candidate for.
	pub para_id: Id,
	/// The hash of the relay-chain block this is executed in the context of.
	pub relay_parent: H,
	/// The core index where the candidate is backed.
	pub core_index: CoreIndex,
	/// Reserved bytes.
	pub reserved28b: [u8; 27],
	/// The blake2-256 hash of the persisted validation data. This is extra data derived from
	/// relay-chain state which may vary based on bitfields included before the candidate.
	/// Thus it cannot be derived entirely from the relay-parent.
	pub persisted_validation_data_hash: Hash,
	/// The blake2-256 hash of the PoV.
	pub pov_hash: Hash,
	/// The root of a block's erasure encoding Merkle tree.
	pub erasure_root: Hash,
	/// Reserved bytes.
	pub reserved64b: [u8; 64],
	/// Hash of the para header that is being generated by this candidate.
	pub para_head: Hash,
	/// The blake2-256 hash of the validation code bytes.
	pub validation_code_hash: ValidationCodeHash,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum ExtendedCommitment {
	#[codec(index = 0)]
	CoreIndex(CoreIndex),
}

pub const UMP_SEPARATOR: Vec<u8> = vec![];


// /// Commitments made in a `CandidateReceipt`. Many of these are outputs of validation.
// #[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
// #[cfg_attr(feature = "std", derive(Default, Hash))]
// pub struct CandidateCommitments<N = BlockNumber> {
// 	/// Messages destined to be interpreted by the Relay chain itself.
// 	pub upward_messages: UpwardMessages,
// 	/// Horizontal messages sent by the parachain.
// 	pub horizontal_messages: HorizontalMessages,
// 	/// New validation code.
// 	pub new_validation_code: Option<ValidationCode>,
// 	/// The head-data produced as a result of execution.
// 	pub head_data: HeadData,
// 	/// The number of messages processed from the DMQ.
// 	pub processed_downward_messages: u32,
// 	/// The mark which specifies the block number up to which all inbound HRMP messages are
// 	/// processed.
// 	pub hrmp_watermark: N,
// }

/// A candidate-receipt with commitments directly included.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct CommittedCandidateReceipt<H = Hash> {
	/// The descriptor of the candidate.
	descriptor: CandidateDescriptor<H>,
	/// The commitments of the candidate receipt.
	commitments: CandidateCommitments,
}

impl CandidateCommitments {
	/// Returns the core index the candidate has commited to.
	pub fn core_index(&self) -> Option<CoreIndex> {
		/// We need at least 2 messages for the separator and core index
		if self.upward_messages.len() < 2 {
			return None
		}

		let upward_commitments = self
			.upward_messages
			.iter()
			.cloned()
			.rev()
			.take_while(|message| message != &UMP_SEPARATOR)
			.collect::<Vec<_>>();

		// We didn't find the separator, no core index commitment.
		if upward_commitments.len() == self.upward_messages.len() ||
			upward_commitments.is_empty()
		{
			return None
		}

		// Use first commitment
		let Some(message) = upward_commitments.into_iter().rev().next() else { return None };

		match Commitment::decode(&mut message.as_slice()).ok()? {
			Commitment::CoreIndex(core_index) => Some(core_index),
		}
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum CandidateReceiptError {
	/// The specified core index is invalid.
	InvalidCoreIndex,
	/// The core index in commitments doesnt match the one in descriptor
	CoreIndexMismatch,
}

impl CommittedCandidateReceipt {
	/// Constructor from descriptor and commitments after sanity checking core index commitments.
	pub fn new(descriptor: CandidateDescriptor, commitments: CandidateCommitments, n_cores: u32) -> Result<Self, CandidateReceiptError> {
		// First check if we have a core index commitment
		if commitments.core_index() != descriptor.core_index {
			return Err(CandidateReceiptError::CoreIndexMismatch)
		}

		match descriptor.core_index {
			Some(core_index) => {
				if core_index.0 > n_cores - 1 {
					return Err(CandidateReceiptError::InvalidCoreIndex)
				} 

				Ok(Self { descriptor, commitments })
			},
			None => Ok(Self { descriptor, commitments })
		}
	}

	/// Returns are reference to commitments
	pub fn commitments(&self) -> &CandidateCommitments {
		&self.commitments
	}

	/// Returns a reference to the descriptor
	pub fn descriptor(&self) -> &CandidateDescriptor {
		&self.descriptor
	}

	
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		v7::{
			tests::dummy_committed_candidate_receipt as dummy_old_committed_candidate_receipt,
			CandidateReceipt as OldCandidateReceipt, Hash, HeadData,
		},
		vstaging::CommittedCandidateReceipt,
	};

	pub fn dummy_committed_candidate_receipt() -> CommittedCandidateReceipt {
		let zeros = Hash::zero();
		let reserved64b = [0; 64];

		CommittedCandidateReceipt {
			descriptor: CandidateDescriptor {
				para_id: 0.into(),
				relay_parent: zeros,
				core_index: Some(CoreIndex(123)),
				reserved28b: Default::default(),
				persisted_validation_data_hash: zeros,
				pov_hash: zeros,
				erasure_root: zeros,
				reserved64b,
				para_head: zeros,
				validation_code_hash: ValidationCode(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]).hash(),
			},
			commitments: CandidateCommitments {
				head_data: HeadData(vec![]),
				upward_messages: vec![].try_into().expect("empty vec fits within bounds"),
				new_validation_code: None,
				horizontal_messages: vec![].try_into().expect("empty vec fits within bounds"),
				processed_downward_messages: 0,
				hrmp_watermark: 0_u32,
			},
		}
	}

	#[test]
	fn is_binary_compatibile() {
		let mut old_ccr = dummy_old_committed_candidate_receipt();
		let mut new_ccr = dummy_committed_candidate_receipt();

		assert_eq!(old_ccr.encoded_size(), new_ccr.encoded_size());
		assert_eq!(new_ccr.commitments().core_index(), None);
	}

	#[test]
	fn test_ump_commitment() {
		let old_ccr = dummy_old_committed_candidate_receipt();
		let mut new_ccr = dummy_committed_candidate_receipt();

		// XCM messages
		new_ccr.commitments.upward_messages.force_push(vec![0u8; 256]);
		new_ccr.commitments.upward_messages.force_push(vec![0xff; 256]);

		// separator
		new_ccr.commitments.upward_messages.force_push(UMP_SEPARATOR);

		// CoreIndex commitment
		new_ccr
			.commitments
			.upward_messages
			.force_push(Commitment::CoreIndex(CoreIndex(123)).encode());

		assert_eq!(new_ccr.descriptor.core_index, Some(CoreIndex(123)));
	}
}

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
pub use crate::v6::*;
use sp_std::prelude::*;

use parity_scale_codec::{Decode, Encode};
use primitives::RuntimeDebug;
use scale_info::TypeInfo;
use sp_arithmetic::Perbill;

/// Approval voting configuration parameters
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
pub struct ApprovalVotingParams {
	/// The maximum number of candidates `approval-voting` can vote for with
	/// a single signatures.
	///
	/// Setting it to 1, means we send the approval as soon as we have it available.
	pub max_approval_coalesce_count: u32,
}

impl Default for ApprovalVotingParams {
	fn default() -> Self {
		Self { max_approval_coalesce_count: 1 }
	}
}

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
	/// The number of blocks a claim stays in the scheduler's claimqueue before getting cleared.
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

use bitvec::vec::BitVec;

/// Bit indices in the `HostConfiguration.node_features` that correspond to different node features.
pub type NodeFeatures = BitVec<u8, bitvec::order::Lsb0>;

/// Module containing feature-specific bit indices into the `NodeFeatures` bitvec.
pub mod node_features {
	/// A feature index used to indentify a bit into the node_features array stored
	/// in the HostConfiguration.
	#[repr(u8)]
	pub enum FeatureIndex {
		/// Tells if tranch0 assignments could be sent in a single certificate.
		/// Reserved for: `<https://github.com/paritytech/polkadot-sdk/issues/628>`
		EnableAssignmentsV2 = 0,
		/// This feature enables the extension of `BackedCandidate::validator_indices` by 8 bits.
		/// The value stored there represents the assumed core index where the candidates
		/// are backed. This is needed for the elastic scaling MVP.
		ElasticScalingMVP = 1,
		/// First unassigned feature bit.
		/// Every time a new feature flag is assigned it should take this value.
		/// and this should be incremented.
		FirstUnassigned = 2,
	}
}

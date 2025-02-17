// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Bridge definitions that can be used by multiple BridgeHub flavors.
//! All configurations here should be dedicated to a single chain; in other words, we don't need two
//! chains for a single pallet configuration.
//!
//! For example, the messaging pallet needs to know the sending and receiving chains, but the
//! GRANDPA tracking pallet only needs to be aware of one chain.

use super::{weights, AccountId, Balance, Balances, BlockNumber, Runtime, RuntimeEvent};
use bp_messages::LegacyLaneId;
use bp_relayers::RewardsAccountParams;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::parameter_types;
use scale_info::TypeInfo;
use xcm::VersionedLocation;

parameter_types! {
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";
}

/// Showcasing that we can handle multiple different rewards with the same pallet.
#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum BridgeReward {
	/// Rewards for the R/W bridge—distinguished by the `RewardsAccountParams` key.
	RococoWestend(RewardsAccountParams<LegacyLaneId>),
	/// Rewards for Snowbridge.
	Snowbridge,
}

impl From<RewardsAccountParams<LegacyLaneId>> for BridgeReward {
	fn from(value: RewardsAccountParams<LegacyLaneId>) -> Self {
		Self::RococoWestend(value)
	}
}

/// Implementation of `bp_relayers::PaymentProcedure` as a pay/claim rewards scheme.
pub struct BridgeRewardPayer;
impl bp_relayers::PaymentProcedure<AccountId, BridgeReward, u128> for BridgeRewardPayer {
	type Error = sp_runtime::DispatchError;
	type AlternativeBeneficiary = VersionedLocation;

	fn pay_reward(
		relayer: &AccountId,
		reward_kind: BridgeReward,
		reward: u128,
		alternative_beneficiary: Option<Self::AlternativeBeneficiary>,
	) -> Result<(), Self::Error> {
		match reward_kind {
			BridgeReward::RococoWestend(lane_params) => {
				frame_support::ensure!(
					alternative_beneficiary.is_none(),
					Self::Error::Other("`alternative_beneficiary` is not supported for `RococoWestend` rewards!")
				);
				bp_relayers::PayRewardFromAccount::<
					Balances,
					AccountId,
					LegacyLaneId,
					u128,
				>::pay_reward(
					relayer, lane_params, reward, None,
				)
			},
			BridgeReward::Snowbridge =>
				Err(sp_runtime::DispatchError::Other("Not implemented yet, check also `fn prepare_rewards_account` to return `alternative_beneficiary`!")),
		}
	}
}

/// Allows collect and claim rewards for relayers
pub type BridgeRelayersInstance = ();
impl pallet_bridge_relayers::Config<BridgeRelayersInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type RewardBalance = u128;
	type Reward = BridgeReward;
	type PaymentProcedure = BridgeRewardPayer;

	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type Balance = Balance;
	type WeightInfo = weights::pallet_bridge_relayers::WeightInfo<Runtime>;
}

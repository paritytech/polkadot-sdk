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

parameter_types! {
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";
}

/// Showcasing that we can handle multiple different rewards with the same pallet.
#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum BridgeRewardKind {
	/// Rewards for the R/W bridge—distinguished by the `RewardsAccountParams` key.
	RococoWestend(RewardsAccountParams<LegacyLaneId>),
	/// Rewards for Snowbridge.
	Snowbridge,
}

impl From<RewardsAccountParams<LegacyLaneId>> for BridgeRewardKind {
	fn from(value: RewardsAccountParams<LegacyLaneId>) -> Self {
		Self::RococoWestend(value)
	}
}

/// Implementation of `bp_relayers::PaymentProcedure` as a pay/claim rewards scheme.
pub struct BridgeRewardPayer;
impl bp_relayers::PaymentProcedure<AccountId, BridgeRewardKind, u128> for BridgeRewardPayer {
	type Error = sp_runtime::DispatchError;

	fn pay_reward(
		relayer: &AccountId,
		reward_kind: BridgeRewardKind,
		reward: u128,
	) -> Result<(), Self::Error> {
		match reward_kind {
			BridgeRewardKind::RococoWestend(lane_params) => bp_relayers::PayRewardFromAccount::<
				Balances,
				AccountId,
				LegacyLaneId,
				u128,
			>::pay_reward(
				relayer, lane_params, reward
			),
			BridgeRewardKind::Snowbridge =>
				Err(sp_runtime::DispatchError::Other("Not implemented yet!")),
		}
	}
}

/// Allows collect and claim rewards for relayers
pub type BridgeRelayersInstance = ();
impl pallet_bridge_relayers::Config<BridgeRelayersInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type Reward = u128;
	type RewardKind = BridgeRewardKind;
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

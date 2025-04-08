// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Bridge definitions that can be used by multiple bridges.

use super::{weights, AccountId, Balance, Balances, BlockNumber, Runtime, RuntimeEvent};
use bp_messages::HashedLaneId;
use bp_relayers::RewardsAccountParams;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use xcm::VersionedLocation;

frame_support::parameter_types! {
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";
	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
}

/// Showcasing that we can handle multiple different rewards with the same pallet.
#[derive(
	Clone,
	Copy,
	Debug,
	Decode,
	DecodeWithMemTracking,
	Encode,
	Eq,
	MaxEncodedLen,
	PartialEq,
	TypeInfo,
)]
pub enum BridgeReward {
	/// Rewards for the R/W bridge—distinguished by the `RewardsAccountParams` key.
	RococoWestend(RewardsAccountParams<HashedLaneId>),
}

impl From<RewardsAccountParams<HashedLaneId>> for BridgeReward {
	fn from(value: RewardsAccountParams<HashedLaneId>) -> Self {
		Self::RococoWestend(value)
	}
}

/// An enum representing the different types of supported beneficiaries.
#[derive(
	Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum BridgeRewardBeneficiaries {
	/// A local chain account.
	LocalAccount(AccountId),
	/// A beneficiary specified by a VersionedLocation.
	AssetHubLocation(VersionedLocation),
}

impl From<sp_runtime::AccountId32> for BridgeRewardBeneficiaries {
	fn from(value: sp_runtime::AccountId32) -> Self {
		BridgeRewardBeneficiaries::LocalAccount(value)
	}
}

/// Implementation of `bp_relayers::PaymentProcedure` as a pay/claim rewards scheme.
pub struct BridgeRewardPayer;
impl bp_relayers::PaymentProcedure<AccountId, BridgeReward, u128> for BridgeRewardPayer {
	type Error = sp_runtime::DispatchError;
	type Beneficiary = BridgeRewardBeneficiaries;

	fn pay_reward(
		relayer: &AccountId,
		reward_kind: BridgeReward,
		reward: u128,
		beneficiary: BridgeRewardBeneficiaries,
	) -> Result<(), Self::Error> {
		match reward_kind {
			BridgeReward::RococoWestend(lane_params) => {
				match beneficiary {
					BridgeRewardBeneficiaries::LocalAccount(account) => {
						bp_relayers::PayRewardFromAccount::<
							Balances,
							AccountId,
							HashedLaneId,
							u128,
						>::pay_reward(
							&relayer, lane_params, reward, account,
						)
					},
					BridgeRewardBeneficiaries::AssetHubLocation(_) => Err(Self::Error::Other("`AssetHubLocation` beneficiary is not supported for `RococoWestend` rewards!")),
				}
			}
		}
	}
}

/// Allows collect and claim rewards for the relayers
pub type BridgeRelayersInstance = pallet_bridge_relayers::Instance1;
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

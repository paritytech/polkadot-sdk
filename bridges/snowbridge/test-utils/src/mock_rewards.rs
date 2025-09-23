// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use bp_relayers::{PaymentProcedure, RewardLedger, RewardsAccountOwner, RewardsAccountParams};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::DispatchResult, parameter_types, sp_runtime};
use scale_info::TypeInfo;
use xcm::opaque::latest::Location;

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
	/// Rewards for Snowbridge.
	Snowbridge,
}

pub struct MockPaymentProcedure;

// Provide a no-op or mock implementation for the required trait
impl PaymentProcedure<sp_runtime::AccountId32, RewardsAccountParams<u64>, u128>
	for MockPaymentProcedure
{
	type Error = DispatchResult;
	type Beneficiary = Location;
	fn pay_reward(
		_who: &sp_runtime::AccountId32,
		_reward_params: bp_relayers::RewardsAccountParams<u64>,
		_reward_balance: u128,
		_beneficiary: Self::Beneficiary,
	) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl From<BridgeReward> for RewardsAccountParams<u64> {
	fn from(_bridge_reward: BridgeReward) -> Self {
		RewardsAccountParams::new(1, [0; 4], RewardsAccountOwner::ThisChain)
	}
}

parameter_types! {
	pub static RegisteredRewardsCount: u128 = 0;
	pub static RegisteredRewardAmount: u128 = 0;
}

pub struct MockRewardLedger;

impl RewardLedger<sp_runtime::AccountId32, BridgeReward, u128> for MockRewardLedger {
	fn register_reward(
		_relayer: &sp_runtime::AccountId32,
		_reward: BridgeReward,
		reward_balance: u128,
	) {
		RegisteredRewardsCount::set(RegisteredRewardsCount::get().saturating_add(1));
		RegisteredRewardAmount::set(reward_balance);
	}
}

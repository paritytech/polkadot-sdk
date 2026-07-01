// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Code that allows relayers pallet to be used as a payment mechanism for
//! the `pallet-bridge-messages` pallet using `RewardsAccountParams`.

use crate::{Config, Pallet};

use alloc::collections::vec_deque::VecDeque;
use bp_messages::{
	source_chain::{DeliveryConfirmationPayments, RelayersRewards},
	MessageNonce,
};
pub use bp_relayers::PayRewardFromAccount;
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::Chain;
use core::{marker::PhantomData, ops::RangeInclusive};
use frame_support::{sp_runtime::SaturatedConversion, traits::Get};
use pallet_bridge_messages::LaneIdOf;
use sp_arithmetic::traits::{Saturating, Zero};

/// Adapter that allows relayers pallet to be used as a delivery+dispatch payment mechanism
/// for the `pallet-bridge-messages` pallet and using `RewardsAccountParams`.
pub struct DeliveryConfirmationPaymentsAdapter<T, MI, RI, DeliveryReward>(
	PhantomData<(T, MI, RI, DeliveryReward)>,
);

impl<T, MI, RI, DeliveryReward> DeliveryConfirmationPayments<T::AccountId, LaneIdOf<T, MI>>
	for DeliveryConfirmationPaymentsAdapter<T, MI, RI, DeliveryReward>
where
	T: Config<RI> + pallet_bridge_messages::Config<MI>,
	MI: 'static,
	RI: 'static,
	DeliveryReward: Get<T::RewardBalance>,
	<T as Config<RI>>::Reward: From<RewardsAccountParams<LaneIdOf<T, MI>>>,
{
	type Error = &'static str;

	fn pay_reward(
		lane_id: LaneIdOf<T, MI>,
		messages_relayers: VecDeque<bp_messages::UnrewardedRelayer<T::AccountId>>,
		confirmation_relayer: &T::AccountId,
		received_range: &RangeInclusive<bp_messages::MessageNonce>,
	) -> MessageNonce {
		let relayers_rewards =
			bp_messages::calc_relayers_rewards::<T::AccountId>(messages_relayers, received_range);
		let rewarded_relayers = relayers_rewards.len();

		register_relayers_rewards::<T, RI, MI>(
			confirmation_relayer,
			relayers_rewards,
			RewardsAccountParams::new(
				lane_id,
				T::BridgedChain::ID,
				RewardsAccountOwner::BridgedChain,
			),
			DeliveryReward::get(),
		);

		rewarded_relayers as _
	}
}

// Update rewards to given relayers, optionally rewarding confirmation relayer.
fn register_relayers_rewards<
	T: Config<RI> + pallet_bridge_messages::Config<MI>,
	RI: 'static,
	MI: 'static,
>(
	confirmation_relayer: &T::AccountId,
	relayers_rewards: RelayersRewards<T::AccountId>,
	lane_id: RewardsAccountParams<LaneIdOf<T, MI>>,
	delivery_fee: T::RewardBalance,
) where
	<T as Config<RI>>::Reward: From<RewardsAccountParams<LaneIdOf<T, MI>>>,
{
	// reward every relayer except `confirmation_relayer`
	let mut confirmation_relayer_reward = T::RewardBalance::zero();
	for (relayer, messages) in relayers_rewards {
		// sane runtime configurations guarantee that the number of messages will be below
		// `u32::MAX`
		let relayer_reward =
			T::RewardBalance::saturated_from(messages).saturating_mul(delivery_fee);

		if relayer != *confirmation_relayer {
			Pallet::<T, RI>::register_relayer_reward(lane_id.into(), &relayer, relayer_reward);
		} else {
			confirmation_relayer_reward =
				confirmation_relayer_reward.saturating_add(relayer_reward);
		}
	}

	// finally - pay reward to confirmation relayer
	Pallet::<T, RI>::register_relayer_reward(
		lane_id.into(),
		confirmation_relayer,
		confirmation_relayer_reward,
	);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, RelayerRewards};
	use bp_messages::LaneIdType;
	use bp_relayers::PaymentProcedure;
	use frame_support::{
		assert_ok,
		traits::fungible::{Inspect, Mutate},
	};

	const RELAYER_1: ThisChainAccountId = 1;
	const RELAYER_2: ThisChainAccountId = 2;
	const RELAYER_3: ThisChainAccountId = 3;

	fn relayers_rewards() -> RelayersRewards<ThisChainAccountId> {
		vec![(RELAYER_1, 2), (RELAYER_2, 3)].into_iter().collect()
	}

	#[test]
	fn confirmation_relayer_is_rewarded_if_it_has_also_delivered_messages() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime, (), ()>(
				&RELAYER_2,
				relayers_rewards(),
				test_reward_account_param(),
				50,
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_1, test_reward_account_param()),
				Some(100)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_2, test_reward_account_param()),
				Some(150)
			);
		});
	}

	#[test]
	fn confirmation_relayer_is_not_rewarded_if_it_has_not_delivered_any_messages() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime, (), ()>(
				&RELAYER_3,
				relayers_rewards(),
				test_reward_account_param(),
				50,
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_1, test_reward_account_param()),
				Some(100)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_2, test_reward_account_param()),
				Some(150)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_3, test_reward_account_param()),
				None
			);
		});
	}

	#[test]
	fn pay_reward_from_account_actually_pays_reward() {
		type Balances = pallet_balances::Pallet<TestRuntime>;
		type PayLaneRewardFromAccount =
			PayRewardFromAccount<Balances, ThisChainAccountId, TestLaneIdType, RewardBalance>;

		run_test(|| {
			let in_lane_0 = RewardsAccountParams::new(
				TestLaneIdType::try_new(1, 2).unwrap(),
				*b"test",
				RewardsAccountOwner::ThisChain,
			);
			let out_lane_1 = RewardsAccountParams::new(
				TestLaneIdType::try_new(1, 3).unwrap(),
				*b"test",
				RewardsAccountOwner::BridgedChain,
			);

			let in_lane0_rewards_account = PayLaneRewardFromAccount::rewards_account(in_lane_0);
			let out_lane1_rewards_account = PayLaneRewardFromAccount::rewards_account(out_lane_1);

			assert_ok!(Balances::mint_into(&in_lane0_rewards_account, 200));
			assert_ok!(Balances::mint_into(&out_lane1_rewards_account, 100));
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 200);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 100);
			assert_eq!(Balances::balance(&1), 0);
			assert_eq!(Balances::balance(&2), 0);

			assert_ok!(PayLaneRewardFromAccount::pay_reward(&1, in_lane_0, 100, 1_u64));
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 100);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 100);
			assert_eq!(Balances::balance(&1), 100);
			assert_eq!(Balances::balance(&2), 0);

			assert_ok!(PayLaneRewardFromAccount::pay_reward(&1, out_lane_1, 100, 1_u64));
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 100);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 0);
			assert_eq!(Balances::balance(&1), 200);
			assert_eq!(Balances::balance(&2), 0);

			assert_ok!(PayLaneRewardFromAccount::pay_reward(&1, in_lane_0, 100, 2_u64));
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 0);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 0);
			assert_eq!(Balances::balance(&1), 200);
			assert_eq!(Balances::balance(&2), 100);
		});
	}
}

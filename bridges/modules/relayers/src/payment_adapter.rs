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

//! Code that allows relayers pallet to be used as a payment mechanism for the messages pallet.

use crate::{Config, Pallet};

use bp_messages::{
	source_chain::{DeliveryConfirmationPayments, RelayersRewards},
	LaneId, MessageNonce,
};
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use frame_support::{sp_runtime::SaturatedConversion, traits::Get};
use sp_arithmetic::traits::{Saturating, Zero};
use sp_std::{collections::vec_deque::VecDeque, marker::PhantomData, ops::RangeInclusive};

/// Adapter that allows relayers pallet to be used as a delivery+dispatch payment mechanism
/// for the messages pallet.
pub struct DeliveryConfirmationPaymentsAdapter<T, MI, DeliveryReward>(
	PhantomData<(T, MI, DeliveryReward)>,
);

impl<T, MI, DeliveryReward> DeliveryConfirmationPayments<T::AccountId>
	for DeliveryConfirmationPaymentsAdapter<T, MI, DeliveryReward>
where
	T: Config + pallet_bridge_messages::Config<MI>,
	MI: 'static,
	DeliveryReward: Get<T::Reward>,
{
	type Error = &'static str;

	fn pay_reward(
		lane_id: LaneId,
		messages_relayers: VecDeque<bp_messages::UnrewardedRelayer<T::AccountId>>,
		confirmation_relayer: &T::AccountId,
		received_range: &RangeInclusive<bp_messages::MessageNonce>,
	) -> MessageNonce {
		let relayers_rewards =
			bp_messages::calc_relayers_rewards::<T::AccountId>(messages_relayers, received_range);
		let rewarded_relayers = relayers_rewards.len();

		register_relayers_rewards::<T>(
			confirmation_relayer,
			relayers_rewards,
			RewardsAccountParams::new(
				lane_id,
				T::BridgedChainId::get(),
				RewardsAccountOwner::BridgedChain,
			),
			DeliveryReward::get(),
		);

		rewarded_relayers as _
	}
}

// Update rewards to given relayers, optionally rewarding confirmation relayer.
fn register_relayers_rewards<T: Config>(
	confirmation_relayer: &T::AccountId,
	relayers_rewards: RelayersRewards<T::AccountId>,
	lane_id: RewardsAccountParams,
	delivery_fee: T::Reward,
) {
	// reward every relayer except `confirmation_relayer`
	let mut confirmation_relayer_reward = T::Reward::zero();
	for (relayer, messages) in relayers_rewards {
		// sane runtime configurations guarantee that the number of messages will be below
		// `u32::MAX`
		let relayer_reward = T::Reward::saturated_from(messages).saturating_mul(delivery_fee);

		if relayer != *confirmation_relayer {
			Pallet::<T>::register_relayer_reward(lane_id, &relayer, relayer_reward);
		} else {
			confirmation_relayer_reward =
				confirmation_relayer_reward.saturating_add(relayer_reward);
		}
	}

	// finally - pay reward to confirmation relayer
	Pallet::<T>::register_relayer_reward(
		lane_id,
		confirmation_relayer,
		confirmation_relayer_reward,
	);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, RelayerRewards};

	const RELAYER_1: AccountId = 1;
	const RELAYER_2: AccountId = 2;
	const RELAYER_3: AccountId = 3;

	fn relayers_rewards() -> RelayersRewards<AccountId> {
		vec![(RELAYER_1, 2), (RELAYER_2, 3)].into_iter().collect()
	}

	#[test]
	fn confirmation_relayer_is_rewarded_if_it_has_also_delivered_messages() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime>(
				&RELAYER_2,
				relayers_rewards(),
				TEST_REWARDS_ACCOUNT_PARAMS,
				50,
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_1, TEST_REWARDS_ACCOUNT_PARAMS),
				Some(100)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_2, TEST_REWARDS_ACCOUNT_PARAMS),
				Some(150)
			);
		});
	}

	#[test]
	fn confirmation_relayer_is_not_rewarded_if_it_has_not_delivered_any_messages() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime>(
				&RELAYER_3,
				relayers_rewards(),
				TEST_REWARDS_ACCOUNT_PARAMS,
				50,
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_1, TEST_REWARDS_ACCOUNT_PARAMS),
				Some(100)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_2, TEST_REWARDS_ACCOUNT_PARAMS),
				Some(150)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_3, TEST_REWARDS_ACCOUNT_PARAMS),
				None
			);
		});
	}
}

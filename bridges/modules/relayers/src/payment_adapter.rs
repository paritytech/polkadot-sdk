// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use bp_messages::source_chain::{DeliveryConfirmationPayments, RelayersRewards};
use frame_support::{sp_runtime::SaturatedConversion, traits::Get};
use sp_arithmetic::traits::{Saturating, UniqueSaturatedFrom, Zero};
use sp_std::{collections::vec_deque::VecDeque, marker::PhantomData, ops::RangeInclusive};

/// Adapter that allows relayers pallet to be used as a delivery+dispatch payment mechanism
/// for the messages pallet.
pub struct DeliveryConfirmationPaymentsAdapter<T, DeliveryReward, ConfirmationReward>(
	PhantomData<(T, DeliveryReward, ConfirmationReward)>,
);

impl<T, DeliveryReward, ConfirmationReward> DeliveryConfirmationPayments<T::AccountId>
	for DeliveryConfirmationPaymentsAdapter<T, DeliveryReward, ConfirmationReward>
where
	T: Config,
	DeliveryReward: Get<T::Reward>,
	ConfirmationReward: Get<T::Reward>,
{
	type Error = &'static str;

	fn pay_reward(
		lane_id: bp_messages::LaneId,
		messages_relayers: VecDeque<bp_messages::UnrewardedRelayer<T::AccountId>>,
		confirmation_relayer: &T::AccountId,
		received_range: &RangeInclusive<bp_messages::MessageNonce>,
	) {
		let relayers_rewards =
			bp_messages::calc_relayers_rewards::<T::AccountId>(messages_relayers, received_range);

		register_relayers_rewards::<T>(
			confirmation_relayer,
			relayers_rewards,
			lane_id,
			DeliveryReward::get(),
			ConfirmationReward::get(),
		);
	}
}

// Update rewards to given relayers, optionally rewarding confirmation relayer.
fn register_relayers_rewards<T: Config>(
	confirmation_relayer: &T::AccountId,
	relayers_rewards: RelayersRewards<T::AccountId>,
	lane_id: bp_messages::LaneId,
	delivery_fee: T::Reward,
	confirmation_fee: T::Reward,
) {
	// reward every relayer except `confirmation_relayer`
	let mut confirmation_relayer_reward = T::Reward::zero();
	for (relayer, messages) in relayers_rewards {
		// sane runtime configurations guarantee that the number of messages will be below
		// `u32::MAX`
		let mut relayer_reward =
			T::Reward::unique_saturated_from(messages).saturating_mul(delivery_fee);

		if relayer != *confirmation_relayer {
			// If delivery confirmation is submitted by other relayer, let's deduct confirmation fee
			// from relayer reward.
			//
			// If confirmation fee has been increased (or if it was the only component of message
			// fee), then messages relayer may receive zero reward.
			let mut confirmation_reward =
				T::Reward::saturated_from(messages).saturating_mul(confirmation_fee);
			confirmation_reward = sp_std::cmp::min(confirmation_reward, relayer_reward);
			relayer_reward = relayer_reward.saturating_sub(confirmation_reward);
			confirmation_relayer_reward =
				confirmation_relayer_reward.saturating_add(confirmation_reward);
			Pallet::<T>::register_relayer_reward(lane_id, &relayer, relayer_reward);
		} else {
			// If delivery confirmation is submitted by this relayer, let's add confirmation fee
			// from other relayers to this relayer reward.
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
				TEST_LANE_ID,
				50,
				10,
			);

			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_1, TEST_LANE_ID), Some(80));
			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_2, TEST_LANE_ID), Some(170));
		});
	}

	#[test]
	fn confirmation_relayer_is_rewarded_if_it_has_not_delivered_any_delivered_messages() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime>(
				&RELAYER_3,
				relayers_rewards(),
				TEST_LANE_ID,
				50,
				10,
			);

			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_1, TEST_LANE_ID), Some(80));
			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_2, TEST_LANE_ID), Some(120));
			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_3, TEST_LANE_ID), Some(50));
		});
	}

	#[test]
	fn only_confirmation_relayer_is_rewarded_if_confirmation_fee_has_significantly_increased() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime>(
				&RELAYER_3,
				relayers_rewards(),
				TEST_LANE_ID,
				50,
				1000,
			);

			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_1, TEST_LANE_ID), None);
			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_2, TEST_LANE_ID), None);
			assert_eq!(RelayerRewards::<TestRuntime>::get(RELAYER_3, TEST_LANE_ID), Some(250));
		});
	}
}

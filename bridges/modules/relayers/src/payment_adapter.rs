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
use bp_runtime::Chain;
use frame_support::traits::Get;
use sp_arithmetic::traits::{Saturating, UniqueSaturatedFrom};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{collections::vec_deque::VecDeque, marker::PhantomData, ops::RangeInclusive};

/// Adapter that allows relayers pallet to be used as a delivery+dispatch payment mechanism
/// for the messages pallet.
///
/// This adapter assumes 1:1 mapping of `RelayerRewardAtSource` to `T::Reward`. The reward for
/// delivering a single message, will never be larger than the `MaxRewardPerMessage`. If relayer
/// has not specified expected reward, it gets the `DefaultRewardPerMessage` for every message.
///
/// We assume that the confirmation transaction cost is refunded by the signed extension,
/// implemented by the pallet. So we do not reward confirmation relayer additionally here.
pub struct DeliveryConfirmationPaymentsAdapter<T, MI, DefaultRewardPerMessage, MaxRewardPerMessage>(
	PhantomData<(T, MI, DefaultRewardPerMessage, MaxRewardPerMessage)>,
);

impl<T, MI, DefaultRewardPerMessage, MaxRewardPerMessage> DeliveryConfirmationPayments<T::AccountId>
	for DeliveryConfirmationPaymentsAdapter<T, MI, DefaultRewardPerMessage, MaxRewardPerMessage>
where
	T: Config + pallet_bridge_messages::Config<MI>,
	MI: 'static,
	DefaultRewardPerMessage: Get<T::Reward>,
	MaxRewardPerMessage: Get<T::Reward>,
{
	type Error = &'static str;

	fn pay_reward(
		lane_id: LaneId,
		messages_relayers: VecDeque<bp_messages::UnrewardedRelayer<T::AccountId>>,
		_confirmation_relayer: &T::AccountId,
		received_range: &RangeInclusive<bp_messages::MessageNonce>,
	) -> MessageNonce {
		let relayers_rewards =
			bp_messages::calc_relayers_rewards_at_source::<T::AccountId, T::Reward>(
				messages_relayers,
				received_range,
				|messages, relayer_reward_per_message| {
					let relayer_reward_per_message = sp_std::cmp::min(
						MaxRewardPerMessage::get(),
						relayer_reward_per_message
							.map(|x| x.unique_saturated_into())
							.unwrap_or_else(|| DefaultRewardPerMessage::get()),
					);

					T::Reward::unique_saturated_from(messages)
						.saturating_mul(relayer_reward_per_message)
				},
			);
		let rewarded_relayers = relayers_rewards.len();

		register_relayers_rewards::<T>(
			relayers_rewards,
			RewardsAccountParams::new(
				lane_id,
				T::BridgedChain::ID,
				RewardsAccountOwner::BridgedChain,
			),
		);

		rewarded_relayers as _
	}
}

/// Register relayer rewards for delivering messages.
fn register_relayers_rewards<T: Config>(
	relayers_rewards: RelayersRewards<T::AccountId, T::Reward>,
	reward_account: RewardsAccountParams,
) {
	for (relayer, relayer_reward) in relayers_rewards {
		Pallet::<T>::register_relayer_reward(reward_account, &relayer, relayer_reward);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, RelayerRewards};

	const RELAYER_1: ThisChainAccountId = 1;
	const RELAYER_2: ThisChainAccountId = 2;
	const RELAYER_3: ThisChainAccountId = 3;

	fn relayers_rewards() -> RelayersRewards<ThisChainAccountId, ThisChainBalance> {
		vec![(RELAYER_1, 2), (RELAYER_2, 3)].into_iter().collect()
	}

	#[test]
	fn register_relayers_rewards_works() {
		run_test(|| {
			register_relayers_rewards::<TestRuntime>(
				relayers_rewards(),
				test_reward_account_param(),
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_1, test_reward_account_param()),
				Some(2)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_2, test_reward_account_param()),
				Some(3)
			);
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(RELAYER_3, test_reward_account_param()),
				None
			);
		});
	}

	#[test]
	fn reward_per_message_is_default_if_not_specified() {
		run_test(|| {
			let mut delivered_messages = bp_messages::DeliveredMessages::new(1, None);
			delivered_messages.note_dispatched_message();

			<TestDeliveryConfirmationPaymentsAdapter as DeliveryConfirmationPayments<
				ThisChainAccountId,
			>>::pay_reward(
				test_lane_id(),
				vec![bp_messages::UnrewardedRelayer { relayer: 42, messages: delivered_messages }]
					.into(),
				&43,
				&(1..=2),
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(42, test_reward_account_param()),
				Some(DEFAULT_REWARD_PER_MESSAGE * 2),
			);
		});
	}

	#[test]
	fn reward_per_message_is_never_larger_than_max_reward_per_message() {
		run_test(|| {
			let mut delivered_messages =
				bp_messages::DeliveredMessages::new(1, Some(MAX_REWARD_PER_MESSAGE + 1));
			delivered_messages.note_dispatched_message();

			TestDeliveryConfirmationPaymentsAdapter::pay_reward(
				test_lane_id(),
				vec![bp_messages::UnrewardedRelayer { relayer: 42, messages: delivered_messages }]
					.into(),
				&43,
				&(1..=2),
			);

			assert_eq!(
				RelayerRewards::<TestRuntime>::get(42, test_reward_account_param()),
				Some(MAX_REWARD_PER_MESSAGE * 2),
			);
		});
	}
}

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

//! Runtime module that is used to store relayer rewards and (in the future) to
//! coordinate relations between relayers.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

use bp_relayers::{PaymentProcedure, RelayerRewardsKeyProvider, RewardsAccountParams};
use bp_runtime::StorageDoubleMapKeyProvider;
use frame_support::sp_runtime::Saturating;
use sp_arithmetic::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::marker::PhantomData;

pub use pallet::*;
pub use payment_adapter::DeliveryConfirmationPaymentsAdapter;
pub use weights::WeightInfo;

pub mod benchmarking;

mod mock;
mod payment_adapter;

pub mod weights;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-relayers";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// `RelayerRewardsKeyProvider` for given configuration.
	type RelayerRewardsKeyProviderOf<T> =
		RelayerRewardsKeyProvider<<T as frame_system::Config>::AccountId, <T as Config>::Reward>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Type of relayer reward.
		type Reward: AtLeast32BitUnsigned + Copy + Parameter + MaxEncodedLen;
		/// Pay rewards adapter.
		type PaymentProcedure: PaymentProcedure<Self::AccountId, Self::Reward>;
		/// Pallet call weights.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Claim accumulated rewards.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_rewards())]
		pub fn claim_rewards(
			origin: OriginFor<T>,
			rewards_account_params: RewardsAccountParams,
		) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			RelayerRewards::<T>::try_mutate_exists(
				&relayer,
				rewards_account_params,
				|maybe_reward| -> DispatchResult {
					let reward = maybe_reward.take().ok_or(Error::<T>::NoRewardForRelayer)?;
					T::PaymentProcedure::pay_reward(&relayer, rewards_account_params, reward)
						.map_err(|e| {
							log::trace!(
								target: LOG_TARGET,
								"Failed to pay {:?} rewards to {:?}: {:?}",
								rewards_account_params,
								relayer,
								e,
							);
							Error::<T>::FailedToPayReward
						})?;

					Self::deposit_event(Event::<T>::RewardPaid {
						relayer: relayer.clone(),
						rewards_account_params,
						reward,
					});
					Ok(())
				},
			)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Register reward for given relayer.
		pub fn register_relayer_reward(
			rewards_account_params: RewardsAccountParams,
			relayer: &T::AccountId,
			reward: T::Reward,
		) {
			if reward.is_zero() {
				return
			}

			RelayerRewards::<T>::mutate(
				relayer,
				rewards_account_params,
				|old_reward: &mut Option<T::Reward>| {
					let new_reward = old_reward.unwrap_or_else(Zero::zero).saturating_add(reward);
					*old_reward = Some(new_reward);

					log::trace!(
						target: crate::LOG_TARGET,
						"Relayer {:?} can now claim reward for serving payer {:?}: {:?}",
						relayer,
						rewards_account_params,
						new_reward,
					);
				},
			);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward has been paid to the relayer.
		RewardPaid {
			/// Relayer account that has been rewarded.
			relayer: T::AccountId,
			/// Relayer has received reward from this account.
			rewards_account_params: RewardsAccountParams,
			/// Reward amount.
			reward: T::Reward,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// No reward can be claimed by given relayer.
		NoRewardForRelayer,
		/// Reward payment procedure has failed.
		FailedToPayReward,
	}

	/// Map of the relayer => accumulated reward.
	#[pallet::storage]
	#[pallet::getter(fn relayer_reward)]
	pub type RelayerRewards<T: Config> = StorageDoubleMap<
		_,
		<RelayerRewardsKeyProviderOf<T> as StorageDoubleMapKeyProvider>::Hasher1,
		<RelayerRewardsKeyProviderOf<T> as StorageDoubleMapKeyProvider>::Key1,
		<RelayerRewardsKeyProviderOf<T> as StorageDoubleMapKeyProvider>::Hasher2,
		<RelayerRewardsKeyProviderOf<T> as StorageDoubleMapKeyProvider>::Key2,
		<RelayerRewardsKeyProviderOf<T> as StorageDoubleMapKeyProvider>::Value,
		OptionQuery,
	>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use mock::{RuntimeEvent as TestEvent, *};

	use crate::Event::RewardPaid;
	use bp_messages::LaneId;
	use bp_relayers::RewardsAccountOwner;
	use frame_support::{
		assert_noop, assert_ok,
		traits::fungible::{Inspect, Mutate},
	};
	use frame_system::{EventRecord, Pallet as System, Phase};
	use sp_runtime::DispatchError;

	fn get_ready_for_events() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();
	}

	#[test]
	fn root_cant_claim_anything() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::claim_rewards(
					RuntimeOrigin::root(),
					TEST_REWARDS_ACCOUNT_PARAMS
				),
				DispatchError::BadOrigin,
			);
		});
	}

	#[test]
	fn relayer_cant_claim_if_no_reward_exists() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::claim_rewards(
					RuntimeOrigin::signed(REGULAR_RELAYER),
					TEST_REWARDS_ACCOUNT_PARAMS
				),
				Error::<TestRuntime>::NoRewardForRelayer,
			);
		});
	}

	#[test]
	fn relayer_cant_claim_if_payment_procedure_fails() {
		run_test(|| {
			RelayerRewards::<TestRuntime>::insert(
				FAILING_RELAYER,
				TEST_REWARDS_ACCOUNT_PARAMS,
				100,
			);
			assert_noop!(
				Pallet::<TestRuntime>::claim_rewards(
					RuntimeOrigin::signed(FAILING_RELAYER),
					TEST_REWARDS_ACCOUNT_PARAMS
				),
				Error::<TestRuntime>::FailedToPayReward,
			);
		});
	}

	#[test]
	fn relayer_can_claim_reward() {
		run_test(|| {
			get_ready_for_events();

			RelayerRewards::<TestRuntime>::insert(
				REGULAR_RELAYER,
				TEST_REWARDS_ACCOUNT_PARAMS,
				100,
			);
			assert_ok!(Pallet::<TestRuntime>::claim_rewards(
				RuntimeOrigin::signed(REGULAR_RELAYER),
				TEST_REWARDS_ACCOUNT_PARAMS
			));
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(REGULAR_RELAYER, TEST_REWARDS_ACCOUNT_PARAMS),
				None
			);

			//Check if the `RewardPaid` event was emitted.
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Relayers(RewardPaid {
						relayer: REGULAR_RELAYER,
						rewards_account_params: TEST_REWARDS_ACCOUNT_PARAMS,
						reward: 100
					}),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn pay_reward_from_account_actually_pays_reward() {
		type Balances = pallet_balances::Pallet<TestRuntime>;
		type PayLaneRewardFromAccount = bp_relayers::PayRewardFromAccount<Balances, AccountId>;

		run_test(|| {
			let in_lane_0 = RewardsAccountParams::new(
				LaneId([0, 0, 0, 0]),
				*b"test",
				RewardsAccountOwner::ThisChain,
			);
			let out_lane_1 = RewardsAccountParams::new(
				LaneId([0, 0, 0, 1]),
				*b"test",
				RewardsAccountOwner::BridgedChain,
			);

			let in_lane0_rewards_account = PayLaneRewardFromAccount::rewards_account(in_lane_0);
			let out_lane1_rewards_account = PayLaneRewardFromAccount::rewards_account(out_lane_1);

			Balances::mint_into(&in_lane0_rewards_account, 100).unwrap();
			Balances::mint_into(&out_lane1_rewards_account, 100).unwrap();
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 100);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 100);
			assert_eq!(Balances::balance(&1), 0);

			PayLaneRewardFromAccount::pay_reward(&1, in_lane_0, 100).unwrap();
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 0);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 100);
			assert_eq!(Balances::balance(&1), 100);

			PayLaneRewardFromAccount::pay_reward(&1, out_lane_1, 100).unwrap();
			assert_eq!(Balances::balance(&in_lane0_rewards_account), 0);
			assert_eq!(Balances::balance(&out_lane1_rewards_account), 0);
			assert_eq!(Balances::balance(&1), 200);
		});
	}
}

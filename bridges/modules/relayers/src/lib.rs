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

//! Runtime module that is used to store relayer rewards and (in the future) to
//! coordinate relations between relayers.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use bp_relayers::RewardLedger;
use bp_relayers::{PaymentProcedure, Registration, RelayerRewardsKeyProvider, StakeAndSlash};
use bp_runtime::StorageDoubleMapKeyProvider;
use core::marker::PhantomData;
use frame_support::{fail, traits::tokens::Balance};
use sp_arithmetic::traits::{AtLeast32BitUnsigned, Zero};
use sp_runtime::{
	traits::{CheckedSub, IdentifyAccount},
	Saturating,
};

pub use pallet::*;
pub use payment_adapter::{DeliveryConfirmationPaymentsAdapter, PayRewardFromAccount};
pub use stake_adapter::StakeAndSlashNamed;
pub use weights::WeightInfo;
pub use weights_ext::WeightInfoExt;

mod mock;
mod payment_adapter;
mod stake_adapter;
mod weights_ext;

pub mod benchmarking;
pub mod extension;
pub mod migration;
pub mod weights;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-relayers";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// `RelayerRewardsKeyProvider` for given configuration.
	type RelayerRewardsKeyProviderOf<T, I> = RelayerRewardsKeyProvider<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Reward,
		<T as Config<I>>::RewardBalance,
	>;

	/// Shortcut to alternative beneficiary type for `Config::PaymentProcedure`.
	pub type BeneficiaryOf<T, I> = <<T as Config<I>>::PaymentProcedure as PaymentProcedure<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Reward,
		<T as Config<I>>::RewardBalance,
	>>::Beneficiary;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type of relayer reward balance.
		type RewardBalance: AtLeast32BitUnsigned + Copy + Member + Parameter + MaxEncodedLen;
		/// Reward discriminator type. The pallet can collect different types of rewards for a
		/// single account, so `Reward` is used as the second key in the `RelayerRewards` double
		/// map.
		///
		/// For example, rewards for different bridges can be stored, where `Reward` is
		/// implemented as an enum representing each bridge.
		type Reward: Parameter + MaxEncodedLen + Send + Sync + Copy + Clone;

		/// Pay rewards scheme.
		type PaymentProcedure: PaymentProcedure<Self::AccountId, Self::Reward, Self::RewardBalance>;

		/// Stake and slash scheme.
		type StakeAndSlash: StakeAndSlash<Self::AccountId, BlockNumberFor<Self>, Self::Balance>;
		/// Type for representing balance of an account used for `T::StakeAndSlash`.
		type Balance: Balance;

		/// Pallet call weights.
		type WeightInfo: WeightInfoExt;
	}

	#[pallet::pallet]
	#[pallet::storage_version(migration::STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		BeneficiaryOf<T, I>: From<<T as frame_system::Config>::AccountId>,
	{
		/// Claim accumulated rewards.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_rewards())]
		pub fn claim_rewards(origin: OriginFor<T>, reward_kind: T::Reward) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			Self::do_claim_rewards(relayer.clone(), reward_kind, relayer.into())
		}

		/// Register relayer or update its registration.
		///
		/// Registration allows relayer to get priority boost for its message delivery transactions.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::register())]
		pub fn register(origin: OriginFor<T>, valid_till: BlockNumberFor<T>) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			// valid till must be larger than the current block number and the lease must be larger
			// than the `RequiredRegistrationLease`
			let lease = valid_till.saturating_sub(frame_system::Pallet::<T>::block_number());
			ensure!(
				lease > Self::required_registration_lease(),
				Error::<T, I>::InvalidRegistrationLease
			);

			RegisteredRelayers::<T, I>::try_mutate(
				&relayer,
				|maybe_registration| -> DispatchResult {
					let mut registration = maybe_registration
						.unwrap_or_else(|| Registration { valid_till, stake: Zero::zero() });

					// new `valid_till` must be larger (or equal) than the old one
					ensure!(
						valid_till >= registration.valid_till,
						Error::<T, I>::CannotReduceRegistrationLease,
					);
					registration.valid_till = valid_till;

					// regarding stake, there are three options:
					// - if relayer stake is larger than required stake, we may do unreserve
					// - if relayer stake equals to required stake, we do nothing
					// - if relayer stake is smaller than required stake, we do additional reserve
					let required_stake = Self::required_stake();
					if let Some(to_unreserve) = registration.stake.checked_sub(&required_stake) {
						Self::do_unreserve(&relayer, to_unreserve)?;
					} else if let Some(to_reserve) = required_stake.checked_sub(&registration.stake)
					{
						T::StakeAndSlash::reserve(&relayer, to_reserve).map_err(|e| {
							log::trace!(
								target: LOG_TARGET,
								"Failed to reserve {:?} on relayer {:?} account: {:?}",
								to_reserve,
								relayer,
								e,
							);

							Error::<T, I>::FailedToReserve
						})?;
					}
					registration.stake = required_stake;

					log::trace!(target: LOG_TARGET, "Successfully registered relayer: {:?}", relayer);
					Self::deposit_event(Event::<T, I>::RegistrationUpdated {
						relayer: relayer.clone(),
						registration,
					});

					*maybe_registration = Some(registration);

					Ok(())
				},
			)
		}

		/// `Deregister` relayer.
		///
		/// After this call, message delivery transactions of the relayer won't get any priority
		/// boost.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::deregister())]
		pub fn deregister(origin: OriginFor<T>) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			RegisteredRelayers::<T, I>::try_mutate(
				&relayer,
				|maybe_registration| -> DispatchResult {
					let registration = match maybe_registration.take() {
						Some(registration) => registration,
						None => fail!(Error::<T, I>::NotRegistered),
					};

					// we can't deregister until `valid_till + 1`
					ensure!(
						registration.valid_till < frame_system::Pallet::<T>::block_number(),
						Error::<T, I>::RegistrationIsStillActive,
					);

					// if stake is non-zero, we should do unreserve
					if !registration.stake.is_zero() {
						Self::do_unreserve(&relayer, registration.stake)?;
					}

					log::trace!(target: LOG_TARGET, "Successfully deregistered relayer: {:?}", relayer);
					Self::deposit_event(Event::<T, I>::Deregistered { relayer: relayer.clone() });

					*maybe_registration = None;

					Ok(())
				},
			)
		}

		/// Claim accumulated rewards and send them to the alternative beneficiary.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::claim_rewards_to())]
		pub fn claim_rewards_to(
			origin: OriginFor<T>,
			reward_kind: T::Reward,
			beneficiary: BeneficiaryOf<T, I>,
		) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			Self::do_claim_rewards(relayer, reward_kind, beneficiary)
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Relayers that have reserved some of their balance to get free priority boost
		/// for their message delivery transactions.
		pub fn registered_relayer(
			relayer: &T::AccountId,
		) -> Option<Registration<BlockNumberFor<T>, T::Balance>> {
			RegisteredRelayers::<T, I>::get(relayer)
		}

		/// Map of the relayer => accumulated reward.
		pub fn relayer_reward<EncodeLikeAccountId, EncodeLikeReward>(
			key1: EncodeLikeAccountId,
			key2: EncodeLikeReward,
		) -> Option<<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Value>
		where
			EncodeLikeAccountId: codec::EncodeLike<
				<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key1,
			>,
			EncodeLikeReward: codec::EncodeLike<
				<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key2,
			>,
		{
			RelayerRewards::<T, I>::get(key1, key2)
		}

		fn do_claim_rewards(
			relayer: T::AccountId,
			reward_kind: T::Reward,
			beneficiary: BeneficiaryOf<T, I>,
		) -> DispatchResult {
			RelayerRewards::<T, I>::try_mutate_exists(
				&relayer,
				reward_kind,
				|maybe_reward| -> DispatchResult {
					let reward_balance =
						maybe_reward.take().ok_or(Error::<T, I>::NoRewardForRelayer)?;
					T::PaymentProcedure::pay_reward(
						&relayer,
						reward_kind,
						reward_balance,
						beneficiary.clone(),
					)
					.map_err(|e| {
						log::error!(
							target: LOG_TARGET,
							"Failed to pay ({:?} / {:?}) rewards to {:?}(beneficiary: {:?}), error: {:?}",
							reward_kind,
							reward_balance,
							relayer,
							beneficiary,
							e,
						);
						Error::<T, I>::FailedToPayReward
					})?;

					Self::deposit_event(Event::<T, I>::RewardPaid {
						relayer: relayer.clone(),
						reward_kind,
						reward_balance,
						beneficiary,
					});
					Ok(())
				},
			)
		}

		/// Returns true if given relayer registration is active at current block.
		///
		/// This call respects both `RequiredStake` and `RequiredRegistrationLease`, meaning that
		/// it'll return false if registered stake is lower than required or if remaining lease
		/// is less than `RequiredRegistrationLease`.
		pub fn is_registration_active(relayer: &T::AccountId) -> bool {
			let registration = match Self::registered_relayer(relayer) {
				Some(registration) => registration,
				None => return false,
			};

			// registration is inactive if relayer stake is less than required
			if registration.stake < Self::required_stake() {
				return false;
			}

			// registration is inactive if it ends soon
			let remaining_lease = registration
				.valid_till
				.saturating_sub(frame_system::Pallet::<T>::block_number());
			if remaining_lease <= Self::required_registration_lease() {
				return false;
			}

			true
		}

		/// Slash and `deregister` relayer. This function slashes all staked balance.
		///
		/// It may fail inside, but error is swallowed and we only log it.
		pub fn slash_and_deregister(
			relayer: &T::AccountId,
			slash_destination: impl IdentifyAccount<AccountId = T::AccountId>,
		) {
			let registration = match RegisteredRelayers::<T, I>::take(relayer) {
				Some(registration) => registration,
				None => {
					log::trace!(
						target: crate::LOG_TARGET,
						"Cannot slash unregistered relayer {:?}",
						relayer,
					);

					return;
				},
			};
			let slash_destination = slash_destination.into_account();

			match T::StakeAndSlash::repatriate_reserved(
				relayer,
				&slash_destination,
				registration.stake,
			) {
				Ok(failed_to_slash) if failed_to_slash.is_zero() => {
					log::trace!(
						target: crate::LOG_TARGET,
						"Relayer account {:?} has been slashed for {:?}. Funds were deposited to {:?}",
						relayer,
						registration.stake,
						slash_destination,
					);
				},
				Ok(failed_to_slash) => {
					log::trace!(
						target: crate::LOG_TARGET,
						"Relayer account {:?} has been partially slashed for {:?}. Funds were deposited to {:?}. \
						Failed to slash: {:?}",
						relayer,
						registration.stake,
						slash_destination,
						failed_to_slash,
					);
				},
				Err(e) => {
					// TODO: document this. Where?

					// it may fail if there's no beneficiary account. For us, it means that this
					// account must exist before we'll deploy the bridge
					log::debug!(
						target: crate::LOG_TARGET,
						"Failed to slash relayer account {:?}: {:?}. Maybe beneficiary account doesn't exist? \
						Beneficiary: {:?}, amount: {:?}, failed to slash: {:?}",
						relayer,
						e,
						slash_destination,
						registration.stake,
						registration.stake,
					);
				},
			}

			Self::deposit_event(Event::<T, I>::SlashedAndDeregistered {
				relayer: relayer.clone(),
				registration,
			});
		}

		/// Register reward for given relayer.
		pub(crate) fn register_relayer_reward(
			reward_kind: T::Reward,
			relayer: &T::AccountId,
			reward_balance: T::RewardBalance,
		) {
			if reward_balance.is_zero() {
				return;
			}

			RelayerRewards::<T, I>::mutate(
				relayer,
				reward_kind,
				|old_reward: &mut Option<T::RewardBalance>| {
					let new_reward =
						old_reward.unwrap_or_else(Zero::zero).saturating_add(reward_balance);
					*old_reward = Some(new_reward);

					log::trace!(
						target: crate::LOG_TARGET,
						"Relayer {:?} can now claim reward for serving payer {:?}: {:?}",
						relayer,
						reward_kind,
						new_reward,
					);

					Self::deposit_event(Event::<T, I>::RewardRegistered {
						relayer: relayer.clone(),
						reward_kind,
						reward_balance,
					});
				},
			);
		}

		/// Return required registration lease.
		pub(crate) fn required_registration_lease() -> BlockNumberFor<T> {
			<T::StakeAndSlash as StakeAndSlash<
				T::AccountId,
				BlockNumberFor<T>,
				T::Balance,
			>>::RequiredRegistrationLease::get()
		}

		/// Return required stake.
		pub(crate) fn required_stake() -> T::Balance {
			<T::StakeAndSlash as StakeAndSlash<
				T::AccountId,
				BlockNumberFor<T>,
				T::Balance,
			>>::RequiredStake::get()
		}

		/// `Unreserve` given amount on relayer account.
		fn do_unreserve(relayer: &T::AccountId, amount: T::Balance) -> DispatchResult {
			let failed_to_unreserve = T::StakeAndSlash::unreserve(relayer, amount);
			if !failed_to_unreserve.is_zero() {
				log::trace!(
					target: LOG_TARGET,
					"Failed to unreserve {:?}/{:?} on relayer {:?} account",
					failed_to_unreserve,
					amount,
					relayer,
				);

				fail!(Error::<T, I>::FailedToUnreserve)
			}

			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Relayer reward has been registered and may be claimed later.
		RewardRegistered {
			/// Relayer account that can claim reward.
			relayer: T::AccountId,
			/// Relayer can claim this kind of reward.
			reward_kind: T::Reward,
			/// Reward amount.
			reward_balance: T::RewardBalance,
		},
		/// Reward has been paid to the relayer.
		RewardPaid {
			/// Relayer account that has been rewarded.
			relayer: T::AccountId,
			/// Relayer has received reward of this kind.
			reward_kind: T::Reward,
			/// Reward amount.
			reward_balance: T::RewardBalance,
			/// Beneficiary.
			beneficiary: BeneficiaryOf<T, I>,
		},
		/// Relayer registration has been added or updated.
		RegistrationUpdated {
			/// Relayer account that has been registered.
			relayer: T::AccountId,
			/// Relayer registration.
			registration: Registration<BlockNumberFor<T>, T::Balance>,
		},
		/// Relayer has been `deregistered`.
		Deregistered {
			/// Relayer account that has been `deregistered`.
			relayer: T::AccountId,
		},
		/// Relayer has been slashed and `deregistered`.
		SlashedAndDeregistered {
			/// Relayer account that has been `deregistered`.
			relayer: T::AccountId,
			/// Registration that was removed.
			registration: Registration<BlockNumberFor<T>, T::Balance>,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// No reward can be claimed by given relayer.
		NoRewardForRelayer,
		/// Reward payment procedure has failed.
		FailedToPayReward,
		/// The relayer has tried to register for past block or registration lease
		/// is too short.
		InvalidRegistrationLease,
		/// New registration lease is less than the previous one.
		CannotReduceRegistrationLease,
		/// Failed to reserve enough funds on relayer account.
		FailedToReserve,
		/// Failed to `unreserve` enough funds on relayer account.
		FailedToUnreserve,
		/// Cannot `deregister` if not registered.
		NotRegistered,
		/// Failed to `deregister` relayer, because lease is still active.
		RegistrationIsStillActive,
	}

	/// Map of the relayer => accumulated reward.
	#[pallet::storage]
	pub type RelayerRewards<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Hasher1,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key1,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Hasher2,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key2,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Value,
		OptionQuery,
	>;

	/// Relayers that have reserved some of their balance to get free priority boost
	/// for their message delivery transactions.
	///
	/// Other relayers may submit transactions as well, but they will have default
	/// priority and will be rejected (without significant tip) in case if registered
	/// relayer is present.
	#[pallet::storage]
	pub type RegisteredRelayers<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Registration<BlockNumberFor<T>, T::Balance>,
		OptionQuery,
	>;
}

/// Implementation of `RewardLedger` for the pallet.
impl<T: Config<I>, I: 'static, Reward, RewardBalance>
	RewardLedger<T::AccountId, Reward, RewardBalance> for Pallet<T, I>
where
	Reward: Into<T::Reward>,
	RewardBalance: Into<T::RewardBalance>,
{
	fn register_reward(relayer: &T::AccountId, reward: Reward, reward_balance: RewardBalance) {
		Self::register_relayer_reward(reward.into(), relayer, reward_balance.into());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use mock::{RuntimeEvent as TestEvent, *};

	use bp_messages::{HashedLaneId, LaneIdType};
	use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
	use frame_support::{assert_noop, assert_ok, traits::fungible::Mutate};
	use frame_system::{EventRecord, Pallet as System, Phase};
	use sp_runtime::DispatchError;

	fn get_ready_for_events() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();
	}

	#[test]
	fn register_relayer_reward_emit_event() {
		run_test(|| {
			get_ready_for_events();

			Pallet::<TestRuntime>::register_relayer_reward(
				test_reward_account_param(),
				&REGULAR_RELAYER,
				100,
			);

			// Check if the `RewardRegistered` event was emitted.
			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RewardRegistered {
						relayer: REGULAR_RELAYER,
						reward_kind: test_reward_account_param(),
						reward_balance: 100
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn slash_and_deregister_works() {
		run_test(|| {
			get_ready_for_events();

			// register
			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150,
			));
			// check if registered
			let registration =
				Pallet::<TestRuntime>::registered_relayer(&REGISTER_RELAYER).unwrap();
			assert_eq!(registration, Registration { valid_till: 150, stake: Stake::get() });

			// slash and deregister
			let slash_destination = RewardsAccountParams::new(
				HashedLaneId::try_new(1, 2).unwrap(),
				*b"test",
				RewardsAccountOwner::ThisChain,
			);
			let slash_destination = bp_relayers::ExplicitOrAccountParams::Params(slash_destination);
			Pallet::<TestRuntime>::slash_and_deregister(&REGISTER_RELAYER, slash_destination);
			// check if event emitted
			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::SlashedAndDeregistered {
						relayer: REGISTER_RELAYER,
						registration,
					}),
					topics: vec![],
				})
			)
		});
	}

	#[test]
	fn root_cant_claim_anything() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::claim_rewards(
					RuntimeOrigin::root(),
					test_reward_account_param()
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
					test_reward_account_param()
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
				test_reward_account_param(),
				100,
			);
			assert_noop!(
				Pallet::<TestRuntime>::claim_rewards(
					RuntimeOrigin::signed(FAILING_RELAYER),
					test_reward_account_param()
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
				test_reward_account_param(),
				100,
			);
			assert_ok!(Pallet::<TestRuntime>::claim_rewards(
				RuntimeOrigin::signed(REGULAR_RELAYER),
				test_reward_account_param()
			));
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(REGULAR_RELAYER, test_reward_account_param()),
				None
			);

			// Check if the `RewardPaid` event was emitted.
			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RewardPaid {
						relayer: REGULAR_RELAYER,
						reward_kind: test_reward_account_param(),
						reward_balance: 100,
						beneficiary: REGULAR_RELAYER,
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn relayer_can_claim_reward_to() {
		run_test(|| {
			get_ready_for_events();

			RelayerRewards::<TestRuntime>::insert(
				REGULAR_RELAYER,
				test_reward_account_param(),
				100,
			);
			assert_ok!(Pallet::<TestRuntime>::claim_rewards_to(
				RuntimeOrigin::signed(REGULAR_RELAYER),
				test_reward_account_param(),
				REGULAR_RELAYER2,
			));
			assert_eq!(
				RelayerRewards::<TestRuntime>::get(REGULAR_RELAYER, test_reward_account_param()),
				None
			);

			// Check if the `RewardPaid` event was emitted.
			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RewardPaid {
						relayer: REGULAR_RELAYER,
						reward_kind: test_reward_account_param(),
						reward_balance: 100,
						beneficiary: REGULAR_RELAYER2,
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn register_fails_if_valid_till_is_a_past_block() {
		run_test(|| {
			System::<TestRuntime>::set_block_number(100);

			assert_noop!(
				Pallet::<TestRuntime>::register(RuntimeOrigin::signed(REGISTER_RELAYER), 50),
				Error::<TestRuntime>::InvalidRegistrationLease,
			);
		});
	}

	#[test]
	fn register_fails_if_valid_till_lease_is_less_than_required() {
		run_test(|| {
			System::<TestRuntime>::set_block_number(100);

			assert_noop!(
				Pallet::<TestRuntime>::register(
					RuntimeOrigin::signed(REGISTER_RELAYER),
					99 + Lease::get()
				),
				Error::<TestRuntime>::InvalidRegistrationLease,
			);
		});
	}

	#[test]
	fn register_works() {
		run_test(|| {
			get_ready_for_events();

			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));
			assert_eq!(Balances::reserved_balance(REGISTER_RELAYER), Stake::get());
			assert_eq!(
				Pallet::<TestRuntime>::registered_relayer(&REGISTER_RELAYER),
				Some(Registration { valid_till: 150, stake: Stake::get() }),
			);

			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RegistrationUpdated {
						relayer: REGISTER_RELAYER,
						registration: Registration { valid_till: 150, stake: Stake::get() },
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn register_fails_if_new_valid_till_is_lesser_than_previous() {
		run_test(|| {
			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));

			assert_noop!(
				Pallet::<TestRuntime>::register(RuntimeOrigin::signed(REGISTER_RELAYER), 125),
				Error::<TestRuntime>::CannotReduceRegistrationLease,
			);
		});
	}

	#[test]
	fn register_fails_if_it_cant_unreserve_some_balance_if_required_stake_decreases() {
		run_test(|| {
			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() + 1 },
			);

			assert_noop!(
				Pallet::<TestRuntime>::register(RuntimeOrigin::signed(REGISTER_RELAYER), 150),
				Error::<TestRuntime>::FailedToUnreserve,
			);
		});
	}

	#[test]
	fn register_unreserves_some_balance_if_required_stake_decreases() {
		run_test(|| {
			get_ready_for_events();

			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() + 1 },
			);
			TestStakeAndSlash::reserve(&REGISTER_RELAYER, Stake::get() + 1).unwrap();
			assert_eq!(Balances::reserved_balance(REGISTER_RELAYER), Stake::get() + 1);
			let free_balance = Balances::free_balance(REGISTER_RELAYER);

			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));
			assert_eq!(Balances::reserved_balance(REGISTER_RELAYER), Stake::get());
			assert_eq!(Balances::free_balance(REGISTER_RELAYER), free_balance + 1);
			assert_eq!(
				Pallet::<TestRuntime>::registered_relayer(&REGISTER_RELAYER),
				Some(Registration { valid_till: 150, stake: Stake::get() }),
			);

			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RegistrationUpdated {
						relayer: REGISTER_RELAYER,
						registration: Registration { valid_till: 150, stake: Stake::get() }
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn register_fails_if_it_cant_reserve_some_balance() {
		run_test(|| {
			Balances::set_balance(&REGISTER_RELAYER, 0);
			assert_noop!(
				Pallet::<TestRuntime>::register(RuntimeOrigin::signed(REGISTER_RELAYER), 150),
				Error::<TestRuntime>::FailedToReserve,
			);
		});
	}

	#[test]
	fn register_fails_if_it_cant_reserve_some_balance_if_required_stake_increases() {
		run_test(|| {
			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() - 1 },
			);
			Balances::set_balance(&REGISTER_RELAYER, 0);

			assert_noop!(
				Pallet::<TestRuntime>::register(RuntimeOrigin::signed(REGISTER_RELAYER), 150),
				Error::<TestRuntime>::FailedToReserve,
			);
		});
	}

	#[test]
	fn register_reserves_some_balance_if_required_stake_increases() {
		run_test(|| {
			get_ready_for_events();

			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() - 1 },
			);
			TestStakeAndSlash::reserve(&REGISTER_RELAYER, Stake::get() - 1).unwrap();

			let free_balance = Balances::free_balance(REGISTER_RELAYER);
			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));
			assert_eq!(Balances::reserved_balance(REGISTER_RELAYER), Stake::get());
			assert_eq!(Balances::free_balance(REGISTER_RELAYER), free_balance - 1);
			assert_eq!(
				Pallet::<TestRuntime>::registered_relayer(&REGISTER_RELAYER),
				Some(Registration { valid_till: 150, stake: Stake::get() }),
			);

			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::RegistrationUpdated {
						relayer: REGISTER_RELAYER,
						registration: Registration { valid_till: 150, stake: Stake::get() }
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn deregister_fails_if_not_registered() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::deregister(RuntimeOrigin::signed(REGISTER_RELAYER)),
				Error::<TestRuntime>::NotRegistered,
			);
		});
	}

	#[test]
	fn deregister_fails_if_registration_is_still_active() {
		run_test(|| {
			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));

			System::<TestRuntime>::set_block_number(100);

			assert_noop!(
				Pallet::<TestRuntime>::deregister(RuntimeOrigin::signed(REGISTER_RELAYER)),
				Error::<TestRuntime>::RegistrationIsStillActive,
			);
		});
	}

	#[test]
	fn deregister_works() {
		run_test(|| {
			get_ready_for_events();

			assert_ok!(Pallet::<TestRuntime>::register(
				RuntimeOrigin::signed(REGISTER_RELAYER),
				150
			));

			System::<TestRuntime>::set_block_number(151);

			let reserved_balance = Balances::reserved_balance(REGISTER_RELAYER);
			let free_balance = Balances::free_balance(REGISTER_RELAYER);
			assert_ok!(Pallet::<TestRuntime>::deregister(RuntimeOrigin::signed(REGISTER_RELAYER)));
			assert_eq!(
				Balances::reserved_balance(REGISTER_RELAYER),
				reserved_balance - Stake::get()
			);
			assert_eq!(Balances::free_balance(REGISTER_RELAYER), free_balance + Stake::get());

			assert_eq!(
				System::<TestRuntime>::events().last(),
				Some(&EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::BridgeRelayers(Event::Deregistered {
						relayer: REGISTER_RELAYER
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn is_registration_active_is_false_for_unregistered_relayer() {
		run_test(|| {
			assert!(!Pallet::<TestRuntime>::is_registration_active(&REGISTER_RELAYER));
		});
	}

	#[test]
	fn is_registration_active_is_false_when_stake_is_too_low() {
		run_test(|| {
			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() - 1 },
			);
			assert!(!Pallet::<TestRuntime>::is_registration_active(&REGISTER_RELAYER));
		});
	}

	#[test]
	fn is_registration_active_is_false_when_remaining_lease_is_too_low() {
		run_test(|| {
			System::<TestRuntime>::set_block_number(150 - Lease::get());

			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 150, stake: Stake::get() },
			);
			assert!(!Pallet::<TestRuntime>::is_registration_active(&REGISTER_RELAYER));
		});
	}

	#[test]
	fn is_registration_active_is_true_when_relayer_is_properly_registeered() {
		run_test(|| {
			System::<TestRuntime>::set_block_number(150 - Lease::get());

			RegisteredRelayers::<TestRuntime>::insert(
				REGISTER_RELAYER,
				Registration { valid_till: 151, stake: Stake::get() },
			);
			assert!(Pallet::<TestRuntime>::is_registration_active(&REGISTER_RELAYER));
		});
	}
}

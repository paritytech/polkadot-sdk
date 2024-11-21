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
#![warn(missing_docs)]

use bp_relayers::{
	ExplicitOrAccountParams, PaymentProcedure, Registration, RelayerRewardsKeyProvider,
	RewardsAccountParams, StakeAndSlash,
};
use bp_runtime::StorageDoubleMapKeyProvider;
use frame_support::fail;
use sp_arithmetic::traits::{AtLeast32BitUnsigned, Zero};
use sp_runtime::{traits::CheckedSub, Saturating};
use sp_std::marker::PhantomData;

use frame_support::{
	traits::{
		fungible::{Inspect, Mutate},
		tokens::Preservation,
	},
	PalletError,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
pub use payment_adapter::DeliveryConfirmationPaymentsAdapter;
use snowbridge_core::rewards::RewardLedger;
use sp_core::H160;
pub use stake_adapter::StakeAndSlashNamed;
pub use weights::WeightInfo;
pub use weights_ext::WeightInfoExt;
use xcm::prelude::{send_xcm, SendError as XcmpSendError, *};
use xcm_executor::traits::TransactAsset;

extern crate alloc;

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

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub type BalanceOf<T, I = ()> =
	<<T as Config<I>>::Token as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bp_messages::LaneIdType;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// `RelayerRewardsKeyProvider` for given configuration.
	type RelayerRewardsKeyProviderOf<T, I> = RelayerRewardsKeyProvider<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Reward,
		<T as Config<I>>::LaneId,
	>;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Type of relayer reward.
		type Reward: AtLeast32BitUnsigned + Copy + Member + Parameter + MaxEncodedLen;
		/// Pay rewards scheme.
		type PaymentProcedure: PaymentProcedure<
			Self::AccountId,
			Self::Reward,
			LaneId = Self::LaneId,
		>;
		/// Stake and slash scheme.
		type StakeAndSlash: StakeAndSlash<Self::AccountId, BlockNumberFor<Self>, Self::Reward>;
		/// Pallet call weights.
		type WeightInfo: WeightInfoExt;
		/// Lane identifier type.
		type LaneId: LaneIdType + Send + Sync;
		/// AssetHub parachain ID
		type AssetHubParaId: Get<u32>;
		type InboundQueuePalletInstance: Get<u8>;
		/// Ethereum network ID including the chain ID
		type EthereumNetwork: Get<NetworkId>;
		/// Message relayers are rewarded with this asset
		type WethAddress: Get<H160>;
		/// XCM message sender
		type XcmSender: SendXcm;
		/// To withdraw and deposit an asset.
		type AssetTransactor: TransactAsset;
		type AssetHubXCMFee: Get<u128>;
		type Token: Mutate<Self::AccountId> + Inspect<Self::AccountId>;
		/// TreasuryAccount to collect fees
		#[pallet::constant]
		type TreasuryAccount: Get<Self::AccountId>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(migration::STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Claim accumulated rewards.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_rewards())]
		pub fn claim_rewards(
			origin: OriginFor<T>,
			rewards_account_params: RewardsAccountParams<T::LaneId>,
		) -> DispatchResult {
			let relayer = ensure_signed(origin)?;

			RelayerRewards::<T, I>::try_mutate_exists(
				&relayer,
				rewards_account_params,
				|maybe_reward| -> DispatchResult {
					let reward = maybe_reward.take().ok_or(Error::<T, I>::NoRewardForRelayer)?;
					T::PaymentProcedure::pay_reward(&relayer, rewards_account_params, reward)
						.map_err(|e| {
							log::trace!(
								target: LOG_TARGET,
								"Failed to pay {:?} rewards to {:?}: {:?}",
								rewards_account_params,
								relayer,
								e,
							);
							Error::<T, I>::FailedToPayReward
						})?;

					Self::deposit_event(Event::<T, I>::RewardPaid {
						relayer: relayer.clone(),
						rewards_account_params,
						reward,
					});
					Ok(())
				},
			)
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

		/// Claim accumulated relayer rewards. Balance is minted on AH.
		/// Fees:
		/// BH execution fee - paid in DOT when executing the claim extrinsic
		/// XCM delivery fee to AH - paid in DOT to Treasury on BH
		/// AH execution fee - paid in Weth, deducated from relayer accumulated rewards
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::claim(), DispatchClass::Operational))]
		pub fn claim(origin: OriginFor<T>, deposit_location: Location) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			Self::process_claim(account_id, deposit_location)?;
			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
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
				return false
			}

			// registration is inactive if it ends soon
			let remaining_lease = registration
				.valid_till
				.saturating_sub(frame_system::Pallet::<T>::block_number());
			if remaining_lease <= Self::required_registration_lease() {
				return false
			}

			true
		}

		/// Slash and `deregister` relayer. This function slashes all staked balance.
		///
		/// It may fail inside, but error is swallowed and we only log it.
		pub fn slash_and_deregister(
			relayer: &T::AccountId,
			slash_destination: ExplicitOrAccountParams<T::AccountId, T::LaneId>,
		) {
			let registration = match RegisteredRelayers::<T, I>::take(relayer) {
				Some(registration) => registration,
				None => {
					log::trace!(
						target: crate::LOG_TARGET,
						"Cannot slash unregistered relayer {:?}",
						relayer,
					);

					return
				},
			};

			match T::StakeAndSlash::repatriate_reserved(
				relayer,
				slash_destination.clone(),
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

					// it may fail if there's no beneficiary account. For us it means that this
					// account must exists before we'll deploy the bridge
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
		}

		/// Register reward for given relayer.
		pub fn register_relayer_reward(
			rewards_account_params: RewardsAccountParams<T::LaneId>,
			relayer: &T::AccountId,
			reward: T::Reward,
		) {
			if reward.is_zero() {
				return
			}

			RelayerRewards::<T, I>::mutate(
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

					Self::deposit_event(Event::<T, I>::RewardRegistered {
						relayer: relayer.clone(),
						rewards_account_params,
						reward,
					});
				},
			);
		}

		/// Return required registration lease.
		pub(crate) fn required_registration_lease() -> BlockNumberFor<T> {
			<T::StakeAndSlash as StakeAndSlash<
				T::AccountId,
				BlockNumberFor<T>,
				T::Reward,
			>>::RequiredRegistrationLease::get()
		}

		/// Return required stake.
		pub(crate) fn required_stake() -> T::Reward {
			<T::StakeAndSlash as StakeAndSlash<
				T::AccountId,
				BlockNumberFor<T>,
				T::Reward,
			>>::RequiredStake::get()
		}

		/// `Unreserve` given amount on relayer account.
		fn do_unreserve(relayer: &T::AccountId, amount: T::Reward) -> DispatchResult {
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

		/// Claim rewards on AH, based on accumulated rewards balance in storage.
		fn process_claim(account_id: AccountIdOf<T>, deposit_location: Location) -> DispatchResult {
			let value = RewardsMapping::<T, I>::get(account_id.clone());
			if value.is_zero() {
				return Err(Error::<T, I>::InsufficientFunds.into());
			}
			let reward_balance: u128 =
				TryInto::<u128>::try_into(value).map_err(|_| Error::<T, I>::InvalidAmount)?;

			let reward_location = snowbridge_core::location::convert_token_address(
				T::EthereumNetwork::get(),
				T::WethAddress::get(),
			);
			let reward_asset: Asset = (reward_location.clone(), reward_balance).into();
			let fee_asset: Asset = (reward_location, T::AssetHubXCMFee::get()).into();

			let xcm: Xcm<()> = alloc::vec![
				DescendOrigin(PalletInstance(T::InboundQueuePalletInstance::get()).into()),
				UniversalOrigin(GlobalConsensus(T::EthereumNetwork::get())),
				ReserveAssetDeposited(reward_asset.clone().into()),
				BuyExecution { fees: fee_asset, weight_limit: Unlimited },
				DepositAsset {
					assets: AllCounted(1).into(),
					beneficiary: deposit_location.clone()
				},
				SetAppendix(Xcm(alloc::vec![
					RefundSurplus,
					DepositAsset {
						assets: AllCounted(1).into(),
						beneficiary: deposit_location.clone()
					},
				])),
			]
			.into();

			// Remove the reward since it has been claimed.
			RewardsMapping::<T, I>::remove(account_id.clone());

			let dest = Location::new(1, [Parachain(T::AssetHubParaId::get().into())]);
			let (_xcm_hash, xcm_delivery_fee) =
				send_xcm::<T::XcmSender>(dest, xcm).map_err(Error::<T, I>::from)?;

			match xcm_delivery_fee.get(0) {
				Some(fee_asset) =>
					if let Fungible(amount) = fee_asset.fun {
						let xcm_delivery_fee_balance: BalanceOf<T, I> =
							TryInto::<BalanceOf<T, I>>::try_into(amount)
								.map_err(|_| Error::<T, I>::InvalidAmount)?;
						let treasury_account = T::TreasuryAccount::get();
						T::Token::transfer(
							&account_id,
							&treasury_account,
							xcm_delivery_fee_balance,
							Preservation::Preserve,
						)?;
					},
				None => (),
			};

			Self::deposit_event(Event::RewardClaimed { account_id, deposit_location, value });
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
			/// Relayer can claim reward from this account.
			rewards_account_params: RewardsAccountParams<T::LaneId>,
			/// Reward amount.
			reward: T::Reward,
		},
		/// Reward has been paid to the relayer.
		RewardPaid {
			/// Relayer account that has been rewarded.
			relayer: T::AccountId,
			/// Relayer has received reward from this account.
			rewards_account_params: RewardsAccountParams<T::LaneId>,
			/// Reward amount.
			reward: T::Reward,
		},
		/// Relayer registration has been added or updated.
		RegistrationUpdated {
			/// Relayer account that has been registered.
			relayer: T::AccountId,
			/// Relayer registration.
			registration: Registration<BlockNumberFor<T>, T::Reward>,
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
			registration: Registration<BlockNumberFor<T>, T::Reward>,
		},
		/// A relayer reward was deposited
		RewardDeposited {
			/// The relayer account to which the reward was deposited.
			account_id: AccountIdOf<T>,
			/// The reward value.
			value: BalanceOf<T, I>,
		},
		RewardClaimed {
			/// The relayer account that claimed the reward.
			account_id: AccountIdOf<T>,
			/// The location that received the reward on AH.
			deposit_location: Location,
			/// The claimed reward value.
			value: BalanceOf<T, I>,
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
		/// XCMP send failure
		Send(SendError),
		/// The relayer rewards balance is lower than the claimed amount.
		InsufficientFunds,
		InvalidAmount,
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, PalletError)]
	pub enum SendError {
		NotApplicable,
		NotRoutable,
		Transport,
		DestinationUnsupported,
		ExceedsMaxMessageSize,
		MissingArgument,
		Fees,
	}

	impl<T: Config<I>, I: 'static> From<XcmpSendError> for Error<T, I> {
		fn from(e: XcmpSendError) -> Self {
			match e {
				XcmpSendError::NotApplicable => Error::<T, I>::Send(SendError::NotApplicable),
				XcmpSendError::Unroutable => Error::<T, I>::Send(SendError::NotRoutable),
				XcmpSendError::Transport(_) => Error::<T, I>::Send(SendError::Transport),
				XcmpSendError::DestinationUnsupported =>
					Error::<T, I>::Send(SendError::DestinationUnsupported),
				XcmpSendError::ExceedsMaxMessageSize =>
					Error::<T, I>::Send(SendError::ExceedsMaxMessageSize),
				XcmpSendError::MissingArgument => Error::<T, I>::Send(SendError::MissingArgument),
				XcmpSendError::Fees => Error::<T, I>::Send(SendError::Fees),
			}
		}
	}

	/// Map of the relayer => accumulated reward.
	#[pallet::storage]
	#[pallet::getter(fn relayer_reward)]
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
	#[pallet::getter(fn registered_relayer)]
	pub type RegisteredRelayers<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Registration<BlockNumberFor<T>, T::Reward>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type RewardsMapping<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, AccountIdOf<T>, BalanceOf<T, I>, ValueQuery>;

	impl<T: Config<I>, I: 'static> RewardLedger<AccountIdOf<T>, BalanceOf<T, I>> for Pallet<T, I> {
		fn deposit(account_id: AccountIdOf<T>, value: BalanceOf<T, I>) -> DispatchResult {
			RewardsMapping::<T, I>::mutate(account_id.clone(), |current_value| {
				*current_value = current_value.saturating_add(value);
			});
			Self::deposit_event(Event::RewardDeposited { account_id, value });

			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::LaneIdType;
	use mock::{RuntimeEvent as TestEvent, *};
	use sp_core::H256;

	use crate::Event::{RewardPaid, RewardRegistered};
	use bp_relayers::RewardsAccountOwner;
	use frame_support::{
		assert_err, assert_noop, assert_ok,
		traits::fungible::{Inspect, Mutate},
	};
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
					event: TestEvent::BridgeRelayers(RewardRegistered {
						relayer: REGULAR_RELAYER,
						rewards_account_params: test_reward_account_param(),
						reward: 100
					}),
					topics: vec![],
				}),
			);
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
					event: TestEvent::BridgeRelayers(RewardPaid {
						relayer: REGULAR_RELAYER,
						rewards_account_params: test_reward_account_param(),
						reward: 100
					}),
					topics: vec![],
				}),
			);
		});
	}

	#[test]
	fn pay_reward_from_account_actually_pays_reward() {
		type Balances = pallet_balances::Pallet<TestRuntime>;
		type PayLaneRewardFromAccount =
			bp_relayers::PayRewardFromAccount<Balances, ThisChainAccountId, TestLaneIdType>;

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
				Pallet::<TestRuntime>::registered_relayer(REGISTER_RELAYER),
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
				Pallet::<TestRuntime>::registered_relayer(REGISTER_RELAYER),
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
				Pallet::<TestRuntime>::registered_relayer(REGISTER_RELAYER),
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

	const WETH: u64 = 1_000_000_000_000_000_000;

	#[test]
	fn test_deposit() {
		run_test(|| {
			// Check a new deposit works
			let relayer: u64 = 1;
			let result = Pallet::<TestRuntime>::deposit(relayer, 2 * WETH);
			assert_ok!(result);
			assert_eq!(<RewardsMapping<TestRuntime>>::get(relayer), 2 * WETH);

			// Check accumulation works
			let result2 = Pallet::<TestRuntime>::deposit(relayer, 3 * WETH);
			assert_ok!(result2);
			assert_eq!(<RewardsMapping<TestRuntime>>::get(relayer), 5 * WETH);

			// Check another relayer deposit works.
			let another_relayer: u64 = 2;
			let result3 = Pallet::<TestRuntime>::deposit(another_relayer, 1 * WETH);
			assert_ok!(result3);
			assert_eq!(<RewardsMapping<TestRuntime>>::get(another_relayer), 1 * WETH);
		});
	}

	#[test]
	fn test_claim() {
		run_test(|| {
			let relayer: u64 = 1;
			let interior: InteriorLocation = [
				Parachain(1000),
				Junction::AccountId32 { network: None, id: H256::random().into() },
			]
			.into();
			let claim_location = Location::new(1, interior);

			let result = Pallet::<TestRuntime>::claim(
				RuntimeOrigin::signed(relayer),
				claim_location.clone(),
			);
			// No rewards yet
			assert_err!(result, Error::<TestRuntime>::InsufficientFunds);

			// Deposit rewards
			let result2 = Pallet::<TestRuntime>::deposit(relayer, 3 * WETH);
			assert_ok!(result2);

			// Claim rewards
			let result3 =
				Pallet::<TestRuntime>::claim(RuntimeOrigin::signed(relayer), claim_location);
			assert_ok!(result3);
			assert_eq!(<RewardsMapping<TestRuntime>>::get(relayer), 0);
		});
	}
}

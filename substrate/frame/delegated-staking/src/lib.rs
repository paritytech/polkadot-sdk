// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(rustdoc::broken_intra_doc_links)]

use frame_support::{
	pallet_prelude::*,
	traits::{fungible::{hold::Mutate as FunHoldMutate, Inspect as FunInspect}, tokens::Precision},
};
use frame_support::traits::tokens::{Fortitude, Preservation};
use frame_system::pallet_prelude::*;
use sp_std::{convert::TryInto, prelude::*};
use pallet::*;
use sp_runtime::{traits::Zero, DispatchError, RuntimeDebug, Saturating};
use sp_staking::delegation::{StakeDelegatee, StakeDelegator};
use sp_staking::{EraIndex, Stake, StakerStatus, StakingInterface};

pub type BalanceOf<T> = <<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: FunHoldMutate<
			Self::AccountId,
			Reason = Self::RuntimeHoldReason
		>;
		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// Core staking implementation.
		type Staking: StakingInterface<Balance = BalanceOf<Self>, AccountId = Self::AccountId>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account cannot perform this operation.
		NotAllowed,
		/// Delegation conditions are not met.
		///
		/// Possible issues are
		/// 1) Account does not accept or has blocked delegation.
		/// 2) Cannot delegate to self,
		/// 3) Cannot delegate to multiple Delegatees,
		InvalidDelegation,
		/// The account does not have enough funds to perform the operation.
		NotEnoughFunds,
		/// Not an existing delegatee account.
		NotDelegatee,
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for stake delegation to another account.
		#[codec(index = 0)]
		Delegating,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Delegated { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		Withdrawn { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their delegation.
	///
	/// Note: We are not using a double map with delegator and delegatee account as keys since we
	/// want to restrict delegators to delegate only to one account.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
	CountedStorageMap<_, Twox64Concat, T::AccountId, (T::AccountId, BalanceOf<T>), OptionQuery>;

	/// Map of Delegatee to their Ledger.
	#[pallet::storage]
	pub(crate) type Delegatees<T: Config> = CountedStorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		DelegationRegister<T>,
		OptionQuery,
	>;
}

/// Register of all delegations to a `Delegatee`.
///
/// This keeps track of the active balance of the delegatee that is made up from the funds that are
/// currently delegated to this delegatee. It also tracks the pending slashes yet to be applied
/// among other things.
#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegationRegister<T: Config> {
	/// Where the reward should be paid out.
	pub payee: T::AccountId,
	/// Sum of all delegated funds to this delegatee.
	#[codec(compact)]
	pub balance: BalanceOf<T>,
	/// Slashes that are not yet applied.
	#[codec(compact)]
	pub pending_slash: BalanceOf<T>,
	/// Whether this delegatee is blocked from receiving new delegations.
	pub blocked: bool,
}

impl<T: Config> DelegationRegister<T> {
	pub fn effective_balance(&self) -> BalanceOf<T> {
		self.balance.saturating_sub(self.pending_slash)
	}
}

impl<T: Config> StakeDelegatee for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn delegate_balance(who: Self::AccountId) -> Self::Balance {
		<Delegatees<T>>::get(who)
			.map_or_else(|| 0u32.into(), |register| register.effective_balance())
	}

	fn accept_delegations(delegatee: &Self::AccountId, payee: &Self::AccountId) -> sp_runtime::DispatchResult {
		// fail if already delegatee
		ensure!(!<Delegatees<T>>::contains_key(delegatee), Error::<T>::NotAllowed);
		// a delegator cannot be delegatee
		ensure!(!<Delegators<T>>::contains_key(delegatee), Error::<T>::NotAllowed);
		// payee account cannot be same as delegatee
		ensure!(payee != delegatee, Error::<T>::InvalidDelegation);

		<Delegatees<T>>::insert(
			delegatee,
			DelegationRegister {
				payee: payee.clone(),
				balance: Zero::zero(),
				pending_slash: Zero::zero(),
				blocked: false,
			},
		);

		Ok(())
	}

	fn block_delegations(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		<Delegatees<T>>::mutate(delegatee, |maybe_register| {
			if let Some(register) = maybe_register {
				register.blocked = true;
				Ok(())
			} else {
				Err(Error::<T>::NotDelegatee.into())
			}
		})
	}

	fn kill_delegatee(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn update_bond(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn apply_slash(delegatee: &Self::AccountId, delegator: &Self::AccountId, value: Self::Balance, reporter: Option<Self::AccountId>) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn delegatee_migrate(new_delegatee: &Self::AccountId, proxy_delegator: &Self::AccountId, payee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn delegator_migrate(delegator_from: &Self::AccountId, delegator_to: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}
}

impl<T: Config> StakeDelegator for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;

	fn delegate(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		let delegator_balance = T::Currency::reducible_balance(&delegator, Preservation::Expendable, Fortitude::Polite);
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		ensure!(delegator_balance >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegatee != delegator, Error::<T>::InvalidDelegation);
		ensure!(<Delegatees<T>>::contains_key(delegatee), Error::<T>::NotDelegatee);

		// cannot delegate to another delegatee.
		if <Delegatees<T>>::contains_key(delegator) {
			return Err(Error::<T>::InvalidDelegation.into())
		}

		let new_delegation_amount =
			if let Some((current_delegatee, current_delegation)) = <Delegators<T>>::get(delegator) {
				ensure!(&current_delegatee == delegatee, Error::<T>::InvalidDelegation);
				value.saturating_add(current_delegation)
			} else {
				value
			};

		<Delegators<T>>::insert(delegator, (delegatee, new_delegation_amount));
		<Delegatees<T>>::mutate(delegatee, |maybe_register| {
			if let Some(register) = maybe_register {
				register.balance.saturating_accrue(value);
			}
		});

		T::Currency::hold(&HoldReason::Delegating.into(), &delegator, value)?;

		Ok(())
	}

	fn request_undelegate(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn withdraw(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		<Delegators<T>>::mutate_exists(delegator, |maybe_delegate| match maybe_delegate {
			Some((current_delegatee, delegate_balance)) => {
				ensure!(&current_delegatee.clone() == delegatee, Error::<T>::InvalidDelegation);
				ensure!(*delegate_balance >= value, Error::<T>::InvalidDelegation);

				delegate_balance.saturating_reduce(value);

				if *delegate_balance == BalanceOf::<T>::zero() {
					*maybe_delegate = None;
				}
				Ok(())
			},
			None => {
				// this should never happen
				return Err(Error::<T>::InvalidDelegation)
			},
		})?;

		<Delegatees<T>>::mutate(delegatee, |maybe_register| match maybe_register {
			Some(ledger) => {
				ledger.balance.saturating_reduce(value);
				Ok(())
			},
			None => {
				// this should never happen
				return Err(Error::<T>::InvalidDelegation)
			},
		})?;

		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&delegator,
			value,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == value, "hold should have been released fully");

		Ok(())
	}
}

impl<T: Config> StakingInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;
	type CurrencyToVote = <T::Staking as StakingInterface>::CurrencyToVote;

	fn minimum_nominator_bond() -> Self::Balance {
		T::Staking::minimum_nominator_bond()
	}

	fn minimum_validator_bond() -> Self::Balance {
		T::Staking::minimum_validator_bond()
	}

	fn stash_by_ctrl(controller: &Self::AccountId) -> Result<Self::AccountId, DispatchError> {
		T::Staking::stash_by_ctrl(controller)
	}

	fn bonding_duration() -> EraIndex {
		T::Staking::bonding_duration()
	}

	fn current_era() -> EraIndex {
		T::Staking::current_era()
	}

	fn stake(who: &Self::AccountId) -> Result<Stake<Self::Balance>, DispatchError> {
		T::Staking::stake(who)
	}

	fn total_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		T::Staking::total_stake(who)
	}

	fn active_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		T::Staking::active_stake(who)
	}

	fn is_unbonding(who: &Self::AccountId) -> Result<bool, DispatchError> {
		T::Staking::is_unbonding(who)
	}

	fn fully_unbond(who: &Self::AccountId) -> sp_runtime::DispatchResult {
		T::Staking::fully_unbond(who)
	}

	fn bond(who: &Self::AccountId, value: Self::Balance, payee: &Self::AccountId) -> sp_runtime::DispatchResult {
		T::Staking::bond(who, value, payee)
	}

	fn nominate(who: &Self::AccountId, validators: Vec<Self::AccountId>) -> sp_runtime::DispatchResult {
		T::Staking::nominate(who, validators)
	}

	fn chill(who: &Self::AccountId) -> sp_runtime::DispatchResult {
		T::Staking::chill(who)
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> sp_runtime::DispatchResult {
		T::Staking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		T::Staking::unbond(stash, value)
	}

	fn withdraw_unbonded(stash: Self::AccountId, num_slashing_spans: u32) -> Result<bool, DispatchError> {
		T::Staking::withdraw_unbonded(stash, num_slashing_spans)
	}

	fn desired_validator_count() -> u32 {
		T::Staking::desired_validator_count()
	}

	fn election_ongoing() -> bool {
		T::Staking::election_ongoing()
	}

	fn force_unstake(who: Self::AccountId) -> sp_runtime::DispatchResult {
		T::Staking::force_unstake(who)
	}

	fn is_exposed_in_era(who: &Self::AccountId, era: &EraIndex) -> bool {
		T::Staking::is_exposed_in_era(who, era)
	}

	fn status(who: &Self::AccountId) -> Result<StakerStatus<Self::AccountId>, DispatchError> {
		T::Staking::status(who)
	}

	fn is_validator(who: &Self::AccountId) -> bool {
		T::Staking::is_validator(who)
	}

	fn nominations(who: &Self::AccountId) -> Option<Vec<Self::AccountId>> {
		T::Staking::nominations(who)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn max_exposure_page_size() -> sp_staking::Page {
		T::Staking::max_exposure_page_size()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_era_stakers(current_era: &EraIndex, stash: &Self::AccountId, exposures: Vec<(Self::AccountId, Self::Balance)>) {
		T::Staking::add_era_stakers(current_era, stash, exposures)
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_era(era: EraIndex) {
		T::Staking::set_current_era(era)
	}
}
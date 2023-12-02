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


#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{hold::Mutate as FunHoldMutate, Inspect as FunInspect, Mutate as FunMutate},
		tokens::{Fortitude, Precision, Preservation},
	},
};
use pallet::*;
use sp_runtime::{traits::Zero, RuntimeDebug, Saturating};
use sp_staking::{
	delegation::{Delegatee, Delegator},
	StakerStatus, StakingInterface,
};
use sp_std::{convert::TryInto, prelude::*};

pub type BalanceOf<T> =
	<<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: FunHoldMutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ FunMutate<Self::AccountId>;
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
		/// Some corruption in internal state.
		BadState,
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for stake delegation to another account.
		#[codec(index = 0)]
		Delegating,
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
		}
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
	pub(crate) type Delegatees<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, DelegationRegister<T>, OptionQuery>;
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

impl<T: Config> Delegatee for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn delegate_balance(who: Self::AccountId) -> Self::Balance {
		<Delegatees<T>>::get(who)
			.map_or_else(|| 0u32.into(), |register| register.effective_balance())
	}

	fn accept_delegations(
		who: &Self::AccountId,
		payee: &Self::AccountId,
	) -> sp_runtime::DispatchResult {
		// Existing delegatee cannot accept delegation
		ensure!(!<Delegatees<T>>::contains_key(who), Error::<T>::NotAllowed);

		// payee account cannot be same as delegatee
		ensure!(payee != who, Error::<T>::InvalidDelegation);

		// if already a delegator, unblock and return success
		<Delegatees<T>>::mutate(who, |maybe_register| {
			if let Some(register) = maybe_register {
				register.blocked = false;
				register.payee = payee.clone();
			} else {
				*maybe_register = Some(DelegationRegister {
					payee: payee.clone(),
					balance: Zero::zero(),
					pending_slash: Zero::zero(),
					blocked: false,
				});
			}
		});

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

	fn update_bond(who: &Self::AccountId) -> sp_runtime::DispatchResult {
		let delegatee = <Delegatees<T>>::get(who).ok_or(Error::<T>::NotDelegatee)?;
		let delegated_balance = delegatee.effective_balance();

		match T::Staking::stake(who) {
			Ok(stake) => {
				let unstaked_delegated_balance = delegated_balance.saturating_sub(stake.total);
				T::Staking::bond_extra(who, unstaked_delegated_balance)
			},
			Err(_) => {
				// If stake not found, it means this is the first bond
				T::Staking::bond(who, delegated_balance, &delegatee.payee)
			},
		}
	}

	fn withdraw(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> sp_runtime::DispatchResult {
		<Delegators<T>>::mutate_exists(delegator, |maybe_delegate| match maybe_delegate {
			Some((current_delegatee, delegate_balance)) => {
				ensure!(&current_delegatee.clone() == delegatee, Error::<T>::NotDelegatee);
				ensure!(*delegate_balance >= value, Error::<T>::NotAllowed);

				delegate_balance.saturating_reduce(value);

				if *delegate_balance == BalanceOf::<T>::zero() {
					*maybe_delegate = None;
				}
				Ok(())
			},
			None => {
				// delegator does not exist
				return Err(Error::<T>::NotAllowed)
			},
		})?;

		<Delegatees<T>>::mutate(delegatee, |maybe_register| match maybe_register {
			Some(ledger) => {
				ledger.balance.saturating_reduce(value);
				Ok(())
			},
			None => {
				// Delegatee not found
				return Err(Error::<T>::NotDelegatee)
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

	fn apply_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		todo!()
	}

	/// Transfers funds from current staked account to `proxy_delegator`. Current staked account
	/// becomes a delegatee with `proxy_delegator` delegating stakes to it.
	fn delegatee_migrate(
		new_delegatee: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
	) -> sp_runtime::DispatchResult {
		ensure!(new_delegatee != proxy_delegator, Error::<T>::InvalidDelegation);

		// ensure proxy delegator has at least minimum balance to keep the account alive.
		ensure!(
			T::Currency::reducible_balance(
				proxy_delegator,
				Preservation::Expendable,
				Fortitude::Polite
			) > Zero::zero(),
			Error::<T>::NotEnoughFunds
		);

		// ensure staker is a nominator
		let status = T::Staking::status(new_delegatee)?;
		match status {
			StakerStatus::Nominator(_) => (),
			_ => return Err(Error::<T>::InvalidDelegation.into()),
		}

		let stake = T::Staking::stake(new_delegatee)?;

		// unlock funds from staker
		T::Staking::force_unlock(new_delegatee)?;

		// try transferring the staked amount. This should never fail but if it does, it indicates
		// bad state and we abort.
		T::Currency::transfer(
			new_delegatee,
			proxy_delegator,
			stake.total,
			Preservation::Expendable,
		)
		.map_err(|_| Error::<T>::BadState)?;

		// delegate from new delegator to staker.
		Self::accept_delegations(new_delegatee, payee)?;
		Self::delegate(proxy_delegator, new_delegatee, stake.total)
	}
}

impl<T: Config> Delegator for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> sp_runtime::DispatchResult {
		let delegator_balance =
			T::Currency::reducible_balance(&delegator, Preservation::Expendable, Fortitude::Polite);
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		ensure!(delegator_balance >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegatee != delegator, Error::<T>::InvalidDelegation);
		ensure!(<Delegatees<T>>::contains_key(delegatee), Error::<T>::NotDelegatee);

		// cannot delegate to another delegatee.
		if <Delegatees<T>>::contains_key(delegator) {
			return Err(Error::<T>::InvalidDelegation.into())
		}

		let new_delegation_amount = if let Some((current_delegatee, current_delegation)) =
			<Delegators<T>>::get(delegator)
		{
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

	fn request_undelegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> sp_runtime::DispatchResult {
		todo!()
	}

	/// Move funds from proxy delegator to actual delegator.
	// TODO: Keep track of proxy delegator and only allow movement from proxy -> new delegator
	fn delegator_migrate(
		existing_delegator: &Self::AccountId,
		new_delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> sp_runtime::DispatchResult {
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);

		// ensure delegatee exists.
		ensure!(!<Delegatees<T>>::contains_key(delegatee), Error::<T>::NotDelegatee);

		// remove delegation of `value` from `existing_delegator`.
		Self::withdraw(existing_delegator, delegatee, value)?;

		// transfer the withdrawn value to `new_delegator`.
		T::Currency::transfer(existing_delegator, new_delegator, value, Preservation::Expendable)
			.map_err(|_| Error::<T>::BadState)?;

		// add the above removed delegation to `new_delegator`.
		Self::delegate(new_delegator, delegatee, value)
	}
}

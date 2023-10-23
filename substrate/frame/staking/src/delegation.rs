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

//! An implementation of a delegation system that can be utilised by an off-chain entity, a smart
//! contract or a runtime module.
//!
//! Delegatee: Someone who accepts delegations. An account can set their intention to accept
//! delegations by calling `accept_delegations`. This account cannot have another role in the
//! staking system and once set as delegatee, can only stake with their delegated balance, i.e.
//! cannot use their own free balance to stake. They can also block new delegations or stop being a
//! delegatee once all delegations to it are removed.
//!
//! Delegator: Someone who delegates their funds to a delegatee. A delegator can delegate their
//! funds to one and only one delegatee. They also can not be a nominator or validator.
//!
//! Reward payouts are always made to another account set by delegatee. This account is a separate
//! account from delegatee and rewards cannot be restaked automatically. The reward payouts can then
//! be distributed to delegators by the delegatee via custom strategies.
//!
//! Any slashes to a delegatee (which is equivalent to nominator as long as StakingLedger is
//! concerned) are recorded in `DelegationRegister` of the Delegatee as a pending slash. It is
//! delegatee's responsibility to apply slash for each delegator at a time. Staking pallet ensures
//! the pending slash never exceeds staked amount and would freeze further withdraws until pending
//! slashes are applied.

use crate::{BalanceOf, Config, Delegatees, Delegators, Error, HoldReason};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	defensive, defensive_assert,
	dispatch::DispatchResult,
	ensure,
	traits::{fungible::MutateHold, tokens::Precision, Currency},
};
use scale_info::TypeInfo;
use sp_runtime::{traits::Zero, DispatchError, RuntimeDebug, Saturating};

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

/// Total balance that is delegated to this account but not yet staked.
pub(crate) fn delegated_balance<T: Config>(delegatee: &T::AccountId) -> BalanceOf<T> {
	<Delegatees<T>>::get(delegatee).map_or_else(|| 0u32.into(), |register| register.balance)
}

pub(crate) fn is_delegatee<T: Config>(delegatee: &T::AccountId) -> bool {
	<Delegatees<T>>::contains_key(delegatee)
}

pub(crate) fn get_payee<T: Config>(
	delegatee: &T::AccountId,
) -> Result<T::AccountId, DispatchError> {
	<Delegatees<T>>::get(delegatee)
		.map(|register| register.payee)
		.ok_or(Error::<T>::NotDelegatee.into())
}

pub(crate) fn accept_delegations<T: Config>(
	delegatee: &T::AccountId,
	payee: &T::AccountId,
) -> DispatchResult {
	// fail if already delegatee
	ensure!(!<Delegatees<T>>::contains_key(delegatee), Error::<T>::AlreadyDelegatee);
	// a delegator cannot be delegatee
	ensure!(!<Delegators<T>>::contains_key(delegatee), Error::<T>::AlreadyDelegator);
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

pub(crate) fn block_delegations<T: Config>(delegatee: &T::AccountId) -> DispatchResult {
	<Delegatees<T>>::mutate(delegatee, |maybe_register| {
		if let Some(register) = maybe_register {
			register.blocked = true;
			Ok(())
		} else {
			Err(Error::<T>::NotDelegatee.into())
		}
	})
}

/// Delegate some amount from delegator to delegatee.
pub(crate) fn delegate<T: Config>(
	delegator: &T::AccountId,
	delegatee: &T::AccountId,
	value: BalanceOf<T>,
) -> DispatchResult {
	let delegator_balance = T::Currency::free_balance(&delegator);
	ensure!(value >= T::Currency::minimum_balance(), Error::<T>::InsufficientBond);
	ensure!(delegator_balance >= value, Error::<T>::InsufficientBond);
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

pub(crate) fn withdraw<T: Config>(
	delegatee: &T::AccountId,
	delegator: &T::AccountId,
	value: BalanceOf<T>,
) -> DispatchResult {
	// fixme(ank4n): Needs refactor..

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

pub(crate) fn report_slash<T: Config>(delegatee: &T::AccountId, slash: BalanceOf<T>) {
	<Delegatees<T>>::mutate(&delegatee, |maybe_register| match maybe_register {
		Some(aggregate) => aggregate.pending_slash.saturating_accrue(slash),
		None => {
			defensive!("should not be called on non-delegatee");
		},
	});
}

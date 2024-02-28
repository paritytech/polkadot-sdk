// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Basic types used in delegated staking.

use super::*;
use frame_support::traits::DefensiveSaturating;

/// The type of pot account being created.
#[derive(Encode, Decode)]
pub(crate) enum AccountType {
	/// A proxy delegator account created for a nominator who migrated to a `delegatee` account.
	///
	/// Funds for unmigrated `delegator` accounts of the `delegatee` are kept here.
	ProxyDelegator,
}

/// Information about delegation of a `delegator`.
#[derive(Default, Encode, Clone, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Delegation<T: Config> {
	/// The target of delegation.
	pub delegatee: T::AccountId,
	/// The amount delegated.
	pub amount: BalanceOf<T>,
}

impl<T: Config> Delegation<T> {
	pub(crate) fn get(delegator: &T::AccountId) -> Option<Self> {
		<Delegators<T>>::get(delegator)
	}

	pub(crate) fn from(delegatee: &T::AccountId, amount: BalanceOf<T>) -> Self {
		Delegation { delegatee: delegatee.clone(), amount }
	}

	pub(crate) fn can_delegate(delegator: &T::AccountId, delegatee: &T::AccountId) -> bool {
		Delegation::<T>::get(delegator)
			.map(|delegation| delegation.delegatee == delegatee.clone())
			.unwrap_or(
				// all good if its a new delegator except it should not be an existing delegatee.
				!<Delegatees<T>>::contains_key(delegator),
			)
	}

	/// Checked decrease of delegation amount. Consumes self and returns a new copy.
	pub(crate) fn decrease_delegation(self, amount: BalanceOf<T>) -> Option<Self> {
		let updated_delegation = self.amount.checked_sub(&amount)?;
		Some(Delegation::from(&self.delegatee, updated_delegation))
	}

	/// Checked increase of delegation amount. Consumes self and returns a new copy.
	#[allow(unused)]
	pub(crate) fn increase_delegation(self, amount: BalanceOf<T>) -> Option<Self> {
		let updated_delegation = self.amount.checked_add(&amount)?;
		Some(Delegation::from(&self.delegatee, updated_delegation))
	}

	pub(crate) fn save_or_kill(self, key: &T::AccountId) {
		// Clean up if no delegation left.
		if self.amount == Zero::zero() {
			<Delegators<T>>::remove(key);
			return
		}

		<Delegators<T>>::insert(key, self)
	}
}

/// Ledger of all delegations to a `Delegatee`.
///
/// This keeps track of the active balance of the `delegatee` that is made up from the funds that
/// are currently delegated to this `delegatee`. It also tracks the pending slashes yet to be
/// applied among other things.
// FIXME(ank4n): Break up into two storage items - bookkeeping stuff and settings stuff.
#[derive(Default, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegateeLedger<T: Config> {
	/// Where the reward should be paid out.
	pub payee: T::AccountId,
	/// Sum of all delegated funds to this `delegatee`.
	#[codec(compact)]
	pub total_delegated: BalanceOf<T>,
	/// Funds that are withdrawn from core staking but not released to delegator/s. It is a subset
	/// of `total_delegated` and can never be greater than it.
	// FIXME(ank4n): Check/test about rebond: where delegator rebond what is unlocking.
	#[codec(compact)]
	pub unclaimed_withdrawals: BalanceOf<T>,
	/// Slashes that are not yet applied.
	#[codec(compact)]
	pub pending_slash: BalanceOf<T>,
	/// Whether this `delegatee` is blocked from receiving new delegations.
	pub blocked: bool,
}

impl<T: Config> DelegateeLedger<T> {
	pub(crate) fn new(reward_destination: &T::AccountId) -> Self {
		DelegateeLedger {
			payee: reward_destination.clone(),
			total_delegated: Zero::zero(),
			unclaimed_withdrawals: Zero::zero(),
			pending_slash: Zero::zero(),
			blocked: false,
		}
	}

	pub(crate) fn get(key: &T::AccountId) -> Option<Self> {
		<Delegatees<T>>::get(key)
	}

	pub(crate) fn can_accept_delegation(delegatee: &T::AccountId) -> bool {
		DelegateeLedger::<T>::get(delegatee)
			.map(|ledger| !ledger.blocked)
			.unwrap_or(false)
	}

	pub(crate) fn save(self, key: &T::AccountId) {
		<Delegatees<T>>::insert(key, self)
	}

	/// Effective total balance of the `delegatee`.
	pub(crate) fn effective_balance(&self) -> BalanceOf<T> {
		defensive_assert!(
			self.total_delegated >= self.pending_slash,
			"slash cannot be higher than actual balance of delegator"
		);

		// pending slash needs to be burned and cannot be used for stake.
		self.total_delegated.saturating_sub(self.pending_slash)
	}

	/// Balance that can be bonded in [`T::CoreStaking`].
	pub(crate) fn stakeable_balance(&self) -> BalanceOf<T> {
		self.effective_balance().saturating_sub(self.unclaimed_withdrawals)
	}
}

/// Wrapper around `DelegateeLedger` to provide additional functionality.
#[derive(Clone)]
pub struct Delegatee<T: Config> {
	pub key: T::AccountId,
	pub ledger: DelegateeLedger<T>,
}

impl<T: Config> Delegatee<T> {
	pub(crate) fn from(delegatee: &T::AccountId) -> Result<Delegatee<T>, DispatchError> {
		let ledger = DelegateeLedger::<T>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		Ok(Delegatee { key: delegatee.clone(), ledger })
	}

	/// Remove funds that are withdrawn from [Config::CoreStaking] but not claimed by a delegator.
	///
	/// Checked decrease of delegation amount from `total_delegated` and `unclaimed_withdrawals`
	/// registers. Consumes self and returns a new instance of self if success.
	pub(crate) fn remove_unclaimed_withdraw(
		self,
		amount: BalanceOf<T>,
	) -> Result<Self, DispatchError> {
		let new_total_delegated = self
			.ledger
			.total_delegated
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;
		let new_unclaimed_withdrawals = self
			.ledger
			.unclaimed_withdrawals
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;

		Ok(Delegatee {
			ledger: DelegateeLedger {
				total_delegated: new_total_delegated,
				unclaimed_withdrawals: new_unclaimed_withdrawals,
				..self.ledger
			},
			..self
		})
	}

	/// Add funds that are withdrawn from [Config::CoreStaking] to be claimed by delegators later.
	pub(crate) fn add_unclaimed_withdraw(
		self,
		amount: BalanceOf<T>,
	) -> Result<Self, DispatchError> {
		let new_unclaimed_withdrawals = self
			.ledger
			.unclaimed_withdrawals
			.checked_add(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;

		Ok(Delegatee {
			ledger: DelegateeLedger {
				unclaimed_withdrawals: new_unclaimed_withdrawals,
				..self.ledger
			},
			..self
		})
	}

	/// Reloads self from storage.
	#[allow(unused)]
	pub(crate) fn refresh(&self) -> Result<Delegatee<T>, DispatchError> {
		Self::from(&self.key)
	}

	/// Amount that is delegated but not bonded yet.
	///
	/// This importantly does not include `unclaimed_withdrawals` as those should not be bonded
	/// again unless explicitly requested.
	pub(crate) fn available_to_bond(&self) -> BalanceOf<T> {
		let bonded_stake = self.bonded_stake();
		let stakeable = self.ledger.stakeable_balance();

		defensive_assert!(
			stakeable >= bonded_stake,
			"cannot be bonded with more than delegatee balance"
		);

		stakeable.saturating_sub(bonded_stake)
	}

	/// Balance of `Delegatee` that is not bonded.
	///
	/// This is similar to [Self::available_to_bond] except it also includes `unclaimed_withdrawals`
	/// of `Delegatee`.
	#[cfg(test)]
	pub(crate) fn total_unbonded(&self) -> BalanceOf<T> {
		let bonded_stake = self.bonded_stake();

		let net_balance = self.ledger.effective_balance();

		defensive_assert!(
			net_balance >= bonded_stake,
			"cannot be bonded with more than the delegatee balance"
		);

		net_balance.saturating_sub(bonded_stake)
	}

	/// Remove slashes that are applied.
	pub(crate) fn remove_slash(self, amount: BalanceOf<T>) -> Self {
		let pending_slash = self.ledger.pending_slash.defensive_saturating_sub(amount);
		let total_delegated = self.ledger.total_delegated.defensive_saturating_sub(amount);

		Delegatee {
			ledger: DelegateeLedger { pending_slash, total_delegated, ..self.ledger },
			..self
		}
	}

	pub(crate) fn bonded_stake(&self) -> BalanceOf<T> {
		T::CoreStaking::total_stake(&self.key).unwrap_or(Zero::zero())
	}

	pub(crate) fn is_bonded(&self) -> bool {
		T::CoreStaking::stake(&self.key).is_ok()
	}

	pub(crate) fn reward_account(&self) -> &T::AccountId {
		&self.ledger.payee
	}

	pub(crate) fn update_status(self, block: bool) -> Self {
		Delegatee { ledger: DelegateeLedger { blocked: block, ..self.ledger }, ..self }
	}

	pub(crate) fn save(self) {
		let key = self.key;
		self.ledger.save(&key)
	}

	/// Save self and remove if no delegation left.
	///
	/// Returns error if the delegate is in an unexpected state.
	pub(crate) fn save_or_kill(self) -> Result<(), DispatchError> {
		let key = self.key;
		// see if delegate can be killed
		if self.ledger.total_delegated == Zero::zero() {
			ensure!(
				self.ledger.unclaimed_withdrawals == Zero::zero() &&
					self.ledger.pending_slash == Zero::zero(),
				Error::<T>::BadState
			);
			<Delegatees<T>>::remove(key);
		} else {
			self.ledger.save(&key)
		}

		Ok(())
	}
}

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

/// The type of pot account being created.
#[derive(Encode, Decode)]
pub(crate) enum AccountType {
	/// A proxy delegator account created for a nominator who migrated to a `delegate` account.
	///
	/// Funds for unmigrated `delegator` accounts of the `delegate` are kept here.
	ProxyDelegator,
}

#[derive(Default, Encode, Clone, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Delegation<T: Config> {
	/// The target of delegation.
	pub delegate: T::AccountId,
	/// The amount delegated.
	pub amount: BalanceOf<T>,
}

impl<T: Config> Delegation<T> {
	pub(crate) fn get(delegator: &T::AccountId) -> Option<Self> {
		<Delegators<T>>::get(delegator)
	}

	pub(crate) fn from(delegate: &T::AccountId, amount: BalanceOf<T>) -> Self {
		Delegation { delegate: delegate.clone(), amount }
	}

	pub(crate) fn can_delegate(delegator: &T::AccountId, delegate: &T::AccountId) -> bool {
		Delegation::<T>::get(delegator)
			.map(|delegation| delegation.delegate == delegate.clone())
			.unwrap_or(
				// all good if its a new delegator expect it should not am existing delegate.
				!<Delegates<T>>::contains_key(delegator),
			)
	}

	/// Checked decrease of delegation amount. Consumes self and returns a new copy.
	pub(crate) fn decrease_delegation(self, amount: BalanceOf<T>) -> Option<Self> {
		let updated_delegation = self.amount.checked_sub(&amount)?;
		Some(Delegation::from(&self.delegate, updated_delegation))
	}

	/// Checked increase of delegation amount. Consumes self and returns a new copy.
	pub(crate) fn increase_delegation(self, amount: BalanceOf<T>) -> Option<Self> {
		let updated_delegation = self.amount.checked_add(&amount)?;
		Some(Delegation::from(&self.delegate, updated_delegation))
	}

	pub(crate) fn save(self, key: &T::AccountId) {
		// Clean up if no delegation left.
		if self.amount == Zero::zero() {
			<Delegators<T>>::remove(key);
			return;
		}

		<Delegators<T>>::insert(key, self)
	}
}

/// Ledger of all delegations to a `Delegate`.
///
/// This keeps track of the active balance of the `delegate` that is made up from the funds that are
/// currently delegated to this `delegate`. It also tracks the pending slashes yet to be applied
/// among other things.
// FIXME(ank4n): Break up into two storage items - bookkeeping stuff and settings stuff.
#[derive(Default, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegationLedger<T: Config> {
	/// Where the reward should be paid out.
	pub payee: T::AccountId,
	/// Sum of all delegated funds to this `delegate`.
	#[codec(compact)]
	pub total_delegated: BalanceOf<T>,
	/// Funds that are withdrawn from core staking but not released to delegator/s.
	// FIXME(ank4n): Check/test about rebond: where delegator rebond what is unlocking.
	#[codec(compact)]
	pub unclaimed_withdrawals: BalanceOf<T>,
	/// Slashes that are not yet applied.
	#[codec(compact)]
	pub pending_slash: BalanceOf<T>,
	/// Whether this `delegate` is blocked from receiving new delegations.
	pub blocked: bool,
}

impl<T: Config> DelegationLedger<T> {
	pub(crate) fn new(reward_destination: &T::AccountId) -> Self {
		DelegationLedger {
			payee: reward_destination.clone(),
			total_delegated: Zero::zero(),
			unclaimed_withdrawals: Zero::zero(),
			pending_slash: Zero::zero(),
			blocked: false,
		}
	}

	pub(crate) fn get(key: &T::AccountId) -> Option<Self> {
		<Delegates<T>>::get(key)
	}

	pub(crate) fn can_accept_delegation(delegate: &T::AccountId) -> bool {
		DelegationLedger::<T>::get(delegate)
			.map(|ledger| !ledger.blocked)
			.unwrap_or(false)
	}

	pub(crate) fn save(self, key: &T::AccountId) {
		<Delegates<T>>::insert(key, self)
	}

	/// Effective total balance of the `delegate`.
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

pub struct Delegate<T: Config> {
	pub key: T::AccountId,
	pub ledger: DelegationLedger<T>,
}

impl<T: Config> Delegate<T> {
	pub(crate) fn from(delegate: &T::AccountId) -> Result<Delegate<T>, DispatchError> {
		let ledger = DelegationLedger::<T>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		Ok(Delegate { key: delegate.clone(), ledger })
	}

	// re-reads the delegate from database and returns a new instance.
	pub(crate) fn refresh(&self) -> Result<Delegate<T>, DispatchError> {
		Self::from(&self.key)
	}

	pub(crate) fn available_to_bond(&self) -> BalanceOf<T> {
		let bonded_stake = self.bonded_stake();

		let stakeable =
			self.ledger().map(|ledger| ledger.stakeable_balance()).unwrap_or(Zero::zero());

		defensive_assert!(stakeable >= bonded_stake, "cannot expose more than delegate balance");

		stakeable.saturating_sub(bonded_stake)
	}

	/// Similar to [`Self::available_to_bond`] but includes `DelegationLedger.unclaimed_withdrawals`
	/// as well.
	pub(crate) fn unbonded(&self) -> BalanceOf<T> {
		let bonded_stake = self.bonded_stake();

		let net_balance =
			self.ledger().map(|ledger| ledger.effective_balance()).unwrap_or(Zero::zero());

		defensive_assert!(net_balance >= bonded_stake, "cannot expose more than delegate balance");

		net_balance.saturating_sub(bonded_stake)
	}

	pub(crate) fn ledger(&self) -> Option<DelegationLedger<T>> {
		DelegationLedger::<T>::get(&self.key)
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
		Delegate { ledger: DelegationLedger { blocked: block, ..self.ledger }, ..self }
	}

	pub(crate) fn save(self) {
		let key = self.key;
		self.ledger.save(&key)
	}
}

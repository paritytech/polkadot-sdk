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

//! Implementations of public traits, namely [StakingInterface], and [DelegateeSupport].

use super::*;
use sp_staking::delegation::DelegatedStakeInterface;

/// StakingInterface implementation with delegation support.
///
/// Only supports Nominators via Delegated Bonds. It is possible for a nominator to migrate and
/// become a `delegatee`.
impl<T: Config> StakingInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;
	type CurrencyToVote = <T::CoreStaking as StakingInterface>::CurrencyToVote;

	fn minimum_nominator_bond() -> Self::Balance {
		T::CoreStaking::minimum_nominator_bond()
	}

	fn minimum_validator_bond() -> Self::Balance {
		T::CoreStaking::minimum_validator_bond()
	}

	fn stash_by_ctrl(_controller: &Self::AccountId) -> Result<Self::AccountId, DispatchError> {
		// ctrl are deprecated, just return err.
		Err(Error::<T>::NotSupported.into())
	}

	fn bonding_duration() -> EraIndex {
		T::CoreStaking::bonding_duration()
	}

	fn current_era() -> EraIndex {
		T::CoreStaking::current_era()
	}

	fn stake(who: &Self::AccountId) -> Result<Stake<Self::Balance>, DispatchError> {
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
		return T::CoreStaking::stake(who);
	}

	fn total_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		if Self::is_delegatee(who) {
			return T::CoreStaking::total_stake(who);
		}

		if Self::is_delegator(who) {
			let delegation = Delegation::<T>::get(who).defensive_ok_or(Error::<T>::BadState)?;
			return Ok(delegation.amount);
		}

		Err(Error::<T>::NotSupported.into())
	}

	fn active_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		T::CoreStaking::active_stake(who)
	}

	fn is_unbonding(who: &Self::AccountId) -> Result<bool, DispatchError> {
		T::CoreStaking::is_unbonding(who)
	}

	fn fully_unbond(who: &Self::AccountId) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
		return T::CoreStaking::fully_unbond(who);
	}

	fn bond(
		who: &Self::AccountId,
		value: Self::Balance,
		payee: &Self::AccountId,
	) -> DispatchResult {
		// ensure who is not already staked
		ensure!(T::CoreStaking::status(who).is_err(), Error::<T>::AlreadyStaking);
		let delegatee = Delegatee::<T>::from(who)?;

		ensure!(delegatee.available_to_bond() >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegatee.ledger.payee == *payee, Error::<T>::InvalidRewardDestination);

		T::CoreStaking::bond(who, value, payee)
	}

	fn nominate(who: &Self::AccountId, validators: Vec<Self::AccountId>) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotDelegatee);
		return T::CoreStaking::nominate(who, validators);
	}

	fn chill(who: &Self::AccountId) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotDelegatee);
		return T::CoreStaking::chill(who);
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> DispatchResult {
		let ledger = <Delegatees<T>>::get(who).ok_or(Error::<T>::NotDelegatee)?;
		ensure!(ledger.stakeable_balance() >= extra, Error::<T>::NotEnoughFunds);

		T::CoreStaking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let delegatee = Delegatee::<T>::from(stash)?;
		ensure!(delegatee.bonded_stake() >= value, Error::<T>::NotEnoughFunds);

		T::CoreStaking::unbond(stash, value)
	}

	/// Withdraw unbonding funds until current era.
	///
	/// Funds are moved to unclaimed_withdrawals register of the `DelegateeLedger`.
	fn withdraw_unbonded(
		delegatee_acc: Self::AccountId,
		num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		Pallet::<T>::withdraw_unbonded(&delegatee_acc, num_slashing_spans)
			.map(|delegatee| delegatee.ledger.total_delegated.is_zero())
	}

	fn desired_validator_count() -> u32 {
		T::CoreStaking::desired_validator_count()
	}

	fn election_ongoing() -> bool {
		T::CoreStaking::election_ongoing()
	}

	fn force_unstake(_who: Self::AccountId) -> DispatchResult {
		Err(Error::<T>::NotSupported.into())
	}

	fn is_exposed_in_era(who: &Self::AccountId, era: &EraIndex) -> bool {
		T::CoreStaking::is_exposed_in_era(who, era)
	}

	fn status(who: &Self::AccountId) -> Result<StakerStatus<Self::AccountId>, DispatchError> {
		ensure!(Self::is_delegatee(who), Error::<T>::NotDelegatee);
		T::CoreStaking::status(who)
	}

	fn is_validator(who: &Self::AccountId) -> bool {
		T::CoreStaking::is_validator(who)
	}

	fn nominations(who: &Self::AccountId) -> Option<Vec<Self::AccountId>> {
		T::CoreStaking::nominations(who)
	}

	fn slash_reward_fraction() -> Perbill {
		T::CoreStaking::slash_reward_fraction()
	}

	fn unsafe_release_all(_who: &Self::AccountId) {
		defensive_assert!(false, "unsafe_release_all is not supported");
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn max_exposure_page_size() -> sp_staking::Page {
		T::CoreStaking::max_exposure_page_size()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_era_stakers(
		current_era: &EraIndex,
		stash: &Self::AccountId,
		exposures: Vec<(Self::AccountId, Self::Balance)>,
	) {
		T::CoreStaking::add_era_stakers(current_era, stash, exposures)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_era(era: EraIndex) {
		T::CoreStaking::set_current_era(era)
	}
}

impl<T: Config> DelegatedStakeInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Effective balance of the delegatee account.
	fn delegatee_balance(who: &Self::AccountId) -> Self::Balance {
		Delegatee::<T>::from(who)
			.map(|delegatee| delegatee.ledger.effective_balance())
			.unwrap_or_default()
	}

	fn delegator_balance(delegator: &Self::AccountId) -> Self::Balance {
		Delegation::<T>::get(delegator).map(|d| d.amount).unwrap_or_default()
	}

	/// Delegate funds to `Delegatee`.
	fn delegate(
		who: &Self::AccountId,
		delegatee: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::register_as_delegatee(
			RawOrigin::Signed(delegatee.clone()).into(),
			reward_account.clone(),
		)?;

		// Delegate the funds from who to the delegatee account.
		Pallet::<T>::delegate_funds(
			RawOrigin::Signed(who.clone()).into(),
			delegatee.clone(),
			amount,
		)
	}

	/// Add more delegation to the delegatee account.
	fn delegate_extra(
		who: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_funds(
			RawOrigin::Signed(who.clone()).into(),
			delegatee.clone(),
			amount,
		)
	}

	/// Withdraw delegation of `delegator` to `delegatee`.
	///
	/// If there are funds in `delegatee` account that can be withdrawn, then those funds would be
	/// unlocked/released in the delegator's account.
	fn withdraw_delegation(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// fixme(ank4n): Can this not require slashing spans?
		Pallet::<T>::release(
			RawOrigin::Signed(delegatee.clone()).into(),
			delegator.clone(),
			amount,
			0,
		)
	}

	/// Returns true if the `delegatee` have any slash pending to be applied.
	fn has_pending_slash(delegatee: &Self::AccountId) -> bool {
		Delegatee::<T>::from(delegatee)
			.map(|d| !d.ledger.pending_slash.is_zero())
			.unwrap_or(false)
	}

	fn delegator_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		Pallet::<T>::do_slash(delegatee.clone(), delegator.clone(), value, maybe_reporter)
	}
}

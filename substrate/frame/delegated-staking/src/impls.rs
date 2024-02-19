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

//! Implementations of public traits, namely [StakingInterface], and [StakingDelegationSupport].

use super::*;
use sp_staking::delegation::PoolAdapter;

/// StakingInterface implementation with delegation support.
///
/// Only supports Nominators via Delegated Bonds. It is possible for a nominator to migrate and
/// become a `Delegate`.
impl<T: Config> StakingInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;
	type CurrencyToVote = <T::CoreStaking as StakingInterface>::CurrencyToVote;

	fn minimum_nominator_bond() -> Self::Balance {
		T::CoreStaking::minimum_nominator_bond()
	}

	fn minimum_validator_bond() -> Self::Balance {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		T::CoreStaking::minimum_validator_bond()
	}

	fn stash_by_ctrl(_controller: &Self::AccountId) -> Result<Self::AccountId, DispatchError> {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
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
		ensure!(Self::is_delegate(who), Error::<T>::NotSupported);
		return T::CoreStaking::stake(who);
	}

	fn total_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		if Self::is_delegate(who) {
			return T::CoreStaking::total_stake(who);
		}

		if Self::is_delegator(who) {
			let delegation = Delegation::<T>::get(who).defensive_ok_or(Error::<T>::BadState)?;
			return Ok(delegation.amount)
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
		ensure!(Self::is_delegate(who), Error::<T>::NotSupported);
		return T::CoreStaking::fully_unbond(who);
	}

	fn bond(
		who: &Self::AccountId,
		value: Self::Balance,
		payee: &Self::AccountId,
	) -> DispatchResult {
		// ensure who is not already staked
		ensure!(T::CoreStaking::status(who).is_err(), Error::<T>::NotDelegate);
		let delegate = Delegate::<T>::from(who)?;

		ensure!(delegate.available_to_bond() >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegate.ledger.payee == *payee, Error::<T>::InvalidRewardDestination);

		T::CoreStaking::bond(who, value, payee)
	}

	fn nominate(who: &Self::AccountId, validators: Vec<Self::AccountId>) -> DispatchResult {
		ensure!(Self::is_delegate(who), Error::<T>::NotSupported);
		return T::CoreStaking::nominate(who, validators);
	}

	fn chill(who: &Self::AccountId) -> DispatchResult {
		ensure!(Self::is_delegate(who), Error::<T>::NotSupported);
		return T::CoreStaking::chill(who);
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> DispatchResult {
		let delegation_register = <Delegates<T>>::get(who).ok_or(Error::<T>::NotDelegate)?;
		ensure!(delegation_register.stakeable_balance() >= extra, Error::<T>::NotEnoughFunds);

		T::CoreStaking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let delegate = Delegate::<T>::from(stash)?;
		ensure!(delegate.bonded_stake() >= value, Error::<T>::NotEnoughFunds);

		T::CoreStaking::unbond(stash, value)
	}

	/// Not supported, call [`DelegationInterface::delegate_withdraw`]
	/// FIXME(ank4n): Support it!!
	fn withdraw_unbonded(
		_stash: Self::AccountId,
		_num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		// FIXME(ank4n): Support withdrawing to self account.
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		Err(Error::<T>::NotSupported.into())
	}

	fn desired_validator_count() -> u32 {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		T::CoreStaking::desired_validator_count()
	}

	fn election_ongoing() -> bool {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		T::CoreStaking::election_ongoing()
	}

	fn force_unstake(_who: Self::AccountId) -> DispatchResult {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		Err(Error::<T>::NotSupported.into())
	}

	fn is_exposed_in_era(who: &Self::AccountId, era: &EraIndex) -> bool {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		T::CoreStaking::is_exposed_in_era(who, era)
	}

	fn status(who: &Self::AccountId) -> Result<StakerStatus<Self::AccountId>, DispatchError> {
		ensure!(Self::is_delegate(who), Error::<T>::NotSupported);
		T::CoreStaking::status(who)
	}

	fn is_validator(who: &Self::AccountId) -> bool {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		T::CoreStaking::is_validator(who)
	}

	fn nominations(who: &Self::AccountId) -> Option<Vec<Self::AccountId>> {
		T::CoreStaking::nominations(who)
	}

	fn slash_reward_fraction() -> Perbill {
		T::CoreStaking::slash_reward_fraction()
	}

	fn release_all(_who: &Self::AccountId) {
		defensive_assert!(false, "not supported for delegated impl of staking interface");
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

// impl<T: Config> DelegationInterface for Pallet<T> {
// FIXME(ank4n): Should be part of NP Adapter.
// 	fn apply_slash(
// 		delegate: &Self::AccountId,
// 		delegator: &Self::AccountId,
// 		value: Self::Balance,
// 		maybe_reporter: Option<Self::AccountId>,
// 	) -> DispatchResult {
// 		let mut delegation_register =
// 			<Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
// 		let delegation = <Delegators<T>>::get(delegator).ok_or(Error::<T>::NotDelegator)?;
//
// 		ensure!(&delegation.delegate == delegate, Error::<T>::NotDelegate);
// 		ensure!(delegation.amount >= value, Error::<T>::NotEnoughFunds);
//
// 		let (mut credit, _missing) =
// 			T::Currency::slash(&HoldReason::Delegating.into(), &delegator, value);
// 		let actual_slash = credit.peek();
// 		// remove the slashed amount
// 		delegation_register.pending_slash.saturating_reduce(actual_slash);
// 		<Delegates<T>>::insert(delegate, delegation_register);
//
// 		if let Some(reporter) = maybe_reporter {
// 			let reward_payout: BalanceOf<T> =
// 				T::CoreStaking::slash_reward_fraction() * actual_slash;
// 			let (reporter_reward, rest) = credit.split(reward_payout);
// 			credit = rest;
// 			// fixme(ank4n): handle error
// 			let _ = T::Currency::resolve(&reporter, reporter_reward);
// 		}
//
// 		T::OnSlash::on_unbalanced(credit);
// 		Ok(())
// 	}
//
// }

impl<T: Config> StakingDelegationSupport for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// this balance is total delegator that can be staked, and importantly not extra balance that
	/// is delegated but not bonded yet.
	fn stakeable_balance(who: &Self::AccountId) -> Self::Balance {
		Delegate::<T>::from(who)
			.map(|delegate| delegate.ledger.stakeable_balance())
			.unwrap_or_default()
	}

	fn restrict_reward_destination(
		who: &Self::AccountId,
		reward_destination: Option<Self::AccountId>,
	) -> bool {
		let maybe_register = <Delegates<T>>::get(who);

		if maybe_register.is_none() {
			// no restrictions for non delegates.
			return false;
		}

		// restrict if reward destination is not set
		if reward_destination.is_none() {
			return true;
		}

		let register = maybe_register.expect("checked above; qed");
		let reward_acc = reward_destination.expect("checked above; qed");

		// restrict if reward account is not what delegate registered.
		register.payee != reward_acc
	}

	fn is_delegate(who: &Self::AccountId) -> bool {
		Self::is_delegate(who)
	}

	fn report_slash(who: &Self::AccountId, slash: Self::Balance) {
		<Delegates<T>>::mutate(who, |maybe_register| match maybe_register {
			Some(register) => register.pending_slash.saturating_accrue(slash),
			None => {
				defensive!("should not be called on non-delegate");
			},
		});
	}
}

impl<T: Config> PoolAdapter for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Return balance of the `Delegate` (pool account) that is not bonded.
	///
	/// Equivalent to [FunInspect::balance] for non delegate accounts.
	fn balance(who: &Self::AccountId) -> Self::Balance {
		Delegate::<T>::from(who)
			.map(|delegate| delegate.unbonded())
			.unwrap_or(Zero::zero())
	}

	/// Returns balance of `Delegate` account including the held balances.
	///
	/// Equivalent to [FunInspect::total_balance] for non delegate accounts.
	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		Delegate::<T>::from(who)
			.map(|delegate| delegate.ledger.effective_balance())
			.unwrap_or(Zero::zero())
	}

	/// Add initial delegation to the pool account.
	///
	/// Equivalent to [FunMutate::transfer] for Direct Staking.
	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_funds(RawOrigin::Signed(who.clone()).into(), pool_account.clone(), amount)
	}

	fn delegate_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Self::delegate(who, pool_account, amount)
	}

	fn release_delegation(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// fixme(ank4n): This should not require slashing spans.
		Pallet::<T>::release(RawOrigin::Signed(pool_account.clone()).into(), who.clone(), amount, 0)
	}
}

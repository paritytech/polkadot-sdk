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

//! Implementations of public traits, namely [StakingInterface], [DelegationInterface] and
//! [StakingDelegationSupport].

use super::*;

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
			let delegation =
				Delegation::<T>::get(who).defensive_ok_or(Error::<T>::BadState)?;
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
		let delegation_register = <Delegates<T>>::get(who).ok_or(Error::<T>::NotDelegate)?;

		ensure!(delegation_register.unbonded_balance() >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegation_register.payee == *payee, Error::<T>::InvalidRewardDestination);

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
		ensure!(delegation_register.unbonded_balance() >= extra, Error::<T>::NotEnoughFunds);

		T::CoreStaking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let delegation_register = <Delegates<T>>::get(stash).ok_or(Error::<T>::NotDelegate)?;
		ensure!(delegation_register.hold >= value, Error::<T>::NotEnoughFunds);

		T::CoreStaking::unbond(stash, value)
	}

	/// Not supported, call [`Delegate::withdraw`]
	fn withdraw_unbonded(
		_stash: Self::AccountId,
		_num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		// FIXME(ank4n): Support withdrawing to self account.
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		Err(Error::<T>::NotSupported.into())
	}

	/// Not supported, call [`Delegate::withdraw`]
	fn withdraw_exact(
		_stash: &Self::AccountId,
		_amount: Self::Balance,
		_num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
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

impl<T: Config> DelegationInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn delegated_balance(who: &Self::AccountId) -> Self::Balance {
		<Delegates<T>>::get(who)
			.map_or_else(|| 0u32.into(), |register| register.delegated_balance())
	}

	fn unbonded_balance(who: &Self::AccountId) -> Self::Balance {
		<Delegates<T>>::get(who).map_or_else(|| 0u32.into(), |register| register.unbonded_balance())
	}

	fn accept_delegations(
		who: &Self::AccountId,
		reward_destination: &Self::AccountId,
	) -> DispatchResult {
		Self::register_as_delegate(
			RawOrigin::Signed(who.clone()).into(),
			reward_destination.clone(),
		)
	}

	/// Transfers funds from current staked account to `proxy_delegator`. Current staked account
	/// becomes a `delegate` with `proxy_delegator` delegating stakes to it.
	fn migrate_accept_delegations(
		new_delegate: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
	) -> DispatchResult {
		ensure!(new_delegate != proxy_delegator, Error::<T>::InvalidDelegation);

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
		let status = T::CoreStaking::status(new_delegate)?;
		match status {
			StakerStatus::Nominator(_) => (),
			_ => return Err(Error::<T>::InvalidDelegation.into()),
		}

		<DelegateMigration<T>>::insert(&new_delegate, &proxy_delegator);
		let stake = T::CoreStaking::stake(new_delegate)?;

		// unlock funds from staker
		T::CoreStaking::release_all(new_delegate);

		// try transferring the staked amount. This should never fail but if it does, it indicates
		// bad state and we abort.
		T::Currency::transfer(new_delegate, proxy_delegator, stake.total, Preservation::Expendable)
			.map_err(|_| Error::<T>::BadState)?;

		// delegate from new delegator to staker.
		// todo(ank4n) : inline this fn and propagate payee to core staking..
		Self::accept_delegations(new_delegate, payee)?;

		Self::delegate(proxy_delegator, new_delegate, stake.total)?;
		Self::bond_all(new_delegate)
	}

	fn block_delegations(delegate: &Self::AccountId) -> DispatchResult {
		let mut register = <Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		register.blocked = true;
		<Delegates<T>>::insert(delegate, register);

		Ok(())
	}

	fn unblock_delegations(delegate: &Self::AccountId) -> DispatchResult {
		let mut register = <Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		register.blocked = false;
		<Delegates<T>>::insert(delegate, register);

		Ok(())
	}

	fn kill_delegate(_delegate: &Self::AccountId) -> DispatchResult {
		todo!()
	}

	fn bond_all(who: &Self::AccountId) -> DispatchResult {
		let delegate = <Delegates<T>>::get(who).ok_or(Error::<T>::NotDelegate)?;
		let amount_to_bond = delegate.unbonded_balance();

		match T::CoreStaking::stake(who) {
			// already bonded
			Ok(_) => T::CoreStaking::bond_extra(who, amount_to_bond),
			// first bond
			Err(_) => T::CoreStaking::bond(who, amount_to_bond, &delegate.payee),
		}
	}

	#[transactional]
	fn delegate_withdraw(
		delegate: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult {
		// check how much is already unbonded
		let delegation_register = <Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		let unbonded_balance = delegation_register.unbonded_balance();

		if unbonded_balance < value {
			// fixme(ank4n) handle killing of stash
			let amount_to_withdraw = value.saturating_sub(unbonded_balance);
			let _stash_killed: bool =
				T::CoreStaking::withdraw_exact(delegate, amount_to_withdraw, num_slashing_spans)
					.map_err(|_| Error::<T>::WithdrawFailed)?;
		}

		Self::delegation_withdraw(delegator, delegate, value)
	}

	fn apply_slash(
		delegate: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult {
		let mut delegation_register =
			<Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		let delegation =
			<Delegators<T>>::get(delegator).ok_or(Error::<T>::NotDelegator)?;

		ensure!(&delegation.delegate == delegate, Error::<T>::NotDelegate);
		ensure!(delegation.amount >= value, Error::<T>::NotEnoughFunds);

		let (mut credit, _missing) =
			T::Currency::slash(&HoldReason::Delegating.into(), &delegator, value);
		let actual_slash = credit.peek();
		// remove the slashed amount
		delegation_register.pending_slash.saturating_reduce(actual_slash);
		<Delegates<T>>::insert(delegate, delegation_register);

		if let Some(reporter) = maybe_reporter {
			let reward_payout: BalanceOf<T> =
				T::CoreStaking::slash_reward_fraction() * actual_slash;
			let (reporter_reward, rest) = credit.split(reward_payout);
			credit = rest;
			// fixme(ank4n): handle error
			let _ = T::Currency::resolve(&reporter, reporter_reward);
		}

		T::OnSlash::on_unbalanced(credit);
		Ok(())
	}

	/// Move funds from proxy delegator to actual delegator.
	#[transactional]
	fn migrate_delegator(
		delegate: &Self::AccountId,
		new_delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		// make sure new delegator is not an existing delegator or a delegate
		ensure!(!<Delegates<T>>::contains_key(new_delegator), Error::<T>::NotAllowed);
		ensure!(!<Delegators<T>>::contains_key(new_delegator), Error::<T>::NotAllowed);
		// ensure we are migrating
		let proxy_delegator =
			<DelegateMigration<T>>::get(delegate).ok_or(Error::<T>::NotMigrating)?;
		// proxy delegator must exist
		// let (assigned_delegate, delegate_balance) =
		let delegation =
			<Delegators<T>>::get(&proxy_delegator).ok_or(Error::<T>::BadState)?;
		ensure!(delegation.delegate == *delegate, Error::<T>::BadState);

		// make sure proxy delegator has enough balance to support this migration.
		ensure!(delegation.amount >= value, Error::<T>::NotEnoughFunds);

		// remove delegation of `value` from `proxy_delegator`.
		let updated_delegate_balance = delegation.amount.saturating_sub(value);

		// if all funds are migrated out of proxy delegator, clean up.
		if updated_delegate_balance == BalanceOf::<T>::zero() {
			<Delegators<T>>::remove(&proxy_delegator);
			<DelegateMigration<T>>::remove(delegate);
		} else {
			// else update proxy delegator
			<Delegators<T>>::insert(
				&proxy_delegator,
				Delegation {
					delegate: delegate.clone(),
					amount: updated_delegate_balance,
				}
			);
		}

		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&proxy_delegator,
			value,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == value, "hold should have been released fully");

		// transfer the withdrawn value to `new_delegator`.
		T::Currency::transfer(&proxy_delegator, new_delegator, value, Preservation::Expendable)
			.map_err(|_| Error::<T>::BadState)?;

		// add the above removed delegation to `new_delegator`.
		Delegation::<T>::from(delegate, value).save(new_delegator);
		// hold the funds again in the new delegator account.
		T::Currency::hold(&HoldReason::Delegating.into(), &new_delegator, value)?;

		Ok(())
	}

	fn delegate(
		delegator: &Self::AccountId,
		delegate: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		let delegator_balance =
			T::Currency::reducible_balance(&delegator, Preservation::Expendable, Fortitude::Polite);
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		ensure!(delegator_balance >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegate != delegator, Error::<T>::InvalidDelegation);

		let mut delegation_register =
			<Delegates<T>>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		ensure!(!delegation_register.blocked, Error::<T>::DelegationsBlocked);

		// A delegate cannot delegate.
		if <Delegates<T>>::contains_key(delegator) {
			return Err(Error::<T>::InvalidDelegation.into())
		}

		let new_delegation_amount =
			if let Some(current_delegation) = <Delegators<T>>::get(delegator) {
				ensure!(&current_delegation.delegate == delegate, Error::<T>::InvalidDelegation);
				value.saturating_add(current_delegation.amount)
			} else {
				value
			};

		delegation_register.total_delegated.saturating_accrue(value);

		Delegation::<T>::from(delegate, new_delegation_amount).save(delegator);
		<Delegates<T>>::insert(delegate, delegation_register);

		T::Currency::hold(&HoldReason::Delegating.into(), &delegator, value)?;

		Self::deposit_event(Event::<T>::Delegated {
			delegate: delegate.clone(),
			delegator: delegator.clone(),
			amount: value,
		});

		Ok(())
	}
}

impl<T: Config> StakingDelegationSupport for Pallet<T> {
    type Balance = BalanceOf<T>;
    type AccountId = T::AccountId;
    fn stakeable_balance(who: &Self::AccountId) -> Self::Balance {
        <Delegates<T>>::get(who)
            .map(|delegate| delegate.delegated_balance())
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

    fn update_hold(who: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
        ensure!(Self::is_delegate(who), Error::<T>::NotSupported);

        // delegation register should exist since `who` is a delegate.
        let delegation_register = <Delegates<T>>::get(who).defensive_ok_or(Error::<T>::BadState)?;

        ensure!(delegation_register.total_delegated >= amount, Error::<T>::NotEnoughFunds);
        ensure!(delegation_register.pending_slash <= amount, Error::<T>::UnappliedSlash);
        let updated_register = DelegationLedger { hold: amount, ..delegation_register };
        <Delegates<T>>::insert(who, updated_register);

        Ok(())
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

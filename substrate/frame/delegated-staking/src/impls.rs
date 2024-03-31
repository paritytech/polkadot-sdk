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

//! Implementations of public traits, namely [StakingInterface], [DelegatedStakeInterface] and
//! [OnStakingUpdate].

use super::*;
use sp_staking::{DelegatedStakeInterface, OnStakingUpdate};

/// Wrapper `StakingInterface` implementation for `Agents`.
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
		ensure!(Self::is_agent(who), Error::<T>::NotSupported);
		T::CoreStaking::stake(who)
	}

	fn total_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		if Self::is_agent(who) {
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
		ensure!(Self::is_agent(who), Error::<T>::NotSupported);
		T::CoreStaking::fully_unbond(who)
	}

	fn bond(
		who: &Self::AccountId,
		value: Self::Balance,
		payee: &Self::AccountId,
	) -> DispatchResult {
		// ensure who is not already staked
		ensure!(T::CoreStaking::status(who).is_err(), Error::<T>::AlreadyStaking);
		let agent = Agent::<T>::from(who)?;

		ensure!(agent.available_to_bond() >= value, Error::<T>::NotEnoughFunds);
		ensure!(agent.ledger.payee == *payee, Error::<T>::InvalidRewardDestination);

		T::CoreStaking::virtual_bond(who, value, payee)
	}

	fn nominate(who: &Self::AccountId, validators: Vec<Self::AccountId>) -> DispatchResult {
		ensure!(Self::is_agent(who), Error::<T>::NotAgent);
		T::CoreStaking::nominate(who, validators)
	}

	fn chill(who: &Self::AccountId) -> DispatchResult {
		ensure!(Self::is_agent(who), Error::<T>::NotAgent);
		T::CoreStaking::chill(who)
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> DispatchResult {
		let ledger = <Agents<T>>::get(who).ok_or(Error::<T>::NotAgent)?;
		ensure!(ledger.stakeable_balance() >= extra, Error::<T>::NotEnoughFunds);

		T::CoreStaking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let agent = Agent::<T>::from(stash)?;
		ensure!(agent.bonded_stake() >= value, Error::<T>::NotEnoughFunds);

		T::CoreStaking::unbond(stash, value)
	}

	fn update_payee(stash: &Self::AccountId, reward_acc: &Self::AccountId) -> DispatchResult {
		T::CoreStaking::update_payee(stash, reward_acc)
	}

	/// Withdraw unbonding funds until current era.
	///
	/// Funds are moved to unclaimed_withdrawals register of the `AgentLedger`.
	fn withdraw_unbonded(
		agent_acc: Self::AccountId,
		num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		Pallet::<T>::withdraw_unbonded(&agent_acc, num_slashing_spans)
			.map(|agent| agent.ledger.total_delegated.is_zero())
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
		ensure!(Self::is_agent(who), Error::<T>::NotAgent);
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
	/// Effective balance of the `Agent` account.
	fn agent_balance(who: &Self::AccountId) -> Self::Balance {
		Agent::<T>::from(who)
			.map(|agent| agent.ledger.effective_balance())
			.unwrap_or_default()
	}

	fn delegator_balance(delegator: &Self::AccountId) -> Self::Balance {
		Delegation::<T>::get(delegator).map(|d| d.amount).unwrap_or_default()
	}

	/// Delegate funds to an `Agent`.
	fn delegate(
		who: &Self::AccountId,
		agent: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::register_agent(
			RawOrigin::Signed(agent.clone()).into(),
			reward_account.clone(),
		)?;

		// Delegate the funds from who to the `Agent` account.
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.clone()).into(), agent.clone(), amount)
	}

	/// Add more delegation to the `Agent` account.
	fn delegate_extra(
		who: &Self::AccountId,
		agent: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.clone()).into(), agent.clone(), amount)
	}

	/// Withdraw delegation of `delegator` to `Agent`.
	///
	/// If there are funds in `Agent` account that can be withdrawn, then those funds would be
	/// unlocked/released in the delegator's account.
	fn withdraw_delegation(
		delegator: &Self::AccountId,
		agent: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// fixme(ank4n): Can this not require slashing spans?
		Pallet::<T>::release_delegation(
			RawOrigin::Signed(agent.clone()).into(),
			delegator.clone(),
			amount,
			0,
		)
	}

	/// Returns true if the `Agent` have any slash pending to be applied.
	fn has_pending_slash(agent: &Self::AccountId) -> bool {
		Agent::<T>::from(agent)
			.map(|d| !d.ledger.pending_slash.is_zero())
			.unwrap_or(false)
	}

	fn delegator_slash(
		agent: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		Pallet::<T>::do_slash(agent.clone(), delegator.clone(), value, maybe_reporter)
	}
}

impl<T: Config> OnStakingUpdate<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn on_slash(
		who: &T::AccountId,
		_slashed_active: BalanceOf<T>,
		_slashed_unlocking: &sp_std::collections::btree_map::BTreeMap<EraIndex, BalanceOf<T>>,
		slashed_total: BalanceOf<T>,
	) {
		<Agents<T>>::mutate(who, |maybe_register| match maybe_register {
			// if existing agent, register the slashed amount as pending slash.
			Some(register) => register.pending_slash.saturating_accrue(slashed_total),
			None => {
				// nothing to do
			},
		});
	}
}

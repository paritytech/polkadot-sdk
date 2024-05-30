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

//! Implementations of public traits, namely [`DelegationInterface`] and [`OnStakingUpdate`].

use super::*;
use sp_staking::{
	Agent, DelegationInterface, DelegationMigrator, Delegator, OnStakingUpdate,
};

impl<T: Config> DelegationInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Effective balance of the `Agent` account.
	fn agent_balance(who: Agent<Self::AccountId>) -> Option<Self::Balance> {
		AgentLedgerOuter::<T>::get(&who.0).map(|agent| agent.ledger.effective_balance()).ok()
	}

	fn delegator_balance(delegator: Delegator<Self::AccountId>) -> Option<Self::Balance> {
		Delegation::<T>::get(&delegator.0).map(|d| d.amount)
	}

	/// Delegate funds to an `Agent`.
	fn delegate(
		who: Delegator<Self::AccountId>,
		agent: Agent<Self::AccountId>,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::register_agent(
			RawOrigin::Signed(agent.0.clone()).into(),
			reward_account.clone(),
		)?;

		// Delegate the funds from who to the `Agent` account.
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.0).into(), agent.0, amount)
	}

	/// Add more delegation to the `Agent` account.
	fn delegate_extra(
		who: Delegator<Self::AccountId>,
		agent: Agent<Self::AccountId>,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.0).into(), agent.0, amount)
	}

	/// Withdraw delegation of `delegator` to `Agent`.
	///
	/// If there are funds in `Agent` account that can be withdrawn, then those funds would be
	/// unlocked/released in the delegator's account.
	fn withdraw_delegation(
		delegator: Delegator<Self::AccountId>,
		agent: Agent<Self::AccountId>,
		amount: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult {
		Pallet::<T>::release_delegation(
			RawOrigin::Signed(agent.0).into(),
			delegator.0,
			amount,
			num_slashing_spans,
		)
	}

	/// Returns pending slash of the `agent`.
	fn pending_slash(agent: Agent<Self::AccountId>) -> Option<Self::Balance> {
		AgentLedgerOuter::<T>::get(&agent.0).map(|d| d.ledger.pending_slash).ok()
	}

	fn delegator_slash(
		agent: Agent<Self::AccountId>,
		delegator: Delegator<Self::AccountId>,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		Pallet::<T>::do_slash(agent, delegator, value, maybe_reporter)
	}
}

impl<T: Config> DelegationMigrator for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn migrate_nominator_to_agent(
		agent: Agent<Self::AccountId>,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		Pallet::<T>::migrate_to_agent(RawOrigin::Signed(agent.0).into(), reward_account.clone())
	}
	fn migrate_delegation(
		agent: Agent<Self::AccountId>,
		delegator: Delegator<Self::AccountId>,
		value: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::migrate_delegation(RawOrigin::Signed(agent.0).into(), delegator.0, value)
	}

	/// Only used for testing.
	#[cfg(feature = "runtime-benchmarks")]
	fn drop_agent(agent: Agent<Self::AccountId>) {
		<Agents<T>>::remove(agent.0.clone());
		<Delegators<T>>::iter()
			.filter(|(_, delegation)| delegation.agent == agent.0)
			.for_each(|(delegator, _)| {
				let _ = T::Currency::release_all(
					&HoldReason::StakingDelegation.into(),
					&delegator,
					Precision::BestEffort,
				);
				<Delegators<T>>::remove(&delegator);
			});

		T::CoreStaking::migrate_to_direct_staker(&agent.0);
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

	fn on_withdraw(stash: &T::AccountId, amount: BalanceOf<T>) {
		// if there is a withdraw to the agent, then add it to the unclaimed withdrawals.
		let _ = AgentLedgerOuter::<T>::get(stash)
			// can't do anything if there is an overflow error. Just raise a defensive error.
			.and_then(|agent| agent.add_unclaimed_withdraw(amount).defensive())
			.map(|agent| agent.save());
	}
}

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

//! Implementations of public traits, namely [`DelegationInterface`] and [`OnStakingUpdate`].

use super::*;
use sp_staking::{DelegationInterface, DelegationMigrator, OnStakingUpdate};

impl<T: Config> DelegationInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Effective balance of the `Agent` account.
	fn agent_balance(agent: Agent<Self::AccountId>) -> Option<Self::Balance> {
		AgentLedgerOuter::<T>::get(&agent.get())
			.map(|a| a.ledger.effective_balance())
			.ok()
	}

	fn agent_transferable_balance(agent: Agent<Self::AccountId>) -> Option<Self::Balance> {
		AgentLedgerOuter::<T>::get(&agent.get())
			.map(|a| a.ledger.unclaimed_withdrawals)
			.ok()
	}

	fn delegator_balance(delegator: Delegator<Self::AccountId>) -> Option<Self::Balance> {
		Delegation::<T>::get(&delegator.get()).map(|d| d.amount)
	}

	/// Delegate funds to an `Agent`.
	fn register_agent(
		agent: Agent<Self::AccountId>,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		Pallet::<T>::register_agent(
			RawOrigin::Signed(agent.clone().get()).into(),
			reward_account.clone(),
		)
	}

	/// Remove `Agent` registration.
	fn remove_agent(agent: Agent<Self::AccountId>) -> DispatchResult {
		Pallet::<T>::remove_agent(RawOrigin::Signed(agent.clone().get()).into())
	}

	/// Add more delegation to the `Agent` account.
	fn delegate(
		who: Delegator<Self::AccountId>,
		agent: Agent<Self::AccountId>,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.get()).into(), agent.get(), amount)
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
			RawOrigin::Signed(agent.get()).into(),
			delegator.get(),
			amount,
			num_slashing_spans,
		)
	}

	/// Returns pending slash of the `agent`.
	fn pending_slash(agent: Agent<Self::AccountId>) -> Option<Self::Balance> {
		AgentLedgerOuter::<T>::get(&agent.get()).map(|d| d.ledger.pending_slash).ok()
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
		Pallet::<T>::migrate_to_agent(RawOrigin::Signed(agent.get()).into(), reward_account.clone())
	}
	fn migrate_delegation(
		agent: Agent<Self::AccountId>,
		delegator: Delegator<Self::AccountId>,
		value: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::migrate_delegation(
			RawOrigin::Signed(agent.get()).into(),
			delegator.get(),
			value,
		)
	}

	/// Only used for testing.
	#[cfg(feature = "runtime-benchmarks")]
	fn force_kill_agent(agent: Agent<Self::AccountId>) {
		<Agents<T>>::remove(agent.clone().get());
		<Delegators<T>>::iter()
			.filter(|(_, delegation)| delegation.agent == agent.clone().get())
			.for_each(|(delegator, _)| {
				let _ = T::Currency::release_all(
					&HoldReason::StakingDelegation.into(),
					&delegator,
					Precision::BestEffort,
				);
				<Delegators<T>>::remove(&delegator);
			});
	}
}

impl<T: Config> OnStakingUpdate<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn on_slash(
		who: &T::AccountId,
		_slashed_active: BalanceOf<T>,
		_slashed_unlocking: &alloc::collections::btree_map::BTreeMap<EraIndex, BalanceOf<T>>,
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

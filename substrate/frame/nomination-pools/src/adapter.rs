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

use crate::*;
use sp_staking::{DelegationInterface, DelegationMigrator};

/// Types of stake strategies.
///
/// Useful for determining current staking strategy of a runtime and enforce integrity tests.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebugNoBound, PartialEq)]
pub enum StakeStrategyType {
	/// Member funds are transferred to pool account and staked.
	///
	/// This is the older staking strategy used by pools. For a new runtime, it is recommended to
	/// use [`StakeStrategyType::Delegate`] strategy instead.
	Transfer,
	/// Member funds are delegated to pool account and staked.
	Delegate,
}

/// An adapter trait that can support multiple staking strategies.
///
/// Depending on which staking strategy we want to use, the staking logic can be slightly
/// different. Refer the two possible strategies currently: [`TransferStake`] and
/// [`DelegateStake`] for more detail.
pub trait StakeStrategy {
	type Balance: frame_support::traits::tokens::Balance;
	type AccountId: Clone + sp_std::fmt::Debug;
	type CoreStaking: StakingInterface<Balance = Self::Balance, AccountId = Self::AccountId>;

	/// The type of staking strategy of the current adapter.
	fn strategy_type() -> StakeStrategyType;

	/// See [`StakingInterface::bonding_duration`].
	fn bonding_duration() -> EraIndex {
		Self::CoreStaking::bonding_duration()
	}

	/// See [`StakingInterface::current_era`].
	fn current_era() -> EraIndex {
		Self::CoreStaking::current_era()
	}

	/// See [`StakingInterface::minimum_nominator_bond`].
	fn minimum_nominator_bond() -> Self::Balance {
		Self::CoreStaking::minimum_nominator_bond()
	}

	/// Balance that can be transferred from pool account to member.
	///
	/// This is part of the pool balance that is not actively staked. That is, tokens that are
	/// in unbonding period or unbonded.
	fn transferable_balance(pool_account: &Self::AccountId) -> Self::Balance;

	/// Total balance of the pool including amount that is actively staked.
	fn total_balance(pool_account: &Self::AccountId) -> Self::Balance;

	/// Amount of tokens delegated by the member.
	fn member_delegation_balance(member_account: &Self::AccountId) -> Self::Balance;

	/// See [`StakingInterface::active_stake`].
	fn active_stake(pool_account: &Self::AccountId) -> Self::Balance {
		Self::CoreStaking::active_stake(pool_account).unwrap_or_default()
	}

	/// See [`StakingInterface::total_stake`].
	fn total_stake(pool_account: &Self::AccountId) -> Self::Balance {
		Self::CoreStaking::total_stake(pool_account).unwrap_or_default()
	}

	/// Which strategy the pool account is using.
	///
	/// This can be different from the [`Self::strategy_type`] of the adapter if the pool has not
	/// migrated to the new strategy yet.
	fn pool_strategy(pool_account: &Self::AccountId) -> StakeStrategyType {
		match Self::CoreStaking::is_virtual_staker(pool_account) {
			true => StakeStrategyType::Delegate,
			false => StakeStrategyType::Transfer,
		}
	}

	/// See [`StakingInterface::nominate`].
	fn nominate(
		pool_account: &Self::AccountId,
		validators: Vec<Self::AccountId>,
	) -> DispatchResult {
		Self::CoreStaking::nominate(pool_account, validators)
	}

	/// See [`StakingInterface::chill`].
	fn chill(pool_account: &Self::AccountId) -> DispatchResult {
		Self::CoreStaking::chill(pool_account)
	}

	/// Pledge `amount` towards `pool_account` and update the pool bond. Also see
	/// [`StakingInterface::bond`].
	fn pledge_bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
		bond_type: BondType,
	) -> DispatchResult;

	/// See [`StakingInterface::unbond`].
	fn unbond(pool_account: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
		Self::CoreStaking::unbond(pool_account, amount)
	}

	/// See [`StakingInterface::withdraw_unbonded`].
	fn withdraw_unbonded(
		pool_account: &Self::AccountId,
		num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		Self::CoreStaking::withdraw_unbonded(pool_account.clone(), num_slashing_spans)
	}

	/// Withdraw funds from pool account to member account.
	fn member_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult;

	/// Check if there is any pending slash for the pool.
	fn has_pending_slash(pool_account: &Self::AccountId) -> bool;

	/// Slash the member account with `amount` against pending slashes for the pool.
	fn member_slash(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult;

	/// Migrate pool account from being a direct nominator to a delegated agent.
	///
	/// This is useful for migrating a pool account from [`StakeStrategyType::Transfer`] to
	/// [`StakeStrategyType::Delegate`].
	fn migrate_nominator_to_agent(
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
	) -> DispatchResult;

	/// Migrate member balance from pool account to member account.
	///
	/// This is useful for a pool account that migrated from [`StakeStrategyType::Transfer`] to
	/// [`StakeStrategyType::Delegate`]. Its members can then migrate their delegated balance
	/// back to their account.
	///
	/// Internally, the member funds that are locked in the pool account are transferred back and
	/// locked in the member account.
	fn migrate_delegation(
		pool: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// List of validators nominated by the pool account.
	#[cfg(feature = "runtime-benchmarks")]
	fn nominations(pool_account: &Self::AccountId) -> Option<Vec<Self::AccountId>> {
		Self::CoreStaking::nominations(pool_account)
	}

	/// Remove the pool account as agent.
	///
	/// Useful for migrating pool account from a delegated agent to a direct nominator. Only used
	/// in tests and benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn remove_as_agent(_pool: &Self::AccountId) {
		// noop by default
	}
}

/// A staking strategy implementation that supports transfer based staking.
///
/// In order to stake, this adapter transfers the funds from the member/delegator account to the
/// pool account and stakes through the pool account on `Staking`.
///
/// This is the older Staking strategy used by pools. To switch to the newer [`DelegateStake`]
/// strategy in an existing runtime, storage migration is required. See
/// [`migration::unversioned::DelegationStakeMigration`]. For new runtimes, it is highly recommended
/// to use the [`DelegateStake`] strategy.
pub struct TransferStake<T: Config, Staking: StakingInterface>(PhantomData<(T, Staking)>);

impl<T: Config, Staking: StakingInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>>
	StakeStrategy for TransferStake<T, Staking>
{
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;
	type CoreStaking = Staking;

	fn strategy_type() -> StakeStrategyType {
		StakeStrategyType::Transfer
	}

	fn transferable_balance(pool_account: &Self::AccountId) -> BalanceOf<T> {
		T::Currency::balance(pool_account).saturating_sub(Self::active_stake(pool_account))
	}

	fn total_balance(pool_account: &Self::AccountId) -> BalanceOf<T> {
		T::Currency::total_balance(pool_account)
	}

	fn member_delegation_balance(_member_account: &T::AccountId) -> Staking::Balance {
		// for transfer stake, delegation balance is always zero.
		Zero::zero()
	}

	fn pledge_bond(
		who: &T::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: BalanceOf<T>,
		bond_type: BondType,
	) -> DispatchResult {
		match bond_type {
			BondType::Create => {
				// first bond
				T::Currency::transfer(who, pool_account, amount, Preservation::Expendable)?;
				Staking::bond(pool_account, amount, &reward_account)
			},
			BondType::Extra => {
				// additional bond
				T::Currency::transfer(who, pool_account, amount, Preservation::Preserve)?;
				Staking::bond_extra(pool_account, amount)
			},
		}
	}

	fn member_withdraw(
		who: &T::AccountId,
		pool_account: &Self::AccountId,
		amount: BalanceOf<T>,
		_num_slashing_spans: u32,
	) -> DispatchResult {
		T::Currency::transfer(pool_account, &who, amount, Preservation::Expendable)?;

		Ok(())
	}

	fn has_pending_slash(_: &Self::AccountId) -> bool {
		// for transfer stake strategy, slashing is greedy and never deferred.
		false
	}

	fn member_slash(
		_who: &T::AccountId,
		_pool: &Self::AccountId,
		_amount: Staking::Balance,
		_maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		Err(Error::<T>::Defensive(DefensiveError::DelegationUnsupported).into())
	}

	fn migrate_nominator_to_agent(
		_pool: &Self::AccountId,
		_reward_account: &Self::AccountId,
	) -> DispatchResult {
		Err(Error::<T>::Defensive(DefensiveError::DelegationUnsupported).into())
	}

	fn migrate_delegation(
		_pool: &Self::AccountId,
		_delegator: &Self::AccountId,
		_value: Self::Balance,
	) -> DispatchResult {
		Err(Error::<T>::Defensive(DefensiveError::DelegationUnsupported).into())
	}
}

/// A staking strategy implementation that supports delegation based staking.
///
/// In this approach, first the funds are delegated from delegator to the pool account and later
/// staked with `Staking`. The advantage of this approach is that the funds are held in the
/// user account itself and not in the pool account.
///
/// This is the newer staking strategy used by pools. Once switched to this and migrated, ideally
/// the `TransferStake` strategy should not be used. Or a separate migration would be required for
/// it which is not provided by this pallet.
///
/// Use [`migration::unversioned::DelegationStakeMigration`] to migrate to this strategy.
pub struct DelegateStake<T: Config, Staking: StakingInterface, Delegation: DelegationInterface>(
	PhantomData<(T, Staking, Delegation)>,
);

impl<
		T: Config,
		Staking: StakingInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>,
		Delegation: DelegationInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>
			+ DelegationMigrator<Balance = BalanceOf<T>, AccountId = T::AccountId>,
	> StakeStrategy for DelegateStake<T, Staking, Delegation>
{
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;
	type CoreStaking = Staking;

	fn strategy_type() -> StakeStrategyType {
		StakeStrategyType::Delegate
	}

	fn transferable_balance(pool_account: &Self::AccountId) -> BalanceOf<T> {
		Delegation::agent_balance(pool_account).saturating_sub(Self::active_stake(pool_account))
	}

	fn total_balance(pool_account: &Self::AccountId) -> BalanceOf<T> {
		Delegation::agent_balance(pool_account)
	}

	fn member_delegation_balance(member_account: &T::AccountId) -> BalanceOf<T> {
		Delegation::delegator_balance(member_account)
	}

	fn pledge_bond(
		who: &T::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: BalanceOf<T>,
		bond_type: BondType,
	) -> DispatchResult {
		match bond_type {
			BondType::Create => {
				// first delegation
				Delegation::delegate(who, pool_account, reward_account, amount)
			},
			BondType::Extra => {
				// additional delegation
				Delegation::delegate_extra(who, pool_account, amount)
			},
		}
	}

	fn member_withdraw(
		who: &T::AccountId,
		pool_account: &Self::AccountId,
		amount: BalanceOf<T>,
		num_slashing_spans: u32,
	) -> DispatchResult {
		Delegation::withdraw_delegation(&who, pool_account, amount, num_slashing_spans)
	}

	fn has_pending_slash(pool_account: &Self::AccountId) -> bool {
		Delegation::has_pending_slash(pool_account)
	}

	fn member_slash(
		who: &T::AccountId,
		pool_account: &Self::AccountId,
		amount: BalanceOf<T>,
		maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		Delegation::delegator_slash(pool_account, who, amount, maybe_reporter)
	}

	fn migrate_nominator_to_agent(
		pool: &Self::AccountId,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		Delegation::migrate_nominator_to_agent(pool, reward_account)
	}

	fn migrate_delegation(
		pool: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		Delegation::migrate_delegation(pool, delegator, value)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn remove_as_agent(pool: &Self::AccountId) {
		Delegation::drop_agent(pool)
	}
}

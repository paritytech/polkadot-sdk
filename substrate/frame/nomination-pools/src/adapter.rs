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
use sp_staking::delegation::DelegatedStakeInterface;

/// An adapter trait that can support multiple staking strategies.
///
/// Depending on which staking strategy we want to use, the staking logic can be slightly
/// different. Refer the two possible strategies currently: [`TransferStake`] and
/// [`DelegateStake`].
pub trait StakeStrategy {
	type Balance: frame_support::traits::tokens::Balance;
	type AccountId: Clone + sp_std::fmt::Debug;

	fn bonding_duration() -> EraIndex;
	fn current_era() -> EraIndex;
	fn minimum_nominator_bond() -> Self::Balance;

	/// Transferable balance of the pool.
	///
	/// This is the amount that can be withdrawn from the pool.
	///
	/// Does not include reward account.
	fn transferable_balance(id: PoolId) -> Self::Balance;

	/// Total balance of the pool including amount that is actively staked.
	fn total_balance(id: PoolId) -> Self::Balance;
	fn member_delegation_balance(member_account: &Self::AccountId) -> Self::Balance;

	fn active_stake(pool: PoolId) -> Self::Balance;
	fn total_stake(pool: PoolId) -> Self::Balance;

	fn nominate(pool_id: PoolId, validators: Vec<Self::AccountId>) -> DispatchResult;

	fn chill(pool_id: PoolId) -> DispatchResult;

	fn bond(
		who: &Self::AccountId,
		pool_id: PoolId,
		amount: Self::Balance,
		bond_type: BondType,
	) -> DispatchResult;

	fn unbond(pool_id: PoolId, amount: Self::Balance) -> DispatchResult;

	fn withdraw_unbonded(pool_id: PoolId, num_slashing_spans: u32) -> Result<bool, DispatchError>;

	fn member_withdraw(
		who: &Self::AccountId,
		pool: PoolId,
		amount: Self::Balance,
	) -> DispatchResult;

	fn has_pending_slash(pool: PoolId) -> bool;

	fn member_slash(
		who: &Self::AccountId,
		pool: PoolId,
		amount: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult;
}

/// A staking strategy implementation that supports transfer based staking.
///
/// In order to stake, this adapter transfers the funds from the delegator account to the pool
/// account and stakes directly on `Staking`.
pub struct TransferStake<T: Config, Staking: StakingInterface>(PhantomData<(T, Staking)>);

impl<T: Config, Staking: StakingInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>>
	StakeStrategy for TransferStake<T, Staking>
{
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn bonding_duration() -> EraIndex {
		Staking::bonding_duration()
	}
	fn current_era() -> EraIndex {
		Staking::current_era()
	}
	fn minimum_nominator_bond() -> Staking::Balance {
		Staking::minimum_nominator_bond()
	}

	fn transferable_balance(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		T::Currency::balance(&pool_account).saturating_sub(Self::active_stake(pool))
	}

	fn total_balance(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		T::Currency::total_balance(&pool_account)
	}

	fn member_delegation_balance(_member_account: &T::AccountId) -> Staking::Balance {
		defensive!("delegation not supported");
		Zero::zero()
	}

	fn active_stake(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::active_stake(&pool_account).unwrap_or_default()
	}

	fn total_stake(pool: PoolId) -> Staking::Balance {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::total_stake(&pool_account).unwrap_or_default()
	}

	fn nominate(pool_id: PoolId, validators: Vec<T::AccountId>) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::nominate(&pool_account, validators)
	}

	fn chill(pool_id: PoolId) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::chill(&pool_account)
	}

	fn bond(
		who: &T::AccountId,
		pool: PoolId,
		amount: BalanceOf<T>,
		bond_type: BondType,
	) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		let reward_account = Pallet::<T>::create_reward_account(pool);

		match bond_type {
			BondType::Create => {
				// first bond
				T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;
				Staking::bond(&pool_account, amount, &reward_account)
			},
			BondType::Later => {
				// additional bond
				T::Currency::transfer(who, &pool_account, amount, Preservation::Preserve)?;
				Staking::bond_extra(&pool_account, amount)
			},
		}
	}

	fn unbond(pool_id: PoolId, amount: Staking::Balance) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::unbond(&pool_account, amount)
	}

	fn withdraw_unbonded(pool_id: PoolId, num_slashing_spans: u32) -> Result<bool, DispatchError> {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::withdraw_unbonded(pool_account, num_slashing_spans)
	}

	fn member_withdraw(who: &T::AccountId, pool: PoolId, amount: BalanceOf<T>) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		T::Currency::transfer(&pool_account, &who, amount, Preservation::Expendable)?;

		Ok(())
	}

	fn has_pending_slash(_pool: PoolId) -> bool {
		// for transfer stake strategy, slashing is greedy
		false
	}

	fn member_slash(
		_who: &T::AccountId,
		_pool: PoolId,
		_amount: Staking::Balance,
		_maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		Err(Error::<T>::Defensive(DefensiveError::DelegationUnsupported).into())
	}
}

/// A staking strategy implementation that supports delegation based staking.
///
/// In this approach, first the funds are delegated from delegator to the pool account and later
/// staked with `Staking`. The advantage of this approach is that the funds are held in the
/// use account itself and not in the pool account.
pub struct DelegateStake<T: Config, Staking: DelegatedStakeInterface>(PhantomData<(T, Staking)>);

impl<
		T: Config,
		Staking: DelegatedStakeInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>,
	> StakeStrategy for DelegateStake<T, Staking>
{
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn bonding_duration() -> EraIndex {
		Staking::bonding_duration()
	}
	fn current_era() -> EraIndex {
		Staking::current_era()
	}
	fn minimum_nominator_bond() -> Staking::Balance {
		Staking::minimum_nominator_bond()
	}

	fn transferable_balance(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::delegatee_balance(&pool_account).saturating_sub(Self::active_stake(pool))
	}

	fn total_balance(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::delegatee_balance(&pool_account)
	}

	fn member_delegation_balance(member_account: &T::AccountId) -> Staking::Balance {
		Staking::delegator_balance(member_account)
	}

	fn active_stake(pool: PoolId) -> BalanceOf<T> {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::active_stake(&pool_account).unwrap_or_default()
	}

	fn total_stake(pool: PoolId) -> Staking::Balance {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::total_stake(&pool_account).unwrap_or_default()
	}

	fn nominate(pool_id: PoolId, validators: Vec<T::AccountId>) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::nominate(&pool_account, validators)
	}

	fn chill(pool_id: PoolId) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::chill(&pool_account)
	}

	fn bond(
		who: &T::AccountId,
		pool: PoolId,
		amount: BalanceOf<T>,
		bond_type: BondType,
	) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool);

		match bond_type {
			BondType::Create => {
				// first delegation
				let reward_account = Pallet::<T>::create_reward_account(pool);
				Staking::delegate(who, &pool_account, &reward_account, amount)
			},
			BondType::Later => {
				// additional delegation
				Staking::delegate_extra(who, &pool_account, amount)
			},
		}
	}

	fn unbond(pool_id: PoolId, amount: Staking::Balance) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::unbond(&pool_account, amount)
	}

	fn withdraw_unbonded(pool_id: PoolId, num_slashing_spans: u32) -> Result<bool, DispatchError> {
		let pool_account = Pallet::<T>::create_bonded_account(pool_id);
		Staking::withdraw_unbonded(pool_account, num_slashing_spans)
	}

	fn member_withdraw(who: &T::AccountId, pool: PoolId, amount: BalanceOf<T>) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::withdraw_delegation(&who, &pool_account, amount)
	}

	fn has_pending_slash(pool: PoolId) -> bool {
		// for transfer stake strategy, slashing is greedy
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::has_pending_slash(&pool_account)
	}

	fn member_slash(
		who: &T::AccountId,
		pool: PoolId,
		amount: Staking::Balance,
		maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		let pool_account = Pallet::<T>::create_bonded_account(pool);
		Staking::delegator_slash(&pool_account, who, amount, maybe_reporter)
	}
}

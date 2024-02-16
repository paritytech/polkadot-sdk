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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(rustdoc::broken_intra_doc_links)]

//! An implementation of a delegation system for staking that can be utilised using
//! [`DelegationInterface`]. In future, if exposed via extrinsic, these primitives could also be
//! used by off-chain entities, smart contracts or by other parachains via xcm.
//!
//! Delegatee: Someone who accepts delegations. An account can set their intention to accept
//! delegations by calling [`DelegationInterface::accept_delegations`]. This account cannot have
//! another role in the staking system and once set as delegatee, can only stake with their
//! delegated balance, i.e. cannot use their own free balance to stake. They can also block new
//! delegations by calling [`DelegationInterface::block_delegations`] or remove themselves from
//! being a delegatee by calling [`DelegationInterface::kill_delegatee`] once all delegations to it
//! are removed.
//!
//! Delegatee is also responsible for managing reward distribution and slashes of delegators.
//!
//! Delegator: Someone who delegates their funds to a delegatee. A delegator can delegate their
//! funds to one and only one delegatee. They also can not be a nominator or validator.
//!
//! Reward payouts destination: Delegatees are restricted to have a reward payout destination that
//! is different from the delegatee account. This means, it cannot be auto-compounded and needs to
//! be staked again as a delegation. However, the reward payouts can then be distributed to
//! delegators by the delegatee.
//!
//! Any slashes to a delegatee are recorded in [`DelegationLedger`] of the Delegatee as a pending
//! slash. Since the actual amount is held in the delegator's account, this pallet does not know how
//! to apply slash. It is Delegatee's responsibility to apply slashes for each delegator, one at a
//! time. Staking pallet ensures the pending slash never exceeds staked amount and would freeze
//! further withdraws until pending slashes are applied.

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{
			hold::{Balanced as FunHoldBalanced, Mutate as FunHoldMutate},
			Balanced, Inspect as FunInspect, Mutate as FunMutate,
		},
		tokens::{fungible::Credit, Fortitude, Precision, Preservation},
		DefensiveOption, Imbalance, OnUnbalanced,
	},
	transactional,
};

use sp_runtime::{traits::Zero, DispatchResult, Perbill, RuntimeDebug, Saturating};
use sp_staking::{
	delegation::{DelegationInterface, StakingDelegationSupport},
	EraIndex, Stake, StakerStatus, StakingInterface,
};
use sp_std::{convert::TryInto, prelude::*};

pub type BalanceOf<T> =
	<<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: FunHoldMutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ FunMutate<Self::AccountId>
			+ FunHoldBalanced<Self::AccountId>;

		/// Handler for the unbalanced reduction when slashing a delegator.
		type OnSlash: OnUnbalanced<Credit<Self::AccountId, Self::Currency>>;
		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// Core staking implementation.
		type CoreStaking: StakingInterface<Balance = BalanceOf<Self>, AccountId = Self::AccountId>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account cannot perform this operation.
		NotAllowed,
		/// An existing staker cannot become a delegatee.
		AlreadyStaker,
		/// Reward Destination cannot be delegatee account.
		InvalidRewardDestination,
		/// Delegation conditions are not met.
		///
		/// Possible issues are
		/// 1) Account does not accept or has blocked delegation.
		/// 2) Cannot delegate to self,
		/// 3) Cannot delegate to multiple Delegatees,
		InvalidDelegation,
		/// The account does not have enough funds to perform the operation.
		NotEnoughFunds,
		/// Not an existing delegatee account.
		NotDelegatee,
		/// Not a Delegator account.
		NotDelegator,
		/// Some corruption in internal state.
		BadState,
		/// Unapplied pending slash restricts operation on delegatee.
		UnappliedSlash,
		/// Failed to withdraw amount from Core Staking Ledger.
		WithdrawFailed,
		/// This operation is not supported with Delegation Staking.
		NotSupported,
		/// This delegatee is not set as a migrating account.
		NotMigrating,
		/// Delegatee no longer accepting new delegations.
		DelegationsBlocked,
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for stake delegation to another account.
		#[codec(index = 0)]
		Delegating,
	}

	// #[pallet::genesis_config]
	// #[derive(frame_support::DefaultNoBound)]
	// pub struct GenesisConfig<T: Config> {}
	//
	// #[pallet::genesis_build]
	// impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
	// 	fn build(&self) {}
	// }

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Delegated { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		Withdrawn { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their delegation, i.e. (delegatee, delegation_amount).
	///
	/// Note: We are not using a double map with delegator and delegatee account as keys since we
	/// want to restrict delegators to delegate only to one account.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, (T::AccountId, BalanceOf<T>), OptionQuery>;

	/// Map of Delegatee to their Ledger.
	#[pallet::storage]
	pub(crate) type Delegatees<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, DelegationLedger<T>, OptionQuery>;

	/// Map of Delegatee and its proxy delegator account while its actual delegators are migrating.
	///
	/// Helps ensure correctness of ongoing migration of a direct nominator to a delegatee. If a
	/// delegatee does not exist, it implies it is not going through migration.
	#[pallet::storage]
	pub(crate) type DelegateeMigration<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, T::AccountId, OptionQuery>;
}

/// Register of all delegations to a `Delegatee`.
///
/// This keeps track of the active balance of the delegatee that is made up from the funds that are
/// currently delegated to this delegatee. It also tracks the pending slashes yet to be applied
/// among other things.
#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegationLedger<T: Config> {
	/// Where the reward should be paid out.
	pub payee: T::AccountId,
	/// Sum of all delegated funds to this delegatee.
	#[codec(compact)]
	pub total_delegated: BalanceOf<T>,
	/// Amount that is bonded and held.
	// FIXME(ank4n) (can we remove it)
	#[codec(compact)]
	pub hold: BalanceOf<T>,
	/// Slashes that are not yet applied.
	#[codec(compact)]
	pub pending_slash: BalanceOf<T>,
	/// Whether this delegatee is blocked from receiving new delegations.
	pub blocked: bool,
}

impl<T: Config> DelegationLedger<T> {
	/// balance that can be staked.
	pub fn delegated_balance(&self) -> BalanceOf<T> {
		// do not allow to stake more than unapplied slash
		self.total_delegated.saturating_sub(self.pending_slash)
	}

	/// balance that is delegated but not bonded.
	pub fn unbonded_balance(&self) -> BalanceOf<T> {
		self.total_delegated.saturating_sub(self.hold)
	}
}

impl<T: Config> DelegationInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn delegated_balance(who: &Self::AccountId) -> Self::Balance {
		<Delegatees<T>>::get(who)
			.map_or_else(|| 0u32.into(), |register| register.delegated_balance())
	}

	fn unbonded_balance(who: &Self::AccountId) -> Self::Balance {
		<Delegatees<T>>::get(who)
			.map_or_else(|| 0u32.into(), |register| register.unbonded_balance())
	}

	fn accept_delegations(
		who: &Self::AccountId,
		reward_destination: &Self::AccountId,
	) -> DispatchResult {
		// Existing delegatee cannot register again.
		ensure!(!<Delegatees<T>>::contains_key(who), Error::<T>::NotAllowed);

		// A delegator cannot become a delegatee.
		ensure!(!<Delegators<T>>::contains_key(who), Error::<T>::NotAllowed);

		// payee account cannot be same as delegatee
		ensure!(reward_destination != who, Error::<T>::InvalidRewardDestination);

		// make sure they are not already a direct staker or they are migrating.
		ensure!(
			T::CoreStaking::status(who).is_err() || <DelegateeMigration<T>>::contains_key(who),
			Error::<T>::AlreadyStaker
		);

		// already checked delegatees exist
		<Delegatees<T>>::insert(
			who,
			DelegationLedger {
				payee: reward_destination.clone(),
				total_delegated: Zero::zero(),
				hold: Zero::zero(),
				pending_slash: Zero::zero(),
				blocked: false,
			},
		);

		Ok(())
	}

	/// Transfers funds from current staked account to `proxy_delegator`. Current staked account
	/// becomes a delegatee with `proxy_delegator` delegating stakes to it.
	fn migrate_accept_delegations(
		new_delegatee: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
	) -> DispatchResult {
		ensure!(new_delegatee != proxy_delegator, Error::<T>::InvalidDelegation);

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
		let status = T::CoreStaking::status(new_delegatee)?;
		match status {
			StakerStatus::Nominator(_) => (),
			_ => return Err(Error::<T>::InvalidDelegation.into()),
		}

		<DelegateeMigration<T>>::insert(&new_delegatee, &proxy_delegator);
		let stake = T::CoreStaking::stake(new_delegatee)?;

		// unlock funds from staker
		T::CoreStaking::release_all(new_delegatee);

		// try transferring the staked amount. This should never fail but if it does, it indicates
		// bad state and we abort.
		T::Currency::transfer(
			new_delegatee,
			proxy_delegator,
			stake.total,
			Preservation::Expendable,
		)
		.map_err(|_| Error::<T>::BadState)?;

		// delegate from new delegator to staker.
		// todo(ank4n) : inline this fn and propagate payee to core staking..
		Self::accept_delegations(new_delegatee, payee)?;

		Self::delegate(proxy_delegator, new_delegatee, stake.total)?;
		Self::bond_all(new_delegatee)
	}

	fn block_delegations(delegatee: &Self::AccountId) -> DispatchResult {
		let mut register = <Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		register.blocked = true;
		<Delegatees<T>>::insert(delegatee, register);

		Ok(())
	}

	fn unblock_delegations(delegatee: &Self::AccountId) -> DispatchResult {
		let mut register = <Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		register.blocked = false;
		<Delegatees<T>>::insert(delegatee, register);

		Ok(())
	}

	fn kill_delegatee(_delegatee: &Self::AccountId) -> DispatchResult {
		todo!()
	}

	fn bond_all(who: &Self::AccountId) -> DispatchResult {
		let delegatee = <Delegatees<T>>::get(who).ok_or(Error::<T>::NotDelegatee)?;
		let amount_to_bond = delegatee.unbonded_balance();

		match T::CoreStaking::stake(who) {
			// already bonded
			Ok(_) => T::CoreStaking::bond_extra(who, amount_to_bond),
			// first bond
			Err(_) => T::CoreStaking::bond(who, amount_to_bond, &delegatee.payee),
		}
	}

	#[transactional]
	fn withdraw(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult {
		// check how much is already unbonded
		let delegation_register =
			<Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		let unbonded_balance = delegation_register.unbonded_balance();

		if unbonded_balance < value {
			// fixme(ank4n) handle killing of stash
			let amount_to_withdraw = value.saturating_sub(unbonded_balance);
			let _stash_killed: bool =
				T::CoreStaking::withdraw_exact(delegatee, amount_to_withdraw, num_slashing_spans)
					.map_err(|_| Error::<T>::WithdrawFailed)?;
		}

		Self::delegation_withdraw(delegator, delegatee, value)
	}

	fn apply_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult {
		let mut delegation_register =
			<Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		let (assigned_delegatee, delegate_balance) =
			<Delegators<T>>::get(delegator).ok_or(Error::<T>::NotDelegator)?;

		ensure!(&assigned_delegatee == delegatee, Error::<T>::NotDelegatee);
		ensure!(delegate_balance >= value, Error::<T>::NotEnoughFunds);

		let (mut credit, _missing) =
			T::Currency::slash(&HoldReason::Delegating.into(), &delegator, value);
		let actual_slash = credit.peek();
		// remove the slashed amount
		delegation_register.pending_slash.saturating_reduce(actual_slash);
		<Delegatees<T>>::insert(delegatee, delegation_register);

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
		delegatee: &Self::AccountId,
		new_delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		// make sure new delegator is not an existing delegator or a delegatee
		ensure!(!<Delegatees<T>>::contains_key(new_delegator), Error::<T>::NotAllowed);
		ensure!(!<Delegators<T>>::contains_key(new_delegator), Error::<T>::NotAllowed);
		// ensure we are migrating
		let proxy_delegator =
			<DelegateeMigration<T>>::get(delegatee).ok_or(Error::<T>::NotMigrating)?;
		// proxy delegator must exist
		let (assigned_delegatee, delegate_balance) =
			<Delegators<T>>::get(&proxy_delegator).ok_or(Error::<T>::BadState)?;
		ensure!(assigned_delegatee == *delegatee, Error::<T>::BadState);

		// make sure proxy delegator has enough balance to support this migration.
		ensure!(delegate_balance >= value, Error::<T>::NotEnoughFunds);

		// remove delegation of `value` from `proxy_delegator`.
		let updated_delegate_balance = delegate_balance.saturating_sub(value);

		// if all funds are migrated out of proxy delegator, clean up.
		if updated_delegate_balance == BalanceOf::<T>::zero() {
			<Delegators<T>>::remove(&proxy_delegator);
			<DelegateeMigration<T>>::remove(delegatee);
		} else {
			// else update proxy delegator
			<Delegators<T>>::insert(&proxy_delegator, (delegatee, updated_delegate_balance));
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
		<Delegators<T>>::insert(new_delegator, (delegatee, value));
		// hold the funds again in the new delegator account.
		T::Currency::hold(&HoldReason::Delegating.into(), &new_delegator, value)?;

		Ok(())
	}

	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		let delegator_balance =
			T::Currency::reducible_balance(&delegator, Preservation::Expendable, Fortitude::Polite);
		ensure!(value >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);
		ensure!(delegator_balance >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegatee != delegator, Error::<T>::InvalidDelegation);

		let mut delegation_register =
			<Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		ensure!(!delegation_register.blocked, Error::<T>::DelegationsBlocked);

		// A delegatee cannot delegate.
		if <Delegatees<T>>::contains_key(delegator) {
			return Err(Error::<T>::InvalidDelegation.into())
		}

		let new_delegation_amount = if let Some((current_delegatee, current_delegation)) =
			<Delegators<T>>::get(delegator)
		{
			ensure!(&current_delegatee == delegatee, Error::<T>::InvalidDelegation);
			value.saturating_add(current_delegation)
		} else {
			value
		};

		delegation_register.total_delegated.saturating_accrue(value);

		<Delegators<T>>::insert(delegator, (delegatee, new_delegation_amount));
		<Delegatees<T>>::insert(delegatee, delegation_register);

		T::Currency::hold(&HoldReason::Delegating.into(), &delegator, value)?;

		Self::deposit_event(Event::<T>::Delegated {
			delegatee: delegatee.clone(),
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
		<Delegatees<T>>::get(who)
			.map(|delegatee| delegatee.delegated_balance())
			.unwrap_or_default()
	}

	fn restrict_reward_destination(
		who: &Self::AccountId,
		reward_destination: Option<Self::AccountId>,
	) -> bool {
		let maybe_register = <Delegatees<T>>::get(who);

		if maybe_register.is_none() {
			// no restrictions for non delegatees.
			return false;
		}

		// restrict if reward destination is not set
		if reward_destination.is_none() {
			return true;
		}

		let register = maybe_register.expect("checked above; qed");
		let reward_acc = reward_destination.expect("checked above; qed");

		// restrict if reward account is not what delegatee registered.
		register.payee != reward_acc
	}

	fn is_delegatee(who: &Self::AccountId) -> bool {
		Self::is_delegatee(who)
	}

	fn update_hold(who: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);

		// delegation register should exist since `who` is a delegatee.
		let delegation_register =
			<Delegatees<T>>::get(who).defensive_ok_or(Error::<T>::BadState)?;

		ensure!(delegation_register.total_delegated >= amount, Error::<T>::NotEnoughFunds);
		ensure!(delegation_register.pending_slash <= amount, Error::<T>::UnappliedSlash);
		let updated_register = DelegationLedger { hold: amount, ..delegation_register };
		<Delegatees<T>>::insert(who, updated_register);

		Ok(())
	}

	fn report_slash(who: &Self::AccountId, slash: Self::Balance) {
		<Delegatees<T>>::mutate(who, |maybe_register| match maybe_register {
			Some(register) => register.pending_slash.saturating_accrue(slash),
			None => {
				defensive!("should not be called on non-delegatee");
			},
		});
	}
}

/// StakingInterface implementation with delegation support.
///
/// Only supports Nominators via Delegated Bonds. It is possible for a nominator to migrate to a
/// Delegatee.
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
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
		return T::CoreStaking::stake(who);
	}

	fn total_stake(who: &Self::AccountId) -> Result<Self::Balance, DispatchError> {
		if Self::is_delegatee(who) {
			return T::CoreStaking::total_stake(who);
		}

		if Self::is_delegator(who) {
			let (_, delegation_amount) =
				<Delegators<T>>::get(who).defensive_ok_or(Error::<T>::BadState)?;
			return Ok(delegation_amount)
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
		ensure!(T::CoreStaking::status(who).is_err(), Error::<T>::NotDelegatee);
		let delegation_register = <Delegatees<T>>::get(who).ok_or(Error::<T>::NotDelegatee)?;

		ensure!(delegation_register.unbonded_balance() >= value, Error::<T>::NotEnoughFunds);
		ensure!(delegation_register.payee == *payee, Error::<T>::InvalidRewardDestination);

		T::CoreStaking::bond(who, value, payee)
	}

	fn nominate(who: &Self::AccountId, validators: Vec<Self::AccountId>) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
		return T::CoreStaking::nominate(who, validators);
	}

	fn chill(who: &Self::AccountId) -> DispatchResult {
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
		return T::CoreStaking::chill(who);
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> DispatchResult {
		let delegation_register = <Delegatees<T>>::get(who).ok_or(Error::<T>::NotDelegatee)?;
		ensure!(delegation_register.unbonded_balance() >= extra, Error::<T>::NotEnoughFunds);

		T::CoreStaking::bond_extra(who, extra)
	}

	fn unbond(stash: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let delegation_register = <Delegatees<T>>::get(stash).ok_or(Error::<T>::NotDelegatee)?;
		ensure!(delegation_register.hold >= value, Error::<T>::NotEnoughFunds);

		T::CoreStaking::unbond(stash, value)
	}

	/// Not supported, call [`Delegatee::withdraw`]
	fn withdraw_unbonded(
		_stash: Self::AccountId,
		_num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		// FIXME(ank4n): Support withdrawing to self account.
		defensive_assert!(false, "not supported for delegated impl of staking interface");
		Err(Error::<T>::NotSupported.into())
	}

	/// Not supported, call [`Delegatee::withdraw`]
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
		ensure!(Self::is_delegatee(who), Error::<T>::NotSupported);
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
impl<T: Config> Pallet<T> {
	fn is_delegatee(who: &T::AccountId) -> bool {
		<Delegatees<T>>::contains_key(who)
	}

	fn is_delegator(who: &T::AccountId) -> bool {
		<Delegators<T>>::contains_key(who)
	}

	fn is_migrating(delegatee: &T::AccountId) -> bool {
		<DelegateeMigration<T>>::contains_key(delegatee)
	}

	fn delegation_withdraw(
		delegator: &T::AccountId,
		delegatee: &T::AccountId,
		value: BalanceOf<T>,
	) -> DispatchResult {
		let mut delegation_register =
			<Delegatees<T>>::get(delegatee).ok_or(Error::<T>::NotDelegatee)?;
		ensure!(delegation_register.unbonded_balance() >= value, Error::<T>::BadState);

		delegation_register.total_delegated.saturating_reduce(value);
		<Delegatees<T>>::insert(delegatee, delegation_register);

		let (assigned_delegatee, delegate_balance) =
			<Delegators<T>>::get(delegator).ok_or(Error::<T>::NotDelegator)?;
		// delegator should already be delegating to delegatee
		ensure!(&assigned_delegatee == delegatee, Error::<T>::NotDelegatee);
		ensure!(delegate_balance >= value, Error::<T>::NotEnoughFunds);
		let updated_delegate_balance = delegate_balance.saturating_sub(value);

		// remove delegator if nothing delegated anymore
		if updated_delegate_balance == BalanceOf::<T>::zero() {
			<Delegators<T>>::remove(delegator);
		} else {
			<Delegators<T>>::insert(delegator, (delegatee, updated_delegate_balance));
		}

		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&delegator,
			value,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == value, "hold should have been released fully");
		Self::deposit_event(Event::<T>::Withdrawn {
			delegatee: delegatee.clone(),
			delegator: delegator.clone(),
			amount: value,
		});

		Ok(())
	}
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Ok(())
	}
}

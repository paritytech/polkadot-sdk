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
//! used by off-chain entities, or by foreign multi-locations (via xcm).
//!
//! Delegate: Someone who accepts delegations. An account can set their intention to accept
//! delegations by calling [`DelegationInterface::accept_delegations`]. This account cannot have
//! another role in the staking system and once set as `delegate`, can only stake with their
//! delegated balance, i.e. cannot use their own free balance to stake. They can also block new
//! delegations by calling [`DelegationInterface::block_delegations`] or remove themselves from
//! being a `delegate` by calling [`DelegationInterface::kill_delegate`] once all delegations to it
//! are removed.
//!
//! Delegate is also responsible for managing reward distribution and slashes of delegators.
//!
//! Delegator: Someone who delegates their funds to a `delegate`. A delegator can delegate their
//! funds to one and only one `delegate`. They also can not be a nominator or validator.
//!
//! Reward payouts destination: Rewards cannot be paid out to `delegate` account since these funds
//! are not directly exposed. This implies, rewards cannot be auto-compounded and needs to be staked
//! again after distributing it to delegators.
//!
//! Any slashes to a `delegate` are posted in its [`DelegationLedger`] as a pending slash. Since the
//! actual amount is held in the multiple `delegator` accounts, this pallet has no way to know how
//! to apply slash. It is `delegate`'s responsibility to apply slashes for each delegator, one at a
//! time. Staking pallet ensures the pending slash never exceeds staked amount and would freeze
//! further withdraws until pending slashes are applied.
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

mod types;

use types::*;

// implementation of public traits.
mod impls;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{
			hold::{
				Balanced as FunHoldBalanced, Inspect as FunHoldInspect, Mutate as FunHoldMutate,
			},
			Balanced, Inspect as FunInspect, Mutate as FunMutate,
		},
		tokens::{fungible::Credit, Fortitude, Precision, Preservation},
		Defensive, DefensiveOption, Imbalance, OnUnbalanced,
	},
	transactional,
	weights::Weight,
};

use sp_runtime::{
	traits::{AccountIdConversion, CheckedAdd, CheckedSub, Zero},
	ArithmeticError, DispatchResult, Perbill, RuntimeDebug, Saturating,
};
use sp_staking::{
	delegation::{DelegationInterface, StakingDelegationSupport},
	EraIndex, Stake, StakerStatus, StakingInterface,
};
use sp_std::{convert::TryInto, prelude::*};

pub type BalanceOf<T> =
	<<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

use frame_system::{ensure_signed, pallet_prelude::*, RawOrigin};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Injected identifier for the pallet.
		#[pallet::constant]
		type PalletId: Get<frame_support::PalletId>;

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
		/// An existing staker cannot become a `delegate`.
		AlreadyStaker,
		/// Reward Destination cannot be `delegate` account.
		InvalidRewardDestination,
		/// Delegation conditions are not met.
		///
		/// Possible issues are
		/// 1) Account does not accept or has blocked delegation.
		/// 2) Cannot delegate to self,
		/// 3) Cannot delegate to multiple delegates,
		InvalidDelegation,
		/// The account does not have enough funds to perform the operation.
		NotEnoughFunds,
		/// Not an existing `delegate` account.
		NotDelegate,
		/// Not a Delegator account.
		NotDelegator,
		/// Some corruption in internal state.
		BadState,
		/// Unapplied pending slash restricts operation on `delegate`.
		UnappliedSlash,
		/// Failed to withdraw amount from Core Staking Ledger.
		WithdrawFailed,
		/// This operation is not supported with Delegation Staking.
		NotSupported,
		/// This `delegate` is not set as a migrating account.
		NotMigrating,
		/// Delegate no longer accepting new delegations.
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
		Delegated { delegate: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		Withdrawn { delegate: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their delegation, i.e. (delegate, delegation_amount).
	///
	/// Note: We are not using a double map with delegator and `delegate` account as keys since we
	/// want to restrict delegators to delegate only to one account.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, Delegation<T>, OptionQuery>;

	/// Map of `Delegate` to their Ledger.
	#[pallet::storage]
	pub(crate) type Delegates<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, DelegationLedger<T>, OptionQuery>;

	/// Map of Delegate and its proxy delegator account while its actual delegators are migrating.
	///
	/// Helps ensure correctness of ongoing migration of a direct nominator to a `delegate`. If a
	/// `delegate` does not exist, it implies it is not going through migration.
	#[pallet::storage]
	pub(crate) type DelegateMigration<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, T::AccountId, OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register an account to be a `Delegate`.
		///
		/// `Delegate` accounts accepts delegations from other `delegator`s and stake funds on their
		/// behalf.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn register_as_delegate(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Existing `delegate` cannot register again.
			ensure!(!Self::is_delegate(&who), Error::<T>::NotAllowed);

			// A delegator cannot become a `delegate`.
			ensure!(!Self::is_delegator(&who), Error::<T>::NotAllowed);

			// They cannot be already a direct staker in the staking pallet.
			ensure!(Self::not_direct_staker(&who), Error::<T>::AlreadyStaker);

			// Reward account cannot be same as `delegate` account.
			ensure!(reward_account != who, Error::<T>::InvalidRewardDestination);

			DelegationLedger::<T>::new(&reward_account).save(&who);

			Ok(())
		}

		/// Migrate from a `Nominator` account to `Delegate` account.
		///
		/// Internally transfers minimum balance to a proxy delegator account created for it.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn migrate_to_delegate(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// Ensure is not already a delegate.
			ensure!(!Self::is_delegate(&who), Error::<T>::NotAllowed);

			ensure!(Self::is_direct_nominator(&who), Error::<T>::NotAllowed);

			// Reward account cannot be same as `delegate` account.
			ensure!(reward_account != who, Error::<T>::InvalidRewardDestination);

			Self::do_migrate_to_delegate(&who, &reward_account)
		}

		/// Release delegated amount to delegator.
		///
		/// Tries to withdraw unbonded fund if needed from staking and release amount to delegator.
		///
		/// Only `delegate` account can call this.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn release(
			origin: OriginFor<T>,
			delegator: T::AccountId,
			amount: BalanceOf<T>,
			num_slashing_spans: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_release(&who, &delegator, amount, num_slashing_spans)
		}

		/// Migrate delegated fund.
		///
		/// This moves delegator funds from `pxoxy_delegator` account to `delegator` account.
		///
		/// Only `delegate` account can call this.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn migrate_delegation(
			origin: OriginFor<T>,
			delegator: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let delegate = ensure_signed(origin)?;

			// Ensure they have minimum delegation.
			ensure!(amount >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);

			// Ensure delegator is sane.
			ensure!(!Self::is_delegate(&delegator), Error::<T>::NotAllowed);
			ensure!(!Self::is_delegator(&delegator), Error::<T>::NotAllowed);
			ensure!(Self::not_direct_staker(&delegator), Error::<T>::AlreadyStaker);

			// ensure delegate is sane.
			ensure!(Self::is_delegate(&delegate), Error::<T>::NotDelegate);

			// and has some delegated balance to migrate.
			let proxy_delegator = Self::sub_account(AccountType::ProxyDelegator, delegate);
			let balance_remaining = Self::held_balance_of(&proxy_delegator);
			ensure!(balance_remaining >= amount, Error::<T>::NotEnoughFunds);

			Self::do_migrate_delegation(&proxy_delegator, &delegator, amount)
		}

		/// Delegate funds to a `Delegate` account.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		// FIXME(ank4n): rename to `delegate`
		pub fn delegate_funds(
			origin: OriginFor<T>,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			todo!()
		}

		/// Add funds to an existing delegation.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::default())]
		pub fn delegate_extra(
			origin: OriginFor<T>,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			todo!()
		}
	}
}

impl<T: Config> Pallet<T> {
	pub(crate) fn sub_account(
		account_type: AccountType,
		delegate_account: T::AccountId,
	) -> T::AccountId {
		T::PalletId::get().into_sub_account_truncating((account_type, delegate_account.clone()))
	}

	/// Balance of a delegator that is delegated.
	pub(crate) fn held_balance_of(who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance_on_hold(&HoldReason::Delegating.into(), who)
	}
	fn is_delegate(who: &T::AccountId) -> bool {
		<Delegates<T>>::contains_key(who)
	}

	fn is_delegator(who: &T::AccountId) -> bool {
		<Delegators<T>>::contains_key(who)
	}

	/// Returns true if who is not already staking.
	fn not_direct_staker(who: &T::AccountId) -> bool {
		T::CoreStaking::status(&who).is_err()
	}

	/// Returns true if who is not already staking.
	fn is_direct_nominator(who: &T::AccountId) -> bool {
		T::CoreStaking::status(who)
			.map(|status| matches!(status, StakerStatus::Nominator(_)))
			.unwrap_or(false)
	}

	fn do_migrate_to_delegate(who: &T::AccountId, reward_account: &T::AccountId) -> DispatchResult {
		// We create a proxy delegator that will keep all the delegation funds until funds are
		// transferred to actual delegator.
		let proxy_delegator = Self::sub_account(AccountType::ProxyDelegator, who.clone());

		// Transfer minimum balance to proxy delegator.
		T::Currency::transfer(
			who,
			&proxy_delegator,
			T::Currency::minimum_balance(),
			Preservation::Protect,
		)
		.map_err(|_| Error::<T>::NotEnoughFunds)?;

		// Get current stake
		let stake = T::CoreStaking::stake(who)?;

		// release funds from core staking.
		T::CoreStaking::release_all(who);

		// transferring just released staked amount. This should never fail but if it does, it
		// indicates bad state and we abort.
		T::Currency::transfer(who, &proxy_delegator, stake.total, Preservation::Protect)
			.map_err(|_| Error::<T>::BadState)?;

		DelegationLedger::<T>::new(&reward_account).save(&who);
		// FIXME(ank4n) expose set payee in staking interface.
		// T::CoreStaking::set_payee(who, reward_account)

		Self::do_delegate(&proxy_delegator, who, stake.total)?;
		Self::do_bond(who, stake.total)
	}

	fn do_bond(delegate: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let ledger = <Delegates<T>>::get(delegate).defensive_ok_or(Error::<T>::NotDelegate)?;

		debug_assert!(amount == ledger.unbonded_balance());

		match T::CoreStaking::stake(delegate) {
			// already bonded
			Ok(_) => T::CoreStaking::bond_extra(delegate, amount),
			// first bond
			Err(_) => T::CoreStaking::bond(delegate, amount, &ledger.payee),
		}
	}

	fn do_delegate(
		delegator: &T::AccountId,
		delegate: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let mut ledger = DelegationLedger::<T>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		ensure!(!ledger.blocked, Error::<T>::DelegationsBlocked);

		let new_delegation_amount =
			if let Some(existing_delegation) = Delegation::<T>::get(delegator) {
				ensure!(&existing_delegation.delegate == delegate, Error::<T>::InvalidDelegation);
				existing_delegation
					.amount
					.checked_add(&amount)
					.ok_or(ArithmeticError::Overflow)?
			} else {
				amount
			};

		Delegation::<T>::from(delegate, new_delegation_amount).save(delegator);

		ledger.total_delegated = ledger
			.total_delegated
			.checked_add(&new_delegation_amount)
			.ok_or(ArithmeticError::Overflow)?;
		ledger.save(delegate);

		T::Currency::hold(&HoldReason::Delegating.into(), delegator, amount)?;

		Self::deposit_event(Event::<T>::Delegated {
			delegate: delegate.clone(),
			delegator: delegator.clone(),
			amount,
		});

		Ok(())
	}

	fn do_release(
		who: &T::AccountId,
		delegator: &T::AccountId,
		amount: BalanceOf<T>,
		num_slashing_spans: u32,
	) -> DispatchResult {
		let mut ledger = DelegationLedger::<T>::get(who).ok_or(Error::<T>::NotDelegate)?;
		let mut delegation = Delegation::<T>::get(delegator).ok_or(Error::<T>::NotDelegator)?;

		// make sure delegation to be released is sound.
		ensure!(&delegation.delegate == who, Error::<T>::NotDelegate);
		ensure!(delegation.amount >= amount, Error::<T>::NotEnoughFunds);

		// if we do not already have enough funds to be claimed, try withdraw some more.
		if ledger.unclaimed_withdrawals < amount {
			ledger = Self::withdraw_unbounded(who, num_slashing_spans)?;
		}

		// if we still do not have enough funds to release, abort.
		ensure!(ledger.unclaimed_withdrawals >= amount, Error::<T>::NotEnoughFunds);

		// book keep into ledger
		ledger.total_delegated = ledger
			.total_delegated
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;
		ledger.unclaimed_withdrawals = ledger
			.unclaimed_withdrawals
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;
		ledger.save(who);

		// book keep delegation
		delegation.amount = delegation
			.amount
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;

		// remove delegator if nothing delegated anymore
		if delegation.amount == BalanceOf::<T>::zero() {
			<Delegators<T>>::remove(delegator);
		} else {
			delegation.save(delegator);
		}

		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&delegator,
			amount,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == amount, "hold should have been released fully");

		Self::deposit_event(Event::<T>::Withdrawn {
			delegate: who.clone(),
			delegator: delegator.clone(),
			amount,
		});

		Ok(())
	}

	fn withdraw_unbounded(
		delegate: &T::AccountId,
		num_slashing_spans: u32,
	) -> Result<DelegationLedger<T>, DispatchError> {
		let mut ledger = DelegationLedger::<T>::get(delegate).ok_or(Error::<T>::NotDelegate)?;

		let pre_total = T::CoreStaking::stake(delegate).defensive()?.total;

		// fixme(ank4n) handle killing of stash
		let _stash_killed: bool =
			T::CoreStaking::withdraw_unbonded(delegate.clone(), num_slashing_spans)
				.map_err(|_| Error::<T>::WithdrawFailed)?;

		let post_total = T::CoreStaking::stake(delegate).defensive()?.total;

		let new_withdrawn =
			post_total.checked_sub(&pre_total).defensive_ok_or(Error::<T>::BadState)?;

		ledger.unclaimed_withdrawals = ledger
			.unclaimed_withdrawals
			.checked_add(&new_withdrawn)
			.ok_or(ArithmeticError::Overflow)?;

		ledger.clone().save(delegate);

		Ok(ledger)
	}

	/// Migrates delegation of `amount` from `source` account to `destination` account.
	fn do_migrate_delegation(
		source_delegator: &T::AccountId,
		destination_delegator: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let mut source_delegation =
			Delegators::<T>::get(&source_delegator).defensive_ok_or(Error::<T>::BadState)?;

		// some checks that must have already been checked before.
		ensure!(source_delegation.amount >= amount, Error::<T>::NotEnoughFunds);
		debug_assert!(
			!Self::is_delegator(destination_delegator) && !Self::is_delegate(destination_delegator)
		);

		// update delegations
		Delegation::<T>::from(&source_delegation.delegate, amount)
			.save(destination_delegator);

		source_delegation
			.decrease_delegation(amount)
			.defensive_ok_or(Error::<T>::BadState)?
			.save(source_delegator);

		// FIXME(ank4n): If all funds are migrated from source, it can be cleaned up and ED returned
		// to delegate or alternatively whoever cleans it up. This could be a permission-less
		// extrinsic.

		// release funds from source
		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&source_delegator,
			amount,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == amount, "hold should have been released fully");

		// transfer the released value to `destination_delegator`.
		// Note: The source should have been funded ED in the beginning so it should not be dusted.
		T::Currency::transfer(
			&source_delegator,
			destination_delegator,
			amount,
			Preservation::Preserve,
		)
		.map_err(|_| Error::<T>::BadState)?;

		// hold the funds again in the new delegator account.
		T::Currency::hold(&HoldReason::Delegating.into(), &destination_delegator, amount)?;

		Ok(())
	}
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Ok(())
	}
}

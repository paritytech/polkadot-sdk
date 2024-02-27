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

//! # Delegated Staking Pallet
//!
//! An abstraction over staking pallet to support delegation of funds to a `delegate` account which
//! can use all the delegated funds to it in the staking pallet as if its own fund.
//!
//! NOTE: The pallet exposes extrinsics which are not yet meant to be exposed in the runtime but
//! only to be used by other pallets in the same runtime.
//!
//! ## Goals
//!
//! The direct nominators on Staking pallet does not scale well. NominationPool was created to
//! address this by pooling delegator funds into one account and then staking it. This though had
//! a very important limitation that the funds were moved from delegator account to pool account
//! and hence the delegator lost control over their funds for using it for other purposes such as
//! governance. This pallet aims to solve this by extending the staking pallet to support a new
//! primitive function: delegation of funds to an account for the intent of staking.
//!
//! #### Reward and Slashing
//! This pallet does not enforce any specific strategy for how rewards or slashes are applied. It
//! is upto the `delegate` account to decide how to apply the rewards and slashes.
//!
//! This importantly allows clients of this pallet to build their own strategies for reward/slashes.
//! For example, a `delegate` account can choose to first slash the reward pot before slashing the
//! delegators. Or part of the reward can go to a insurance fund that can be used to cover any
//! potential future slashes. The goal is to eventually allow foreign MultiLocations
//! (smart contracts or pallets on another chain) to build their own pooled staking solutions
//! similar to `NominationPool`.
//!
//! ## Key Terminologies
//! - *Delegate*: An account who accepts delegations from other accounts (called `Delegators`).
//! - *Delegator*: An account who delegates their funds to a `delegate`.
//! - *DelegationLedger*: A data structure that stores important information about the `delegate`
//! 	such as their total delegated stake.
//! - *Delegation*: A data structure that stores the amount of funds delegated to a `delegate` by a
//! 	`delegator`.
//!
//! ## Interface
//!
//! #### Dispatchable Calls
//! The pallet exposes the following [`Call`]s:
//! - `register_as_delegate`: Register an account to be a `Delegate`. Once an account is registered
//! 	as a `Delegate`, for staking operations, only its delegated funds are used. This means it
//! 	cannot use its own free balance to stake.
//! - `migrate_to_delegate`: This allows a `Nominator` account to become a `Delegate` account.
//! 	Explained in more detail in the `Migration` section.
//! - `release`: Release funds to `delegator` from `unclaimed_withdrawals` register of the
//!   `delegate`.
//! - `migrate_delegation`: Migrate delegated funds from one account to another. This is useful for
//!   example, delegators to a pool account which has migrated to be `delegate` to migrate their
//!   funds from pool account back to their own account and delegated to pool as a `delegator`. Once
//!   the funds are migrated, the `delegator` can use the funds for other purposes which allows
//!   usage of held funds in an account, such as governance.
//! - `delegate_funds`: Delegate funds to a `Delegate` account and update the bond to staking.
//!
//! #### [Staking Interface](StakingInterface)
//! This pallet reimplements the staking interface as a wrapper implementation over
//! [Config::CoreStaking] to provide delegation based staking. NominationPool can use this pallet as
//! its Staking provider to support delegation based staking from pool accounts.
//!
//! #### [Staking Delegation Support](StakingDelegationSupport)
//! The pallet implements the staking delegation support trait which staking pallet can use to
//! provide compatibility with this pallet.
//!
//! #### [Pool Adapter](sp_staking::delegation::PoolAdapter)
//! The pallet also implements the pool adapter trait which allows NominationPool to use this pallet
//! to support delegation based staking from pool accounts. This strategy also allows the pool to
//! switch implementations while having minimal changes to its own logic.
//!
//! ## Lazy Slashing
//! One of the reasons why direct nominators on staking pallet cannot scale well is because all
//! nominators are slashed at the same time. This is expensive and needs to be bounded operation.
//!
//! This pallet implements a lazy slashing mechanism. Any slashes to a `delegate` are posted in its
//! `DelegationLedger` as a pending slash. Since the actual amount is held in the multiple
//! `delegator` accounts, this pallet has no way to know how to apply slash. It is `delegate`'s
//! responsibility to apply slashes for each delegator, one at a time. Staking pallet ensures the
//! pending slash never exceeds staked amount and would freeze further withdraws until pending
//! slashes are applied.
//!
//! `NominationPool` can apply slash for all its members by calling
//! [PoolAdapter::delegator_slash](sp_staking::delegation::PoolAdapter::delegator_slash).
//!
//! ## Migration from Nominator to Delegate
//! More details [here](https://hackmd.io/@ak0n/np-delegated-staking-migration).
//!
//! ## Reward Destination Restrictions
//! This pallets set an important restriction of rewards account to be separate from `delegate`
//! account. This is because, `delegate` balance is not what is directly exposed but the funds that
//! are delegated to it. For `delegate` accounts, we have also no way to auto-compound rewards. The
//! rewards need to be paid out to delegators and then delegated again to the `delegate` account.
//!
//! ## Nomination Pool vs Delegation Staking
//! This pallet is not a replacement for Nomination Pool but adds a new primitive over staking
//! pallet that can be used by Nomination Pool to support delegation based staking. It can be
//! thought of as something in middle of Nomination Pool and Staking Pallet. Technically, these
//! changes could be made in one of those pallets as well but that would have meant significant
//! refactoring and high chances of introducing a regression. With this approach, we can keep the
//! existing pallets with minimal changes and introduce a new pallet that can be optionally used by
//! Nomination Pool. This is completely configurable and a runtime can choose whether to use
//! this pallet or not.
//!
//! With that said, following is the main difference between
//! #### Nomination Pool without delegation support
//!  1) transfer fund from delegator to pool account, and
//!  2) stake from pool account as a direct nominator.
//!
//! #### Nomination Pool with delegation support
//!  1) delegate fund from delegator to pool account, and
//!  2) stake from pool account as a `Delegate` account on the staking pallet.
//!
//! The difference being, in the second approach, the delegated funds will be locked in-place in
//! user's account enabling them to participate in use cases that allows use of `held` funds such
//! as participation in governance voting.
//!
//! Nomination pool still does all the heavy lifting around pool administration, reward
//! distribution, lazy slashing and as such, is not meant to be replaced with this pallet.
//!
//! ## Limitations
//! - Rewards can not be auto-compounded.
//! - Slashes are lazy and hence there could be a period of time when an account can use funds for
//! 	operations such as voting in governance even though they should be slashed.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(rustdoc::broken_intra_doc_links)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

mod types;

use types::*;

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
	weights::Weight,
};

use sp_runtime::{
	traits::{AccountIdConversion, CheckedAdd, CheckedSub, Zero},
	ArithmeticError, DispatchResult, Perbill, RuntimeDebug, Saturating,
};
use sp_staking::{
	delegation::StakingDelegationSupport, EraIndex, Stake, StakerStatus, StakingInterface,
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

		/// Currency type.
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
		/// 1) Cannot delegate to self,
		/// 2) Cannot delegate to multiple delegates,
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
		/// Failed to withdraw amount from Core Staking.
		WithdrawFailed,
		/// Operation not supported by this pallet.
		NotSupported,
		/// Account does not accept delegations.
		NotAcceptingDelegations,
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for stake delegation to another account.
		#[codec(index = 0)]
		Delegating,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Funds delegated by a delegator.
		Delegated { delegate: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		/// Funds released to a delegator.
		Released { delegate: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		/// Funds slashed from a delegator.
		Slashed { delegate: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their `Delegation`.
	///
	/// Implementation note: We are not using a double map with `delegator` and `delegate` account
	/// as keys since we want to restrict delegators to delegate only to one account at a time.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, Delegation<T>, OptionQuery>;

	/// Map of `Delegate` to their `DelegationLedger`.
	#[pallet::storage]
	pub(crate) type Delegates<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, DelegationLedger<T>, OptionQuery>;

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

			Self::do_register_delegator(&who, &reward_account);
			Ok(())
		}

		/// Migrate from a `Nominator` account to `Delegate` account.
		///
		/// The origin needs to
		/// - be a `Nominator` with `CoreStaking`,
		/// - not already a `Delegate`,
		/// - have enough funds to transfer existential deposit to a delegator account created for
		///   the migration.
		///
		/// This operation will create a new delegator account for the origin called
		/// `proxy_delegator` and transfer the staked amount to it. The `proxy_delegator` delegates
		/// the funds to the origin making origin a `Delegate` account. The actual `delegator`
		/// accounts of the origin can later migrate their funds using [Call::migrate_delegation] to
		/// claim back their share of delegated funds from `proxy_delegator` to self.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn migrate_to_delegate(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// ensure who is not already a delegate.
			ensure!(!Self::is_delegate(&who), Error::<T>::NotAllowed);

			// and they should already be a nominator in `CoreStaking`.
			ensure!(Self::is_direct_nominator(&who), Error::<T>::NotAllowed);

			// Reward account cannot be same as `delegate` account.
			ensure!(reward_account != who, Error::<T>::InvalidRewardDestination);

			Self::do_migrate_to_delegate(&who, &reward_account)
		}

		/// Release delegated amount to delegator.
		///
		/// This can be called by existing `delegate` accounts.
		///
		/// Tries to withdraw unbonded fund from `CoreStaking` if needed and release amount to
		/// `delegator`.
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
		/// This can be called by migrating `delegate` accounts.
		///
		/// This moves delegator funds from `pxoxy_delegator` account to `delegator` account.
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

			// and has enough delegated balance to migrate.
			let proxy_delegator = Self::sub_account(AccountType::ProxyDelegator, delegate);
			let balance_remaining = Self::held_balance_of(&proxy_delegator);
			ensure!(balance_remaining >= amount, Error::<T>::NotEnoughFunds);

			Self::do_migrate_delegation(&proxy_delegator, &delegator, amount)
		}

		/// Delegate funds to a `Delegate` account and bonds it to [Config::CoreStaking].
		///
		/// If delegation already exists, it increases the delegation by `amount`.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		pub fn delegate_funds(
			origin: OriginFor<T>,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// ensure amount is over minimum to delegate
			ensure!(amount > T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);

			// ensure delegator is sane.
			ensure!(Delegation::<T>::can_delegate(&who, &delegate), Error::<T>::InvalidDelegation);
			ensure!(Self::not_direct_staker(&who), Error::<T>::AlreadyStaker);

			// ensure delegate is sane.
			ensure!(
				DelegationLedger::<T>::can_accept_delegation(&delegate),
				Error::<T>::NotAcceptingDelegations
			);

			let delegator_balance =
				T::Currency::reducible_balance(&who, Preservation::Preserve, Fortitude::Polite);
			ensure!(delegator_balance >= amount, Error::<T>::NotEnoughFunds);

			// add to delegation
			Self::do_delegate(&who, &delegate, amount)?;
			// bond the amount to `CoreStaking`.
			Self::do_bond(&delegate, amount)
		}

		/// Toggle delegate status to start or stop accepting new delegations.
		///
		/// This can only be used by existing delegates. If not a delegate yet, use
		/// [Call::register_as_delegate] first.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::default())]
		pub fn toggle_delegate_status(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let delegate = Delegate::<T>::from(&who)?;
			let should_block = !delegate.ledger.blocked;
			delegate.update_status(should_block).save();

			Ok(())
		}

		/// Apply slash to a delegator account.
		///
		/// `Delegate` accounts with pending slash in their ledger can call this to apply slash to
		/// one of its `delegator` account. Each slash to a delegator account needs to be posted
		/// separately until all pending slash is cleared.
		#[pallet::call_index(6)]
		#[pallet::weight(Weight::default())]
		pub fn apply_slash(
			origin: OriginFor<T>,
			delegator: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_slash(who, delegator, amount, None)
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
			Self::do_try_state()
		}

		fn integrity_test() {}
	}
}

impl<T: Config> Pallet<T> {
	/// Derive a (keyless) pot account from the given delegate account and account type.
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

	/// Returns true if who is registered as a `Delegate`.
	fn is_delegate(who: &T::AccountId) -> bool {
		<Delegates<T>>::contains_key(who)
	}

	/// Returns true if who is delegating to a `Delegate` account.
	fn is_delegator(who: &T::AccountId) -> bool {
		<Delegators<T>>::contains_key(who)
	}

	/// Returns true if who is not already staking on [`Config::CoreStaking`].
	fn not_direct_staker(who: &T::AccountId) -> bool {
		T::CoreStaking::status(&who).is_err()
	}

	/// Returns true if who is a [`StakerStatus::Nominator`] on [`Config::CoreStaking`].
	fn is_direct_nominator(who: &T::AccountId) -> bool {
		T::CoreStaking::status(who)
			.map(|status| matches!(status, StakerStatus::Nominator(_)))
			.unwrap_or(false)
	}

	fn do_register_delegator(who: &T::AccountId, reward_account: &T::AccountId) {
		DelegationLedger::<T>::new(reward_account).save(who);

		// Pool account is a virtual account. Make this account exist.
		// TODO: Someday if we expose these calls in a runtime, we should take a deposit for
		// being a delegator.
		frame_system::Pallet::<T>::inc_providers(who);
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
		T::CoreStaking::unsafe_release_all(who);

		// transferring just released staked amount. This should never fail but if it does, it
		// indicates bad state and we abort.
		T::Currency::transfer(who, &proxy_delegator, stake.total, Preservation::Protect)
			.map_err(|_| Error::<T>::BadState)?;

		Self::do_register_delegator(who, reward_account);
		// FIXME(ank4n) expose set payee in staking interface.
		// T::CoreStaking::set_payee(who, reward_account)

		Self::do_delegate(&proxy_delegator, who, stake.total)
	}

	fn do_bond(delegate_acc: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let delegate = Delegate::<T>::from(delegate_acc)?;

		let available_to_bond = delegate.available_to_bond();
		defensive_assert!(amount == available_to_bond, "not expected value to bond");

		if delegate.is_bonded() {
			T::CoreStaking::bond_extra(&delegate.key, amount)
		} else {
			T::CoreStaking::bond(&delegate.key, amount, &delegate.reward_account())
		}
	}

	fn do_delegate(
		delegator: &T::AccountId,
		delegate: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let mut ledger = DelegationLedger::<T>::get(delegate).ok_or(Error::<T>::NotDelegate)?;
		debug_assert!(!ledger.blocked);

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
		ledger.total_delegated =
			ledger.total_delegated.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
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
		let mut delegate = Delegate::<T>::from(who)?;
		let mut delegation = Delegation::<T>::get(delegator).ok_or(Error::<T>::NotDelegator)?;

		// make sure delegation to be released is sound.
		ensure!(&delegation.delegate == who, Error::<T>::NotDelegate);
		ensure!(delegation.amount >= amount, Error::<T>::NotEnoughFunds);

		// if we do not already have enough funds to be claimed, try withdraw some more.
		if delegate.ledger.unclaimed_withdrawals < amount {
			// get the updated delegate
			delegate = Self::withdraw_unbonded(who, num_slashing_spans)?;
		}

		// if we still do not have enough funds to release, abort.
		ensure!(delegate.ledger.unclaimed_withdrawals >= amount, Error::<T>::NotEnoughFunds);

		// claim withdraw from delegate.
		delegate.remove_unclaimed_withdraw(amount)?.save_or_kill()?;

		// book keep delegation
		delegation.amount = delegation
			.amount
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;

		// remove delegator if nothing delegated anymore
		delegation.save(delegator);


		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			&delegator,
			amount,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == amount, "hold should have been released fully");

		Self::deposit_event(Event::<T>::Released {
			delegate: who.clone(),
			delegator: delegator.clone(),
			amount,
		});

		Ok(())
	}

	fn withdraw_unbonded(
		delegate_acc: &T::AccountId,
		num_slashing_spans: u32,
	) -> Result<Delegate<T>, DispatchError> {
		let delegate = Delegate::<T>::from(delegate_acc)?;
		let pre_total = T::CoreStaking::stake(delegate_acc).defensive()?.total;

		let stash_killed: bool =
			T::CoreStaking::withdraw_unbonded(delegate_acc.clone(), num_slashing_spans)
				.map_err(|_| Error::<T>::WithdrawFailed)?;

		let maybe_post_total = T::CoreStaking::stake(delegate_acc);
		// One of them should be true
		defensive_assert!(
			!(stash_killed && maybe_post_total.is_ok()),
			"something horrible happened while withdrawing"
		);

		let post_total = maybe_post_total.map_or(Zero::zero(), |s| s.total);

		let new_withdrawn =
			pre_total.checked_sub(&post_total).defensive_ok_or(Error::<T>::BadState)?;

		let delegate = delegate.add_unclaimed_withdraw(new_withdrawn)?;

		delegate.clone().save();

		Ok(delegate)
	}

	/// Migrates delegation of `amount` from `source` account to `destination` account.
	fn do_migrate_delegation(
		source_delegator: &T::AccountId,
		destination_delegator: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let source_delegation =
			Delegators::<T>::get(&source_delegator).defensive_ok_or(Error::<T>::BadState)?;

		// some checks that must have already been checked before.
		ensure!(source_delegation.amount >= amount, Error::<T>::NotEnoughFunds);
		debug_assert!(
			!Self::is_delegator(destination_delegator) && !Self::is_delegate(destination_delegator)
		);

		// update delegations
		Delegation::<T>::from(&source_delegation.delegate, amount).save(destination_delegator);

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

		// transfer the released amount to `destination_delegator`.
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

	pub fn do_slash(
		delegate_acc: T::AccountId,
		delegator: T::AccountId,
		amount: BalanceOf<T>,
		maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		let delegate = Delegate::<T>::from(&delegate_acc)?;
		let delegation = <Delegators<T>>::get(&delegator).ok_or(Error::<T>::NotDelegator)?;

		ensure!(delegation.delegate == delegate_acc, Error::<T>::NotDelegate);
		ensure!(delegation.amount >= amount, Error::<T>::NotEnoughFunds);

		let (mut credit, missing) =
			T::Currency::slash(&HoldReason::Delegating.into(), &delegator, amount);

		defensive_assert!(missing.is_zero(), "slash should have been fully applied");

		let actual_slash = credit.peek();

		// remove the applied slashed amount from delegate.
		delegate.remove_slash(actual_slash).save();

		delegation
			.decrease_delegation(actual_slash)
			.ok_or(ArithmeticError::Overflow)?
			.save(&delegator);

		if let Some(reporter) = maybe_reporter {
			let reward_payout: BalanceOf<T> =
				T::CoreStaking::slash_reward_fraction() * actual_slash;
			let (reporter_reward, rest) = credit.split(reward_payout);
			credit = rest;

			// fixme(ank4n): handle error
			let _ = T::Currency::resolve(&reporter, reporter_reward);
		}

		T::OnSlash::on_unbalanced(credit);

		Self::deposit_event(Event::<T>::Slashed { delegate: delegate_acc, delegator, amount });

		Ok(())
	}
}

#[cfg(any(test, feature = "try-runtime"))]
use sp_std::collections::btree_map::BTreeMap;

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// build map to avoid reading storage multiple times.
		let delegation_map = Delegators::<T>::iter().collect::<BTreeMap<_, _>>();
		let ledger_map = Delegates::<T>::iter().collect::<BTreeMap<_, _>>();

		Self::check_delegates(ledger_map.clone())?;
		Self::check_delegators(delegation_map, ledger_map)?;

		Ok(())
	}

	fn check_delegates(
		ledgers: BTreeMap<T::AccountId, DelegationLedger<T>>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		for (delegate, ledger) in ledgers {
			ensure!(
				matches!(
					T::CoreStaking::status(&delegate).expect("delegate should be bonded"),
					StakerStatus::Nominator(_) | StakerStatus::Idle
				),
				"delegate should be bonded and not validator"
			);

			ensure!(
				ledger.stakeable_balance() >=
					T::CoreStaking::total_stake(&delegate)
						.expect("delegate should exist as a nominator"),
				"Cannot stake more than balance"
			);
		}

		Ok(())
	}

	fn check_delegators(
		delegations: BTreeMap<T::AccountId, Delegation<T>>,
		ledger: BTreeMap<T::AccountId, DelegationLedger<T>>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		let mut delegation_aggregation = BTreeMap::<T::AccountId, BalanceOf<T>>::new();
		for (delegator, delegation) in delegations.iter() {
			ensure!(
				T::CoreStaking::status(delegator).is_err(),
				"delegator should not be directly staked"
			);
			ensure!(!Self::is_delegate(delegator), "delegator cannot be delegate");

			delegation_aggregation
				.entry(delegation.delegate.clone())
				.and_modify(|e| *e += delegation.amount)
				.or_insert(delegation.amount);
		}

		for (delegate, total_delegated) in delegation_aggregation {
			ensure!(!Self::is_delegator(&delegate), "delegate cannot be delegator");

			let ledger = ledger.get(&delegate).expect("ledger should exist");
			ensure!(
				ledger.total_delegated == total_delegated,
				"ledger total delegated should match delegations"
			);
		}

		Ok(())
	}
}

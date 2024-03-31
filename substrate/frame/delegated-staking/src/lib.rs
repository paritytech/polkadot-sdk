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
//! This pallet implements [`sp_staking::DelegatedStakeInterface`] that extends [`StakingInterface`]
//! to support delegation of stake. It consumes [`Config::CoreStaking`] to provide primitive staking
//! functions and only implements the delegation features.
//!
//! Currently, it does not expose any dispatchable calls but is written with a vision to expose them
//! in the future such that it can be utilised by any external account, off-chain entity or xcm
//! multi location such as a parachain or a smart contract.
//!
//! ## Key Terminologies
//! - **Agent**: An account who accepts delegations from other accounts and act as an agent on their
//!   behalf for staking these delegated funds. Also, sometimes referred as `Delegatee`.
//! - **Delegator**: An account who delegates their funds to an `agent` and authorises them to use
//!   it for staking.
//! - **DelegateeLedger**: A data structure that holds important information about the `agent` such
//!   as total delegations they have received, any slashes posted to them, etc.
//! - **Delegation**: A data structure that stores the amount of funds delegated to an `agent` by a
//!   `delegator`.
//!
//! ## Goals
//!
//! Direct nomination on the Staking pallet does not scale well. Nominations pools were created to
//! address this by pooling delegator funds into one account and then staking it. This though had
//! a very critical limitation that the funds were moved from delegator account to pool account
//! and hence the delegator lost control over their funds for using it for other purposes such as
//! governance. This pallet aims to solve this by extending the staking pallet to support a new
//! primitive function: delegation of funds to an `agent` with the intent of staking. The agent can
//! then stake the delegated funds to [`Config::CoreStaking`] on behalf of the delegators.
//!
//! #### Reward and Slashing
//! This pallet does not enforce any specific strategy for how rewards or slashes are applied. It
//! is upto the `agent` account to decide how to apply the rewards and slashes.
//!
//! This importantly allows clients of this pallet to build their own strategies for reward/slashes.
//! For example, an `agent` account can choose to first slash the reward pot before slashing the
//! delegators. Or part of the reward can go to an insurance fund that can be used to cover any
//! potential future slashes. The goal is to eventually allow foreign MultiLocations
//! (smart contracts or pallets on another chain) to build their own pooled staking solutions
//! similar to `NominationPools`.

//! ## Core functions
//!
//! - Allow an account to receive delegations. See [`Pallet::register_agent`].
//! - Delegate funds to an `agent` account. See [`Pallet::delegate_to_agent`].
//! - Release delegated funds from an `agent` account to the `delegator`. See
//!   [`Pallet::release_delegation`].
//! - Migrate a `Nominator` account to an `agent` account. See [`Pallet::migrate_to_agent`].
//!   Explained in more detail in the `Migration` section.
//! - Migrate unclaimed delegated funds from `agent` to delegator. When a nominator migrates to an
//! agent, the funds are held in a proxy account. This function allows the delegator to claim their
//! share of the funds from the proxy account. See [`Pallet::claim_delegation`].
//!
//! #### [Staking Interface](StakingInterface)
//! This pallet reimplements the staking interface as a wrapper implementation over
//! [Config::CoreStaking] to provide delegation based staking. Concretely, a pallet like
//! `NominationPools` can switch to this pallet as its Staking provider to support delegation based
//! staking from pool accounts, allowing its members to lock funds in their own account.
//!
//! ## Lazy Slashing
//! One of the reasons why direct nominators on staking pallet cannot scale well is because all
//! nominators are slashed at the same time. This is expensive and needs to be bounded operation.
//!
//! This pallet implements a lazy slashing mechanism. Any slashes to the `agent` are posted in its
//! `DelegateeLedger` as a pending slash. Since the actual amount is held in the multiple
//! `delegator` accounts, this pallet has no way to know how to apply slash. It is the `agent`'s
//! responsibility to apply slashes for each delegator, one at a time. Staking pallet ensures the
//! pending slash never exceeds staked amount and would freeze further withdraws until all pending
//! slashes are cleared.
//!
//! The user of this pallet can apply slash using
//! [DelegatedStakeInterface::delegator_slash](sp_staking::DelegatedStakeInterface::delegator_slash).
//!
//! ## Migration from Nominator to Agent
//! More details [here](https://hackmd.io/@ak0n/np-delegated-staking-migration).
//!
//! ## Nomination Pool vs Delegation Staking
//! This pallet is not a replacement for Nomination Pool but adds a new primitive over staking
//! pallet that can be used by Nomination Pool to support delegation based staking. It can be
//! thought of as layer in between of Nomination Pool and Staking Pallet. Technically, these
//! changes could be made in one of those pallets as well but that would have meant significant
//! refactoring and high chances of introducing a regression. With this approach, we can keep the
//! existing pallets with minimal changes and introduce a new pallet that can be optionally used by
//! Nomination Pool. The vision is to build this in a configurable way such that runtime can choose
//! whether to use this pallet or not.
//!
//! With that said, following is the main difference between
//! #### Nomination Pool without delegation support
//!  1) transfer fund from delegator to pool account, and
//!  2) stake from pool account as a direct nominator.
//!
//! #### Nomination Pool with delegation support
//!  1) delegate fund from delegator to pool account, and
//!  2) stake from pool account as an `Agent` account on the staking pallet.
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
//!   operations such as voting in governance even though they should be slashed.

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
};

use sp_runtime::{
	traits::{AccountIdConversion, CheckedAdd, CheckedSub, Zero},
	ArithmeticError, DispatchResult, Perbill, RuntimeDebug, Saturating,
};
use sp_staking::{EraIndex, Stake, StakerStatus, StakingInterface, StakingUnsafe};
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
		type CoreStaking: StakingUnsafe<Balance = BalanceOf<Self>, AccountId = Self::AccountId>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account cannot perform this operation.
		NotAllowed,
		/// An existing staker cannot perform this action.
		AlreadyStaking,
		/// Reward Destination cannot be same as `Agent` account.
		InvalidRewardDestination,
		/// Delegation conditions are not met.
		///
		/// Possible issues are
		/// 1) Cannot delegate to self,
		/// 2) Cannot delegate to multiple delegates,
		InvalidDelegation,
		/// The account does not have enough funds to perform the operation.
		NotEnoughFunds,
		/// Not an existing `Agent` account.
		NotAgent,
		/// Not a Delegator account.
		NotDelegator,
		/// Some corruption in internal state.
		BadState,
		/// Unapplied pending slash restricts operation on `Agent`.
		UnappliedSlash,
		/// Failed to withdraw amount from Core Staking.
		WithdrawFailed,
		/// Operation not supported by this pallet.
		NotSupported,
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
		Delegated { agent: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		/// Funds released to a delegator.
		Released { agent: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		/// Funds slashed from a delegator.
		Slashed { agent: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their `Delegation`.
	///
	/// Implementation note: We are not using a double map with `delegator` and `agent` account
	/// as keys since we want to restrict delegators to delegate only to one account at a time.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, Delegation<T>, OptionQuery>;

	/// Map of `Agent` to their `Ledger`.
	#[pallet::storage]
	pub(crate) type Delegatees<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, DelegateeLedger<T>, OptionQuery>;

	// This pallet is not currently written with the intention of exposing any calls. But the
	// functions defined in the following impl block should act as a good reference for how the
	// exposed calls would look like when exposed.
	impl<T: Config> Pallet<T> {
		/// Register an account to become a stake `Agent`. Sometimes also called a `Delegatee`.
		///
		/// Delegators can authorize `Agent`s to stake on their behalf by delegating their funds to
		/// them. The `Agent` can then use the delegated funds to stake to [`Config::CoreStaking`].
		///
		/// Implementation note: This function allows any account to become an agent. It is
		/// important though that accounts that call [`StakingUnsafe::virtual_bond`] are keyless
		/// accounts. This is not a problem for now since this is only used by other pallets in the
		/// runtime which use keyless account as agents. If we later want to expose this as a
		/// dispatchable call, we should derive a sub-account from the caller and use that as the
		/// agent account.
		pub fn register_agent(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Existing `agent` cannot register again.
			ensure!(!Self::is_agent(&who), Error::<T>::NotAllowed);

			// A delegator cannot become an `agent`.
			ensure!(!Self::is_delegator(&who), Error::<T>::NotAllowed);

			// They cannot be already a direct staker in the staking pallet.
			ensure!(Self::not_direct_staker(&who), Error::<T>::AlreadyStaking);

			// Reward account cannot be same as `agent` account.
			ensure!(reward_account != who, Error::<T>::InvalidRewardDestination);

			Self::do_register_agent(&who, &reward_account);
			Ok(())
		}

		/// Migrate from a `Nominator` account to `Agent` account.
		///
		/// The origin needs to
		/// - be a `Nominator` with `CoreStaking`,
		/// - not already a `Delegatee`,
		/// - have enough funds to transfer existential deposit to a delegator account created for
		///   the migration.
		///
		/// This function will create a proxy account to the agent called `proxy_delegator` and
		/// transfer the directly staked amount by the agent to it. The `proxy_delegator` delegates
		/// the funds to the origin making origin an `Agent` account. The real `delegator`
		/// accounts of the origin can later migrate their funds using [Self::claim_delegation] to
		/// claim back their share of delegated funds from `proxy_delegator` to self.
		pub fn migrate_to_agent(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// ensure who is not already an agent.
			ensure!(!Self::is_agent(&who), Error::<T>::NotAllowed);

			// and they should already be a nominator in `CoreStaking`.
			ensure!(Self::is_direct_nominator(&who), Error::<T>::NotAllowed);

			// Reward account cannot be same as `agent` account.
			ensure!(reward_account != who, Error::<T>::InvalidRewardDestination);

			Self::do_migrate_to_agent(&who, &reward_account)
		}

		/// Release previously delegated funds by delegator to origin.
		///
		/// Only agents can call this.
		///
		/// Tries to withdraw unbonded funds from `CoreStaking` if needed and release amount to
		/// `delegator`.
		pub fn release_delegation(
			origin: OriginFor<T>,
			delegator: T::AccountId,
			amount: BalanceOf<T>,
			num_slashing_spans: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_release(&who, &delegator, amount, num_slashing_spans)
		}

		/// Claim delegated funds that are held in `proxy_delegator` to the claiming delegator's
		/// account. If successful, the specified funds will be delegated directly from `delegator`
		/// account to the agent.
		///
		/// This can be called by `agent` accounts that were previously a direct `Nominator` with
		/// [`Config::CoreStaking`] and has some remaining unclaimed delegations.
		///
		/// Internally, it moves some delegations from `pxoxy_delegator` account to `delegator`
		/// account and reapplying the holds.
		pub fn claim_delegation(
			origin: OriginFor<T>,
			delegator: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let agent = ensure_signed(origin)?;

			// Ensure they have minimum delegation.
			ensure!(amount >= T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);

			// Ensure delegator is sane.
			ensure!(!Self::is_agent(&delegator), Error::<T>::NotAllowed);
			ensure!(!Self::is_delegator(&delegator), Error::<T>::NotAllowed);
			ensure!(Self::not_direct_staker(&delegator), Error::<T>::AlreadyStaking);

			// ensure delegatee is sane.
			ensure!(Self::is_agent(&agent), Error::<T>::NotAgent);

			// and has enough delegated balance to migrate.
			let proxy_delegator = Self::sub_account(AccountType::ProxyDelegator, agent);
			let balance_remaining = Self::held_balance_of(&proxy_delegator);
			ensure!(balance_remaining >= amount, Error::<T>::NotEnoughFunds);

			Self::do_migrate_delegation(&proxy_delegator, &delegator, amount)
		}

		/// Delegate given `amount` of tokens to an `Agent` account.
		///
		/// If `origin` is the first time delegator, we add them to state. If they are already
		/// delegating, we increase the delegation.
		///
		/// Conditions:
		/// - Delegators cannot delegate to more than one agent.
		/// - The `agent` account should already be registered as such. See [`Self::register_agent`]
		pub fn delegate_to_agent(
			origin: OriginFor<T>,
			agent: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let delegator = ensure_signed(origin)?;

			// ensure amount is over minimum to delegate
			ensure!(amount > T::Currency::minimum_balance(), Error::<T>::NotEnoughFunds);

			// ensure delegator is sane.
			ensure!(
				Delegation::<T>::can_delegate(&delegator, &agent),
				Error::<T>::InvalidDelegation
			);
			ensure!(Self::not_direct_staker(&delegator), Error::<T>::AlreadyStaking);

			// ensure agent is sane.
			ensure!(Self::is_agent(&agent), Error::<T>::NotAgent);

			let delegator_balance = T::Currency::reducible_balance(
				&delegator,
				Preservation::Preserve,
				Fortitude::Polite,
			);
			ensure!(delegator_balance >= amount, Error::<T>::NotEnoughFunds);

			// add to delegation
			Self::do_delegate(&delegator, &agent, amount)?;

			// bond the newly delegated amount to `CoreStaking`.
			Self::do_bond(&agent, amount)
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}

		fn integrity_test() {}
	}
}

impl<T: Config> Pallet<T> {
	/// Derive a (keyless) pot account from the given delegatee account and account type.
	pub(crate) fn sub_account(
		account_type: AccountType,
		delegatee_account: T::AccountId,
	) -> T::AccountId {
		T::PalletId::get().into_sub_account_truncating((account_type, delegatee_account.clone()))
	}

	/// Balance of a delegator that is delegated.
	pub(crate) fn held_balance_of(who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance_on_hold(&HoldReason::Delegating.into(), who)
	}

	/// Returns true if who is registered as a `Delegatee`.
	fn is_agent(who: &T::AccountId) -> bool {
		<Delegatees<T>>::contains_key(who)
	}

	/// Returns true if who is delegating to a `Delegatee` account.
	fn is_delegator(who: &T::AccountId) -> bool {
		<Delegators<T>>::contains_key(who)
	}

	/// Returns true if who is not already staking on [`Config::CoreStaking`].
	fn not_direct_staker(who: &T::AccountId) -> bool {
		T::CoreStaking::status(who).is_err()
	}

	/// Returns true if who is a [`StakerStatus::Nominator`] on [`Config::CoreStaking`].
	fn is_direct_nominator(who: &T::AccountId) -> bool {
		T::CoreStaking::status(who)
			.map(|status| matches!(status, StakerStatus::Nominator(_)))
			.unwrap_or(false)
	}

	fn do_register_agent(who: &T::AccountId, reward_account: &T::AccountId) {
		DelegateeLedger::<T>::new(reward_account).save(who);

		// Delegatee is a virtual account. Make this account exist.
		// TODO: Someday if we expose these calls in a runtime, we should take a deposit for
		// being a delegator.
		frame_system::Pallet::<T>::inc_providers(who);
	}

	fn do_migrate_to_agent(who: &T::AccountId, reward_account: &T::AccountId) -> DispatchResult {
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
		T::CoreStaking::force_release(who);

		// transferring just released staked amount. This should never fail but if it does, it
		// indicates bad state and we abort.
		T::Currency::transfer(who, &proxy_delegator, stake.total, Preservation::Protect)
			.map_err(|_| Error::<T>::BadState)?;

		Self::do_register_agent(who, reward_account);
		T::CoreStaking::update_payee(who, reward_account)?;

		Self::do_delegate(&proxy_delegator, who, stake.total)
	}

	fn do_bond(delegatee_acc: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let delegatee = Delegatee::<T>::from(delegatee_acc)?;

		let available_to_bond = delegatee.available_to_bond();
		defensive_assert!(amount == available_to_bond, "not expected value to bond");

		if delegatee.is_bonded() {
			T::CoreStaking::bond_extra(&delegatee.key, amount)
		} else {
			T::CoreStaking::virtual_bond(&delegatee.key, amount, delegatee.reward_account())
		}
	}

	fn do_delegate(
		delegator: &T::AccountId,
		delegatee: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let mut ledger = DelegateeLedger::<T>::get(delegatee).ok_or(Error::<T>::NotAgent)?;

		let new_delegation_amount =
			if let Some(existing_delegation) = Delegation::<T>::get(delegator) {
				ensure!(&existing_delegation.delegatee == delegatee, Error::<T>::InvalidDelegation);
				existing_delegation
					.amount
					.checked_add(&amount)
					.ok_or(ArithmeticError::Overflow)?
			} else {
				amount
			};

		Delegation::<T>::from(delegatee, new_delegation_amount).save_or_kill(delegator);
		ledger.total_delegated =
			ledger.total_delegated.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
		ledger.save(delegatee);

		T::Currency::hold(&HoldReason::Delegating.into(), delegator, amount)?;

		Self::deposit_event(Event::<T>::Delegated {
			agent: delegatee.clone(),
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
		let mut delegatee = Delegatee::<T>::from(who)?;
		let mut delegation = Delegation::<T>::get(delegator).ok_or(Error::<T>::NotDelegator)?;

		// make sure delegation to be released is sound.
		ensure!(&delegation.delegatee == who, Error::<T>::NotAgent);
		ensure!(delegation.amount >= amount, Error::<T>::NotEnoughFunds);

		// if we do not already have enough funds to be claimed, try withdraw some more.
		if delegatee.ledger.unclaimed_withdrawals < amount {
			// get the updated delegatee
			delegatee = Self::withdraw_unbonded(who, num_slashing_spans)?;
		}

		// if we still do not have enough funds to release, abort.
		ensure!(delegatee.ledger.unclaimed_withdrawals >= amount, Error::<T>::NotEnoughFunds);

		// claim withdraw from delegatee.
		delegatee.remove_unclaimed_withdraw(amount)?.save_or_kill()?;

		// book keep delegation
		delegation.amount = delegation
			.amount
			.checked_sub(&amount)
			.defensive_ok_or(ArithmeticError::Overflow)?;

		// remove delegator if nothing delegated anymore
		delegation.save_or_kill(delegator);

		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			delegator,
			amount,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == amount, "hold should have been released fully");

		Self::deposit_event(Event::<T>::Released {
			agent: who.clone(),
			delegator: delegator.clone(),
			amount,
		});

		Ok(())
	}

	fn withdraw_unbonded(
		delegatee_acc: &T::AccountId,
		num_slashing_spans: u32,
	) -> Result<Delegatee<T>, DispatchError> {
		let delegatee = Delegatee::<T>::from(delegatee_acc)?;
		let pre_total = T::CoreStaking::stake(delegatee_acc).defensive()?.total;

		let stash_killed: bool =
			T::CoreStaking::withdraw_unbonded(delegatee_acc.clone(), num_slashing_spans)
				.map_err(|_| Error::<T>::WithdrawFailed)?;

		let maybe_post_total = T::CoreStaking::stake(delegatee_acc);
		// One of them should be true
		defensive_assert!(
			!(stash_killed && maybe_post_total.is_ok()),
			"something horrible happened while withdrawing"
		);

		let post_total = maybe_post_total.map_or(Zero::zero(), |s| s.total);

		let new_withdrawn =
			pre_total.checked_sub(&post_total).defensive_ok_or(Error::<T>::BadState)?;

		let delegatee = delegatee.add_unclaimed_withdraw(new_withdrawn)?;

		delegatee.clone().save();

		Ok(delegatee)
	}

	/// Migrates delegation of `amount` from `source` account to `destination` account.
	fn do_migrate_delegation(
		source_delegator: &T::AccountId,
		destination_delegator: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let source_delegation =
			Delegators::<T>::get(source_delegator).defensive_ok_or(Error::<T>::BadState)?;

		// some checks that must have already been checked before.
		ensure!(source_delegation.amount >= amount, Error::<T>::NotEnoughFunds);
		debug_assert!(
			!Self::is_delegator(destination_delegator) && !Self::is_agent(destination_delegator)
		);

		// update delegations
		Delegation::<T>::from(&source_delegation.delegatee, amount)
			.save_or_kill(destination_delegator);

		source_delegation
			.decrease_delegation(amount)
			.defensive_ok_or(Error::<T>::BadState)?
			.save_or_kill(source_delegator);

		// release funds from source
		let released = T::Currency::release(
			&HoldReason::Delegating.into(),
			source_delegator,
			amount,
			Precision::BestEffort,
		)?;

		defensive_assert!(released == amount, "hold should have been released fully");

		// transfer the released amount to `destination_delegator`.
		// Note: The source should have been funded ED in the beginning so it should not be dusted.
		T::Currency::transfer(
			source_delegator,
			destination_delegator,
			amount,
			Preservation::Preserve,
		)
		.map_err(|_| Error::<T>::BadState)?;

		// hold the funds again in the new delegator account.
		T::Currency::hold(&HoldReason::Delegating.into(), destination_delegator, amount)?;

		Ok(())
	}

	pub fn do_slash(
		delegatee_acc: T::AccountId,
		delegator: T::AccountId,
		amount: BalanceOf<T>,
		maybe_reporter: Option<T::AccountId>,
	) -> DispatchResult {
		let delegatee = Delegatee::<T>::from(&delegatee_acc)?;
		let delegation = <Delegators<T>>::get(&delegator).ok_or(Error::<T>::NotDelegator)?;

		ensure!(delegation.delegatee == delegatee_acc, Error::<T>::NotAgent);
		ensure!(delegation.amount >= amount, Error::<T>::NotEnoughFunds);

		let (mut credit, missing) =
			T::Currency::slash(&HoldReason::Delegating.into(), &delegator, amount);

		defensive_assert!(missing.is_zero(), "slash should have been fully applied");

		let actual_slash = credit.peek();

		// remove the applied slashed amount from delegatee.
		delegatee.remove_slash(actual_slash).save();

		delegation
			.decrease_delegation(actual_slash)
			.ok_or(ArithmeticError::Overflow)?
			.save_or_kill(&delegator);

		if let Some(reporter) = maybe_reporter {
			let reward_payout: BalanceOf<T> =
				T::CoreStaking::slash_reward_fraction() * actual_slash;
			let (reporter_reward, rest) = credit.split(reward_payout);
			credit = rest;

			// fixme(ank4n): handle error
			let _ = T::Currency::resolve(&reporter, reporter_reward);
		}

		T::OnSlash::on_unbalanced(credit);

		Self::deposit_event(Event::<T>::Slashed { agent: delegatee_acc, delegator, amount });

		Ok(())
	}

	/// Total balance that is available for stake. Includes already staked amount.
	#[cfg(test)]
	pub(crate) fn stakeable_balance(who: &T::AccountId) -> BalanceOf<T> {
		Delegatee::<T>::from(who)
			.map(|delegatee| delegatee.ledger.stakeable_balance())
			.unwrap_or_default()
	}
}

#[cfg(any(test, feature = "try-runtime"))]
use sp_std::collections::btree_map::BTreeMap;

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// build map to avoid reading storage multiple times.
		let delegation_map = Delegators::<T>::iter().collect::<BTreeMap<_, _>>();
		let ledger_map = Delegatees::<T>::iter().collect::<BTreeMap<_, _>>();

		Self::check_delegates(ledger_map.clone())?;
		Self::check_delegators(delegation_map, ledger_map)?;

		Ok(())
	}

	fn check_delegates(
		ledgers: BTreeMap<T::AccountId, DelegateeLedger<T>>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		for (delegatee, ledger) in ledgers {
			ensure!(
				matches!(
					T::CoreStaking::status(&delegatee).expect("delegatee should be bonded"),
					StakerStatus::Nominator(_) | StakerStatus::Idle
				),
				"delegatee should be bonded and not validator"
			);

			ensure!(
				ledger.stakeable_balance() >=
					T::CoreStaking::total_stake(&delegatee)
						.expect("delegatee should exist as a nominator"),
				"Cannot stake more than balance"
			);
		}

		Ok(())
	}

	fn check_delegators(
		delegations: BTreeMap<T::AccountId, Delegation<T>>,
		ledger: BTreeMap<T::AccountId, DelegateeLedger<T>>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		let mut delegation_aggregation = BTreeMap::<T::AccountId, BalanceOf<T>>::new();
		for (delegator, delegation) in delegations.iter() {
			ensure!(
				T::CoreStaking::status(delegator).is_err(),
				"delegator should not be directly staked"
			);
			ensure!(!Self::is_agent(delegator), "delegator cannot be delegatee");

			delegation_aggregation
				.entry(delegation.delegatee.clone())
				.and_modify(|e| *e += delegation.amount)
				.or_insert(delegation.amount);
		}

		for (delegatee, total_delegated) in delegation_aggregation {
			ensure!(!Self::is_delegator(&delegatee), "delegatee cannot be delegator");

			let ledger = ledger.get(&delegatee).expect("ledger should exist");
			ensure!(
				ledger.total_delegated == total_delegated,
				"ledger total delegated should match delegations"
			);
		}

		Ok(())
	}
}

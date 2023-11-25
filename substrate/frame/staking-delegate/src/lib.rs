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

use frame_support::{
	pallet_prelude::*,
	traits::fungible::{hold::Mutate as FunHoldMutate, Inspect as FunInspect},
};
use frame_system::pallet_prelude::*;
use sp_std::{convert::TryInto, prelude::*};
use pallet::*;
use sp_runtime::{traits::Zero, DispatchError, RuntimeDebug, Saturating};
use sp_staking::delegation::{StakeDelegatee, StakeDelegator};

pub type BalanceOf<T> = <<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: FunHoldMutate<
			Self::AccountId,
			Reason = Self::RuntimeHoldReason
		>;
		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account cannot perform this operation.
		NotAllowed,
		/// Delegation conditions are not met.
		///
		/// Possible issues are
		/// 1) Account does not accept or has blocked delegation.
		/// 2) Cannot delegate to self,
		/// 3) Cannot delegate to multiple Delegatees,
		InvalidDelegation,
		/// The account does not have enough funds to perform the operation.
		NotEnoughFunds,
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for stake delegation to another account.
		#[codec(index = 0)]
		Delegating,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Delegated { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
		Withdrawn { delegatee: T::AccountId, delegator: T::AccountId, amount: BalanceOf<T> },
	}

	/// Map of Delegators to their delegation.
	///
	/// Note: We are not using a double map with delegator and delegatee account as keys since we
	/// want to restrict delegators to delegate only to one account.
	#[pallet::storage]
	pub(crate) type Delegators<T: Config> =
	CountedStorageMap<_, Twox64Concat, T::AccountId, (T::AccountId, BalanceOf<T>), OptionQuery>;

	/// Map of Delegatee to their Ledger.
	#[pallet::storage]
	pub(crate) type Delegatees<T: Config> = CountedStorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		DelegationRegister<T>,
		OptionQuery,
	>;
}

impl<T: Config> StakeDelegatee for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;

	fn balance(who: Self::AccountId) -> Self::Balance {
		todo!()
	}

	fn accept_delegations(delegatee: &Self::AccountId, payee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn block_delegations(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn kill_delegatee(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn update_bond(delegatee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn apply_slash(delegatee: &Self::AccountId, delegator: &Self::AccountId, value: Self::Balance, reporter: Option<Self::AccountId>) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn delegatee_migrate(new_delegatee: &Self::AccountId, proxy_delegator: &Self::AccountId, payee: &Self::AccountId) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn delegator_migrate(delegator_from: &Self::AccountId, delegator_to: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}
}

impl<T: Config> StakeDelegator for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;

	fn delegate(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn request_undelegate(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}

	fn withdraw(delegator: &Self::AccountId, delegatee: &Self::AccountId, value: Self::Balance) -> sp_runtime::DispatchResult {
		todo!()
	}
}

/// Register of all delegations to a `Delegatee`.
///
/// This keeps track of the active balance of the delegatee that is made up from the funds that are
/// currently delegated to this delegatee. It also tracks the pending slashes yet to be applied
/// among other things.
#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegationRegister<T: Config> {
	/// Where the reward should be paid out.
	pub payee: T::AccountId,
	/// Sum of all delegated funds to this delegatee.
	#[codec(compact)]
	pub balance: BalanceOf<T>,
	/// Slashes that are not yet applied.
	#[codec(compact)]
	pub pending_slash: BalanceOf<T>,
	/// Whether this delegatee is blocked from receiving new delegations.
	pub blocked: bool,
}

impl<T: Config> DelegationRegister<T> {
	pub fn effective_balance(&self) -> BalanceOf<T> {
		self.balance.saturating_sub(self.pending_slash)
	}
}

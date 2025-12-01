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

//! # Dynamic Allocation Pool (DAP) Pallet
//!
//! Minimal initial implementation: only `FundingSink` (return_funds) is functional.
//! This allows replacing burns in other pallets with returns to DAP buffer.
//!
//! Future phases will add:
//! - `FundingSource` (request_funds) for pulling funds
//! - Issuance curve and minting logic
//! - Distribution rules and scheduling

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate},
		tokens::{FundingSink, FundingSource, Preservation},
		Currency, Imbalance, OnUnbalanced,
	},
	PalletId,
};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// The DAP pallet ID, used to derive the buffer account.
pub const DAP_PALLET_ID: PalletId = PalletId(*b"dap/buff");

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::traits::AccountIdConversion;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency type.
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;
	}

	impl<T: Config> Pallet<T> {
		/// Get the DAP buffer account derived from the pallet ID.
		pub fn buffer_account() -> T::AccountId {
			DAP_PALLET_ID.into_account_truncating()
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Funds returned to DAP buffer.
		FundsReturned { from: T::AccountId, amount: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// FundingSource not yet implemented.
		NotImplemented,
	}
}

/// Implementation of FundingSource - NOT YET IMPLEMENTED.
/// Will panic if called.
pub struct PullFromDap<T>(core::marker::PhantomData<T>);

impl<T: Config> FundingSource<T::AccountId, BalanceOf<T>> for PullFromDap<T> {
	fn request_funds(
		_beneficiary: &T::AccountId,
		_amount: BalanceOf<T>,
	) -> Result<BalanceOf<T>, DispatchError> {
		unimplemented!("PullFromDap::request_funds not yet implemented")
	}
}

/// Implementation of FundingSink that returns funds to DAP buffer.
/// When using this, returned funds are transferred to the buffer account instead of being burned.
pub struct ReturnToDap<T>(core::marker::PhantomData<T>);

impl<T: Config> FundingSink<T::AccountId, BalanceOf<T>> for ReturnToDap<T> {
	fn return_funds(source: &T::AccountId, amount: BalanceOf<T>) -> Result<(), DispatchError> {
		let buffer = Pallet::<T>::buffer_account();

		T::Currency::transfer(source, &buffer, amount, Preservation::Preserve)?;

		Pallet::<T>::deposit_event(Event::FundsReturned { from: source.clone(), amount });

		log::debug!(
			target: LOG_TARGET,
			"Returned {amount:?} from {source:?} to DAP buffer"
		);

		Ok(())
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
/// This is for the `fungible::Balanced` trait as used by staking-async.
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of OnUnbalanced for the fungible::Balanced trait.
/// Use this as `type Slash = SlashToDap<Runtime>` in staking-async config.
pub struct SlashToDap<T>(core::marker::PhantomData<T>);

impl<T: Config> OnUnbalanced<CreditOf<T>> for SlashToDap<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let buffer = Pallet::<T>::buffer_account();
		let numeric_amount = amount.peek();

		// Resolve the imbalance by depositing into the buffer account
		let _ = T::Currency::resolve(&buffer, amount);

		log::debug!(
			target: LOG_TARGET,
			"Deposited slash of {numeric_amount:?} to DAP buffer"
		);
	}
}

/// Implementation of OnUnbalanced for the old Currency trait (still used by treasury).
/// Use this as `type BurnDestination = BurnToDap<Runtime, Balances>` e.g. in treasury config.
pub struct BurnToDap<T, C>(core::marker::PhantomData<(T, C)>);

impl<T, C> OnUnbalanced<C::NegativeImbalance> for BurnToDap<T, C>
where
	T: Config,
	C: Currency<T::AccountId>,
{
	fn on_nonzero_unbalanced(amount: C::NegativeImbalance) {
		let buffer = Pallet::<T>::buffer_account();
		let numeric_amount = amount.peek();

		// Resolve the imbalance by depositing into the buffer account
		C::resolve_creating(&buffer, amount);

		log::debug!(
			target: LOG_TARGET,
			"Deposited burn of {numeric_amount:?} to DAP buffer"
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		assert_noop, assert_ok, derive_impl, sp_runtime::traits::AccountIdConversion,
		traits::tokens::FundingSink,
	};
	use sp_runtime::BuildStorage;

	type Block = frame_system::mocking::MockBlock<Test>;

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Balances: pallet_balances,
			Dap: crate,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountData = pallet_balances::AccountData<u64>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Test {
		type AccountStore = System;
	}

	impl Config for Test {
		type Currency = Balances;
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Test> {
			balances: vec![(1, 100), (2, 200), (3, 300)],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();
		t.into()
	}

	#[test]
	fn buffer_account_is_derived_from_pallet_id() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();
			let expected: u64 = DAP_PALLET_ID.into_account_truncating();
			assert_eq!(buffer, expected);
		});
	}

	#[test]
	fn return_funds_transfers_to_buffer() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let buffer = Dap::buffer_account();

			// Given: account 1 has 100, buffer has 0
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: return 30 from account 1
			assert_ok!(ReturnToDap::<Test>::return_funds(&1, 30));

			// Then: account 1 has 70, buffer has 30
			assert_eq!(Balances::free_balance(1), 70);
			assert_eq!(Balances::free_balance(buffer), 30);
			// ...and an event is emitted
			System::assert_last_event(Event::<Test>::FundsReturned { from: 1, amount: 30 }.into());
		});
	}

	#[test]
	fn return_funds_fails_with_insufficient_balance() {
		new_test_ext().execute_with(|| {
			// Given: account 1 has 100
			assert_eq!(Balances::free_balance(1), 100);

			// When: try to return 150 (more than balance)
			// Then: fails
			assert_noop!(
				ReturnToDap::<Test>::return_funds(&1, 150),
				sp_runtime::TokenError::FundsUnavailable
			);
		});
	}

	#[test]
	#[should_panic(expected = "not yet implemented")]
	fn pull_from_dap_panics() {
		new_test_ext().execute_with(|| {
			let _ = PullFromDap::<Test>::request_funds(&1, 10);
		});
	}
}

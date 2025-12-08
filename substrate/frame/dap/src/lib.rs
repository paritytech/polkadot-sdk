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
		tokens::{Fortitude, FundingSink, FundingSource, Precision, Preservation},
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
		/// Failed to deposit funds to DAP buffer.
		ResolveFailed,
	}
}

/// Implementation of FundingSource - NOT YET IMPLEMENTED.
/// Returns `Error::NotImplemented` if called.
pub struct PullFromDap<T>(core::marker::PhantomData<T>);

impl<T: Config> FundingSource<T::AccountId, BalanceOf<T>> for PullFromDap<T> {
	fn request_funds(
		_beneficiary: &T::AccountId,
		_amount: BalanceOf<T>,
	) -> Result<BalanceOf<T>, DispatchError> {
		Err(Error::<T>::NotImplemented.into())
	}
}

/// Implementation of FundingSink that returns funds to DAP buffer.
/// When using this, returned funds are transferred to the buffer account instead of being burned.
pub struct ReturnToDap<T>(core::marker::PhantomData<T>);

impl<T: Config> FundingSink<T::AccountId, BalanceOf<T>> for ReturnToDap<T> {
	fn return_funds(
		source: &T::AccountId,
		amount: BalanceOf<T>,
		preservation: Preservation,
	) -> Result<(), DispatchError> {
		let buffer = Pallet::<T>::buffer_account();

		// We use withdraw + resolve instead of transfer to avoid the ED requirement for the
		// destination account. This way, we can also avoid the migration on production and the
		// genesis configuration's update for benchmark / tests to ensure the destination
		// account pre-exists.
		// This imbalance-based approach is the same used e.g. for the StakingPot in system
		// parachains.
		let credit = T::Currency::withdraw(
			source,
			amount,
			Precision::Exact,
			preservation,
			Fortitude::Polite,
		)?;

		if let Err(remaining) = T::Currency::resolve(&buffer, credit) {
			let remaining_amount = remaining.peek();
			if !remaining_amount.is_zero() {
				log::error!(
					target: LOG_TARGET,
					"💸 Failed to resolve {remaining_amount:?} to DAP buffer - funds will be burned!"
				);
				return Err(Error::<T>::ResolveFailed.into());
			}
		}

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
		if let Err(remaining) = T::Currency::resolve(&buffer, amount) {
			let remaining_amount = remaining.peek();
			if !remaining_amount.is_zero() {
				log::error!(
					target: LOG_TARGET,
					"💸 Failed to deposit slash to DAP buffer - {remaining_amount:?} will be burned!"
				);
			}
		}

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
		assert_noop, assert_ok, derive_impl,
		sp_runtime::traits::AccountIdConversion,
		traits::{
			fungible::Balanced, tokens::FundingSink, Currency as CurrencyT, ExistenceRequirement,
			OnUnbalanced, WithdrawReasons,
		},
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

	// ===== return_funds tests =====

	#[test]
	fn return_funds_accumulates_from_multiple_sources() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let buffer = Dap::buffer_account();

			// Given: accounts have balances, buffer has 0
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(2), 200);
			assert_eq!(Balances::free_balance(3), 300);
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: return funds from multiple accounts
			assert_ok!(ReturnToDap::<Test>::return_funds(&1, 20, Preservation::Preserve));
			assert_ok!(ReturnToDap::<Test>::return_funds(&2, 50, Preservation::Preserve));
			assert_ok!(ReturnToDap::<Test>::return_funds(&3, 100, Preservation::Preserve));

			// Then: buffer has accumulated all returns (20 + 50 + 100 = 170)
			assert_eq!(Balances::free_balance(buffer), 170);
			assert_eq!(Balances::free_balance(1), 80);
			assert_eq!(Balances::free_balance(2), 150);
			assert_eq!(Balances::free_balance(3), 200);

			// ...and all three events are emitted
			System::assert_has_event(Event::<Test>::FundsReturned { from: 1, amount: 20 }.into());
			System::assert_has_event(Event::<Test>::FundsReturned { from: 2, amount: 50 }.into());
			System::assert_has_event(Event::<Test>::FundsReturned { from: 3, amount: 100 }.into());
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
				ReturnToDap::<Test>::return_funds(&1, 150, Preservation::Preserve),
				sp_runtime::TokenError::FundsUnavailable
			);
		});
	}

	#[test]
	fn return_funds_with_zero_amount_succeeds() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: account 1 has 100, buffer has 0
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: return 0 from account 1
			assert_ok!(ReturnToDap::<Test>::return_funds(&1, 0, Preservation::Preserve));

			// Then: balances unchanged (no-op)
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);
		});
	}

	#[test]
	fn return_funds_with_expendable_allows_full_drain() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let buffer = Dap::buffer_account();

			// Given: account 1 has 100
			assert_eq!(Balances::free_balance(1), 100);

			// When: return full balance with Expendable (allows going to 0)
			assert_ok!(ReturnToDap::<Test>::return_funds(&1, 100, Preservation::Expendable));

			// Then: account 1 is empty, buffer has 100
			assert_eq!(Balances::free_balance(1), 0);
			assert_eq!(Balances::free_balance(buffer), 100);
		});
	}

	#[test]
	fn return_funds_with_preserve_respects_existential_deposit() {
		new_test_ext().execute_with(|| {
			// Given: account 1 has 100, ED is 1 (from TestDefaultConfig)
			assert_eq!(Balances::free_balance(1), 100);

			// When: try to return 100 with Preserve (would go below ED)
			// Then: fails because it would kill the account
			assert_noop!(
				ReturnToDap::<Test>::return_funds(&1, 100, Preservation::Preserve),
				sp_runtime::TokenError::FundsUnavailable
			);

			// But returning 99 works (leaves 1 for ED)
			assert_ok!(ReturnToDap::<Test>::return_funds(&1, 99, Preservation::Preserve));
			assert_eq!(Balances::free_balance(1), 1);
		});
	}

	// ===== SlashToDap tests =====

	#[test]
	fn slash_to_dap_accumulates_multiple_slashes_to_buffer() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: buffer has 0
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: multiple slashes occur via OnUnbalanced (simulating a staking slash)
			let credit1 = <Balances as Balanced<u64>>::issue(30);
			SlashToDap::<Test>::on_unbalanced(credit1);

			let credit2 = <Balances as Balanced<u64>>::issue(20);
			SlashToDap::<Test>::on_unbalanced(credit2);

			let credit3 = <Balances as Balanced<u64>>::issue(50);
			SlashToDap::<Test>::on_unbalanced(credit3);

			// Then: buffer has accumulated all slashes (30 + 20 + 50 = 100)
			assert_eq!(Balances::free_balance(buffer), 100);
		});
	}

	#[test]
	fn slash_to_dap_handles_zero_amount() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: buffer has 0
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: slash with zero amount
			let credit = <Balances as Balanced<u64>>::issue(0);
			SlashToDap::<Test>::on_unbalanced(credit);

			// Then: buffer still has 0 (no-op)
			assert_eq!(Balances::free_balance(buffer), 0);
		});
	}

	// ===== BurnToDap tests =====

	#[test]
	fn burn_to_dap_accumulates_multiple_burns_to_buffer() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: accounts have balances, buffer has 0
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: create multiple negative imbalances (simulating treasury burns) and send to DAP
			let imbalance1 = <Balances as CurrencyT<u64>>::withdraw(
				&1,
				30,
				WithdrawReasons::FEE,
				ExistenceRequirement::KeepAlive,
			)
			.unwrap();
			BurnToDap::<Test, Balances>::on_unbalanced(imbalance1);

			let imbalance2 = <Balances as CurrencyT<u64>>::withdraw(
				&2,
				50,
				WithdrawReasons::FEE,
				ExistenceRequirement::KeepAlive,
			)
			.unwrap();
			BurnToDap::<Test, Balances>::on_unbalanced(imbalance2);

			// Then: buffer has accumulated all burns (30 + 50 = 80)
			assert_eq!(Balances::free_balance(buffer), 80);
			assert_eq!(Balances::free_balance(1), 70);
			assert_eq!(Balances::free_balance(2), 150);
		});
	}

	// ===== request_funds tests =====

	#[test]
	fn pull_from_dap_returns_not_implemented_error() {
		new_test_ext().execute_with(|| {
			// When: request_funds is called
			// Then: returns NotImplemented error
			assert_noop!(PullFromDap::<Test>::request_funds(&1, 10), Error::<Test>::NotImplemented);
		});
	}
}

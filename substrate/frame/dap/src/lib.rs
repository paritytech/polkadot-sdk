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
//! This pallet implements `FundingSink` to collect funds into a buffer account instead of burning
//! them. The buffer account is created via `inc_providers` at genesis or on runtime upgrade,
//! ensuring it can receive any amount including those below ED.
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
		tokens::{Fortitude, FundingSink, Precision, Preservation},
		Currency, Imbalance, OnUnbalanced,
	},
	PalletId,
};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::traits::AccountIdConversion;

	/// The in-code storage version.
	const STORAGE_VERSION: frame_support::traits::StorageVersion =
		frame_support::traits::StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency type.
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the buffer account.
		///
		/// Each runtime should configure a unique ID to avoid collisions if multiple
		/// DAP instances are used.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	impl<T: Config> Pallet<T> {
		/// Get the DAP buffer account
		/// NOTE: We may need more accounts in the future, for instance, to manage the strategic
		/// reserve. We will add them as necessary, generating them with additional seed.
		pub fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Ensure the buffer account exists by incrementing its provider count.
		///
		/// This is called at genesis and on runtime upgrade.
		/// It's idempotent - calling it multiple times is safe.
		pub fn ensure_buffer_account_exists() {
			let buffer = Self::buffer_account();
			if !frame_system::Pallet::<T>::account_exists(&buffer) {
				frame_system::Pallet::<T>::inc_providers(&buffer);
				log::info!(
					target: LOG_TARGET,
					"Created DAP buffer account: {buffer:?}"
				);
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<frame_system::pallet_prelude::BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			// Create the buffer account if it doesn't exist (for chains upgrading to DAP).
			Self::ensure_buffer_account_exists();
			// Weight: 1 read (account_exists) + potentially 1 write (inc_providers)
			T::DbWeight::get().reads_writes(1, 1)
		}
	}

	/// Genesis config for the DAP pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Create the buffer account at genesis so it can receive funds of any amount.
			Pallet::<T>::ensure_buffer_account_exists();
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Funds returned to DAP buffer.
		FundsReturned { from: T::AccountId, amount: BalanceOf<T> },
	}
}

/// Implementation of FundingSink that returns funds to DAP buffer.
/// When using this, returned funds are transferred to the buffer account instead of being burned.
pub struct ReturnToDap<T>(core::marker::PhantomData<T>);

impl<T: Config> FundingSink<T::AccountId, BalanceOf<T>> for ReturnToDap<T> {
	fn return_funds(source: &T::AccountId, amount: BalanceOf<T>, preservation: Preservation) {
		let buffer = Pallet::<T>::buffer_account();

		// Withdraw from source, resolve to buffer, emit event. If withdraw fails, nothing happens.
		// If resolve fails (should never happen - buffer pre-created at genesis or via runtime
		// upgrade), funds are burned.
		T::Currency::withdraw(source, amount, Precision::Exact, preservation, Fortitude::Polite)
			.ok()
			.map(|credit| T::Currency::resolve(&buffer, credit))
			.map(|_| {
				Pallet::<T>::deposit_event(Event::FundsReturned { from: source.clone(), amount })
			});
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
/// This is for the `fungible::Balanced` trait as used by staking-async.
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of OnUnbalanced for the fungible::Balanced trait.
/// Use this as `type Slash = SlashToDap<Runtime>` in staking-async config.
///
/// Note: This handler does NOT emit events because it can be called very frequently
/// (e.g., for every fee-paying transaction via fee splitting).
pub struct SlashToDap<T>(core::marker::PhantomData<T>);

impl<T: Config> OnUnbalanced<CreditOf<T>> for SlashToDap<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let buffer = Pallet::<T>::buffer_account();
		let numeric_amount = amount.peek();

		// The buffer account is created at genesis or on_runtime_upgrade, so resolve should
		// always succeed. If it somehow fails, log the error.
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
///
/// Note: This handler does NOT emit events because it can be called very frequently
/// (e.g., for every fee-paying transaction via fee splitting).
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
		derive_impl, parameter_types,
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

	parameter_types! {
		pub const DapPalletId: PalletId = PalletId(*b"dap/buff");
	}

	impl Config for Test {
		type Currency = Balances;
		type PalletId = DapPalletId;
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Test> {
			balances: vec![(1, 100), (2, 200), (3, 300)],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();
		crate::pallet::GenesisConfig::<Test>::default()
			.assimilate_storage(&mut t)
			.unwrap();
		t.into()
	}

	#[test]
	fn genesis_creates_buffer_account() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();
			// Buffer account should exist after genesis (created via inc_providers)
			assert!(System::account_exists(&buffer));
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
			ReturnToDap::<Test>::return_funds(&1, 20, Preservation::Preserve);
			ReturnToDap::<Test>::return_funds(&2, 50, Preservation::Preserve);
			ReturnToDap::<Test>::return_funds(&3, 100, Preservation::Preserve);

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
	fn return_funds_with_insufficient_balance_is_noop() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: account 1 has 100, buffer has 0
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: try to return 150 (more than balance)
			ReturnToDap::<Test>::return_funds(&1, 150, Preservation::Preserve);

			// Then: balances unchanged (infallible no-op)
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);
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
			ReturnToDap::<Test>::return_funds(&1, 0, Preservation::Preserve);

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
			ReturnToDap::<Test>::return_funds(&1, 100, Preservation::Expendable);

			// Then: account 1 is empty, buffer has 100
			assert_eq!(Balances::free_balance(1), 0);
			assert_eq!(Balances::free_balance(buffer), 100);
		});
	}

	#[test]
	fn return_funds_with_preserve_respects_existential_deposit() {
		new_test_ext().execute_with(|| {
			let buffer = Dap::buffer_account();

			// Given: account 1 has 100, ED is 1 (from TestDefaultConfig)
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);

			// When: try to return 100 with Preserve (would go below ED)
			ReturnToDap::<Test>::return_funds(&1, 100, Preservation::Preserve);

			// Then: balances unchanged (infallible - would have killed account)
			assert_eq!(Balances::free_balance(1), 100);
			assert_eq!(Balances::free_balance(buffer), 0);

			// But returning 99 works (leaves 1 for ED)
			ReturnToDap::<Test>::return_funds(&1, 99, Preservation::Preserve);
			assert_eq!(Balances::free_balance(1), 1);
			assert_eq!(Balances::free_balance(buffer), 99);
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
}

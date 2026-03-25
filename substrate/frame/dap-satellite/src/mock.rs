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

//! Test mock for the DAP Satellite pallet.

use crate::{self as pallet_dap_satellite, Config};
use frame_support::{
	derive_impl, parameter_types,
	sp_runtime::traits::AccountIdConversion,
	traits::{
		fungible::{Balanced, Dust, Inspect, Mutate, Unbalanced},
		tokens::{
			DepositConsequence, Fortitude, Precision, Preservation, Provenance, WithdrawConsequence,
		},
	},
	weights::constants::RocksDbWeight,
	PalletId,
};
use pallet_balances::{NegativeImbalance, PositiveImbalance};
use sp_runtime::{BuildStorage, DispatchError};
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		DapSatellite: pallet_dap_satellite,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	/// Use non-zero DB weights so that weight exhaustion can be tested.
	type DbWeight = RocksDbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type ExistentialDeposit = ExistentialDeposit;
	type DustRemoval = DapSatellite;
}

thread_local! {
	/// Counts successful `MockSendToDap::send` calls.
	pub static SEND_COUNT: RefCell<u32> = RefCell::new(0);
	/// Set to `true` to make `MockSendToDap::send` return an error.
	pub static SEND_FAIL: RefCell<bool> = RefCell::new(false);
	/// Records the amount from the most recent successful `MockSendToDap::send` call.
	pub static LAST_SENT_AMOUNT: RefCell<Option<u64>> = RefCell::new(None);
	/// Set to `true` to make `MockCurrency::burn_from` return an error.
	pub static BURN_FAIL: RefCell<bool> = RefCell::new(false);
}

/// Mock implementation of [`pallet_dap_satellite::SendToDap`].
pub struct MockSendToDap;

impl pallet_dap_satellite::SendToDap<u64> for MockSendToDap {
	type Error = ();

	fn send(amount: u64) -> Result<(), ()> {
		if SEND_FAIL.with(|f| *f.borrow()) {
			return Err(());
		}
		SEND_COUNT.with(|c| *c.borrow_mut() += 1);
		LAST_SENT_AMOUNT.with(|a| *a.borrow_mut() = Some(amount));
		Ok(())
	}
}

/// A thin wrapper around [`Balances`] that allows `burn_from` to be made to fail on demand via
/// the [`BURN_FAIL`] thread-local, without requiring any test hooks in the pallet's own code.
///
/// All operations delegate to [`Balances`]; only `burn_from` is intercepted.
/// See the [`Balanced`] impl below for why `DustRemoval` and other test modules need no changes.
pub struct MockCurrency;

impl Inspect<u64> for MockCurrency {
	type Balance = u64;

	fn total_issuance() -> u64 {
		<Balances as Inspect<u64>>::total_issuance()
	}
	fn minimum_balance() -> u64 {
		<Balances as Inspect<u64>>::minimum_balance()
	}
	fn total_balance(who: &u64) -> u64 {
		<Balances as Inspect<u64>>::total_balance(who)
	}
	fn balance(who: &u64) -> u64 {
		<Balances as Inspect<u64>>::balance(who)
	}
	fn reducible_balance(who: &u64, preservation: Preservation, force: Fortitude) -> u64 {
		<Balances as Inspect<u64>>::reducible_balance(who, preservation, force)
	}
	fn can_deposit(who: &u64, amount: u64, provenance: Provenance) -> DepositConsequence {
		<Balances as Inspect<u64>>::can_deposit(who, amount, provenance)
	}
	fn can_withdraw(who: &u64, amount: u64) -> WithdrawConsequence<u64> {
		<Balances as Inspect<u64>>::can_withdraw(who, amount)
	}
}

impl Unbalanced<u64> for MockCurrency {
	fn handle_dust(dust: Dust<u64, Self>) {
		<Balances as Unbalanced<u64>>::handle_raw_dust(dust.0)
	}
	fn write_balance(who: &u64, amount: u64) -> Result<Option<u64>, DispatchError> {
		<Balances as Unbalanced<u64>>::write_balance(who, amount)
	}
	fn set_total_issuance(amount: u64) {
		<Balances as Unbalanced<u64>>::set_total_issuance(amount)
	}
}

impl Mutate<u64> for MockCurrency {
	fn burn_from(
		who: &u64,
		amount: u64,
		preservation: Preservation,
		precision: Precision,
		force: Fortitude,
	) -> Result<u64, DispatchError> {
		if BURN_FAIL.with(|f| *f.borrow()) {
			return Err(DispatchError::Other("MockCurrency: burn_from failed"));
		}
		<Balances as Mutate<u64>>::burn_from(who, amount, preservation, precision, force)
	}
}

/// By mirroring `pallet_balances::Pallet<Test>`'s own `OnDrop{Debt,Credit}` types,
/// `Credit<u64, MockCurrency>` and `Credit<u64, Balances>` expand to the same concrete type.
/// This means `DustRemoval = DapSatellite` and all existing `<Balances as Balanced<_>>` calls
/// in other test modules continue to work without modification.
impl Balanced<u64> for MockCurrency {
	type OnDropDebt = PositiveImbalance<Test>;
	type OnDropCredit = NegativeImbalance<Test>;
}

parameter_types! {
	pub const DapSatellitePalletId: PalletId = PalletId(*b"dap/satl");
	pub const ExistentialDeposit: u64 = 10;
	/// The transfer period in blocks.
	pub const TransferPeriod: u64 = 5;
	/// The smallest transferable amount (above ED).
	pub const MinTransferAmount: u64 = 10;
}

impl Config for Test {
	type Currency = MockCurrency;
	type PalletId = DapSatellitePalletId;
	type SendToDap = MockSendToDap;
	type TransferPeriod = TransferPeriod;
	type MinTransferAmount = MinTransferAmount;
}

pub fn new_test_ext(fund_satellite: bool) -> sp_io::TestExternalities {
	let mut balances = vec![(1, 100), (2, 200), (3, 300)];

	if fund_satellite {
		let satellite: u64 = DapSatellitePalletId::get().into_account_truncating();
		balances.push((satellite, ExistentialDeposit::get()));
	}

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	t.into()
}

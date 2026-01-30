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

//! Tests for BurnHandler invocation in pallet-balances.
//!
//! These tests verify that the configurable `BurnDestination` handler is properly
//! called when burning tokens via `burn_from`.

use crate::{self as pallet_balances, Config};
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{fungible::Mutate, tokens::BurnHandler, ConstU32},
};
use sp_runtime::BuildStorage;
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
	}
);

// Track BurnHandler calls: amounts
thread_local! {
	pub static BURN_HANDLER_CALLS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
}

/// Mock BurnHandler that records all calls for verification.
pub struct MockBurnHandler;

impl BurnHandler<u64> for MockBurnHandler {
	fn on_burned(amount: u64) {
		BURN_HANDLER_CALLS.with(|c| c.borrow_mut().push(amount));
	}
}

/// Helper to get and clear recorded burn handler calls.
fn take_burn_handler_calls() -> Vec<u64> {
	BURN_HANDLER_CALLS.with(|c| c.take())
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = crate::AccountData<u64>;
}

parameter_types! {
	pub static ExistentialDeposit: u64 = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl Config for Test {
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxReserves = ConstU32<2>;
	type BurnDestination = MockBurnHandler;
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
fn burn_from_invokes_burn_handler() {
	use frame_support::traits::tokens::{Fortitude, Precision, Preservation};

	new_test_ext().execute_with(|| {
		// Given: account 1 has 100 balance, no prior burn handler calls.
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(2), 200);
		assert_eq!(Balances::free_balance(3), 300);
		assert!(take_burn_handler_calls().is_empty());

		let initial_issuance = Balances::total_issuance();

		// When: burn_from is called multiple times from different accounts.
		assert_ok!(<Balances as Mutate<_>>::burn_from(
			&1,
			10,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		));
		assert_ok!(<Balances as Mutate<_>>::burn_from(
			&2,
			20,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		));
		assert_ok!(<Balances as Mutate<_>>::burn_from(
			&3,
			0,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		));

		// Then: BurnHandler was called for each burn with the correct amounts,
		// including for the burn with an amount of 0.
		let calls = take_burn_handler_calls();
		assert_eq!(calls, vec![10, 20, 0]);

		// And: balances were correctly reduced.
		assert_eq!(Balances::free_balance(1), 90);
		assert_eq!(Balances::free_balance(2), 180);
		assert_eq!(Balances::free_balance(3), 300);

		// And: total issuance unchanged (MockBurnHandler doesn't reduce it).
		assert_eq!(Balances::total_issuance(), initial_issuance);
	});
}

#[test]
fn burn_extrinsic_invokes_burn_handler() {
	new_test_ext().execute_with(|| {
		// Given: account 1 has 100 balance, no prior burn handler calls.
		assert_eq!(Balances::free_balance(1), 100);
		assert!(take_burn_handler_calls().is_empty());

		// When: burn extrinsic is called.
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(1).into(), 25, false));

		// Then: BurnHandler was called with correct amount.
		let calls = take_burn_handler_calls();
		assert_eq!(calls, vec![25]);

		// And: balance was reduced.
		assert_eq!(Balances::free_balance(1), 75);
	});
}

#[test]
fn burn_entire_balance_reaps_account() {
	new_test_ext().execute_with(|| {
		// Given: account 1 has 100 balance.
		assert_eq!(Balances::free_balance(1), 100);
		assert!(take_burn_handler_calls().is_empty());

		// When: burn entire balance with keep_alive = false.
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(1).into(), 100, false));

		// Then: BurnHandler was called with entire balance.
		let calls = take_burn_handler_calls();
		assert_eq!(calls, vec![100]);

		// And: account is reaped (balance is zero).
		assert_eq!(Balances::free_balance(1), 0);
	});
}

#[test]
fn burn_below_ed_with_keep_alive_fails() {
	use frame_support::assert_noop;
	use sp_runtime::TokenError;

	new_test_ext().execute_with(|| {
		// Given: account 1 has 100 balance, ED is 1.
		assert_eq!(Balances::free_balance(1), 100);
		assert!(take_burn_handler_calls().is_empty());

		// When: try to burn 100 (entire balance) with keep_alive = true.
		// This should fail because it would leave account below ED.
		assert_noop!(
			Balances::burn(frame_system::RawOrigin::Signed(1).into(), 100, true),
			TokenError::FundsUnavailable
		);

		// Then: BurnHandler was NOT called (burn failed).
		let calls = take_burn_handler_calls();
		assert!(calls.is_empty());

		// And: balance is unchanged.
		assert_eq!(Balances::free_balance(1), 100);
	});
}

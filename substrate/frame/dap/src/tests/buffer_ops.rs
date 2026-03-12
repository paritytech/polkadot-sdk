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

//! Tests for withdraw_buffer and deposit_buffer extrinsics.

use crate::{
	mock::{new_test_ext, Balances, Dap, RuntimeOrigin, System, Test},
	Event,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::fungible::{Inspect, Mutate},
};

type DapPallet = crate::Pallet<Test>;

#[test]
fn withdraw_buffer_works_with_root() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		let buffer = DapPallet::buffer_account();
		let dest: u128 = 42;

		// Fund buffer with extra funds and deactivate them (simulating inflow).
		Balances::mint_into(&buffer, 100).unwrap();
		DapPallet::deactivate_buffer_funds(100);

		let buffer_bal = Balances::free_balance(&buffer);
		let active_before = <Balances as Inspect<_>>::active_issuance();

		// Withdraw 50 from buffer.
		assert_ok!(Dap::withdraw_buffer(RuntimeOrigin::root(), dest, 50));

		// Buffer lost 50, dest gained 50.
		assert_eq!(Balances::free_balance(&buffer), buffer_bal - 50);
		assert_eq!(Balances::free_balance(&dest), 50);

		// Active issuance increased by 50 (reactivated).
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), active_before + 50);

		// Event emitted.
		System::assert_has_event(Event::BufferWithdrawn { dest, amount: 50 }.into());
	});
}

#[test]
fn withdraw_buffer_fails_for_non_root() {
	new_test_ext(true).execute_with(|| {
		assert_noop!(
			Dap::withdraw_buffer(RuntimeOrigin::signed(1), 42, 50),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn withdraw_buffer_fails_if_insufficient_balance() {
	new_test_ext(true).execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Buffer only has ED. Withdrawing more than available (minus ED for Preserve) should fail.
		assert_eq!(Balances::free_balance(&buffer), ed);

		assert_noop!(
			Dap::withdraw_buffer(RuntimeOrigin::root(), 42, ed),
			sp_runtime::TokenError::NotExpendable
		);
	});
}

#[test]
fn deposit_buffer_works_with_root() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		let buffer = DapPallet::buffer_account();
		let source: u128 = 1; // has 100 balance from genesis

		let buffer_bal = Balances::free_balance(&buffer);
		let active_before = <Balances as Inspect<_>>::active_issuance();

		// Deposit 50 from source to buffer.
		assert_ok!(Dap::deposit_buffer(RuntimeOrigin::root(), source, 50));

		// Buffer gained 50, source lost 50.
		assert_eq!(Balances::free_balance(&buffer), buffer_bal + 50);
		assert_eq!(Balances::free_balance(&source), 100 - 50);

		// Active issuance decreased by 50 (deactivated).
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), active_before - 50);

		// Event emitted.
		System::assert_has_event(Event::BufferDeposited { source, amount: 50 }.into());
	});
}

#[test]
fn deposit_buffer_fails_for_non_root() {
	new_test_ext(true).execute_with(|| {
		assert_noop!(
			Dap::deposit_buffer(RuntimeOrigin::signed(1), 1, 50),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn round_trip_deposit_then_withdraw_preserves_active_issuance() {
	new_test_ext(true).execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let account: u128 = 1;

		// Fund buffer so it can handle withdrawals.
		Balances::mint_into(&buffer, 100).unwrap();
		DapPallet::deactivate_buffer_funds(100);

		let active_before = <Balances as Inspect<_>>::active_issuance();

		// Deposit 30 from account to buffer.
		assert_ok!(Dap::deposit_buffer(RuntimeOrigin::root(), account, 30));
		// Active went down by 30.
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), active_before - 30);

		// Withdraw 30 from buffer back to account.
		assert_ok!(Dap::withdraw_buffer(RuntimeOrigin::root(), account, 30));
		// Active restored.
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), active_before);
	});
}

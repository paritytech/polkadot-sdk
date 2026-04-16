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

//! OnUnbalanced tests for the DAP pallet.

use crate::mock::{build_and_execute, set_default_budget_allocation, Balances, Test};
use frame_support::traits::{
	fungible::{Balanced, Inspect},
	tokens::{Fortitude, Precision, Preservation},
	Currency, OnUnbalanced,
};

type DapPallet = crate::Pallet<Test>;
type DapLegacy = crate::DapLegacyAdapter<Test, Balances>;

#[test]
#[should_panic(expected = "Failed to deposit slash to DAP buffer")]
fn on_unbalanced_panics_when_buffer_not_funded_and_deposit_below_ed() {
	build_and_execute(false, || {
		set_default_budget_allocation();

		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer is not funded
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: deposit < ED -> triggers defensive panic
		let credit = <Balances as Balanced<_>>::withdraw(
			&1,
			ed - 1,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit);
	});
}

#[test]
fn on_unbalanced_creates_buffer_when_not_funded_and_deposit_at_least_ed() {
	build_and_execute(false, || {
		set_default_budget_allocation();

		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer is not funded
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: deposit >= ED
		let credit = <Balances as Balanced<_>>::withdraw(
			&1,
			ed,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit);

		// Then: buffer is created and funded
		assert_eq!(Balances::free_balance(buffer), ed);
	});
}

#[test]
fn slash_to_dap_accumulates_multiple_slashes_to_buffer() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		let alice = 1; // slashable user
		let bob = 2; // slashable user
		let charlie = 3; // slashable user

		// Given: buffer has ED, users have balances (alice: 100, bob: 200, charlie: 300)
		assert_eq!(Balances::free_balance(buffer), ed);
		let initial_active = <Balances as Inspect<_>>::active_issuance();
		let initial_total = <Balances as Inspect<_>>::total_issuance();

		// When: multiple slashes occur via OnUnbalanced (simulating staking slashes)
		let credit1 = <Balances as Balanced<_>>::withdraw(
			&alice,
			30,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit1);

		let credit2 = <Balances as Balanced<_>>::withdraw(
			&bob,
			20,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit2);

		let credit3 = <Balances as Balanced<_>>::withdraw(
			&charlie,
			50,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit3);

		// Then: buffer accumulated all slashes
		assert_eq!(Balances::free_balance(&buffer), ed + 100);

		// And: users lost their slashed amounts
		assert_eq!(Balances::free_balance(alice), 100 - 30);
		assert_eq!(Balances::free_balance(bob), 200 - 20);
		assert_eq!(Balances::free_balance(charlie), 300 - 50);

		// And: active issuance decreased by 100 (funds deactivated in DAP buffer)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 100);

		// When: slash with zero amount (no-op)
		let credit = <Balances as Balanced<_>>::issue(0);
		DapPallet::on_unbalanced(credit);

		// Then: buffer unchanged (still ED + 100)
		assert_eq!(Balances::free_balance(&buffer), ed + 100);

		// And: total issuance unchanged (funds moved, not created/destroyed)
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), initial_total);

		// And: active issuance decreased by 100 (funds deactivated in DAP buffer)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 100);
	});
}

#[test]
fn legacy_adapter_redirects_slash_to_buffer() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer has ED, alice has 100
		assert_eq!(Balances::free_balance(buffer), ed);
		let initial_active = <Balances as Inspect<_>>::active_issuance();
		let initial_total = <Balances as Inspect<_>>::total_issuance();

		// When: legacy slash via Currency::slash -> DapLegacyAdapter
		let (imbalance, _) = <Balances as Currency<_>>::slash(&1, 30);
		DapLegacy::on_unbalanced(imbalance);

		// Then: buffer accumulated the slash
		assert_eq!(Balances::free_balance(buffer), ed + 30);

		// And: alice lost the slashed amount
		assert_eq!(Balances::free_balance(1), 100 - 30);

		// And: total issuance unchanged (funds moved, not destroyed)
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), initial_total);

		// And: active issuance decreased by 30 (funds deactivated in DAP buffer)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 30);
	});
}

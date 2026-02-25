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

use crate::mock::{new_test_ext, Balances, Test};
use frame_support::traits::{
	fungible::{Balanced, Inspect},
	tokens::{Fortitude, Precision, Preservation},
	OnUnbalanced,
};

type DapPallet = crate::Pallet<Test>;

#[test]
#[should_panic(expected = "Failed to deposit slash to DAP buffer")]
fn on_unbalanced_panics_when_buffer_not_funded_and_deposit_below_ed() {
	new_test_ext(false).execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer is not funded
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: deposit < ED -> triggers defensive panic
		let credit = <Balances as Balanced<u64>>::withdraw(
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
	new_test_ext(false).execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer is not funded
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: deposit >= ED
		let credit = <Balances as Balanced<u64>>::withdraw(
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
	new_test_ext(true).execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer has ED (funded at genesis)
		assert_eq!(Balances::free_balance(buffer), ed);
		let initial_active = <Balances as Inspect<_>>::active_issuance();

		// When: multiple slashes occur via OnUnbalanced (simulating a staking slash)
		let credit1 = <Balances as Balanced<u64>>::withdraw(
			&1,
			30,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit1);

		let credit2 = <Balances as Balanced<u64>>::withdraw(
			&2,
			20,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit2);

		let credit3 = <Balances as Balanced<u64>>::withdraw(
			&3,
			50,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapPallet::on_unbalanced(credit3);

		// Then: buffer has ED + all slashes (1 + 30 + 20 + 50 = 101)
		assert_eq!(Balances::free_balance(buffer), ed + 100);

		// And: active issuance decreased by 100 (funds deactivated in DAP buffer)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 100);

		// When: slash with zero amount (no-op)
		let credit = <Balances as Balanced<u64>>::issue(0);
		DapPallet::on_unbalanced(credit);

		// Then: buffer unchanged (still ED + 100)
		assert_eq!(Balances::free_balance(buffer), ed + 100);
	});
}

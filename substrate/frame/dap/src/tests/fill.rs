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

//! Fill (FundingSink) tests for the DAP pallet.

use crate::mock::*;
use frame_support::traits::{
	fungible::Inspect,
	tokens::{FundingSink, Preservation},
};

type DapPallet = crate::Pallet<Test>;

#[test]
fn fill_accumulates_from_multiple_sources() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: accounts have balances, buffer has ED (funded at genesis)
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(2), 200);
		assert_eq!(Balances::free_balance(3), 300);
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: fill buffer from multiple accounts
		DapPallet::fill(&1, 20, Preservation::Preserve);
		DapPallet::fill(&2, 50, Preservation::Preserve);
		DapPallet::fill(&3, 100, Preservation::Preserve);

		// Then: buffer has ED + all fills (1 + 20 + 50 + 100 = 171)
		assert_eq!(Balances::free_balance(buffer), ed + 170);
		assert_eq!(Balances::free_balance(1), 80);
		assert_eq!(Balances::free_balance(2), 150);
		assert_eq!(Balances::free_balance(3), 200);

		// When: fill with zero amount (no-op)
		DapPallet::fill(&1, 0, Preservation::Preserve);

		// Then: balances unchanged
		assert_eq!(Balances::free_balance(buffer), ed + 170);
		assert_eq!(Balances::free_balance(1), 80);
	});
}

#[test]
fn fill_with_insufficient_balance_transfers_available() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: account 1 has 100, buffer has ED, ED is 1
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: try to fill 150 (more than balance) with Preserve
		DapPallet::fill(&1, 150, Preservation::Preserve);

		// Then: best-effort transfers 99 (leaving ED of 1)
		assert_eq!(Balances::free_balance(1), 1);
		assert_eq!(Balances::free_balance(buffer), ed + 99);
	});
}

#[test]
fn fill_with_expendable_allows_full_drain() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: account 1 has 100, buffer has ED
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: fill full balance with Expendable (allows going to 0)
		DapPallet::fill(&1, 100, Preservation::Expendable);

		// Then: account 1 is empty, buffer has ED + 100
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(buffer), ed + 100);
	});
}

#[test]
fn fill_with_preserve_respects_existential_deposit() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: account 1 has 100, buffer has ED, ED is 1 (from TestDefaultConfig)
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: try to fill 100 with Preserve (would go below ED)
		DapPallet::fill(&1, 100, Preservation::Preserve);

		// Then: best-effort transfers 99 (leaving ED of 1)
		assert_eq!(Balances::free_balance(1), 1);
		assert_eq!(Balances::free_balance(buffer), ed + 99);
	});
}

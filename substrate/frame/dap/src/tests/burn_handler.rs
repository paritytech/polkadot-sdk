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

//! Tests for DapCurrency wrapper: verifies that burn_from redirects to buffer.

use crate::mock::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		tokens::{Fortitude::Polite, Precision::Exact, Preservation::Expendable},
	},
};

type DapPallet = crate::Pallet<Test>;

// ============================================================================
// Tests for pallet using DapCurrency (burn redirects to buffer)
// ============================================================================

#[test]
fn pallet_burn_via_dap_currency_redirects_to_buffer() {
	new_test_ext(true).execute_with(|| {
		// Given
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();
		let initial_active = Balances::active_issuance();
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: multiple burns via MockBurner pallet (uses DapCurrency)
		assert_ok!(MockBurner::burn(RuntimeOrigin::signed(1), 30));
		assert_ok!(MockBurner::burn(RuntimeOrigin::signed(2), 50));
		assert_ok!(MockBurner::burn(RuntimeOrigin::signed(3), 100));

		// Then: buffer accumulated all burns
		assert_eq!(Balances::free_balance(buffer), ed + 180);
		// And: total issuance unchanged (funds transferred, not destroyed)
		assert_eq!(Balances::total_issuance(), initial_total);
		// And: active issuance decreased (funds deactivated)
		assert_eq!(Balances::active_issuance(), initial_active - 180);
		// And: user balances updated
		assert_eq!(Balances::free_balance(1), 70);
		assert_eq!(Balances::free_balance(2), 150);
		assert_eq!(Balances::free_balance(3), 200);
	});
}

#[test]
fn pallet_burn_via_dap_currency_can_reap_account() {
	new_test_ext(true).execute_with(|| {
		// Given
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();
		assert_eq!(Balances::free_balance(1), 100);

		// When: burn entire balance via MockBurner pallet
		assert_ok!(MockBurner::burn(RuntimeOrigin::signed(1), 100));

		// Then: account reaped
		assert_eq!(Balances::free_balance(1), 0);
		// And: buffer received funds
		assert_eq!(Balances::free_balance(buffer), ed + 100);
		// And: total issuance unchanged
		assert_eq!(Balances::total_issuance(), initial_total);
	});
}

// ============================================================================
// Tests verifying standard Balances::burn_from still reduces total issuance
// ============================================================================

#[test]
fn standard_balances_burn_from_reduces_total_issuance() {
	new_test_ext(true).execute_with(|| {
		// Given
		let initial_total = Balances::total_issuance();
		assert_eq!(Balances::free_balance(1), 100);

		// When: direct Balances::burn_from (not via DapCurrency wrapper)
		let burned = <Balances as Mutate<_>>::burn_from(&1, 50, Expendable, Exact, Polite);
		assert_ok!(burned);
		assert_eq!(burned.unwrap(), 50);

		// Then: total issuance REDUCED (standard burn behavior)
		assert_eq!(Balances::total_issuance(), initial_total - 50);
		// And: user balance decreased
		assert_eq!(Balances::free_balance(1), 50);
	});
}

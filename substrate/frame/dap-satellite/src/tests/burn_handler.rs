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

//! Tests for BurnHandler: both the burn extrinsic and direct Currency::burn_from calls.

use crate::mock::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		tokens::{Fortitude::Polite, Precision::Exact, Preservation::Expendable},
	},
};

type DapSatellitePallet = crate::Pallet<Test>;

// ============================================================================
// Tests for burn extrinsic (user-initiated burns)
// ============================================================================

#[test]
fn burn_extrinsic_preserves_total_issuance_and_accumulates_in_satellite() {
	new_test_ext().execute_with(|| {
		// Given
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();
		let initial_active = Balances::active_issuance();
		assert_eq!(initial_total, 601); // 100 + 200 + 300 + 1 (ED)
		assert_eq!(initial_active, 600); // ED already deactivated
		assert_eq!(Balances::free_balance(satellite), ed);

		// When: multiple burns including full balance burn (reaps account)
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(1).into(), 50, true));
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(2).into(), 50, true));
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(3).into(), 300, false)); // reaps

		// Then: total issuance unchanged, satellite accumulated 400, active decreased by 400
		assert_eq!(Balances::total_issuance(), initial_total);
		assert_eq!(Balances::free_balance(satellite), ed + 400);
		assert_eq!(Balances::active_issuance(), initial_active - 400);

		// And: user balances updated correctly
		assert_eq!(Balances::free_balance(1), 50);
		assert_eq!(Balances::free_balance(2), 150);
		assert_eq!(Balances::free_balance(3), 0); // reaped
	});
}

// ============================================================================
// Tests for direct Currency::burn_from (pallet-initiated burns)
// ============================================================================

#[test]
fn direct_burn_from_credits_satellite_and_preserves_total_issuance() {
	new_test_ext().execute_with(|| {
		// Given
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();
		let initial_active = Balances::active_issuance();
		assert_eq!(Balances::free_balance(satellite), ed);

		// When: Currency::burn_from is called directly (e.g. from another pallet)
		let burned = <Balances as Mutate<_>>::burn_from(&1, 50, Expendable, Exact, Polite);
		assert_ok!(burned);
		assert_eq!(burned.unwrap(), 50);

		// Then: total issuance unchanged
		assert_eq!(Balances::total_issuance(), initial_total);
		// And: user balance decreased
		assert_eq!(Balances::free_balance(1), 50);
		// And: satellite received funds
		assert_eq!(Balances::free_balance(satellite), ed + 50);
		// And: active issuance decreased (funds deactivated)
		assert_eq!(Balances::active_issuance(), initial_active - 50);
	});
}

#[test]
fn direct_burn_from_accumulates_multiple_burns() {
	new_test_ext().execute_with(|| {
		// Given
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();

		// When: multiple burns from different accounts
		assert_ok!(<Balances as Mutate<_>>::burn_from(&1, 30, Expendable, Exact, Polite));
		assert_ok!(<Balances as Mutate<_>>::burn_from(&2, 50, Expendable, Exact, Polite));
		assert_ok!(<Balances as Mutate<_>>::burn_from(&3, 100, Expendable, Exact, Polite));

		// Then: satellite accumulated all burns
		assert_eq!(Balances::free_balance(satellite), ed + 180);
		// And: total issuance unchanged
		assert_eq!(Balances::total_issuance(), initial_total);
		// And: user balances updated
		assert_eq!(Balances::free_balance(1), 70);
		assert_eq!(Balances::free_balance(2), 150);
		assert_eq!(Balances::free_balance(3), 200);
	});
}

#[test]
fn direct_burn_from_can_reap_account() {
	new_test_ext().execute_with(|| {
		// Given
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_total = Balances::total_issuance();
		assert_eq!(Balances::free_balance(1), 100);

		// When: burn entire balance (Expendable allows reaping)
		assert_ok!(<Balances as Mutate<_>>::burn_from(&1, 100, Expendable, Exact, Polite));

		// Then: account reaped
		assert_eq!(Balances::free_balance(1), 0);
		// And: satellite received funds
		assert_eq!(Balances::free_balance(satellite), ed + 100);
		// And: total issuance unchanged
		assert_eq!(Balances::total_issuance(), initial_total);
	});
}

#[test]
fn direct_burn_from_respects_preservation() {
	use frame_support::{assert_noop, traits::tokens::Preservation::Preserve};
	use sp_runtime::TokenError;

	new_test_ext().execute_with(|| {
		// Given: user has 100
		assert_eq!(Balances::free_balance(1), 100);

		// When: try to burn all with Preserve (should fail to keep account alive)
		let result = <Balances as Mutate<_>>::burn_from(&1, 100, Preserve, Exact, Polite);

		// Then: fails because it would kill the account
		assert_noop!(result, TokenError::FundsUnavailable);

		// And: can burn amount that keeps account alive
		let ed = <Balances as Inspect<_>>::minimum_balance();
		assert_ok!(<Balances as Mutate<_>>::burn_from(&1, 100 - ed, Preserve, Exact, Polite));
		assert_eq!(Balances::free_balance(1), ed);
	});
}

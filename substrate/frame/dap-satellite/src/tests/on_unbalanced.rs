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

//! OnUnbalanced tests for the DAP Satellite pallet.

use crate::mock::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Balanced, Inspect, Mutate},
		tokens::{Fortitude, Precision, Preservation},
		OnUnbalanced,
	},
};

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn on_unbalanced_deposits_to_satellite() {
	new_test_ext(true).execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite has ED, users have balances (1: 100, 2: 200, 3: 300)
		assert_eq!(Balances::free_balance(satellite), ed);
		let initial_total = <Balances as Inspect<_>>::total_issuance();
		let initial_active = <Balances as Inspect<_>>::active_issuance();

		// When: multiple imbalances are deposited (e.g., coretime revenue from user payments)
		// withdraw() takes funds from an account and returns a Credit
		let credit1 = <Balances as Balanced<u64>>::withdraw(
			&1,
			30,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapSatellitePallet::on_unbalanced(credit1);

		let credit2 = <Balances as Balanced<u64>>::withdraw(
			&2,
			20,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapSatellitePallet::on_unbalanced(credit2);

		let credit3 = <Balances as Balanced<u64>>::withdraw(
			&3,
			50,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapSatellitePallet::on_unbalanced(credit3);

		// Then: satellite has accumulated all credits
		assert_eq!(Balances::free_balance(satellite), ed + 100);

		// And: users lost their amounts
		assert_eq!(Balances::free_balance(1), 100 - 30);
		assert_eq!(Balances::free_balance(2), 200 - 20);
		assert_eq!(Balances::free_balance(3), 300 - 50);

		// And: total issuance unchanged (funds moved, not created/destroyed)
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), initial_total);

		// And: active issuance unchanged (satellite chains don't deactivate)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active);
	});
}

#[test]
fn on_unbalanced_handles_zero_amount() {
	new_test_ext(true).execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let initial_active = <Balances as Inspect<_>>::active_issuance();

		// Given: satellite has ED
		assert_eq!(Balances::free_balance(satellite), ed);

		// When: imbalance with zero amount
		let credit = <Balances as Balanced<u64>>::issue(0);
		DapSatellitePallet::on_unbalanced(credit);

		// Then: satellite still has just ED (no-op)
		assert_eq!(Balances::free_balance(satellite), ed);
		// And: active issuance unchanged
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active);
	});
}

#[test]
#[should_panic(expected = "Failed to deposit to DAP satellite")]
fn on_unbalanced_panics_when_satellite_not_funded_and_deposit_below_ed() {
	new_test_ext(false).execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite is not funded
		assert_eq!(Balances::free_balance(satellite), 0);

		// When: deposit < ED -> triggers defensive panic
		let credit = <Balances as Balanced<u64>>::withdraw(
			&1,
			ed - 1,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapSatellitePallet::on_unbalanced(credit);
	});
}

#[test]
fn on_unbalanced_creates_satellite_when_not_funded_and_deposit_at_least_ed() {
	new_test_ext(false).execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite is not funded
		assert_eq!(Balances::free_balance(satellite), 0);

		// When: deposit >= ED
		let credit = <Balances as Balanced<u64>>::withdraw(
			&1,
			ed,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Force,
		)
		.unwrap();
		DapSatellitePallet::on_unbalanced(credit);

		// Then: satellite is created and funded
		assert_eq!(Balances::free_balance(satellite), ed);
	});
}

#[test]
fn on_unbalanced_multiple_dust_removals_accumulate_to_satellite() {
	new_test_ext(true).execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();
		let dust = ed / 2;

		// Given: satellite has ED. Create 3 accounts with ED + dust each.
		for acct in 10..=12u64 {
			assert_ok!(<Balances as Mutate<_>>::mint_into(&acct, ed + dust));
		}
		let satellite_before = Balances::free_balance(satellite);
		let issuance_before = <Balances as Inspect<_>>::total_issuance();

		// When: each account transfers ED away, leaving dust < ED → reaped.
		// DustRemoval = DapSatellite → dust goes to satellite.
		for acct in 10..=12u64 {
			assert_ok!(Balances::transfer_allow_death(
				frame_system::RawOrigin::Signed(acct).into(),
				1,
				ed,
			));
			assert_eq!(Balances::free_balance(acct), 0);
		}

		// Then: satellite accumulated dust from all 3 reaps.
		assert_eq!(Balances::free_balance(satellite), satellite_before + 3 * dust);

		// And: total issuance unchanged (dust moved, not destroyed).
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), issuance_before);
	});
}

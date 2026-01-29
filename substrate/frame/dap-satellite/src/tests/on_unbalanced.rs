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
use frame_support::traits::{
	fungible::{Balanced, Inspect},
	tokens::{Fortitude, Precision, Preservation},
	OnUnbalanced,
};

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn on_unbalanced_deposits_to_satellite() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite has ED, users have balances (1: 100, 2: 200, 3: 300)
		assert_eq!(Balances::free_balance(satellite), ed);
		let initial_active = <Balances as Inspect<_>>::active_issuance();
		let initial_total = <Balances as Inspect<_>>::total_issuance();

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

		// And: active issuance decreased by 100 (funds deactivated in satellite)
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 100);
	});
}

#[test]
fn on_unbalanced_handles_zero_amount() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(Balances::free_balance(satellite), 1);
		let initial_active = <Balances as Inspect<_>>::active_issuance();

		// When: imbalance with zero amount
		let credit = <Balances as Balanced<u64>>::issue(0);
		DapSatellitePallet::on_unbalanced(credit);

		// Then: satellite still has just ED (no-op)
		assert_eq!(Balances::free_balance(satellite), 1);
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active);
	});
}

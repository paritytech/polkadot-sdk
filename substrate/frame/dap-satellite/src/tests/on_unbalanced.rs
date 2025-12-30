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
use frame_support::traits::{fungible::Balanced, OnUnbalanced};

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn on_unbalanced_deposits_to_satellite() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(Balances::free_balance(satellite), 1);

		// When: multiple imbalances are deposited (e.g., coretime revenue)
		let credit1 = <Balances as Balanced<u64>>::issue(30);
		DapSatellitePallet::on_unbalanced(credit1);

		let credit2 = <Balances as Balanced<u64>>::issue(20);
		DapSatellitePallet::on_unbalanced(credit2);

		let credit3 = <Balances as Balanced<u64>>::issue(50);
		DapSatellitePallet::on_unbalanced(credit3);

		// Then: satellite has accumulated all credits (ED + 30 + 20 + 50 = 101)
		assert_eq!(Balances::free_balance(satellite), 101);
	});
}

#[test]
fn on_unbalanced_handles_zero_amount() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(Balances::free_balance(satellite), 1);

		// When: imbalance with zero amount
		let credit = <Balances as Balanced<u64>>::issue(0);
		DapSatellitePallet::on_unbalanced(credit);

		// Then: satellite still has just ED (no-op)
		assert_eq!(Balances::free_balance(satellite), 1);
	});
}

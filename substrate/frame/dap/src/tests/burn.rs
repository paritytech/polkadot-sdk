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

//! Tests for burn flow with DAP: total issuance preserved, funds go to buffer.

use crate::mock::*;
use frame_support::{assert_ok, traits::fungible::Inspect};

type DapPallet = crate::Pallet<Test>;

#[test]
fn burn_preserves_total_issuance_and_accumulates_in_buffer() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: users have funds, buffer has ED (deactivated at genesis)
		let initial_total = Balances::total_issuance();
		let initial_active = Balances::active_issuance();
		assert_eq!(initial_total, 601); // 100 + 200 + 300 + 1 (ED)
		assert_eq!(initial_active, 600); // ED already deactivated
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: multiple burns including full balance burn (reaps account)
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(1).into(), 50, true));
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(2).into(), 50, true));
		assert_ok!(Balances::burn(frame_system::RawOrigin::Signed(3).into(), 300, false)); // reaps

		// Then: total issuance unchanged, buffer accumulated 400, active decreased by 400
		assert_eq!(Balances::total_issuance(), initial_total);
		assert_eq!(Balances::free_balance(buffer), ed + 400);
		assert_eq!(Balances::active_issuance(), initial_active - 400);

		// And: user balances updated correctly
		assert_eq!(Balances::free_balance(1), 50); // 100 - 50
		assert_eq!(Balances::free_balance(2), 150); // 200 - 50
		assert_eq!(Balances::free_balance(3), 0); // reaped
	});
}

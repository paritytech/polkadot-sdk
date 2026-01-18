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

use crate::mock::*;
use frame_support::traits::{
	fungible::{Balanced, Inspect},
	OnUnbalanced,
};

type DapPallet = crate::Pallet<Test>;

#[test]
fn slash_to_dap_accumulates_multiple_slashes_to_buffer() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer has ED (funded at genesis)
		assert_eq!(Balances::free_balance(&buffer), ed);

		// When: multiple slashes occur via OnUnbalanced (simulating a staking slash)
		let credit1 = Balances::issue(30);
		DapPallet::on_unbalanced(credit1);

		let credit2 = Balances::issue(20);
		DapPallet::on_unbalanced(credit2);

		let credit3 = Balances::issue(50);
		DapPallet::on_unbalanced(credit3);

		// Then: buffer has ED + all slashes (1 + 30 + 20 + 50 = 101)
		assert_eq!(Balances::free_balance(&buffer), ed + 100);

		// When: slash with zero amount (no-op)
		let credit = Balances::issue(0);
		DapPallet::on_unbalanced(credit);

		// Then: buffer unchanged (still ED + 100)
		assert_eq!(Balances::free_balance(&buffer), ed + 100);
	});
}

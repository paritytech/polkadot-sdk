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

//! DealWithFeesSplit tests for the DAP Satellite pallet.

use crate::{mock::*, CreditOf, DealWithFeesSplit};
use frame_support::{parameter_types, traits::OnUnbalanced};
use pallet_balances::Pallet as BalancesPallet;
use std::cell::Cell;

type DapSatellitePallet = crate::Pallet<Test>;

// Thread-local storage for tracking what OtherHandler receives
thread_local! {
	static OTHER_HANDLER_RECEIVED: Cell<u64> = const { Cell::new(0) };
}

/// Mock handler that tracks how much it receives
struct MockOtherHandler;
impl OnUnbalanced<CreditOf<Test>> for MockOtherHandler {
	fn on_unbalanced(amount: CreditOf<Test>) {
		use frame_support::traits::Imbalance;
		OTHER_HANDLER_RECEIVED.with(|r| r.set(r.get() + amount.peek()));
		// Drop the credit (it would normally be handled by the real handler)
		drop(amount);
	}
}

fn reset_other_handler() {
	OTHER_HANDLER_RECEIVED.with(|r| r.set(0));
}

fn get_other_handler_received() -> u64 {
	OTHER_HANDLER_RECEIVED.with(|r| r.get())
}

parameter_types! {
	pub const ZeroPercent: u32 = 0;
	pub const FiftyPercent: u32 = 50;
	pub const HundredPercent: u32 = 100;
}

#[test]
fn deal_with_fees_split_zero_percent_to_dap() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::Balanced;

		reset_other_handler();
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);

		// When: fees of 100 with 0% to DAP (all to other handler) + tips of 50
		// Tips should ALWAYS go to other handler, regardless of DAP percent
		let fees = <BalancesPallet<Test> as Balanced<u64>>::issue(100);
		let tips = <BalancesPallet<Test> as Balanced<u64>>::issue(50);
		<DealWithFeesSplit<Test, ZeroPercent, MockOtherHandler> as OnUnbalanced<_>>::on_unbalanceds(
			[fees, tips].into_iter(),
		);

		// Then: satellite unchanged (still just ED), other handler gets 150 (100% fees + tips)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);
		assert_eq!(get_other_handler_received(), 150);
	});
}

#[test]
fn deal_with_fees_split_hundred_percent_to_dap() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::Balanced;

		reset_other_handler();
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);

		// When: fees of 100 with 100% to DAP + tips of 50
		// Tips should ALWAYS go to other handler, regardless of DAP percent
		let fees = <BalancesPallet<Test> as Balanced<u64>>::issue(100);
		let tips = <BalancesPallet<Test> as Balanced<u64>>::issue(50);
		<DealWithFeesSplit<Test, HundredPercent, MockOtherHandler> as OnUnbalanced<_>>::on_unbalanceds(
			[fees, tips].into_iter(),
		);

		// Then: satellite gets ED + 100 (fees), other handler gets 50 (tips)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 101);
		assert_eq!(get_other_handler_received(), 50);
	});
}

#[test]
fn deal_with_fees_split_fifty_percent() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::Balanced;

		reset_other_handler();
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);

		// When: fees of 100 with 50% to DAP + tips of 40
		// Fees split 50/50, tips 100% to other handler
		let fees = <BalancesPallet<Test> as Balanced<u64>>::issue(100);
		let tips = <BalancesPallet<Test> as Balanced<u64>>::issue(40);
		<DealWithFeesSplit<Test, FiftyPercent, MockOtherHandler> as OnUnbalanced<_>>::on_unbalanceds(
			[fees, tips].into_iter(),
		);

		// Then: satellite gets ED + 50 (half of fees), other handler gets 90 (half of fees + tips)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 51);
		assert_eq!(get_other_handler_received(), 90);
	});
}

#[test]
fn deal_with_fees_split_handles_empty_iterator() {
	new_test_ext().execute_with(|| {
		reset_other_handler();
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite has ED (=1)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);

		// When: no fees, no tips (empty iterator)
		<DealWithFeesSplit<Test, FiftyPercent, MockOtherHandler> as OnUnbalanced<_>>::on_unbalanceds(
			core::iter::empty(),
		);

		// Then: nothing happens (still just ED)
		assert_eq!(BalancesPallet::<Test>::free_balance(satellite), 1);
		assert_eq!(get_other_handler_received(), 0);
	});
}

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

//! Tests for paying registered draws out of the buffer.

use super::key;
use crate::{
	mock::{
		account_id, build_and_execute, set_default_budget_allocation, Balances, Dap, RuntimeOrigin,
		System, Test,
	},
	BufferDraws, DrawBudget, Error, Event, LastIssuanceTimestamp, MAX_BUFFER_DRAWS,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::fungible::{Inspect, Mutate, Unbalanced},
};

/// Seed the buffer with `amount` of deactivated funds, as an inflow would.
fn fund_buffer(amount: u64) {
	Balances::mint_into(&Dap::buffer_account(), amount).unwrap();
	<Balances as Unbalanced<_>>::deactivate(amount);
}

fn register(name: &[u8], limit: u64) {
	assert_ok!(Dap::set_draw_budget(RuntimeOrigin::root(), key(name), Some(limit)));
}

fn spent(name: &[u8]) -> u64 {
	BufferDraws::<Test>::get()
		.get(&key(name))
		.map(|draw| draw.spent)
		.unwrap_or_default()
}

#[test]
fn pay_from_buffer_transfers_and_reactivates() {
	build_and_execute(true, || {
		System::set_block_number(1);
		set_default_budget_allocation();
		fund_buffer(1_000);
		register(b"drawer", 500);

		let who = account_id(1);
		let who_before = Balances::balance(&who);
		let buffer_before = Balances::balance(&Dap::buffer_account());
		let inactive_before = Balances::total_issuance() - Balances::active_issuance();
		let ti_before = Balances::total_issuance();

		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 300));

		assert_eq!(Balances::balance(&who), who_before + 300);
		assert_eq!(Balances::balance(&Dap::buffer_account()), buffer_before - 300);
		assert_eq!(Balances::total_issuance(), ti_before, "a payout must not mint");
		assert_eq!(
			Balances::total_issuance() - Balances::active_issuance(),
			inactive_before - 300,
			"the paid amount must be reactivated"
		);
		assert_eq!(spent(b"drawer"), 300);
		System::assert_has_event(
			Event::BufferDrawn { key: key(b"drawer"), beneficiary: who, amount: 300 }.into(),
		);
	});
}

#[test]
fn pay_from_buffer_rejects_unregistered_draw() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		fund_buffer(1_000);

		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &account_id(1), 1),
			Error::<Test>::UnregisteredDraw
		);
	});
}

#[test]
fn pay_from_buffer_caps_spending_per_period() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		fund_buffer(1_000);
		register(b"drawer", 100);

		let who = account_id(1);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 60));

		// The rest of the budget is still available, but no more than that.
		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &who, 41),
			Error::<Test>::DrawBudgetExceeded
		);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 40));
		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &who, 1),
			Error::<Test>::DrawBudgetExceeded
		);
		assert_eq!(spent(b"drawer"), 100);
	});
}

#[test]
fn draw_budget_refreshes_on_the_next_drip() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		fund_buffer(1_000);
		register(b"drawer", 100);

		let who = account_id(1);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 100));
		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &who, 1),
			Error::<Test>::DrawBudgetExceeded
		);

		// A drip stamps a new period, so the full budget is available again.
		LastIssuanceTimestamp::<Test>::mutate(|last| *last += 60_000);
		assert_eq!(Dap::buffer_draws(), vec![(key(b"drawer"), 100, 100)]);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 100));
		assert_eq!(spent(b"drawer"), 100);
	});
}

#[test]
fn failed_payment_leaves_budget_and_funds_untouched() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		// The buffer holds only its ED, which `Preservation::Preserve` will not dip into.
		register(b"drawer", 100);

		let buffer_before = Balances::balance(&Dap::buffer_account());
		assert!(Dap::pay_from_buffer(&key(b"drawer"), &account_id(1), 100).is_err());

		assert_eq!(Balances::balance(&Dap::buffer_account()), buffer_before);
		assert_eq!(spent(b"drawer"), 0, "a failed payment must not consume budget");
	});
}

#[test]
fn zero_payment_is_a_noop() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		fund_buffer(1_000);

		// Not even registered, yet a zero payment still succeeds without touching anything.
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &account_id(1), 0));
		assert!(BufferDraws::<Test>::get().is_empty());
	});
}

#[test]
fn set_draw_budget_keeps_spending_and_can_deregister() {
	build_and_execute(true, || {
		System::set_block_number(1);
		set_default_budget_allocation();
		fund_buffer(1_000);
		register(b"drawer", 100);

		let who = account_id(1);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 100));

		// Raising the limit mid-period does not forget what was already spent.
		register(b"drawer", 150);
		assert_eq!(spent(b"drawer"), 100);
		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &who, 51),
			Error::<Test>::DrawBudgetExceeded
		);
		assert_ok!(Dap::pay_from_buffer(&key(b"drawer"), &who, 50));

		// Deregistering stops payments entirely.
		assert_ok!(Dap::set_draw_budget(RuntimeOrigin::root(), key(b"drawer"), None));
		assert!(BufferDraws::<Test>::get().is_empty());
		System::assert_has_event(
			Event::DrawBudgetUpdated { key: key(b"drawer"), limit: None }.into(),
		);
		assert_noop!(
			Dap::pay_from_buffer(&key(b"drawer"), &who, 1),
			Error::<Test>::UnregisteredDraw
		);
	});
}

#[test]
fn set_draw_budget_requires_budget_origin() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		assert_noop!(
			Dap::set_draw_budget(RuntimeOrigin::signed(account_id(1)), key(b"drawer"), Some(1)),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn set_draw_budget_is_bounded() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		for i in 0..MAX_BUFFER_DRAWS {
			register(format!("draw{i}").as_bytes(), 1);
		}

		assert_noop!(
			Dap::set_draw_budget(RuntimeOrigin::root(), key(b"one_too_many"), Some(1)),
			Error::<Test>::TooManyDraws
		);
	});
}

#[test]
fn try_state_catches_a_future_period() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		BufferDraws::<Test>::mutate(|draws| {
			let period = LastIssuanceTimestamp::<Test>::get() + 1;
			draws
				.try_insert(key(b"drawer"), DrawBudget { limit: 1, spent: 0, period })
				.unwrap();
		});

		crate::mock::assert_try_state_invalid();
		// Leave storage consistent for the `build_and_execute` post-check.
		BufferDraws::<Test>::kill();
	});
}

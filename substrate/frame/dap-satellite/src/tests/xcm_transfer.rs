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

//! Tests for the periodic satellite-to-DAP XCM transfer logic.

use crate::{mock::*, Event};
use frame_support::{
	assert_ok,
	pallet_prelude::Weight,
	traits::{
		fungible::{Inspect, Mutate},
		Hooks,
	},
	weights::constants::RocksDbWeight,
};

type DapSatellitePallet = crate::Pallet<Test>;

fn get_satellite_account() -> u64 {
	DapSatellitePallet::satellite_account()
}

/// Add `amount` tokens above ED to the satellite account.
fn fund_satellite_account(amount: u64) {
	assert_ok!(Balances::mint_into(&get_satellite_account(), amount));
}

fn get_send_count() -> u32 {
	SEND_COUNT.with(|c| *c.borrow())
}

fn reset_send_count() {
	SEND_COUNT.with(|c| *c.borrow_mut() = 0);
}

fn get_last_sent_amount() -> Option<u64> {
	LAST_SENT_AMOUNT.with(|a| *a.borrow())
}

fn reset_last_sent_amount() {
	LAST_SENT_AMOUNT.with(|a| *a.borrow_mut() = None);
}

// Verify that `on_idle` does not trigger a transfer on blocks that are not
// exact multiples of `TransferPeriod`.
#[test]
fn rate_limit_rejects_non_period_blocks() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let ed = Balances::minimum_balance();
		let funds = 10u64;

		fund_satellite_account(funds);
		reset_send_count();

		// Non-multiples within and around the first period.
		for block in (period.saturating_sub(4)..=period.saturating_add(4)).filter(|b| *b != period)
		{
			System::set_block_number(block);
			DapSatellitePallet::on_idle(block, Weight::from_all(u64::MAX));
			assert_eq!(get_send_count(), 0, "unexpected send at block {block}");

			assert_eq!(
				Balances::free_balance(get_satellite_account()),
				ed.saturating_add(funds),
				"satellite should retain all funds at block {block}"
			);
		}
	});
}

// Verify that `on_idle` triggers a transfer on every block that is an exact
// multiple of `TransferPeriod`, independently of prior calls.
// After each transfer the satellite account should retain exactly ED.
#[test]
fn transfer_triggers_on_period_multiple() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let ed = Balances::minimum_balance();
		let funds = 10u64;

		for i in 1u64..=4 {
			fund_satellite_account(funds);
			reset_send_count();

			let block = period.saturating_mul(i);
			System::set_block_number(block);
			DapSatellitePallet::on_idle(block, Weight::from_all(u64::MAX));
			assert_eq!(get_send_count(), 1, "expected send at block {block} (iteration {i})");
			assert_eq!(
				Balances::free_balance(get_satellite_account()),
				ed,
				"satellite should retain only ED after transfer at block {block}"
			);
		}
	});
}

// Verify that each period-multiple block can independently trigger a transfer
// without requiring any shared state between calls.
#[test]
fn each_period_multiple_triggers_independently() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let funds = 30u64;

		fund_satellite_account(funds);
		reset_send_count();
		reset_last_sent_amount();

		// First transfer at block `period`.
		System::set_block_number(period);
		DapSatellitePallet::on_idle(period, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(get_last_sent_amount(), Some(funds));

		// Replenish and trigger at block `2 * period` — no stored state required.
		// The mock burns funds on success, so after the first transfer only ED remains;
		// funding 20 here means available_funds = 20 for this transfer.
		fund_satellite_account(funds);
		reset_last_sent_amount();

		System::set_block_number(period.saturating_mul(2));
		DapSatellitePallet::on_idle(period.saturating_mul(2), Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 2);
		assert_eq!(get_last_sent_amount(), Some(funds));
	});
}

// Verify that no transfer occurs when available funds (balance minus ED) are
// below the `MinTransferAmount` threshold, but does occur once they reach it.
#[test]
fn ensure_minimum_amount_limit_is_respected() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let limit = MinTransferAmount::get();

		// Fund the satellite with less than the minimum transferable amount above ED.
		fund_satellite_account(limit - 1);
		reset_send_count();
		reset_last_sent_amount();

		System::set_block_number(period);
		DapSatellitePallet::on_idle(period, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 0);

		// Top up so that available funds exactly meet the minimum.
		fund_satellite_account(1);
		assert_eq!(
			Balances::free_balance(get_satellite_account()),
			Balances::minimum_balance() + limit
		);

		// Next period multiple — transfer should now succeed.
		System::set_block_number(2 * period);
		DapSatellitePallet::on_idle(2 * period, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(get_last_sent_amount(), Some(limit));
	});
}

// Check the full success path: verify the send count, event, and forwarded amount.
#[test]
fn verify_success_path() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let funds = 50u64;

		reset_send_count();
		reset_last_sent_amount();
		fund_satellite_account(funds);

		System::set_block_number(period);
		DapSatellitePallet::on_idle(period, Weight::from_all(u64::MAX));

		assert_eq!(get_send_count(), 1);
		System::assert_has_event(Event::<Test>::SendSucceeded { amount: funds }.into());
		assert_eq!(get_last_sent_amount(), Some(funds));
	});
}

// Check the failure path: when a send fails, a `SendFailed` event is emitted
// and the satellite balance is unchanged (mock does not withdraw).
#[test]
fn verify_failure_path() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let sat = get_satellite_account();
		let funds = 50u64;

		reset_send_count();
		reset_last_sent_amount();
		fund_satellite_account(funds);

		System::set_block_number(period);
		SEND_FAIL.with(|f| *f.borrow_mut() = true);

		let balance_before = Balances::free_balance(sat);
		let issuance_before = Balances::total_issuance();

		DapSatellitePallet::on_idle(period, Weight::from_all(u64::MAX));

		assert_eq!(get_send_count(), 0);
		assert_eq!(get_last_sent_amount(), None);
		assert_eq!(Balances::free_balance(sat), balance_before);
		assert_eq!(Balances::total_issuance(), issuance_before);
		System::assert_has_event(Event::<Test>::SendFailed { amount: funds }.into());

		SEND_FAIL.with(|f| *f.borrow_mut() = false);
	});
}

// Verify that `on_idle` returns `Weight::zero()` immediately (no work done)
// on blocks that are not multiples of `TransferPeriod`.
#[test]
fn on_idle_consumes_no_weight_on_non_period_block() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let funds = 70u64;

		// Ensure that the transfer period is not 1.
		assert_ne!(period, 1);
		fund_satellite_account(funds);
		reset_send_count();

		// Block 1 is not a multiple of TransferPeriod.
		System::set_block_number(1);
		let consumed = DapSatellitePallet::on_idle(1, Weight::from_all(u64::MAX));

		assert_eq!(consumed, Weight::zero());
		assert_eq!(get_send_count(), 0);
	});
}

// Verify that `on_idle` exits without sending when there is not enough weight
// to perform the single balance read on a period block.
#[test]
fn on_idle_skips_when_no_weight_for_balance_read() {
	new_test_ext(true).execute_with(|| {
		let funds = 70u64;

		fund_satellite_account(funds);
		reset_send_count();

		let period = TransferPeriod::get();
		System::set_block_number(period);
		let consumed = DapSatellitePallet::on_idle(period, Weight::zero());

		assert_eq!(consumed, Weight::zero());
		assert_eq!(get_send_count(), 0);
	});
}

// Verify that `on_idle` consumes exactly one read's worth of weight when the
// balance check passes but the amount is below `MinTransferAmount`.
#[test]
fn on_idle_consumes_one_read_when_below_min_transfer() {
	new_test_ext(true).execute_with(|| {
		// Fund below MinTransferAmount so the transfer is skipped after the balance read.
		fund_satellite_account(MinTransferAmount::get() - 1);
		reset_send_count();

		let period = TransferPeriod::get();
		System::set_block_number(period);
		let one_read = RocksDbWeight::get().reads(1);
		let consumed = DapSatellitePallet::on_idle(period, one_read);

		assert_eq!(consumed, one_read);
		assert_eq!(get_send_count(), 0);
	});
}

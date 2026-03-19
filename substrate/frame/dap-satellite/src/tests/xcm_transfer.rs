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

use crate::{mock::*, pallet::LastTransferBlock, Event};
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

/// Add `extra` tokens above ED to the satellite account.
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

// Ensure that no transfer occurs in `on_idle` if at most `TransferPeriod` blocks
// have elapsed since the genesis block (i.e. no other transfers have occurred yet).
#[test]
fn rate_limit_on_first_transfer() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();

		reset_send_count();
		reset_last_sent_amount();
		fund_satellite_account(70);

		// Stricly less than the block limit - no transfer.
		DapSatellitePallet::on_idle(period - 1, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastTransferBlock::<Test>::get(), None);

		// Equal to the block limit - still no transfer.
		DapSatellitePallet::on_idle(period, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastTransferBlock::<Test>::get(), None);

		// Greater than the block limit - a transfer occurs, and LastTransferBlock is set.
		DapSatellitePallet::on_idle(period + 1, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(TransferPeriod::get() + 1));
		assert_eq!(get_last_sent_amount(), Some(70));
	});
}

// Ensure that following a successful transfer, the next transfer will not occur until
// until an additional transfer period has occurred.
#[test]
fn rate_limit_after_first_transfer() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let next_transfer_threshold = 7 + period;

		reset_send_count();
		reset_last_sent_amount();
		fund_satellite_account(30);

		// First transfer at block 7.
		DapSatellitePallet::on_idle(7, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));
		assert_eq!(get_last_sent_amount(), Some(30));

		// Replenish the source account in preparation for the next transfer.
		fund_satellite_account(30);

		// Before or at the period threshold - no second transfer.
		DapSatellitePallet::on_idle(next_transfer_threshold - 1, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));

		DapSatellitePallet::on_idle(next_transfer_threshold, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));

		// Immediately after the period threshold - second transfer occurs.
		DapSatellitePallet::on_idle(next_transfer_threshold + 1, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 2);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(next_transfer_threshold + 1));
		assert_eq!(get_last_sent_amount(), Some(30));
	});
}

// Ensure that no transfer occurs if the available funds (balance minus ED) are
// below the `MinTransferAmount` threshold.
#[test]
fn ensure_minimum_amount_limit_is_respected() {
	new_test_ext(true).execute_with(|| {
		let limit = MinTransferAmount::get();

		// Fund the satellite with less than the minimum transferable amount above ED.
		fund_satellite_account(limit - 1);
		reset_send_count();
		reset_last_sent_amount();

		// Block 7 is past the rate limit.
		DapSatellitePallet::on_idle(7, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastTransferBlock::<Test>::get(), None);

		// Ensure the satellite account now has the expected balance (ED + limit - 1).
		fund_satellite_account(1);
		assert_eq!(
			Balances::free_balance(get_satellite_account()),
			Balances::minimum_balance() + limit
		);

		// Retry the transfer and expect it to succeed this time.
		DapSatellitePallet::on_idle(7, Weight::from_all(u64::MAX));
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));
		assert_eq!(Balances::free_balance(get_satellite_account()), Balances::minimum_balance());
		assert_eq!(get_last_sent_amount(), Some(limit));
	});
}

// Check the full success path - when the satellite has enough funds and the period has elapsed.
// Verify the balance changes, storage, event, and the amount forwarded to `SendToDap`.
#[test]
fn verify_success_path() {
	new_test_ext(true).execute_with(|| {
		reset_send_count();
		reset_last_sent_amount();
		System::set_block_number(1);

		// Capture total issuance before funding so we can verify it is restored after burn.
		let funds = 50;
		let ed = Balances::minimum_balance();
		let total_before_fund = Balances::total_issuance();

		// Fund the satellite account with an amount above the threshold (ED not included).
		fund_satellite_account(funds);

		// Attempt a transfer at block 7, which is past the initial rate limit.
		DapSatellitePallet::on_idle(7, Weight::from_all(u64::MAX));

		// Verify that one send was dispatched.
		assert_eq!(get_send_count(), 1);

		// Check that the funds have been burnt from the satellite account (balance equals ED).
		assert_eq!(Balances::free_balance(get_satellite_account()), ed);

		// Ensure the total issuance sees the burn.
		assert_eq!(Balances::total_issuance(), total_before_fund);

		// Ensure the block of the last transfer has been recorded.
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));

		// Ensure the sending succeeded with the correct amount.
		System::assert_has_event(Event::<Test>::SendSucceeded { amount: funds }.into());

		// Verify the exact amount forwarded to the SendToDap implementation.
		assert_eq!(get_last_sent_amount(), Some(funds));
	});
}

// Check the failure path - when a send fails, burned funds are restored via
// `mint_into` and a `SendFailed` event is emitted. `LastTransferBlock` is updated regardless.
#[test]
fn verify_failure_path() {
	new_test_ext(true).execute_with(|| {
		reset_send_count();
		reset_last_sent_amount();
		System::set_block_number(1);

		// Configure the transfer to fail.
		SEND_FAIL.with(|f| *f.borrow_mut() = true);

		let funds = 50;
		let sat = get_satellite_account();

		fund_satellite_account(funds);

		let balance_before_transfer = Balances::free_balance(sat);
		let total_before_transfer = Balances::total_issuance();

		DapSatellitePallet::on_idle(7, Weight::from_all(u64::MAX));

		// Verify that nothing was sent.
		assert_eq!(get_send_count(), 0);
		assert_eq!(get_last_sent_amount(), None);

		// LastTransferBlock should have been updated despite the failure.
		assert_eq!(LastTransferBlock::<Test>::get(), Some(7));

		// Check that the satellite balance was fully restored.
		assert_eq!(Balances::free_balance(sat), balance_before_transfer);
		assert_eq!(Balances::total_issuance(), total_before_transfer);

		// Check that the failure event was emitted.
		System::assert_has_event(Event::<Test>::SendFailed { amount: funds }.into());

		// Reset the failure flag for other tests.
		SEND_FAIL.with(|f| *f.borrow_mut() = false);
	});
}

// Verify that `on_idle` exits immediately and consumes no weight when the budget
// is too small for even the first storage read (LastTransferBlock).
#[test]
fn on_idle_skips_when_no_weight_for_first_read() {
	new_test_ext(true).execute_with(|| {
		reset_send_count();
		fund_satellite_account(70);

		let consumed = DapSatellitePallet::on_idle(7, Weight::zero());

		assert_eq!(consumed, Weight::zero());
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastTransferBlock::<Test>::get(), None);
	});
}

// Verify that `on_idle` exits after the rate-limit check when the budget covers one
// read but not the second (the balance read).
#[test]
fn on_idle_skips_when_no_weight_for_second_read() {
	new_test_ext(true).execute_with(|| {
		reset_send_count();
		fund_satellite_account(70);

		let one_read = RocksDbWeight::get().reads(1);
		let consumed = DapSatellitePallet::on_idle(7, one_read);

		assert_eq!(consumed, one_read);
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastTransferBlock::<Test>::get(), None);
	});
}

// Verify that `on_idle` exits before updating storage when the budget covers both
// reads but not the write (the LastTransferBlock update).
#[test]
fn on_idle_skips_when_no_weight_for_write() {
	new_test_ext(true).execute_with(|| {
		reset_send_count();
		fund_satellite_account(70);

		let two_reads = RocksDbWeight::get().reads(2);
		let consumed = DapSatellitePallet::on_idle(7, two_reads);

		assert_eq!(consumed, two_reads);
		assert_eq!(get_send_count(), 0);
		// The write was never reached, so LastTransferBlock must remain unset.
		assert_eq!(LastTransferBlock::<Test>::get(), None);
	});
}

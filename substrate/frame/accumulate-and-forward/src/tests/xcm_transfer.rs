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

//! Tests for the periodic accumulation-account-to-destination forwarding logic.

use crate::{mock::*, Event, LastForwardBlock};
use frame_support::{
	assert_ok,
	pallet_prelude::Weight,
	traits::{
		fungible::{Inspect, Mutate},
		Hooks,
	},
	weights::constants::RocksDbWeight,
};

type AccumulateForwardPallet = crate::Pallet<Test>;

fn get_accumulation_account() -> u64 {
	AccumulateForwardPallet::accumulation_account()
}

/// Add `amount` tokens above ED to the accumulation account.
fn fund_accumulation_account(amount: u64) {
	assert_ok!(Balances::mint_into(&get_accumulation_account(), amount));
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

/// Run `on_idle` with an unconstrained weight budget at provider block `block`.
fn run_on_idle_at(block: u64) {
	System::set_block_number(block);
	AccumulateForwardPallet::on_idle(block, Weight::from_all(u64::MAX));
}

/// Run `on_idle` on each of `blocks`, keeping the account funded, and return the blocks that
/// forwarded.
fn blocks_that_forwarded(blocks: impl IntoIterator<Item = u64>) -> Vec<u64> {
	let mut sent_at = Vec::new();
	for block in blocks {
		fund_accumulation_account(MinTransferAmount::get());
		reset_send_count();
		run_on_idle_at(block);
		if get_send_count() == 1 {
			sent_at.push(block);
		}
	}
	sent_at
}

// The first forward is not rate limited: nothing recorded means no period to wait out.
#[test]
fn first_forward_is_not_rate_limited() {
	new_test_ext(true).execute_with(|| {
		let funds = 50u64;

		fund_accumulation_account(funds);
		reset_send_count();

		// Deliberately not a multiple of the period.
		assert_ne!(1 % TransferPeriod::get(), 0);
		run_on_idle_at(1);

		assert_eq!(get_send_count(), 1);
		assert_eq!(LastForwardBlock::<Test>::get(), Some(1));
	});
}

// After a forward, the next one waits exactly `TransferPeriod` blocks, wherever the first landed.
#[test]
fn no_forward_until_the_period_elapsed() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let ed = Balances::minimum_balance();
		let funds = 10u64;
		let first = 7u64;

		fund_accumulation_account(funds);
		run_on_idle_at(first);
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastForwardBlock::<Test>::get(), Some(first));

		// Refund, so only the rate limit can hold the next forward back.
		fund_accumulation_account(funds);
		reset_send_count();

		for block in first + 1..first + period {
			run_on_idle_at(block);
			assert_eq!(get_send_count(), 0, "unexpected send at block {block}");
			assert_eq!(
				Balances::free_balance(get_accumulation_account()),
				ed.saturating_add(funds),
				"accumulation account should retain all funds at block {block}"
			);
			assert_eq!(LastForwardBlock::<Test>::get(), Some(first));
		}

		// Exactly `period` blocks later, the next forward is allowed.
		run_on_idle_at(first + period);
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastForwardBlock::<Test>::get(), Some(first + period));
		assert_eq!(Balances::free_balance(get_accumulation_account()), ed);
	});
}

// A parachain sees the relay chain block number advance in steps of two or more, so the former
// `now % TransferPeriod == 0` check could never fire.
#[test]
fn forwards_when_period_multiples_are_never_observed() {
	new_test_ext(true).execute_with(|| {
		// An even period with only odd blocks observed: never an exact multiple.
		TransferPeriod::set(6);
		let period = TransferPeriod::get();
		let observed: Vec<u64> = (1..=25).step_by(2).collect();
		assert!(observed.iter().all(|block| block % period != 0));

		// Measured from the last forward, the cadence holds regardless.
		assert_eq!(blocks_that_forwarded(observed), vec![1, 7, 13, 19, 25]);
	});
}

// A long gap in observed blocks produces one forward, not a burst of catch-up forwards.
#[test]
fn a_long_gap_produces_a_single_forward() {
	new_test_ext(true).execute_with(|| {
		assert_eq!(blocks_that_forwarded([2, 4, 100, 101, 102]), vec![2, 100]);
		assert_eq!(LastForwardBlock::<Test>::get(), Some(100));
	});
}

// No forward below `MinTransferAmount`. Nothing is recorded either, so the funds go out as soon
// as they reach the threshold instead of waiting for another period.
#[test]
fn ensure_minimum_amount_limit_is_respected() {
	new_test_ext(true).execute_with(|| {
		let limit = MinTransferAmount::get();

		// Less than the minimum forwardable amount above ED.
		fund_accumulation_account(limit - 1);
		reset_send_count();
		reset_last_sent_amount();

		run_on_idle_at(1);
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastForwardBlock::<Test>::get(), None);

		// Top up to exactly the minimum.
		fund_accumulation_account(1);
		assert_eq!(
			Balances::free_balance(get_accumulation_account()),
			Balances::minimum_balance() + limit
		);

		// The next block forwards: no period to wait out yet.
		run_on_idle_at(2);
		assert_eq!(get_send_count(), 1);
		assert_eq!(get_last_sent_amount(), Some(limit));
		assert_eq!(LastForwardBlock::<Test>::get(), Some(2));
	});
}

// The success path: send count, event, forwarded amount and the recorded block.
#[test]
fn verify_success_path() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let funds = 50u64;

		reset_send_count();
		reset_last_sent_amount();
		fund_accumulation_account(funds);

		run_on_idle_at(period);

		assert_eq!(get_send_count(), 1);
		System::assert_has_event(Event::<Test>::ForwardSucceeded { amount: funds }.into());
		assert_eq!(get_last_sent_amount(), Some(funds));
		assert_eq!(LastForwardBlock::<Test>::get(), Some(period));
	});
}

// The failure path: `ForwardFailed` is emitted and the balance is unchanged (the mock does not
// withdraw). The attempt is recorded all the same, so the retry waits another `TransferPeriod`.
#[test]
fn verify_failure_path() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();
		let acc = get_accumulation_account();
		let funds = 50u64;

		reset_send_count();
		reset_last_sent_amount();
		fund_accumulation_account(funds);

		SEND_FAIL.with(|f| *f.borrow_mut() = true);

		let balance_before = Balances::free_balance(acc);
		let issuance_before = Balances::total_issuance();

		run_on_idle_at(period);

		assert_eq!(get_send_count(), 0);
		assert_eq!(get_last_sent_amount(), None);
		assert_eq!(Balances::free_balance(acc), balance_before);
		assert_eq!(Balances::total_issuance(), issuance_before);
		System::assert_has_event(Event::<Test>::ForwardFailed { amount: funds }.into());
		assert_eq!(LastForwardBlock::<Test>::get(), Some(period));

		// No retry on the very next block.
		System::reset_events();
		run_on_idle_at(period + 1);
		assert!(System::events().is_empty());
		assert_eq!(LastForwardBlock::<Test>::get(), Some(period));

		// The retry happens a period later.
		SEND_FAIL.with(|f| *f.borrow_mut() = false);
		run_on_idle_at(period * 2);
		assert_eq!(get_send_count(), 1);
		assert_eq!(get_last_sent_amount(), Some(funds));
		assert_eq!(LastForwardBlock::<Test>::get(), Some(period * 2));
	});
}

// Without weight for even the single read of the recorded block, `on_idle` does nothing.
#[test]
fn on_idle_consumes_no_weight_without_budget_for_the_period_read() {
	new_test_ext(true).execute_with(|| {
		fund_accumulation_account(70);
		reset_send_count();

		System::set_block_number(1);
		let consumed = AccumulateForwardPallet::on_idle(1, Weight::zero());

		assert_eq!(consumed, Weight::zero());
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastForwardBlock::<Test>::get(), None);
	});
}

// While rate limited, `on_idle` costs exactly one read.
#[test]
fn on_idle_consumes_one_read_when_rate_limited() {
	new_test_ext(true).execute_with(|| {
		let period = TransferPeriod::get();

		// Ensure that the transfer period is not 1.
		assert_ne!(period, 1);
		fund_accumulation_account(70);

		// Forward once, so the rate limit applies from here on.
		run_on_idle_at(1);
		assert_eq!(get_send_count(), 1);
		reset_send_count();

		System::set_block_number(2);
		let consumed = AccumulateForwardPallet::on_idle(2, Weight::from_all(u64::MAX));

		assert_eq!(consumed, RocksDbWeight::get().reads(1));
		assert_eq!(get_send_count(), 0);
	});
}

// Two reads (recorded block and balance) when the period elapsed but the amount is below the
// minimum.
#[test]
fn on_idle_consumes_two_reads_when_below_min_transfer() {
	new_test_ext(true).execute_with(|| {
		// Below `MinTransferAmount`, so the forward is skipped after the balance read.
		fund_accumulation_account(MinTransferAmount::get() - 1);
		reset_send_count();

		let two_reads = RocksDbWeight::get().reads(2);
		System::set_block_number(1);
		let consumed = AccumulateForwardPallet::on_idle(1, two_reads);

		assert_eq!(consumed, two_reads);
		assert_eq!(get_send_count(), 0);
	});
}

// A forward that does not fit in the remaining weight is not recorded, so it retries next block.
#[test]
fn on_idle_does_not_record_when_the_send_does_not_fit() {
	new_test_ext(true).execute_with(|| {
		fund_accumulation_account(50);
		reset_send_count();

		// Enough for both reads, but not for the send and its write.
		let two_reads = RocksDbWeight::get().reads(2);
		System::set_block_number(1);
		let consumed = AccumulateForwardPallet::on_idle(1, two_reads);

		assert_eq!(consumed, two_reads);
		assert_eq!(get_send_count(), 0);
		assert_eq!(LastForwardBlock::<Test>::get(), None);

		// With room for the write it goes through next block (`send_native` is zero for `()`).
		let with_write = two_reads.saturating_add(RocksDbWeight::get().writes(1));
		System::set_block_number(2);
		let consumed = AccumulateForwardPallet::on_idle(2, with_write);

		assert_eq!(consumed, with_write);
		assert_eq!(get_send_count(), 1);
		assert_eq!(LastForwardBlock::<Test>::get(), Some(2));
	});
}

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

//! Tests for the footprint pallet's quota and cleanup invariants.

#![cfg(test)]

use crate::{
	mock::{
		purchased_hold, set_base_allowance, ExtBuilder, FirstReason, Footprint as FootprintPallet,
		RuntimeOrigin, Test, TestReason,
	},
	Allowances, Claims, Error, QuotaConsideration, Usage, UsageByReason,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{Consideration, Footprint},
};

fn footprint(count: u64, size: u64) -> Footprint {
	Footprint { count, size }
}

#[test]
fn weighted_bytes_include_the_per_item_trie_overhead() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 100));
		let charged = footprint(2, 10);

		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, charged));
		assert_eq!(Usage::<Test>::get(1), 74);
	});
}

#[test]
fn charges_only_fail_after_the_allowance_is_exhausted() {
	ExtBuilder::default().build_and_execute(|| {
		let exact_allowance = footprint(1, 32);
		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 64));

		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, exact_allowance));
		assert_eq!(Usage::<Test>::get(1), 64);
		assert_noop!(
			crate::Pallet::<Test>::charge(&1, TestReason::First, footprint(0, 1)),
			Error::<Test>::Exhausted
		);
	});
}

#[test]
fn purchased_allowance_holds_exactly_and_only_refunds_after_usage_is_released() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 10));
		assert_eq!(purchased_hold(1), 50);

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 20));
		assert_eq!(purchased_hold(1), 100);

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 100));
		assert_eq!(purchased_hold(1), 500);
		let live_usage = footprint(0, 60);
		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, live_usage));

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 80));
		assert_eq!(purchased_hold(1), 400);
		assert_noop!(
			FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 59),
			Error::<Test>::AllowanceBelowUsage
		);

		assert_ok!(crate::Pallet::<Test>::release(&1, TestReason::First, live_usage));
		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 10));
		assert_eq!(purchased_hold(1), 50);
		assert_noop!(
			FootprintPallet::set_purchased(RuntimeOrigin::signed(1), (1 << 20) + 1),
			Error::<Test>::ExceedsMaxPurchased
		);
	});
}

#[test]
fn base_claims_cannot_displace_usage_that_still_needs_the_old_base() {
	ExtBuilder::default().build_and_execute(|| {
		set_base_allowance(1, Some(100));
		set_base_allowance(2, Some(200));

		assert_ok!(FootprintPallet::claim_base(RuntimeOrigin::signed(1), 10));
		assert_eq!(Allowances::<Test>::get(10).base, 100);
		assert_noop!(
			FootprintPallet::claim_base(RuntimeOrigin::signed(2), 10),
			Error::<Test>::AccountAlreadyClaimed
		);

		let old_usage = footprint(0, 50);
		assert_ok!(crate::Pallet::<Test>::charge(&10, TestReason::First, old_usage));
		assert_noop!(
			FootprintPallet::claim_base(RuntimeOrigin::signed(1), 11),
			Error::<Test>::BaseInUse
		);

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(10), 50));
		assert_ok!(FootprintPallet::claim_base(RuntimeOrigin::signed(1), 11));
		let old_allowance = Allowances::<Test>::get(10);
		assert_eq!(old_allowance.base, 0);
		assert_eq!(old_allowance.purchased, 50);
		assert_eq!(old_allowance.token, None);
		let new_allowance = Allowances::<Test>::get(11);
		assert_eq!(new_allowance.base, 100);
		assert_eq!(new_allowance.token, Some(1));
		assert_eq!(Claims::<Test>::get(1), Some(11));
	});
}

#[test]
fn revalidation_keeps_over_quota_data_cleanable_after_demotion_or_revocation() {
	ExtBuilder::default().build_and_execute(|| {
		set_base_allowance(1, Some(100));
		assert_ok!(FootprintPallet::claim_base(RuntimeOrigin::signed(1), 1));
		let charged = footprint(0, 80);
		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, charged));

		set_base_allowance(1, Some(40));
		assert_ok!(FootprintPallet::revalidate_base(RuntimeOrigin::signed(2), 1));
		assert_eq!(Allowances::<Test>::get(1).base, 40);
		assert_eq!(Usage::<Test>::get(1), 80);
		assert_noop!(
			crate::Pallet::<Test>::charge(&1, TestReason::First, footprint(0, 1)),
			Error::<Test>::Exhausted
		);

		assert_ok!(crate::Pallet::<Test>::release(&1, TestReason::First, footprint(0, 20)));
		assert_eq!(Usage::<Test>::get(1), 60);

		set_base_allowance(1, None);
		assert_ok!(FootprintPallet::revalidate_base(RuntimeOrigin::signed(2), 1));
		assert_eq!(Allowances::<Test>::get(1).base, 0);
		assert_eq!(Allowances::<Test>::get(1).token, None);
		assert_eq!(Claims::<Test>::get(1), None);
		assert_eq!(Usage::<Test>::get(1), 60);
	});
}

#[test]
fn consideration_tickets_preserve_cleanup_when_over_quota_and_burns_remain_charged() {
	ExtBuilder::default().build_and_execute(|| {
		type Ticket = QuotaConsideration<Test, FirstReason>;

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 100));
		let ticket = match <Ticket as Consideration<u64, Footprint>>::new(&1, footprint(0, 50)) {
			Ok(ticket) => ticket,
			Err(error) => panic!("ticket fits the purchased allowance: {error:?}"),
		};
		let ticket = match ticket.update(&1, footprint(0, 80)) {
			Ok(ticket) => ticket,
			Err(error) => panic!("growth remains within the purchased allowance: {error:?}"),
		};
		assert_noop!(ticket.update(&1, footprint(0, 120)), Error::<Test>::Exhausted);
		assert_eq!(Usage::<Test>::get(1), 80);

		set_base_allowance(2, Some(100));
		assert_ok!(FootprintPallet::claim_base(RuntimeOrigin::signed(2), 2));
		let ticket = match <Ticket as Consideration<u64, Footprint>>::new(&2, footprint(0, 80)) {
			Ok(ticket) => ticket,
			Err(error) => panic!("base allowance covers the ticket: {error:?}"),
		};
		set_base_allowance(2, Some(40));
		assert_ok!(FootprintPallet::revalidate_base(RuntimeOrigin::signed(1), 2));
		let ticket = match ticket.update(&2, footprint(0, 20)) {
			Ok(ticket) => ticket,
			Err(error) => panic!("shrinking remains available while over quota: {error:?}"),
		};
		assert_ok!(ticket.drop(&2));
		assert_eq!(Usage::<Test>::get(2), 0);
		assert!(!UsageByReason::<Test>::contains_key(2, TestReason::First));

		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(3), 20));
		let burned = match <Ticket as Consideration<u64, Footprint>>::new(&3, footprint(0, 10)) {
			Ok(ticket) => ticket,
			Err(error) => panic!("purchased allowance covers the burned ticket: {error:?}"),
		};
		burned.burn(&3);
		assert_eq!(Usage::<Test>::get(3), 10);
		assert_eq!(UsageByReason::<Test>::get(3, TestReason::First), footprint(0, 10));
	});
}

#[test]
fn usage_by_reason_keeps_each_feature_visible_independently() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(FootprintPallet::set_purchased(RuntimeOrigin::signed(1), 200));
		let first = footprint(1, 8);
		let second = footprint(2, 10);

		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, first));
		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::Second, second));
		assert_eq!(UsageByReason::<Test>::get(1, TestReason::First), first);
		assert_eq!(UsageByReason::<Test>::get(1, TestReason::Second), second);
		assert_eq!(Usage::<Test>::get(1), 114);

		assert_ok!(crate::Pallet::<Test>::release(&1, TestReason::First, first));
		assert!(!UsageByReason::<Test>::contains_key(1, TestReason::First));
		assert_eq!(UsageByReason::<Test>::get(1, TestReason::Second), second);
	});
}

#[test]
fn zero_weight_footprints_are_allowed_without_any_allowance() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(crate::Pallet::<Test>::charge(&1, TestReason::First, Footprint::default()));
		assert_eq!(Usage::<Test>::get(1), 0);
		assert!(!UsageByReason::<Test>::contains_key(1, TestReason::First));
	});
}

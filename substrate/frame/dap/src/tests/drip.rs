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

//! Tests for inflation drip and distribution.

use super::budget_map;
use crate::{
	mock::{new_test_ext, Balances, Dap, MockTime, RuntimeOrigin, System, Test},
	Event,
};
use frame_support::{assert_ok, traits::fungible::Inspect};
use sp_runtime::BuildStorage;

fn advance_time_and_drip(elapsed_ms: u64) {
	let now = MockTime::get();
	MockTime::set(now + elapsed_ms);
	Dap::drip_inflation();
}

#[test]
fn drip_distributes_according_to_budget() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);

		// GIVEN: 60% staker, 25% validator incentive, 15% buffer
		let allocs =
			budget_map(&[(b"staker_rewards", 60), (b"validator_incentive", 25), (b"buffer", 15)]);
		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), allocs));

		let staker_pot = 500u128; // TestStakerRecipient
		let incentive_pot = 501u128; // TestValidatorIncentiveRecipient
		let buffer = Dap::buffer_account();

		let staker_before = Balances::balance(&staker_pot);
		let incentive_before = Balances::balance(&incentive_pot);
		let buffer_before = Balances::balance(&buffer);

		// WHEN: 60 seconds elapse → TestInflationCurve returns 100
		advance_time_and_drip(60_000);

		// THEN: 60% of 100 = 60 to stakers, 25% = 25 to incentive, remainder to buffer
		assert_eq!(Balances::balance(&staker_pot) - staker_before, 60);
		assert_eq!(Balances::balance(&incentive_pot) - incentive_before, 25);
		// Buffer gets 15 (explicit) + 0 (remainder from rounding) = 15
		assert_eq!(Balances::balance(&buffer) - buffer_before, 15);
	});
}

#[test]
fn drip_skips_when_cadence_not_reached() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: only 30 seconds pass (cadence = 60s)
		advance_time_and_drip(30_000);

		// THEN: no inflation minted
		assert_eq!(Balances::balance(&buffer), buffer_before);
	});
}

#[test]
fn drip_fires_after_cadence_reached() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: 30s passes (no drip), then another 30s (total 60s, drip fires)
		advance_time_and_drip(30_000);
		assert_eq!(Balances::balance(&buffer), buffer_before);

		advance_time_and_drip(30_000);
		// With no budget set, 100% goes to buffer as remainder.
		// 60s elapsed total → TestInflationCurve returns 100.
		assert_eq!(Balances::balance(&buffer) - buffer_before, 100);
	});
}

#[test]
fn unallocated_percentage_goes_to_buffer() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);

		// GIVEN: only 50% allocated to staker, rest is unallocated
		let allocs = budget_map(&[(b"staker_rewards", 50)]);
		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), allocs));

		let staker_pot = 500u128;
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: drip fires
		advance_time_and_drip(60_000);

		// THEN: staker gets 50, buffer gets remainder (50)
		assert_eq!(Balances::balance(&staker_pot), 50);
		assert_eq!(Balances::balance(&buffer) - buffer_before, 50);
	});
}

#[test]
fn inflation_ceiling_is_applied() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: large time gap would produce inflation > MaxInflationPerDrip (1000)
		// 10 minutes = 600_000ms → TestInflationCurve would return 1000
		// 20 minutes = 1_200_000ms → would return 2000, but ceiling is 1000
		advance_time_and_drip(1_200_000);

		// THEN: total minted is capped at 1000 (all to buffer since no budget set)
		assert_eq!(Balances::balance(&buffer) - buffer_before, 1000);

		// AND: InflationCapped event emitted
		System::assert_has_event(
			Event::<Test>::InflationCapped { computed: 2000, ceiling: 1000 }.into(),
		);
	});
}

#[test]
fn first_block_initializes_timestamp_without_dripping() {
	// Test that when LastInflationTimestamp is 0 (genesis), it initializes without dripping.
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances: vec![(1, 100)], ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();

	ext.execute_with(|| {
		// LastInflationTimestamp defaults to 0 (not initialized)
		assert_eq!(crate::LastInflationTimestamp::<Test>::get(), 0);

		MockTime::set(1_000_000);
		Dap::drip_inflation();

		// Timestamp should be set but no inflation minted.
		assert_eq!(crate::LastInflationTimestamp::<Test>::get(), 1_000_000);
		// Total issuance unchanged (only the initial 100 balance).
		assert_eq!(Balances::total_issuance(), 100);
	});
}

#[test]
fn drip_emits_inflation_dripped_event() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);

		advance_time_and_drip(60_000);

		// With no budget set, 100% goes to buffer.
		System::assert_has_event(
			Event::<Test>::InflationDripped {
				total_minted: 100,
				buffer_amount: 100,
				elapsed_millis: 60_000,
			}
			.into(),
		);
	});
}

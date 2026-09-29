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

//! Tests for issuance drip and distribution.

use super::budget_map;
use crate::{
	mock::{
		account_id, build_and_execute, set_default_budget_allocation, AccountId, Balances, Dap,
		MockTime, NativeAndAssets, RuntimeOrigin, System, Test,
	},
	tests::asset_allocations,
	Event,
};
use frame_benchmarking::whitelisted_caller;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate, NativeOrWithId},
		fungibles::{Create, Inspect, Mutate},
	},
};
use sp_runtime::BuildStorage;

fn advance_time_and_drip(elapsed_ms: u64) {
	let now = MockTime::get();
	MockTime::set(now + elapsed_ms);
	Dap::drip_issuance();
}

fn balance_of_asset(asset_id: u32, account: &AccountId) -> u64 {
	NativeAndAssets::balance(NativeOrWithId::WithId(asset_id), account)
}

fn create_asset(asset_id: u32) {
	let caller: AccountId = whitelisted_caller();
	assert_ok!(NativeAndAssets::create(NativeOrWithId::WithId(asset_id), caller, false, 1));
}

fn event_count(event: Event<Test>) -> usize {
	let event: <Test as frame_system::Config>::RuntimeEvent = event.into();
	System::events().into_iter().filter(|e| e.event == event).count()
}

#[test]
fn drip_distributes_according_to_budget() {
	build_and_execute(true, || {
		System::set_block_number(1);

		// GIVEN: 60% staker, 25% validator incentive, 15% buffer
		let allocs =
			budget_map(&[(b"staker_rewards", 60), (b"validator_incentive", 25), (b"buffer", 15)]);
		assert_ok!(Dap::set_allocations(RuntimeOrigin::root(), Some(allocs), None));

		let staker_pot = account_id(500); // TestStakerRecipient pot account
		let incentive_pot = account_id(501); // TestValidatorIncentiveRecipient pot account
		let buffer = Dap::buffer_account();

		let staker_before = Balances::balance(&staker_pot);
		let incentive_before = Balances::balance(&incentive_pot);
		let buffer_before = Balances::balance(&buffer);

		// WHEN: 60 seconds elapse → TestIssuanceCurve returns 100
		advance_time_and_drip(60_000);

		// THEN: 60% of 100 = 60 to stakers, 25% = 25 to incentive, 15% = 15 to buffer
		assert_eq!(Balances::balance(&staker_pot) - staker_before, 60);
		assert_eq!(Balances::balance(&incentive_pot) - incentive_before, 25);
		assert_eq!(Balances::balance(&buffer) - buffer_before, 15);

		System::assert_has_event(
			Event::<Test>::IssuanceMinted { total_minted: 100, elapsed_millis: 60_000 }.into(),
		);
	});
}

#[test]
fn assets_are_distributed_on_drip() {
	build_and_execute(true, || {
		System::set_block_number(1);

		let budget_allocations = budget_map(&[(b"buffer", 100)]);
		let asset_allocations = asset_allocations(&[
			(NativeOrWithId::Native, &[(b"buffer", 10), (b"staker_rewards", 60)]),
			(NativeOrWithId::WithId(10), &[(b"validator_incentive", 10), (b"staker_rewards", 20)]),
		]);
		assert_ok!(Dap::set_allocations(
			RuntimeOrigin::root(),
			Some(budget_allocations),
			Some(asset_allocations.clone())
		));

		let staker_pot = account_id(500); // TestStakerRecipient pot account
		let incentive_pot = account_id(501); // TestValidatorIncentiveRecipient pot account
		let buffer = Dap::buffer_account();

		// Check the Native asset.

		Balances::mint_into(&buffer, 100_000_000);
		Balances::mint_into(&staker_pot, 10);
		Balances::mint_into(&incentive_pot, 10);

		let staker_before = Balances::balance(&staker_pot);
		let incentive_before = Balances::balance(&incentive_pot);
		let buffer_before = Balances::balance(&buffer);

		advance_time_and_drip(60_000);

		assert_eq!(staker_before + 60_000 * 60, Balances::balance(&staker_pot));
		assert_eq!(incentive_before, Balances::balance(&incentive_pot));
		// Distribute 60_000 * 60 to staker pot, distribute 60_000 * 10 to itself, receive 100 from
		// budget
		assert_eq!(
			buffer_before - 60_000 * 60 - 60_000 * 10 + 60_000 * 10 + 100,
			Balances::balance(&buffer)
		);

		System::assert_has_event(
			Event::<Test>::AssetDistributed {
				asset: NativeOrWithId::Native,
				amount: 60_000 * (60 + 10),
				elapsed_millis: 60_000,
			}
			.into(),
		);

		// Check the WithId(10) asset.

		create_asset(10);
		NativeAndAssets::mint_into(NativeOrWithId::WithId(10), &buffer, 100_000_000);

		let staker_before = balance_of_asset(10, &staker_pot);
		let incentive_before = balance_of_asset(10, &incentive_pot);
		let buffer_before = balance_of_asset(10, &buffer);

		advance_time_and_drip(60_000);

		assert_eq!(staker_before + 60_000 * 20, balance_of_asset(10, &staker_pot));
		assert_eq!(incentive_before + 60_000 * 10, balance_of_asset(10, &incentive_pot));
		// Distribute 60_000 * 20 to staker pot, distribute 60_000 * 10 to incentive pot.
		assert_eq!(buffer_before - 60_000 * 20 - 60_000 * 10, balance_of_asset(10, &buffer));

		System::assert_has_event(
			Event::<Test>::AssetDistributed {
				asset: NativeOrWithId::WithId(10),
				amount: 60_000 * (20 + 10),
				elapsed_millis: 60_000,
			}
			.into(),
		);
	});
}

#[test]
fn asset_distribution_silently_fails_when_there_are_not_enough_funds() {
	build_and_execute(true, || {
		System::set_block_number(1);

		let budget_allocations = budget_map(&[(b"buffer", 100)]);
		let asset_allocations = asset_allocations(&[
			(NativeOrWithId::Native, &[(b"buffer", 10), (b"staker_rewards", 60)]),
			(NativeOrWithId::WithId(10), &[(b"validator_incentive", 10), (b"staker_rewards", 20)]),
		]);
		assert_ok!(Dap::set_allocations(
			RuntimeOrigin::root(),
			Some(budget_allocations),
			Some(asset_allocations.clone())
		));

		let staker_pot = account_id(500); // TestStakerRecipient pot account
		let incentive_pot = account_id(501); // TestValidatorIncentiveRecipient pot account
		let buffer = Dap::buffer_account();

		// Check the Native asset.

		Balances::mint_into(&buffer, 10);
		Balances::mint_into(&staker_pot, 10);
		Balances::mint_into(&incentive_pot, 10);

		let staker_before = Balances::balance(&staker_pot);
		let incentive_before = Balances::balance(&incentive_pot);
		let buffer_before = Balances::balance(&buffer);

		advance_time_and_drip(60_000);

		assert_eq!(staker_before, Balances::balance(&staker_pot));
		assert_eq!(incentive_before, Balances::balance(&incentive_pot));
		// Nothing was distributed, so only +100 from the budget.
		assert_eq!(buffer_before + 100, Balances::balance(&buffer));

		assert_eq!(
			event_count(Event::<Test>::Unexpected(
				crate::UnexpectedKind::DistributionTransferFailed { asset: NativeOrWithId::Native },
			)),
			2
		);
		System::assert_has_event(
			Event::<Test>::AssetDistributed {
				asset: NativeOrWithId::Native,
				amount: 0,
				elapsed_millis: 60_000,
			}
			.into(),
		);

		System::reset_events();

		// Check the WithId(10) asset.

		create_asset(10);
		NativeAndAssets::mint_into(NativeOrWithId::WithId(10), &buffer, 10);

		let staker_before = balance_of_asset(10, &staker_pot);
		let incentive_before = balance_of_asset(10, &incentive_pot);
		let buffer_before = balance_of_asset(10, &buffer);

		advance_time_and_drip(60_000);

		assert_eq!(staker_before, balance_of_asset(10, &staker_pot));
		assert_eq!(incentive_before, balance_of_asset(10, &incentive_pot));
		assert_eq!(buffer_before, balance_of_asset(10, &buffer));

		assert_eq!(
			event_count(Event::<Test>::Unexpected(
				crate::UnexpectedKind::DistributionTransferFailed {
					asset: NativeOrWithId::WithId(10),
				},
			)),
			2
		);
		System::assert_has_event(
			Event::<Test>::AssetDistributed {
				asset: NativeOrWithId::WithId(10),
				amount: 0,
				elapsed_millis: 60_000,
			}
			.into(),
		);
	});
}

#[test]
fn drip_skips_when_cadence_not_reached() {
	build_and_execute(true, || {
		set_default_budget_allocation();
		System::set_block_number(1);
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: only 30 seconds pass (cadence = 60s)
		advance_time_and_drip(30_000);

		// THEN: nothing minted
		assert_eq!(Balances::balance(&buffer), buffer_before);
	});
}

#[test]
fn drip_fires_after_cadence_reached() {
	build_and_execute(true, || {
		System::set_block_number(1);

		// Set 100% to buffer.
		let allocs = budget_map(&[(b"buffer", 100)]);
		assert_ok!(Dap::set_allocations(RuntimeOrigin::root(), Some(allocs), None));

		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: 30s passes (no drip), then another 30s (total 60s, drip fires)
		advance_time_and_drip(30_000);
		assert_eq!(Balances::balance(&buffer), buffer_before);

		advance_time_and_drip(30_000);
		// 60s elapsed total → TestIssuanceCurve returns 100. All to buffer.
		assert_eq!(Balances::balance(&buffer) - buffer_before, 100);
	});
}

#[test]
fn no_drip_when_budget_not_set() {
	build_and_execute(true, || {
		System::set_block_number(1);

		// GIVEN: no budget allocation set.

		let staker_pot = account_id(500); // TestStakerRecipient pot account
		let balance_before = Balances::balance(&staker_pot);

		// WHEN: drip fires with empty budget — no panic, just early return.
		advance_time_and_drip(60_000);

		// THEN: no funds distributed.
		assert_eq!(Balances::balance(&staker_pot), balance_before);

		// Restore for post-test try_state.
		set_default_budget_allocation();
	});
}

#[test]
fn try_state_fails_with_empty_allocation() {
	build_and_execute(true, || {
		// BudgetAllocation is empty — try_state should catch this.
		assert!(Dap::do_try_state().is_err());

		// Set valid allocation so post-test try_state passes.
		set_default_budget_allocation();
	});
}

#[test]
fn elapsed_ceiling_is_applied() {
	build_and_execute(true, || {
		System::set_block_number(1);

		// Set 100% to buffer.
		let allocs = budget_map(&[(b"buffer", 100)]);
		assert_ok!(Dap::set_allocations(RuntimeOrigin::root(), Some(allocs), None));

		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: 20 minutes pass but MaxElapsedPerDrip = 600_000ms (10 minutes)
		// Without clamping: 1_200_000ms → TestIssuanceCurve returns 2000
		// With clamping: 600_000ms → TestIssuanceCurve returns 1000
		advance_time_and_drip(1_200_000);

		// THEN: issuance based on clamped elapsed (1000, not 2000)
		assert_eq!(Balances::balance(&buffer) - buffer_before, 1000);

		// AND: ElapsedClamped event emitted
		System::assert_has_event(
			Event::<Test>::Unexpected(crate::UnexpectedKind::ElapsedClamped {
				actual_elapsed: 1_200_000,
				ceiling: 600_000,
			})
			.into(),
		);
	});
}

#[test]
fn first_block_initializes_timestamp_without_dripping() {
	// Test that when LastIssuanceTimestamp is 0 (genesis), it initializes without dripping.
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(account_id(1), 100)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();

	ext.execute_with(|| {
		// LastIssuanceTimestamp defaults to 0 (not initialized)
		assert_eq!(crate::LastIssuanceTimestamp::<Test>::get(), 0);

		MockTime::set(1_000_000);
		Dap::drip_issuance();

		// Timestamp should be set but nothing minted.
		assert_eq!(crate::LastIssuanceTimestamp::<Test>::get(), 1_000_000);
		// Total issuance unchanged (only the initial 100 balance).
		assert_eq!(Balances::total_issuance(), 100);
	});
}

#[test]
fn drip_emits_issuance_minted_event() {
	build_and_execute(true, || {
		System::set_block_number(1);

		// Set 100% to buffer so drip distributes.
		let allocs = budget_map(&[(b"buffer", 100)]);
		assert_ok!(Dap::set_allocations(RuntimeOrigin::root(), Some(allocs), None));

		advance_time_and_drip(60_000);

		System::assert_has_event(
			Event::<Test>::IssuanceMinted { total_minted: 100, elapsed_millis: 60_000 }.into(),
		);
	});
}

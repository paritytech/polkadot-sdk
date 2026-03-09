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

//! Tests for budget allocation functionality.
// TODO(ank4n): Verify tests again!
use super::budget_map;
use crate::{
	mock::{new_test_ext, Dap, RuntimeOrigin, System},
	Error, Event,
};
use frame_support::{assert_noop, assert_ok};

#[test]
fn set_budget_allocation_works_with_root() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);

		let allocs =
			budget_map(&[(b"buffer", 20), (b"staker_rewards", 60), (b"validator_incentive", 20)]);

		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), allocs.clone()));

		assert_eq!(crate::BudgetAllocation::<crate::mock::Test>::get(), allocs);
		System::assert_has_event(Event::BudgetAllocationUpdated { allocations: allocs }.into());
	});
}

#[test]
fn set_budget_allocation_rejects_unknown_key() {
	new_test_ext(true).execute_with(|| {
		let allocs = budget_map(&[(b"unknown_key", 50)]);

		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::root(), allocs),
			Error::<crate::mock::Test>::UnknownBudgetKey
		);
	});
}

#[test]
fn set_budget_allocation_rejects_over_100_percent() {
	new_test_ext(true).execute_with(|| {
		let allocs = budget_map(&[(b"buffer", 50), (b"staker_rewards", 60)]);

		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::root(), allocs),
			Error::<crate::mock::Test>::BudgetNotExact
		);
	});
}

#[test]
fn set_budget_allocation_rejects_under_100_percent() {
	new_test_ext(true).execute_with(|| {
		let allocs = budget_map(&[(b"staker_rewards", 50)]);

		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::root(), allocs),
			Error::<crate::mock::Test>::BudgetNotExact
		);
	});
}

#[test]
fn set_budget_allocation_requires_budget_origin() {
	new_test_ext(true).execute_with(|| {
		let allocs = budget_map(&[(b"staker_rewards", 80)]);

		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::signed(1), allocs),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

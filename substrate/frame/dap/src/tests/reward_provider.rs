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

//! Tests for StakingRewardProvider implementation.

use crate::{
	mock::{new_test_ext, Balances, Dap, System},
	Event,
};
use frame_support::traits::fungible::Inspect;
use sp_staking::StakingRewardProvider;

#[test]
fn allocate_era_rewards_funds_pot_account() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era and a system account provided by staking
		System::set_block_number(1);
		let era = 1;
		let total_staked = 1000;
		let era_duration_millis = 60_000;
		// Simulating staking's era system account
		let staking_account = 100_000;

		let balance_before = Balances::balance(&staking_account);

		// WHEN: Allocating era rewards
		let staker_rewards =
			Dap::allocate_era_rewards(era, total_staked, era_duration_millis, &staking_account);

		// THEN: System account balance increases by staker rewards amount
		assert_eq!(Balances::balance(&staking_account), balance_before + staker_rewards);
		// 85 is hardcoded to mint (from TestEraPayout)
		assert_eq!(staker_rewards, 85);
		// EraRewardsAllocated event is emitted
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards, treasury_rewards: 15 }.into(),
		);
	});
}

#[test]
fn treasury_rewards_go_to_buffer() {
	new_test_ext().execute_with(|| {
		// GIVEN: A buffer account with initial balance
		System::set_block_number(1);
		let era = 3;
		let staking_account = 100_003;
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: Allocating era rewards
		let staker_rewards =
			Dap::allocate_era_rewards(era, 1000, 60_000, &staking_account);

		// THEN: Buffer receives treasury portion (15 per TestEraPayout)
		let buffer_after = Balances::balance(&buffer);
		assert_eq!(buffer_after - buffer_before, 15);
		// EraRewardsAllocated event is emitted
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards, treasury_rewards: 15 }.into(),
		);
	});
}

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

use crate::{
	mock::{new_test_ext, Balances, Dap, RuntimeOrigin, System},
	BudgetConfig, Error, Event,
};
use frame_support::{assert_noop, assert_ok, traits::fungible::Inspect};
use sp_runtime::Perbill;
use sp_staking::StakingRewardProvider;

#[test]
fn default_budget_config_is_valid() {
	let config = BudgetConfig::default_config();
	assert!(config.is_valid());
	assert_eq!(config.staker_rewards, Perbill::from_percent(85));
	assert_eq!(config.validator_self_stake_incentive, Perbill::from_percent(0));
	assert_eq!(config.buffer, Perbill::from_percent(15));
}

#[test]
fn budget_config_validation_accepts_valid_configs() {
	// Exactly 100% - balanced split
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(70),
		validator_self_stake_incentive: Perbill::from_percent(15),
		buffer: Perbill::from_percent(15),
	};
	assert!(config.is_valid());

	// Exactly 100% - larger buffer
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(50),
		validator_self_stake_incentive: Perbill::from_percent(10),
		buffer: Perbill::from_percent(40),
	};
	assert!(config.is_valid());

	// Exactly 100% - no buffer
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(85),
		validator_self_stake_incentive: Perbill::from_percent(15),
		buffer: Perbill::from_percent(0),
	};
	assert!(config.is_valid());
}

#[test]
fn budget_config_validation_rejects_invalid_configs() {
	// Over 100%
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(70),
		validator_self_stake_incentive: Perbill::from_percent(20),
		buffer: Perbill::from_percent(20),
	};
	assert!(!config.is_valid());

	// Under 100%
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(50),
		validator_self_stake_incentive: Perbill::from_percent(10),
		buffer: Perbill::from_percent(10),
	};
	assert!(!config.is_valid());

	// Way over 100%
	let config = BudgetConfig {
		staker_rewards: Perbill::from_percent(100),
		validator_self_stake_incentive: Perbill::from_percent(100),
		buffer: Perbill::from_percent(100),
	};
	assert!(!config.is_valid());
}

#[test]
fn set_budget_allocation_works_with_root() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let new_config = BudgetConfig {
			staker_rewards: Perbill::from_percent(70),
			validator_self_stake_incentive: Perbill::from_percent(10),
			buffer: Perbill::from_percent(20),
		};

		// Root can set budget allocation
		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), new_config));

		// Verify storage was updated
		assert_eq!(Dap::budget_allocation(), new_config);

		// Verify event was emitted
		System::assert_has_event(Event::BudgetAllocationUpdated { config: new_config }.into());
	});
}

#[test]
fn set_budget_allocation_rejects_invalid_config() {
	new_test_ext().execute_with(|| {
		let invalid_config = BudgetConfig {
			staker_rewards: Perbill::from_percent(70),
			validator_self_stake_incentive: Perbill::from_percent(40),
			buffer: Perbill::from_percent(10),
		};

		// Should fail with InvalidBudgetConfig
		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::root(), invalid_config),
			Error::<crate::mock::Test>::InvalidBudgetConfig
		);
	});
}

#[test]
fn set_budget_allocation_requires_budget_origin() {
	new_test_ext().execute_with(|| {
		let new_config = BudgetConfig {
			staker_rewards: Perbill::from_percent(80),
			validator_self_stake_incentive: Perbill::from_percent(5),
			buffer: Perbill::from_percent(15),
		};

		// Regular signed origin should fail (in test runtime, BudgetOrigin is EnsureRoot)
		assert_noop!(
			Dap::set_budget_allocation(RuntimeOrigin::signed(1), new_config),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn budget_allocation_affects_era_rewards() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Set a custom budget allocation: 60% stakers, 20% validator incentive, 20% buffer
		let new_config = BudgetConfig {
			staker_rewards: Perbill::from_percent(60),
			validator_self_stake_incentive: Perbill::from_percent(20),
			buffer: Perbill::from_percent(20),
		};
		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), new_config));

		let era = 5;
		let staker_pot_account = 100_005;
		let validator_incentive_pot_account = 100_006;
		let buffer = Dap::buffer_account();

		let staker_balance_before = Balances::balance(&staker_pot_account);
		let validator_incentive_balance_before =
			Balances::balance(&validator_incentive_pot_account);
		let buffer_balance_before = Balances::balance(&buffer);

		// Allocate era rewards (TestEraPayout returns 100 total)
		let allocation = Dap::allocate_era_rewards(
			era,
			1000,
			60_000,
			&staker_pot_account,
			&validator_incentive_pot_account,
		);

		// Verify staker rewards (60% of 100 = 60)
		assert_eq!(allocation.staker_rewards, 60);
		assert_eq!(Balances::balance(&staker_pot_account), staker_balance_before + 60);

		// Verify validator incentive (20% of 100 = 20)
		assert_eq!(allocation.validator_incentive, 20);
		assert_eq!(
			Balances::balance(&validator_incentive_pot_account),
			validator_incentive_balance_before + 20
		);

		// Verify buffer received only its portion (20% of 100 = 20)
		let buffer_increase = Balances::balance(&buffer) - buffer_balance_before;
		assert_eq!(buffer_increase, 20);

		// Verify event (staker=60%, validator_incentive=20%, buffer=20% but gets remainder)
		System::assert_has_event(
			Event::EraRewardsAllocated {
				era,
				staker_rewards: 60,
				validator_incentive: 20,
				buffer_rewards: 20,
			}
			.into(),
		);
	});
}

#[test]
fn budget_allocation_with_zero_treasury() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Set budget with zero buffer: 90% stakers, 10% validator incentive, 0% buffer
		let new_config = BudgetConfig {
			staker_rewards: Perbill::from_percent(90),
			validator_self_stake_incentive: Perbill::from_percent(10),
			buffer: Perbill::from_percent(0),
		};
		assert_ok!(Dap::set_budget_allocation(RuntimeOrigin::root(), new_config));

		let era = 6;
		let staker_pot_account = 100_007;
		let validator_incentive_pot_account = 100_008;
		let buffer = Dap::buffer_account();

		let staker_balance_before = Balances::balance(&staker_pot_account);
		let validator_incentive_balance_before =
			Balances::balance(&validator_incentive_pot_account);
		let buffer_balance_before = Balances::balance(&buffer);

		// Allocate era rewards
		let allocation = Dap::allocate_era_rewards(
			era,
			1000,
			60_000,
			&staker_pot_account,
			&validator_incentive_pot_account,
		);

		// Verify staker rewards (90% of 100 = 90)
		assert_eq!(allocation.staker_rewards, 90);
		assert_eq!(Balances::balance(&staker_pot_account), staker_balance_before + 90);

		// Verify validator incentive (10% of 100 = 10)
		assert_eq!(allocation.validator_incentive, 10);
		assert_eq!(
			Balances::balance(&validator_incentive_pot_account),
			validator_incentive_balance_before + 10
		);

		// Verify buffer received zero (0% of 100 = 0)
		let buffer_increase = Balances::balance(&buffer) - buffer_balance_before;
		assert_eq!(buffer_increase, 0);

		// Verify event (staker=90%, validator_incentive=10%, buffer=0% but gets remainder)
		System::assert_has_event(
			Event::EraRewardsAllocated {
				era,
				staker_rewards: 90,
				validator_incentive: 10,
				buffer_rewards: 0,
			}
			.into(),
		);
	});
}

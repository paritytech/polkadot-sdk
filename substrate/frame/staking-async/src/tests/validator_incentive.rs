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

use super::*;

#[test]
fn set_validator_self_stake_incentive_config_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting all parameters works
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)), // 0.5
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		// Noop does nothing
		assert_storage_noop!(assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		)));

		// Removing works
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
		));
		assert!(!OptimumSelfStake::<Test>::exists());
		assert!(!HardCapSelfStake::<Test>::exists());
		assert!(!SelfStakeSlopeFactor::<Test>::exists());
	});
}

#[test]
fn set_validator_self_stake_incentive_config_requires_admin() {
	ExtBuilder::default().build_and_execute(|| {
		// as setup in mock
		let admin = 1;

		// Non-admin origin should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::signed(2),
				ConfigOp::Set(30_000),
				ConfigOp::Set(100_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			DispatchError::BadOrigin
		);

		// Admin origin should work
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::signed(admin),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_partial_update() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Update only optimum_self_stake
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Noop,
			ConfigOp::Noop,
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		// Update only slope factor
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(Perbill::from_rational(3u32, 4u32)),
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(3u32, 4u32));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_optimum_greater_than_cap() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting both with optimum > cap should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// optimum
				ConfigOp::Set(100_000),
				// hard cap
				ConfigOp::Set(50_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_setting_optimum_greater_than_existing_cap() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config with valid values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Try to update optimum to be greater than existing cap should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// optimum
				ConfigOp::Set(150_000),
				// existing hard cap is 100_000
				ConfigOp::Noop,
				ConfigOp::Noop,
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_setting_cap_less_than_existing_optimum() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config with valid values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Try to update cap to be less than existing optimum should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// existing optimum is 50_000
				ConfigOp::Noop,
				// hard cap
				ConfigOp::Set(30_000),
				ConfigOp::Noop,
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_accepts_equal_values() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting both with optimum = cap should succeed
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(50_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 50_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_removing_parameters() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Removing optimum while keeping cap should succeed (no validation needed)
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Noop,
			ConfigOp::Noop,
		));
		assert!(!OptimumSelfStake::<Test>::exists());
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);

		// Set optimum again
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Noop,
			ConfigOp::Noop,
		));

		// Removing cap while keeping optimum should succeed (no validation needed)
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Remove,
			ConfigOp::Noop,
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert!(!HardCapSelfStake::<Test>::exists());
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_setting_optimum_when_cap_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting optimum when cap is zero (not configured) should succeed
		// because the config is incomplete and won't be used
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(100_000),
			ConfigOp::Noop, // cap remains 0
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 100_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 0); // Still zero
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_setting_cap_when_optimum_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting cap when optimum is zero (not configured) should succeed
		// because the config is incomplete and won't be used
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop, // optimum remains 0
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 0); // Still zero
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
	});
}

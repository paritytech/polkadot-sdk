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
use crate::{session_rotation::Eras, tests::Test, UnbondingQueueConfig, UnbondingQueueParams};
use frame_support::traits::fungible::Inspect;
use sp_runtime::Perbill;
use std::collections::BTreeMap;

/*
- Election tests:
	- (always) ElectableStashes is recording total stake as well
	- (this file) ErasLowestStake Ratio is set at the end of each era correctly + retroactive update
- Unbond
	- (always) ErasTotalUnbond is set
	- (always) Check in try-states of the pallet as well.
	- (this file) Others unbound more in our era: our schedule is delayed
	- (this file) Other rebond at later eras: our schedule is shortened.
- Params
	- Runtime APIs + view functions are correct.

Open questions:
	- Migration strategy:
		* We need an MBM to migrate all ledgers to the new value.
		* New params can already be set.. but since `ErasTotalUnbond` is not set, unbonding will always be at max time.
		* Then, we need at least 28 days for the previous data to be back-filled ()
	- Check the migration overall
	- Talk to UIs already
*/


#[test]
fn set_unbonding_queue_config_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(<Test as Config>::MaxUnbondingDuration::get(), 3);
		assert_eq!(UnbondingQueueParams::<T>::get(), UnbondingQueueConfig::fixed(3));

		// invalid
		assert_noop!(
			Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					min_time: 4,
					max_time: 2,
				})
			),
			Error::<Test>::BoundNotMet
		);

		// invalid
		assert_noop!(
			Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					min_time: 2,
					max_time: 4,
				})
			),
			Error::<Test>::BoundNotMet
		);

		// valid
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				min_time: 2,
				max_time: 3,
			})
		));

		assert_eq!(
			UnbondingQueueParams::<T>::get(),
			UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				min_time: 2,
				max_time: 3,
			}
		);

		// then remove it
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Remove,
		));

		// goes back to default
		assert_eq!(UnbondingQueueParams::<T>::get(), UnbondingQueueConfig::fixed(3));

	});
}

#[test]
fn stores_min_lowest_stake() {
	todo!();
}

#[test]
fn update_to_min_lowest_stake_ratio() {
	// a transaction should allow us to adjust the previous values, if someone claims they are wrong.
	todo!();
}

#[test]
fn tracks_eras_total_unbond() {
	// Also in try-state
	todo!();
}

#[test]
fn unbonding_time_before_min() {
	todo!();
}

#[test]
fn unbonding_time_between_min_and_max() {
	todo!();
}

#[test]
fn unbonding_time_after_max() {
	todo!();
}

#[test]
fn self_rebonding() {
	todo!();
}

#[test]
fn self_more_unbonding() {
	todo!();
}

#[test]
fn others_more_unbonding() {
	todo!();
}

#[test]
fn others_rebonding() {
	todo!();
}


#[test]
fn get_min_lowest_stake_works() {
	ExtBuilder::default()
		.set_stake(11, 10_000)
		.set_stake(21, 11_000)
		.set_stake(31, 400)
		.validator_count(3)
		.nominate(false)
		.unbonding_queue()
		.build_and_execute(|| {

			// Initial conditions.
			assert_eq!(Staking::current_era(), 1);
			assert_eq!(ErasLowestRatioTotalStake::<T>::iter().collect::<Vec<_>>(), vec![]);

			// Setup to nominate.
			assert_ok!(Staking::bond(RuntimeOrigin::signed(999), 100, RewardDestination::Stash));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(999), vec![31]));

			// Era 1 -> 2
			Session::roll_until_active_era(2);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500)])
			);

			// Era 2 -> 3
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(3);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500), (3, 600)])
			);

			// Era 3 -> 4
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(4);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500), (3, 600), (4, 700)])
			);

			// Era 4 -> 5
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(5);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(3, 600), (4, 700), (5, 800)])
			);

			// Era 5 -> 6
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(6);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(4, 700), (5, 800), (6, 900)])
			);
		});
}

#[test]
fn calculate_lowest_total_stake_works() {
	ExtBuilder::default()
		.has_stakers(false)
		.validator_count(4)
		.unbonding_queue()
		.build_and_execute(|| {
			Session::roll_until_active_era(4);
			assert_eq!(current_era(), 4);
			// There are no stakers
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 0), (3, 0), (4, 0)])
			);
			let ed = Balances::minimum_balance();

			// Validator 1
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 1000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 1000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(1), ValidatorPrefs::default()));

			// Validator 2
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 2000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(2), 2000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(2), ValidatorPrefs::default()));

			// Validator 3
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 3, 3000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 3000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()));

			// Validator 4
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 4, 4000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(4), 4000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(4), ValidatorPrefs::default()));

			// Trigger new era to calculate the lowest proportion.
			Session::roll_until_active_era(5);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(3, 0), (4, 0), (5, 1000)])
			);

			Session::roll_until_active_era(6);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(4, 0), (5, 1000), (6, 1000)])
			);

			// Ensure old entry is pruned after bonding duration (3 eras).
			Session::roll_until_active_era(7);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(5, 1000), (6, 1000), (7, 1000)])
			);
		});
}

#[test]
fn correct_unbond_era_is_being_calculated_without_config_set() {
	ExtBuilder::default().build_and_execute(|| {
		// Start at era 1 with known minimum lowest stake
		assert_eq!(UnbondingQueueParams::<Test>::get(), DefaultUnbondingConfig::get());
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// The first attempt before unbonding should yield no unbonds.
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);

		// First unbond
		assert_eq!(Staking::estimate_unbonding_duration(10), 3);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 1, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(1 + 3, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(1), 10);

		// Second unbond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(Staking::estimate_unbonding_duration(10), 3);
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 20, era: 1, previous_unbonded_stake: 10 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(1 + 3, 20)]);
		assert_eq!(Eras::<T>::get_total_unbond(1), 20);
	});
}

#[test]
fn rebonding_after_one_era_and_unbonding_should_place_the_new_unbond_era_in_the_queue() {
	ExtBuilder::default().unbonding_queue().build_and_execute(|| {
		// Start at era 10 with known minimum lowest stake
		Session::roll_until_active_era(10);
		let current_era = Staking::current_era();
		assert_eq!(current_era, 10);

		// First unbond
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);
		assert_eq!(Eras::<T>::get_total_unbond(2), 0);

		assert_eq!(Staking::estimate_unbonding_duration(10), 2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// Second unbond
		Session::roll_until_active_era(11);
		assert_eq!(Staking::estimate_unbonding_duration(500), 2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 500));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 },
				UnlockChunk { value: 500, era: 11, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10), (11 + 2, 500)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);
		assert_eq!(Eras::<T>::get_total_unbond(11), 500);

		assert_ok!(Staking::rebond(RuntimeOrigin::signed(11), 490));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 },
				UnlockChunk { value: 10, era: 11, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10), (11 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);
		assert_eq!(Eras::<T>::get_total_unbond(11), 10);

		// Rebond so that the last chunk gets removed and part of the previous one gets subtracted.
		assert_ok!(Staking::rebond(RuntimeOrigin::signed(11), 15));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 5, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 5)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 5);
		assert_eq!(Eras::<T>::get_total_unbond(11), 0);
	});
}

#[test]
fn test_withdrawing_with_favorable_global_stake_threshold_should_work() {
	ExtBuilder::default().unbonding_queue().build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);

		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::estimate_unbonding_duration(10), 2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);

		// Should be able to withdraw after one era.
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// Should not have withdrawn any funds yet.
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);

		// After one era the user still not be able to withdraw
		Session::roll_until_active_era(11);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// After two eras the user can withdraw.
		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);
		// This must not change. It includes the stake to be withdrawn and one already withdrawn.
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);
	});
}

#[test]
fn test_withdrawing_over_global_stake_threshold_should_not_work() {
	ExtBuilder::default().unbonding_queue().build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);
		// Assume there was no previous stake in any era.
		let _ = ErasLowestRatioTotalStake::<Test>::clear(u32::MAX, None);

		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::estimate_unbonding_duration(10), 3);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 3, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// Should not have withdrawn any funds yet.
		Session::roll_until_active_era(11);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 3, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// With the lowest stake set the user should be able to withdraw.
		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 3, 10)]);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 3, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		// After three eras the user is finally able to withdraw.
		Session::roll_until_active_era(13);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 0);
	});
}

#[test]
fn old_unbonding_chunks_should_be_withdrawable_in_current_era() {
	ExtBuilder::default().unbonding_queue().build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);

		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 0);

		assert_eq!(Staking::estimate_unbonding_duration(10), 2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);

		Session::roll_until_active_era(11);
		assert_eq!(Staking::estimate_unbonding_duration(10), 2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 },
				UnlockChunk { value: 10, era: 11, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10), (11 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);
		assert_eq!(Eras::<T>::get_total_unbond(11), 10);

		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10), (11 + 2, 10)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 10);
		assert_eq!(Eras::<T>::get_total_unbond(11), 10);

		// Now both unbonds get collapsed in a single entry.
		Session::roll_until_active_era(13);
		assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(13, 20)]);
		assert_eq!(Eras::<T>::get_total_unbond(10), 0);
		assert_eq!(Eras::<T>::get_total_unbond(11), 10);
	});
}

#[test]
fn increasing_unbond_amount_should_delay_expected_withdrawal() {
	ExtBuilder::default()
		.has_stakers(false)
		.unbonding_queue()
		.build_and_execute(|| {
			let ed = Balances::minimum_balance();

			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 11, 10_000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(11), 10_000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(11), ValidatorPrefs::default()));

			Session::roll_until_active_era(10);
			assert_eq!(Staking::current_era(), 10);

			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![]
			);
			assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![]);
			assert_eq!(Eras::<T>::get_total_unbond(10), 0);

			assert_eq!(Staking::estimate_unbonding_duration(10), 2);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
			);
			assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 2, 10)]);
			assert_eq!(Eras::<T>::get_total_unbond(10), 10);

			// Unbond a huge amount of stake.
			assert_eq!(Staking::estimate_unbonding_duration(10), 2);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 8000 - 10));
			assert_eq!(Staking::estimate_unbonding_duration(10), 3);
			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![UnlockChunk { value: 8000, era: 10, previous_unbonded_stake: 10 }]
			);

			// The expected release has been increased.
			assert_eq!(Staking::unbonding_schedule(11).unwrap(), vec![(10 + 3, 8000)]);
			assert_eq!(Eras::<T>::get_total_unbond(10), 8000);
		});
}

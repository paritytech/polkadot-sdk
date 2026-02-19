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

use sp_staking::{Stake, StakingInterface, StakingUnchecked};

use super::*;

#[test]
fn virtual_bond_does_not_lock() {
	ExtBuilder::default().build_and_execute(|| {
		mock::start_active_era(1);
		assert_eq!(asset::total_balance::<Test>(&10), 1);
		// 10 can bond more than its balance amount since we do not require lock for virtual
		// bonding.
		assert_ok!(<Staking as StakingUnchecked>::virtual_bond(&10, 100, &15));
		// nothing is locked on 10.
		assert_eq!(asset::staked::<Test>(&10), 0);
		// adding more balance does not lock anything as well.
		assert_ok!(<Staking as StakingInterface>::bond_extra(&10, 1000));
		// but ledger is updated correctly.
		assert_eq!(
			<Staking as StakingInterface>::stake(&10),
			Ok(Stake { total: 1100, active: 1100 })
		);

		// lets try unbonding some amount.
		assert_ok!(<Staking as StakingInterface>::unbond(&10, 200));
		assert_eq!(
			Staking::ledger(10.into()).unwrap(),
			StakingLedgerInspect {
				stash: 10,
				total: 1100,
				active: 1100 - 200,
				unlocking: bounded_vec![UnlockChunk { value: 200, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		assert_eq!(
			<Staking as StakingInterface>::stake(&10),
			Ok(Stake { total: 1100, active: 900 })
		);
		// still no locks.
		assert_eq!(asset::staked::<Test>(&10), 0);

		mock::start_active_era(2);
		// cannot withdraw without waiting for unbonding period.
		assert_ok!(<Staking as StakingInterface>::withdraw_unbonded(10, 0));
		assert_eq!(
			<Staking as StakingInterface>::stake(&10),
			Ok(Stake { total: 1100, active: 900 })
		);

		// in era 4, 10 can withdraw unlocking amount.
		mock::start_active_era(4);
		assert_ok!(<Staking as StakingInterface>::withdraw_unbonded(10, 0));
		assert_eq!(
			<Staking as StakingInterface>::stake(&10),
			Ok(Stake { total: 900, active: 900 })
		);

		// unbond all.
		assert_ok!(<Staking as StakingInterface>::unbond(&10, 900));
		assert_eq!(
			<Staking as StakingInterface>::stake(&10),
			Ok(Stake { total: 900, active: 0 })
		);
		mock::start_active_era(7);
		assert_ok!(<Staking as StakingInterface>::withdraw_unbonded(10, 0));

		// ensure withdrawing all amount cleans up storage.
		assert_eq!(Staking::ledger(10.into()), Err(Error::<Test>::NotStash));
		assert_eq!(VirtualStakers::<Test>::contains_key(10), false);
	})
}

#[test]
fn virtual_staker_cannot_pay_reward_to_self_account() {
	ExtBuilder::default().build_and_execute(|| {
		// cannot set payee to self
		assert_noop!(
			<Staking as StakingUnchecked>::virtual_bond(&10, 100, &10),
			Error::<Test>::RewardDestinationRestricted
		);

		// to another account works
		assert_ok!(<Staking as StakingUnchecked>::virtual_bond(&10, 100, &11));

		// cannot set via set_payee as well.
		assert_noop!(
			<Staking as StakingInterface>::set_payee(&10, &10),
			Error::<Test>::RewardDestinationRestricted
		);
	});
}

#[test]
fn virtual_staker_cannot_bond_again() {
	ExtBuilder::default().build_and_execute(|| {
		// 200 virtual bonds
		bond_virtual_nominator(200, 201, 500, vec![11, 21]);

		// Tries bonding again
		assert_noop!(
			<Staking as StakingUnchecked>::virtual_bond(&200, 200, &201),
			Error::<Test>::AlreadyBonded
		);

		// And again with a different reward destination.
		assert_noop!(
			<Staking as StakingUnchecked>::virtual_bond(&200, 200, &202),
			Error::<Test>::AlreadyBonded
		);

		// Direct bond is not allowed as well.
		assert_noop!(
			<Staking as StakingInterface>::bond(&200, 200, &202),
			Error::<Test>::AlreadyBonded
		);
	});
}

#[test]
fn normal_staker_cannot_virtual_bond() {
	ExtBuilder::default().build_and_execute(|| {
		// 101 is a nominator trying to virtual bond
		assert_noop!(
			<Staking as StakingUnchecked>::virtual_bond(&101, 200, &102),
			Error::<Test>::AlreadyBonded
		);

		// validator 21 tries to virtual bond
		assert_noop!(
			<Staking as StakingUnchecked>::virtual_bond(&21, 200, &22),
			Error::<Test>::AlreadyBonded
		);
	});
}

#[test]
fn migrate_virtual_staker() {
	ExtBuilder::default().build_and_execute(|| {
		// give some balance to 200
		asset::set_stakeable_balance::<Test>(&200, 2000);

		// stake
		assert_ok!(Staking::bond(RuntimeOrigin::signed(200), 1000, RewardDestination::Staked));
		assert_eq!(asset::staked::<Test>(&200), 1000);

		// migrate them to virtual staker
		assert_ok!(<Staking as StakingUnchecked>::migrate_to_virtual_staker(&200));
		// payee needs to be updated to a non-stash account.
		assert_ok!(<Staking as StakingInterface>::set_payee(&200, &201));

		// ensure the balance is not locked anymore
		assert_eq!(asset::staked::<Test>(&200), 0);

		// and they are marked as virtual stakers
		assert_eq!(Pallet::<Test>::is_virtual_staker(&200), true);
	});
}

#[test]
fn virtual_nominators_are_lazily_slashed() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			let slash_percent = Perbill::from_percent(5);
			let initial_exposure = Staking::eras_stakers(active_era(), &11);
			// 101 is a nominator for 11
			assert_eq!(initial_exposure.others.first().unwrap().who, 101);
			// make 101 a virtual nominator
			assert_ok!(<Staking as StakingUnchecked>::migrate_to_virtual_staker(&101));
			// set payee different to self.
			assert_ok!(<Staking as StakingInterface>::set_payee(&101, &102));

			// cache values
			let nominator_stake = Staking::ledger(101.into()).unwrap().active;
			let nominator_balance = balances(&101).0;
			let validator_stake = Staking::ledger(11.into()).unwrap().active;
			let validator_balance = balances(&11).0;
			let exposed_stake = initial_exposure.total;
			let exposed_validator = initial_exposure.own;
			let exposed_nominator = initial_exposure.others.first().unwrap().value;

			// 11 goes offline
			on_offence_now(&[offence_from(11, None)], &[slash_percent]);

			let slash_amount = slash_percent * exposed_stake;
			let validator_share =
				Perbill::from_rational(exposed_validator, exposed_stake) * slash_amount;
			let nominator_share =
				Perbill::from_rational(exposed_nominator, exposed_stake) * slash_amount;

			// both slash amounts need to be positive for the test to make sense.
			assert!(validator_share > 0);
			assert!(nominator_share > 0);

			// both stakes must have been decreased pro-rata.
			assert_eq!(
				Staking::ledger(101.into()).unwrap().active,
				nominator_stake - nominator_share
			);
			assert_eq!(
				Staking::ledger(11.into()).unwrap().active,
				validator_stake - validator_share
			);

			// validator balance is slashed as usual
			assert_eq!(balances(&11).0, validator_balance - validator_share);
			// Because slashing happened.
			assert!(is_disabled(11));

			// but virtual nominator's balance is not slashed.
			assert_eq!(asset::stakeable_balance::<Test>(&101), nominator_balance);
			// but slash is broadcasted to slash observers.
			assert_eq!(SlashObserver::get().get(&101).unwrap(), &nominator_share);
		})
}

#[test]
fn virtual_stakers_cannot_be_reaped() {
	ExtBuilder::default()
		// we need enough validators such that disables are allowed.
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			// make 101 only nominate 11.
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![11]));

			mock::start_active_era(1);

			// slash all stake.
			let slash_percent = Perbill::from_percent(100);
			let initial_exposure = Staking::eras_stakers(active_era(), &11);
			// 101 is a nominator for 11
			assert_eq!(initial_exposure.others.first().unwrap().who, 101);
			// make 101 a virtual nominator
			assert_ok!(<Staking as StakingUnchecked>::migrate_to_virtual_staker(&101));
			// set payee different to self.
			assert_ok!(<Staking as StakingInterface>::set_payee(&101, &102));

			// cache values
			let validator_balance = asset::stakeable_balance::<Test>(&11);
			let validator_stake = Staking::ledger(11.into()).unwrap().total;
			let nominator_balance = asset::stakeable_balance::<Test>(&101);
			let nominator_stake = Staking::ledger(101.into()).unwrap().total;

			// 11 goes offline
			on_offence_now(&[offence_from(11, None)], &[slash_percent]);

			// both stakes must have been decreased to 0.
			assert_eq!(Staking::ledger(101.into()).unwrap().active, 0);
			assert_eq!(Staking::ledger(11.into()).unwrap().active, 0);

			// all validator stake is slashed
			assert_eq_error_rate!(
				validator_balance - validator_stake,
				asset::stakeable_balance::<Test>(&11),
				1
			);
			// Because slashing happened.
			assert!(is_disabled(11));

			// Virtual nominator's balance is not slashed.
			assert_eq!(asset::stakeable_balance::<Test>(&101), nominator_balance);
			// Slash is broadcasted to slash observers.
			assert_eq!(SlashObserver::get().get(&101).unwrap(), &nominator_stake);

			// validator can be reaped.
			assert_ok!(Staking::reap_stash(RuntimeOrigin::signed(10), 11, u32::MAX));
			// nominator is a virtual staker and cannot be reaped.
			assert_noop!(
				Staking::reap_stash(RuntimeOrigin::signed(10), 101, u32::MAX),
				Error::<Test>::VirtualStakerNotAllowed
			);
		})
}

#[test]
fn restore_ledger_not_allowed_for_virtual_stakers() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		setup_double_bonded_ledgers();
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);
		// 333 is corrupted
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
		// migrate to virtual staker.
		assert_ok!(<Staking as StakingUnchecked>::migrate_to_virtual_staker(&333));

		// recover the ledger won't work for virtual staker
		assert_noop!(
			Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None),
			Error::<Test>::VirtualStakerNotAllowed
		);

		// migrate 333 back to normal staker
		<VirtualStakers<Test>>::remove(333);

		// try restore again
		assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));
	})
}

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

//! Tests for the module.

use super::*;
use crate::{asset, ledger::StakingLedgerInspect, mock::Session};
use frame_election_provider_support::{
	bounds::{DataProviderBounds, ElectionBoundsBuilder},
	SortedListProvider,
};
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop, hypothetically,
	pallet_prelude::*,
	traits::{Get, ReservableCurrency},
};
use mock::*;
use sp_runtime::{
	assert_eq_error_rate, bounded_vec, traits::BadOrigin, Perbill, Percent, TokenError,
};
use substrate_test_utils::assert_eq_uvec;

mod bonding;
mod configs;
mod controller;
mod election;
mod election_data_provider;
mod era_rotation;
mod force_unstake_kill_stash;
mod rewards;
mod slashing;
mod voter_list;

#[test]
fn basic_setup_session_queuing_should_work() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq!(Session::current_index(), 3);

		// put some money in account that we'll use.
		for i in 1..5 {
			let _ = asset::set_stakeable_balance::<Test>(&i, 2000);
		}

		// add a new candidate for being a validator. account 3.
		assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 1500, RewardDestination::Account(3)));
		assert_ok!(Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()));

		// No effects will be seen so far.
		assert_eq_uvec!(Session::validators(), vec![21, 11]);

		Session::roll_until_session(4);
		assert_eq_uvec!(Session::validators(), vec![21, 11]);
		assert_eq!(Session::queued_validators(), None);

		Session::roll_until_session(5);
		assert_eq_uvec!(Session::validators(), vec![21, 11]);
		assert_eq_uvec!(Session::queued_validators().unwrap(), vec![21, 3]);

		Session::roll_until_session(6);
		assert_eq_uvec!(Session::validators(), vec![21, 3]);
		assert_eq!(Session::queued_validators(), None);

		// then chill 3
		Staking::chill(RuntimeOrigin::signed(3)).unwrap();

		// nothing. 3 is still there.
		Session::roll_until_session(7);
		assert_eq_uvec!(Session::validators(), vec![21, 3]);

		Session::roll_until_session(8);
		assert_eq_uvec!(Session::validators(), vec![21, 3]);

		// now are back -- 3 is gone
		Session::roll_until_session(9);
		assert_eq_uvec!(Session::validators(), vec![21, 11]);

		// 3 is still bonded though
		assert_eq!(
			Staking::ledger(3.into()).unwrap(),
			StakingLedgerInspect {
				stash: 3,
				total: 1500,
				active: 1500,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// e.g. it cannot reserve more than 500 that it has free from the total 2000
		assert_noop!(Balances::reserve(&3, 501), DispatchError::ConsumerRemaining);
		assert_ok!(Balances::reserve(&3, 409));
	});
}

#[test]
fn blocking_and_kicking_works() {
	ExtBuilder::default()
		.minimum_validator_count(1)
		.validator_count(4)
		.nominate(true)
		.build_and_execute(|| {
			// given
			assert_ok!(Staking::validate(
				RuntimeOrigin::signed(11),
				ValidatorPrefs { blocked: true, ..Default::default() }
			));

			// attempt to nominate from 101
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![11]));

			// should have worked since we're already nominated them
			assert_eq!(Nominators::<Test>::get(&101).unwrap().targets, vec![11]);

			// kick the nominator
			assert_ok!(Staking::kick(RuntimeOrigin::signed(11), vec![101]));

			// should have been kicked now
			assert!(Nominators::<Test>::get(&101).unwrap().targets.is_empty());

			// attempt to nominate from 100/101...
			assert_noop!(
				Staking::nominate(RuntimeOrigin::signed(101), vec![11]),
				Error::<Test>::BadTarget
			);
		});
}

#[test]
fn basic_setup_works() {
	// Verifies initial conditions of mock
	ExtBuilder::default().build_and_execute(|| {
		// Account 11 is stashed and locked, and is the controller
		assert_eq!(Staking::bonded(&11), Some(11));
		// Account 21 is stashed and locked and is the controller
		assert_eq!(Staking::bonded(&21), Some(21));
		// Account 1 is not a stashed
		assert_eq!(Staking::bonded(&1), None);

		// Account 11 controls its own stash, which is 100 * balance_factor units
		assert_eq!(
			Ledger::get(&11).unwrap(),
			StakingLedgerInspect::<Test> {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		// Account 21 controls its own stash, which is 200 * balance_factor units
		assert_eq!(
			Ledger::get(&21).unwrap(),
			StakingLedgerInspect::<Test> {
				stash: 21,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		// Account 1 does not control any stash
		assert!(Staking::ledger(1.into()).is_err());

		// ValidatorPrefs are default
		assert_eq_uvec!(
			<Validators<Test>>::iter().collect::<Vec<_>>(),
			vec![
				(31, ValidatorPrefs::default()),
				(21, ValidatorPrefs::default()),
				(11, ValidatorPrefs::default())
			]
		);

		// check the single nominators we have
		assert_eq!(
			Staking::ledger(101.into()).unwrap(),
			StakingLedgerInspect {
				stash: 101,
				total: 500,
				active: 500,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

		assert_eq!(
			Staking::eras_stakers(active_era(), &11),
			Exposure {
				total: 1250,
				own: 1000,
				others: vec![IndividualExposure { who: 101, value: 250 }]
			},
		);
		assert_eq!(
			Staking::eras_stakers(active_era(), &21),
			Exposure {
				total: 1250,
				own: 1000,
				others: vec![IndividualExposure { who: 101, value: 250 }]
			},
		);

		// Current active and planned era
		assert_eq!(active_era(), 1);
		assert_eq!(current_era(), 1);
		assert_eq!(Session::current_index(), 3);
		assert_eq_uvec!(Session::validators(), vec![11, 21]);

		// initial total stake = 1125 + 1375
		assert_eq!(ErasTotalStake::<Test>::get(active_era()), 2500);

		// The number of validators required.
		assert_eq!(ValidatorCount::<Test>::get(), 2);

		// New era is not being forced
		assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

		// Events so far
		assert_eq!(
			staking_events(),
			vec![
				Event::SessionRotated { starting_session: 1, active_era: 0, planned_era: 1 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 2, active_era: 0, planned_era: 1 },
				Event::EraPaid { era_index: 0, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 3, active_era: 1, planned_era: 1 }
			]
		);
	});
}

#[test]
fn basic_setup_session_rotation() {
	ExtBuilder::default().build_and_execute(|| {
		// our initial clean state at active era 1.
		assert_eq!(active_era(), 1);
		assert_eq!(current_era(), 1);
		assert_eq!(Session::current_index(), 3);
		assert_eq_uvec!(Session::validators(), vec![11, 21]);

		// roll one session, we have planned our era
		Session::roll_to_next_session();
		assert_eq!(Session::current_index(), 4);
		assert_eq!(current_era(), 2);
		assert_eq!(active_era(), 1);

		// roll one session, still in planning, and something is queued in session now.
		Session::roll_to_next_session();
		assert_eq!(Session::current_index(), 5);
		assert_eq!(active_era(), 1);
		assert_eq!(current_era(), 2);

		// roll one session, we activate the era.
		Session::roll_to_next_session();
		assert_eq!(Session::current_index(), 6);
		assert_eq!(active_era(), 2);
		assert_eq!(current_era(), 2);
	});
}

#[test]
fn basic_setup_sessions_per_era() {
	ExtBuilder::default()
		.session_per_era(6)
		.no_flush_events()
		.build_and_execute(|| {
			// test state forwards us to the end of session 6 / active era 1
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 1, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 2, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 3, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 4, active_era: 0, planned_era: 1 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 5, active_era: 0, planned_era: 1 },
					Event::EraPaid { era_index: 0, validator_payout: 15000, remainder: 15000 },
					Event::SessionRotated { starting_session: 6, active_era: 1, planned_era: 1 }
				]
			);
			assert_eq!(Session::current_index(), 6);
			assert_eq!(active_era(), 1);

			Session::roll_until_active_era(2);
			assert_eq!(Session::current_index(), 12);
			assert_eq!(active_era(), 2);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 9, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 10, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 11, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 15000, remainder: 15000 },
					Event::SessionRotated { starting_session: 12, active_era: 2, planned_era: 2 }
				]
			);
		});
}

/*

#[test]
fn max_staked_rewards_default_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(<MaxStakedRewards<Test>>::get(), None);

		let default_stakers_payout = validator_payout_for(time_per_era());
		assert!(default_stakers_payout > 0);
		start_active_era(1);

		// the final stakers reward is the same as the reward before applied the cap.
		assert_eq!(ErasValidatorReward::<Test>::get(0).unwrap(), default_stakers_payout);

		// which is the same behaviour if the `MaxStakedRewards` is set to 100%.
		<MaxStakedRewards<Test>>::set(Some(Percent::from_parts(100)));

		let default_stakers_payout = validator_payout_for(time_per_era());
		assert_eq!(ErasValidatorReward::<Test>::get(0).unwrap(), default_stakers_payout);
	})
}

#[test]
fn max_staked_rewards_works() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		let max_staked_rewards = 10;

		// sets new max staked rewards through set_staking_configs.
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(Percent::from_percent(max_staked_rewards)),
		));

		assert_eq!(<MaxStakedRewards<Test>>::get(), Some(Percent::from_percent(10)));

		// check validators account state.
		assert_eq!(Session::validators().len(), 2);
		assert!(Session::validators().contains(&11) & Session::validators().contains(&21));
		// balance of the mock treasury account is 0
		assert_eq!(RewardRemainderUnbalanced::get(), 0);

		let max_stakers_payout = validator_payout_for(time_per_era());

		start_active_era(1);

		let treasury_payout = RewardRemainderUnbalanced::get();
		let validators_payout = ErasValidatorReward::<Test>::get(0).unwrap();
		let total_payout = treasury_payout + validators_payout;

		// max stakers payout (without max staked rewards cap applied) is larger than the final
		// validator rewards. The final payment and remainder should be adjusted by redistributing
		// the era inflation to apply the cap...
		assert!(max_stakers_payout > validators_payout);

		// .. which means that the final validator payout is 10% of the total payout..
		assert_eq!(validators_payout, Percent::from_percent(max_staked_rewards) * total_payout);
		// .. and the remainder 90% goes to the treasury.
		assert_eq!(
			treasury_payout,
			Percent::from_percent(100 - max_staked_rewards) * (treasury_payout + validators_payout)
		);
	})
}

#[test]
fn reward_to_stake_works() {
	ExtBuilder::default()
		.nominate(false)
		.set_status(31, StakerStatus::Idle)
		.set_status(41, StakerStatus::Idle)
		.set_stake(21, 2000)
		.try_state(false)
		.build_and_execute(|| {
			assert_eq!(ValidatorCount::<Test>::get(), 2);
			// Confirm account 10 and 20 are validators
			assert!(<Validators<Test>>::contains_key(&11) && <Validators<Test>>::contains_key(&21));

			assert_eq!(Staking::eras_stakers(active_era(), &11).total, 1000);
			assert_eq!(Staking::eras_stakers(active_era(), &21).total, 2000);

			// Give the man some money.
			let _ = asset::set_stakeable_balance::<Test>(&10, 1000);
			let _ = asset::set_stakeable_balance::<Test>(&20, 1000);

			// Bypass logic and change current exposure
			Eras::<Test>::upsert_exposure(
				0,
				&21,
				Exposure { total: 69, own: 69, others: vec![] },
			);
			<Ledger<Test>>::insert(
				&20,
				StakingLedgerInspect {
					stash: 21,
					total: 69,
					active: 69,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				},
			);

			// Compute total payout now for whole duration as other parameter won't change
			let validator_payout_0 = validator_payout_for(time_per_era());
			Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
			Pallet::<Test>::reward_by_ids(vec![(21, 1)]);

			// New era --> rewards are paid --> stakes are changed
			mock::start_active_era(1);
			mock::make_all_reward_payment(0);

			assert_eq!(Staking::eras_stakers(active_era(), &11).total, 1000);
			assert_eq!(Staking::eras_stakers(active_era(), &21).total, 2000);

			let _11_balance = asset::stakeable_balance::<Test>(&11);
			assert_eq!(_11_balance, 1000 + validator_payout_0 / 2);

			// Trigger another new era as the info are frozen before the era start.
			mock::start_active_era(2);

			// -- new infos
			assert_eq!(Staking::eras_stakers(active_era(), &11).total, 1000 + validator_payout_0 / 2);
			assert_eq!(Staking::eras_stakers(active_era(), &21).total, 2000 + validator_payout_0 / 2);
		});
}

#[test]
fn reap_stash_works() {
	ExtBuilder::default()
		.existential_deposit(10)
		.balance_factor(10)
		.build_and_execute(|| {
			// given
			assert_eq!(asset::staked::<Test>(&11), 10 * 1000);
			assert_eq!(Staking::bonded(&11), Some(11));

			assert!(<Ledger<Test>>::contains_key(&11));
			assert!(<Bonded<Test>>::contains_key(&11));
			assert!(<Validators<Test>>::contains_key(&11));
			assert!(<Payee<Test>>::contains_key(&11));

			// stash is not reapable
			assert_noop!(
				Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0),
				Error::<Test>::FundedTarget
			);

			// no easy way to cause an account to go below ED, we tweak their staking ledger
			// instead.
			Ledger::<Test>::insert(11, StakingLedger::<Test>::new(11, 5));

			// reap-able
			assert_ok!(Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0));

			// then
			assert!(!<Ledger<Test>>::contains_key(&11));
			assert!(!<Bonded<Test>>::contains_key(&11));
			assert!(!<Validators<Test>>::contains_key(&11));
			assert!(!<Payee<Test>>::contains_key(&11));
			// lock is removed.
			assert_eq!(asset::staked::<Test>(&11), 0);
		});
}

#[test]
fn reap_stash_works_with_existential_deposit_zero() {
	ExtBuilder::default()
		.existential_deposit(0)
		.balance_factor(10)
		.build_and_execute(|| {
			// given
			assert_eq!(asset::staked::<Test>(&11), 10 * 1000);
			assert_eq!(Staking::bonded(&11), Some(11));

			assert!(<Ledger<Test>>::contains_key(&11));
			assert!(<Bonded<Test>>::contains_key(&11));
			assert!(<Validators<Test>>::contains_key(&11));
			assert!(<Payee<Test>>::contains_key(&11));

			// stash is not reapable
			assert_noop!(
				Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0),
				Error::<Test>::FundedTarget
			);

			// no easy way to cause an account to go below ED, we tweak their staking ledger
			// instead.
			Ledger::<Test>::insert(11, StakingLedger::<Test>::new(11, 0));

			// reap-able
			assert_ok!(Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0));

			// then
			assert!(!<Ledger<Test>>::contains_key(&11));
			assert!(!<Bonded<Test>>::contains_key(&11));
			assert!(!<Validators<Test>>::contains_key(&11));
			assert!(!<Payee<Test>>::contains_key(&11));
			// lock is removed.
			assert_eq!(asset::staked::<Test>(&11), 0);
		});
}

#[test]
fn switching_roles() {
	// Test that it should be possible to switch between roles (nominator, validator, idle) with
	// minimal overhead.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// Reset reward destination
		for i in &[11, 21] {
			assert_ok!(Staking::set_payee(RuntimeOrigin::signed(*i), RewardDestination::Stash));
		}

		assert_eq_uvec!(session_validators(), vec![21, 11]);

		// put some money in account that we'll use.
		for i in 1..7 {
			let _ = Balances::deposit_creating(&i, 5000);
		}

		// add 2 nominators
		assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 2000, RewardDestination::Account(1)));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(1), vec![11, 5]));

		assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 500, RewardDestination::Account(3)));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![21, 1]));

		// add a new validator candidate
		assert_ok!(Staking::bond(RuntimeOrigin::signed(5), 1000, RewardDestination::Account(5)));
		assert_ok!(Staking::validate(RuntimeOrigin::signed(5), ValidatorPrefs::default()));
		assert_ok!(Session::set_keys(
			RuntimeOrigin::signed(5),
			SessionKeys { other: 6.into() },
			vec![]
		));

		mock::start_active_era(1);

		// with current nominators 11 and 5 have the most stake
		assert_eq_uvec!(session_validators(), vec![5, 11]);

		// 2 decides to be a validator. Consequences:
		assert_ok!(Staking::validate(RuntimeOrigin::signed(1), ValidatorPrefs::default()));
		assert_ok!(Session::set_keys(
			RuntimeOrigin::signed(1),
			SessionKeys { other: 2.into() },
			vec![]
		));
		// new stakes:
		// 11: 1000 self vote
		// 21: 1000 self vote + 250 vote
		// 5 : 1000 self vote
		// 1 : 2000 self vote + 250 vote.
		// Winners: 21 and 1

		mock::start_active_era(2);

		assert_eq_uvec!(session_validators(), vec![1, 21]);
	});
}

#[test]
fn wrong_vote_is_moot() {
	ExtBuilder::default()
		.add_staker(
			61,
			61,
			500,
			StakerStatus::Nominator(vec![
				11, 21, // good votes
				1, 2, 15, 1000, 25, // crap votes. No effect.
			]),
		)
		.build_and_execute(|| {
			// the genesis validators already reflect the above vote, nonetheless start a new era.
			mock::start_active_era(1);

			// new validators
			assert_eq_uvec!(session_validators(), vec![21, 11]);

			// our new voter is taken into account
			assert!(Staking::eras_stakers(active_era(), &11).others.iter().any(|i| i.who == 61));
			assert!(Staking::eras_stakers(active_era(), &21).others.iter().any(|i| i.who == 61));
		});
}

#[test]
fn bond_with_no_staked_value() {
	// Behavior when someone bonds with no staked value.
	// Particularly when they votes and the candidate is elected.
	ExtBuilder::default()
		.validator_count(3)
		.existential_deposit(5)
		.balance_factor(5)
		.nominate(false)
		.minimum_validator_count(1)
		.build_and_execute(|| {
			// Can't bond with 1
			assert_noop!(
				Staking::bond(RuntimeOrigin::signed(1), 1, RewardDestination::Account(1)),
				Error::<Test>::InsufficientBond,
			);
			// bonded with absolute minimum value possible.
			assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 5, RewardDestination::Account(1)));
			assert_eq!(pallet_balances::Holds::<Test>::get(&1)[0].amount, 5);

			// unbonding even 1 will cause all to be unbonded.
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(1), 1));
			assert_eq!(
				Staking::ledger(1.into()).unwrap(),
				StakingLedgerInspect {
					stash: 1,
					active: 0,
					total: 5,
					unlocking: bounded_vec![UnlockChunk { value: 5, era: 3 }],
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			mock::start_active_era(1);
			mock::start_active_era(2);

			// not yet removed.
			assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(1), 0));
			assert!(Staking::ledger(1.into()).is_ok());
			assert_eq!(pallet_balances::Holds::<Test>::get(&1)[0].amount, 5);

			mock::start_active_era(3);

			// poof. Account 1 is removed from the staking system.
			assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(1), 0));
			assert!(Staking::ledger(1.into()).is_err());
			assert_eq!(pallet_balances::Holds::<Test>::get(&1).len(), 0);
		});
}

#[test]
fn bond_with_little_staked_value_bounded() {
	ExtBuilder::default()
		.validator_count(3)
		.nominate(false)
		.minimum_validator_count(1)
		.build_and_execute(|| {
			// setup
			assert_ok!(Staking::chill(RuntimeOrigin::signed(31)));
			assert_ok!(Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Stash));
			let init_balance_1 = asset::stakeable_balance::<Test>(&1);
			let init_balance_11 = asset::stakeable_balance::<Test>(&11);

			// Stingy validator.
			assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 1, RewardDestination::Account(1)));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(1), ValidatorPrefs::default()));
			assert_ok!(Session::set_keys(
				RuntimeOrigin::signed(1),
				SessionKeys { other: 1.into() },
				vec![]
			));

			// 1 era worth of reward. BUT, we set the timestamp after on_initialize, so outdated by
			// one block.
			let validator_payout_0 = validator_payout_for(time_per_era());

			reward_all_elected();
			mock::start_active_era(1);
			mock::make_all_reward_payment(0);

			// 1 is elected.
			assert_eq_uvec!(session_validators(), vec![21, 11, 1]);
			assert_eq!(Staking::eras_stakers(active_era(), &2).total, 0);

			// Old ones are rewarded.
			assert_eq_error_rate!(
				asset::stakeable_balance::<Test>(&11),
				init_balance_11 + validator_payout_0 / 3,
				1
			);
			// no rewards paid to 2. This was initial election.
			assert_eq!(asset::stakeable_balance::<Test>(&1), init_balance_1);

			// reward era 2
			let total_payout_1 = validator_payout_for(time_per_era());
			reward_all_elected();
			mock::start_active_era(2);
			mock::make_all_reward_payment(1);

			assert_eq_uvec!(session_validators(), vec![21, 11, 1]);
			assert_eq!(Staking::eras_stakers(active_era(), &2).total, 0);

			// 2 is now rewarded.
			assert_eq_error_rate!(
				asset::stakeable_balance::<Test>(&1),
				init_balance_1 + total_payout_1 / 3,
				1
			);
			assert_eq_error_rate!(
				asset::stakeable_balance::<Test>(&11),
				init_balance_11 + validator_payout_0 / 3 + total_payout_1 / 3,
				2,
			);
		});
}

#[test]
fn bond_with_duplicate_vote_should_be_ignored_by_election_provider() {
	ExtBuilder::default()
		.validator_count(2)
		.nominate(false)
		.minimum_validator_count(1)
		.set_stake(31, 1000)
		.build_and_execute(|| {
			// ensure all have equal stake.
			assert_eq!(
				<Validators<Test>>::iter()
					.map(|(v, _)| (v, Staking::ledger(v.into()).unwrap().total))
					.collect::<Vec<_>>(),
				vec![(31, 1000), (21, 1000), (11, 1000)],
			);
			// no nominators shall exist.
			assert!(<Nominators<Test>>::iter().map(|(n, _)| n).collect::<Vec<_>>().is_empty());

			// give the man some money.
			let initial_balance = 1000;
			for i in [1, 2, 3, 4].iter() {
				let _ = asset::set_stakeable_balance::<Test>(&i, initial_balance);
			}

			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(1),
				1000,
				RewardDestination::Account(1)
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(1), vec![11, 11, 11, 21, 31]));

			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(3),
				1000,
				RewardDestination::Account(3)
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![21, 31]));

			// winners should be 21 and 31. Otherwise this election is taking duplicates into
			// account.
			let supports = <Test as Config>::ElectionProvider::elect(SINGLE_PAGE).unwrap();

			let expected_supports = vec![
				(21, Support { total: 1800, voters: vec![(21, 1000), (1, 400), (3, 400)] }),
				(31, Support { total: 2200, voters: vec![(31, 1000), (1, 600), (3, 600)] }),
			];

			assert_eq!(supports, to_bounded_supports(expected_supports));
		});
}

#[test]
fn bond_with_duplicate_vote_should_be_ignored_by_election_provider_elected() {
	// same as above but ensures that even when the dupe is being elected, everything is sane.
	ExtBuilder::default()
		.validator_count(2)
		.nominate(false)
		.set_stake(31, 1000)
		.minimum_validator_count(1)
		.build_and_execute(|| {
			// ensure all have equal stake.
			assert_eq!(
				<Validators<Test>>::iter()
					.map(|(v, _)| (v, Staking::ledger(v.into()).unwrap().total))
					.collect::<Vec<_>>(),
				vec![(31, 1000), (21, 1000), (11, 1000)],
			);

			// no nominators shall exist.
			assert!(<Nominators<Test>>::iter().collect::<Vec<_>>().is_empty());

			// give the man some money.
			let initial_balance = 1000;
			for i in [1, 2, 3, 4].iter() {
				let _ = asset::set_stakeable_balance::<Test>(&i, initial_balance);
			}

			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(1),
				1000,
				RewardDestination::Account(1)
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(1), vec![11, 11, 11, 21]));

			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(3),
				1000,
				RewardDestination::Account(3)
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![21]));

			// winners should be 21 and 11.
			let supports = <Test as Config>::ElectionProvider::elect(SINGLE_PAGE).unwrap();
			let expected_supports = vec![
				(11, Support { total: 1500, voters: vec![(11, 1000), (1, 500)] }),
				(21, Support { total: 2500, voters: vec![(21, 1000), (1, 500), (3, 1000)] }),
			];

			assert_eq!(supports, to_bounded_supports(expected_supports));
		});
}

#[test]
fn new_era_elects_correct_number_of_validators() {
	ExtBuilder::default().nominate(true).validator_count(1).build_and_execute(|| {
		assert_eq!(ValidatorCount::<Test>::get(), 1);
		assert_eq!(session_validators().len(), 1);

		Session::on_initialize(System::block_number());

		assert_eq!(session_validators().len(), 1);
	})
}


#[test]
fn reward_validator_slashing_validator_does_not_overflow() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		let stake = u64::MAX as Balance * 2;
		let reward_slash = u64::MAX as Balance * 2;

		// Assert multiplication overflows in balance arithmetic.
		assert!(stake.checked_mul(reward_slash).is_none());

		// Set staker
		let _ = asset::set_stakeable_balance::<Test>(&11, stake);

		let reward = EraRewardPoints::<AccountId> {
			total: 1,
			individual: vec![(11, 1)].into_iter().collect(),
		};

		// Check reward
		ErasRewardPoints::<Test>::insert(0, reward);

		// force exposure metadata to account for the overflowing `stake`.
		ErasStakersOverview::<Test>::insert(
			current_era(),
			11,
			PagedExposureMetadata { total: stake, own: stake, nominator_count: 0, page_count: 0 },
		);

		// we want to slash only self-stake, confirm that no others exposed.
		let full_exposure_after = Eras::<Test>::get_full_exposure(current_era(), &11);
		assert_eq!(full_exposure_after.total, stake);
		assert_eq!(full_exposure_after.others, vec![]);

		ErasValidatorReward::<Test>::insert(0, stake);
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 0, 0));
		assert_eq!(asset::stakeable_balance::<Test>(&11), stake * 2);

		// ensure ledger has `stake` and no more.
		Ledger::<Test>::insert(
			11,
			StakingLedgerInspect {
				stash: 11,
				total: stake,
				active: stake,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![1],
			},
		);
		// Set staker (unsafe, can reduce balance below actual stake)
		let _ = asset::set_stakeable_balance::<Test>(&11, stake);
		let _ = asset::set_stakeable_balance::<Test>(&2, stake);

		// only slashes out of bonded stake are applied. without this line, it is 0.
		Staking::bond(RuntimeOrigin::signed(2), stake - 1, RewardDestination::Staked).unwrap();

		// Override metadata and exposures of 11 so that it exposes minmal self stake and `stake` -
		// 1 from nominator 2.
		ErasStakersOverview::<Test>::insert(
			current_era(),
			11,
			PagedExposureMetadata { total: stake, own: 1, nominator_count: 1, page_count: 1 },
		);

		ErasStakersPaged::<Test>::insert(
			(current_era(), &11, 0),
			ExposurePage {
				page_total: stake - 1,
				others: vec![IndividualExposure { who: 2, value: stake - 1 }],
			},
		);

		// Check slashing
		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(100)], true);

		assert_eq!(asset::stakeable_balance::<Test>(&11), stake - 1);
		assert_eq!(asset::stakeable_balance::<Test>(&2), 1);
	})
}

#[test]
fn offence_doesnt_force_new_era() {
	ExtBuilder::default().build_and_execute(|| {
		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(5)], true);

		assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
	});
}

#[test]
fn offence_ensures_new_era_without_clobbering() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::force_new_era_always(RuntimeOrigin::root()));
		assert_eq!(ForceEra::<Test>::get(), Forcing::ForceAlways);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(5)], true);

		assert_eq!(ForceEra::<Test>::get(), Forcing::ForceAlways);
	});
}

#[test]
fn offence_deselects_validator_even_when_slash_is_zero() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			assert!(Session::validators().contains(&11));
			assert!(<Validators<Test>>::contains_key(11));

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(0)], true);

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
			assert!(is_disabled(11));

			mock::start_active_era(1);

			// The validator should be reenabled in the new era
			assert!(!is_disabled(11));
		});
}

#[test]
fn slashing_performed_according_exposure() {
	// This test checks that slashing is performed according the exposure (or more precisely,
	// historical exposure), not the current balance.
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(Staking::eras_stakers(active_era(), &11).own, 1000);

		// Handle an offence with a historical exposure.
		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(50)], true);

		// The stash account should be slashed for 250 (50% of 500).
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000 / 2);
	});
}

#[test]
fn validator_is_not_disabled_for_an_offence_in_previous_era() {
	ExtBuilder::default()
		.validator_count(4)
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);

			assert!(<Validators<Test>>::contains_key(11));
			assert!(Session::validators().contains(&11));

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(0)], true);

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
			assert!(is_disabled(11));

			mock::start_active_era(2);

			// the validator is not disabled in the new era
			Staking::validate(RuntimeOrigin::signed(11), Default::default()).unwrap();
			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
			assert!(<Validators<Test>>::contains_key(11));
			assert!(Session::validators().contains(&11));

			mock::start_active_era(3);

			// an offence committed in era 1 is reported in era 3
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(0)], 1, true);

			// the validator doesn't get disabled for an old offence
			assert!(Validators::<Test>::iter().any(|(stash, _)| stash == 11));
			assert!(!is_disabled(11));

			// and we are not forcing a new era
			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

			on_offence_in_era(
				&[offence_from(11, None)],
				// NOTE: A 100% slash here would clean up the account, causing de-registration.
				&[Perbill::from_percent(95)],
				1,
				true,
			);

			// the validator doesn't get disabled again
			assert!(Validators::<Test>::iter().any(|(stash, _)| stash == 11));
			assert!(!is_disabled(11));
			// and we are still not forcing a new era
			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
		});
}

#[test]
fn only_first_reporter_receive_the_slice() {
	// This test verifies that the first reporter of the offence receive their slice from the
	// slashed amount.
	ExtBuilder::default().build_and_execute(|| {
		// The reporters' reward is calculated from the total exposure.
		let initial_balance = 1125;

		assert_eq!(Staking::eras_stakers(active_era(), &11).total, initial_balance);

		on_offence_now(
			&[OffenceDetails { offender: (11, ()), reporters: vec![1, 2] }],
			&[Perbill::from_percent(50)],
			true,
		);

		// F1 * (reward_proportion * slash - 0)
		// 50% * (10% * initial_balance / 2)
		let reward = (initial_balance / 20) / 2;
		assert_eq!(asset::total_balance::<Test>(&1), 10 + reward);
		assert_eq!(asset::total_balance::<Test>(&2), 20 + 0);
	});
}

#[test]
fn subsequent_reports_in_same_span_pay_out_less() {
	// This test verifies that the reporters of the offence receive their slice from the slashed
	// amount, but less and less if they submit multiple reports in one span.
	ExtBuilder::default().build_and_execute(|| {
		// The reporters' reward is calculated from the total exposure.
		let initial_balance = 1125;

		assert_eq!(Staking::eras_stakers(active_era(), &11).total, initial_balance);

		on_offence_now(&[offence_from(11, Some(1))], &[Perbill::from_percent(20)], true);

		// F1 * (reward_proportion * slash - 0)
		// 50% * (10% * initial_balance * 20%)
		let reward = (initial_balance / 5) / 20;
		assert_eq!(asset::total_balance::<Test>(&1), 10 + reward);

		on_offence_now(&[offence_from(11, Some(1))], &[Perbill::from_percent(50)], true);

		let prior_payout = reward;

		// F1 * (reward_proportion * slash - prior_payout)
		// 50% * (10% * (initial_balance / 2) - prior_payout)
		let reward = ((initial_balance / 20) - prior_payout) / 2;
		assert_eq!(asset::total_balance::<Test>(&1), 10 + prior_payout + reward);
	});
}

#[test]
fn invulnerables_are_not_slashed() {
	// For invulnerable validators no slashing is performed.
	ExtBuilder::default().invulnerables(vec![11]).build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 2000);

		let exposure = Staking::eras_stakers(active_era(), &21);
		let initial_balance = Staking::slashable_balance_of(&21);

		let nominator_balances: Vec<_> = exposure
			.others
			.iter()
			.map(|o| asset::stakeable_balance::<Test>(&o.who))
			.collect();

		on_offence_now(
			&[offence_from(11, None), offence_from(21, None)],
			&[Perbill::from_percent(50), Perbill::from_percent(20)],
			true,
		);

		// The validator 11 hasn't been slashed, but 21 has been.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		// 2000 - (0.2 * initial_balance)
		assert_eq!(asset::stakeable_balance::<Test>(&21), 2000 - (2 * initial_balance / 10));

		// ensure that nominators were slashed as well.
		for (initial_balance, other) in nominator_balances.into_iter().zip(exposure.others) {
			assert_eq!(
				asset::stakeable_balance::<Test>(&other.who),
				initial_balance - (2 * other.value / 10),
			);
		}
	});
}

#[test]
fn dont_slash_if_fraction_is_zero() {
	// Don't slash if the fraction is zero.
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(0)], true);

		// The validator hasn't been slashed. The new era is not forced.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
	});
}

#[test]
fn only_slash_for_max_in_era() {
	// multiple slashes within one era are only applied if it is more than any previous slash in the
	// same era.
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(50)], true);

		// The validator has been slashed and has been force-chilled.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 500);
		assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(25)], true);

		// The validator has not been slashed additionally.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 500);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(60)], true);

		// The validator got slashed 10% more.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 400);
	})
}

#[test]
fn garbage_collection_after_slashing() {
	// ensures that `SlashingSpans` and `SpanSlash` of an account is removed after reaping.
	ExtBuilder::default()
		.existential_deposit(2)
		.balance_factor(2)
		.build_and_execute(|| {
			assert_eq!(asset::stakeable_balance::<Test>(&11), 2000);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);

			assert_eq!(asset::stakeable_balance::<Test>(&11), 2000 - 200);
			assert!(SlashingSpans::<Test>::get(&11).is_some());
			assert_eq!(SpanSlash::<Test>::get(&(11, 0)).amount(), &200);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(100)], true);

			// validator and nominator slash in era are garbage-collected by era change,
			// so we don't test those here.

			assert_eq!(asset::stakeable_balance::<Test>(&11), 0);
			// Non staked balance is not touched.
			assert_eq!(asset::total_balance::<Test>(&11), ExistentialDeposit::get());

			let slashing_spans = SlashingSpans::<Test>::get(&11).unwrap();
			assert_eq!(slashing_spans.iter().count(), 2);

			// reap_stash respects num_slashing_spans so that weight is accurate
			assert_noop!(
				Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0),
				Error::<Test>::IncorrectSlashingSpans
			);
			assert_ok!(Staking::reap_stash(RuntimeOrigin::signed(20), 11, 2));

			assert!(SlashingSpans::<Test>::get(&11).is_none());
			assert_eq!(SpanSlash::<Test>::get(&(11, 0)).amount(), &0);
		})
}

#[test]
fn garbage_collection_on_window_pruning() {
	// ensures that `ValidatorSlashInEra` and `NominatorSlashInEra` are cleared after
	// `BondingDuration`.
	ExtBuilder::default().build_and_execute(|| {
		mock::start_active_era(1);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		let now = active_era();

		let exposure = Staking::eras_stakers(now, &11);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		add_slash(&11);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 900);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - (nominated_value / 10));

		assert!(ValidatorSlashInEra::<Test>::get(&now, &11).is_some());
		assert!(NominatorSlashInEra::<Test>::get(&now, &101).is_some());

		// + 1 because we have to exit the bonding window.
		for era in (0..(BondingDuration::get() + 1)).map(|offset| offset + now + 1) {
			assert!(ValidatorSlashInEra::<Test>::get(&now, &11).is_some());
			assert!(NominatorSlashInEra::<Test>::get(&now, &101).is_some());

			mock::start_active_era(era);
		}

		assert!(ValidatorSlashInEra::<Test>::get(&now, &11).is_none());
		assert!(NominatorSlashInEra::<Test>::get(&now, &101).is_none());
	})
}

#[test]
fn slashing_nominators_by_span_max() {
	ExtBuilder::default().build_and_execute(|| {
		mock::start_active_era(1);
		mock::start_active_era(2);
		mock::start_active_era(3);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 2000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		assert_eq!(Staking::slashable_balance_of(&21), 1000);

		let exposure_11 = Staking::eras_stakers(active_era(), &11);
		let exposure_21 = Staking::eras_stakers(active_era(), &21);
		let nominated_value_11 = exposure_11.others.iter().find(|o| o.who == 101).unwrap().value;
		let nominated_value_21 = exposure_21.others.iter().find(|o| o.who == 101).unwrap().value;

		on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(10)], 2, true);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 900);

		let slash_1_amount = Perbill::from_percent(10) * nominated_value_11;
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - slash_1_amount);

		let expected_spans = vec![
			slashing::SlashingSpan { index: 1, start: 4, length: None },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		let get_span = |account| SlashingSpans::<Test>::get(&account).unwrap();

		assert_eq!(get_span(11).iter().collect::<Vec<_>>(), expected_spans);

		assert_eq!(get_span(101).iter().collect::<Vec<_>>(), expected_spans);

		// second slash: higher era, higher value, same span.
		on_offence_in_era(&[offence_from(21, None)], &[Perbill::from_percent(30)], 3, true);

		// 11 was not further slashed, but 21 and 101 were.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 900);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 1700);

		let slash_2_amount = Perbill::from_percent(30) * nominated_value_21;
		assert!(slash_2_amount > slash_1_amount);

		// only the maximum slash in a single span is taken.
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - slash_2_amount);

		// third slash: in same era and on same validator as first, higher
		// in-era value, but lower slash value than slash 2.
		on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(20)], 2, true);

		// 11 was further slashed, but 21 and 101 were not.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 800);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 1700);

		let slash_3_amount = Perbill::from_percent(20) * nominated_value_21;
		assert!(slash_3_amount < slash_2_amount);
		assert!(slash_3_amount > slash_1_amount);

		// only the maximum slash in a single span is taken.
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - slash_2_amount);
	});
}

#[test]
fn slashes_are_summed_across_spans() {
	ExtBuilder::default().build_and_execute(|| {
		mock::start_active_era(1);
		mock::start_active_era(2);
		mock::start_active_era(3);

		assert_eq!(asset::stakeable_balance::<Test>(&21), 2000);
		assert_eq!(Staking::slashable_balance_of(&21), 1000);

		let get_span = |account| SlashingSpans::<Test>::get(&account).unwrap();

		on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(10)], true);

		let expected_spans = vec![
			slashing::SlashingSpan { index: 1, start: 4, length: None },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		assert_eq!(get_span(21).iter().collect::<Vec<_>>(), expected_spans);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 1900);

		// 21 has been force-chilled. re-signal intent to validate.
		Staking::validate(RuntimeOrigin::signed(21), Default::default()).unwrap();

		mock::start_active_era(4);

		assert_eq!(Staking::slashable_balance_of(&21), 900);

		on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(10)], true);

		let expected_spans = vec![
			slashing::SlashingSpan { index: 2, start: 5, length: None },
			slashing::SlashingSpan { index: 1, start: 4, length: Some(1) },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		assert_eq!(get_span(21).iter().collect::<Vec<_>>(), expected_spans);
		assert_eq!(asset::stakeable_balance::<Test>(&21), 1810);
	});
}

#[test]
fn deferred_slashes_are_deferred() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		mock::start_active_era(1);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);

		let exposure = Staking::eras_stakers(active_era(), &11);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		System::reset_events();

		// only 1 page of exposure, so slashes will be applied in one block.
		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 1);

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);

		// nominations are not removed regardless of the deferring.
		assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		mock::start_active_era(2);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				Event::OffenceReported { validator: 11, offence_era: 1, .. },
				Event::SlashComputed { offence_era: 1, slash_era: 3, page: 0, .. },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::StakersElected,
				..,
			]
		));

		// the slashes for era 1 will start applying in era 3, to end before era 4.
		mock::start_active_era(3);
		// Slashes not applied yet. Will apply in the next block after era starts.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		// trigger slashing by advancing block.
		roll_blocks(1);
		assert_eq!(asset::stakeable_balance::<Test>(&11), 900);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - (nominated_value / 10));

		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				// era 3 elections
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::StakersElected,
				Event::EraPaid { .. },
				// slashes applied from era 1 between era 3 and 4.
				Event::Slashed { staker: 11, amount: 100 },
				Event::Slashed { staker: 101, amount: 12 },
			]
		));
	})
}

#[test]
fn retroactive_deferred_slashes_two_eras_before() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		assert_eq!(BondingDuration::get(), 3);

		mock::start_active_era(1);

		assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

		System::reset_events();
		on_offence_in_era(
			&[offence_from(11, None)],
			&[Perbill::from_percent(10)],
			1, // should be deferred for two eras, and applied at the beginning of era 3.
			true,
		);

		mock::start_active_era(3);
		// Slashes not applied yet. Will apply in the next block after era starts.
		roll_blocks(1);

		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				Event::OffenceReported { validator: 11, offence_era: 1, .. },
				Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 },
				..,
				Event::Slashed { staker: 11, amount: 100 },
				Event::Slashed { staker: 101, amount: 12 }
			]
		));
	})
}

#[test]
fn retroactive_deferred_slashes_one_before() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		assert_eq!(BondingDuration::get(), 3);

		// unbond at slash era.
		mock::start_active_era(2);
		assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 100));

		mock::start_active_era(3);
		System::reset_events();
		on_offence_in_era(
			&[offence_from(11, None)],
			&[Perbill::from_percent(10)],
			2, // should be deferred for two eras, and applied before the beginning of era 4.
			true,
		);

		mock::start_active_era(4);

		assert_eq!(Staking::ledger(11.into()).unwrap().total, 1000);
		// slash happens at next blocks.
		roll_blocks(1);

		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				Event::OffenceReported { validator: 11, offence_era: 2, .. },
				Event::SlashComputed { offence_era: 2, slash_era: 4, offender: 11, page: 0 },
				..,
				Event::Slashed { staker: 11, amount: 100 },
				Event::Slashed { staker: 101, amount: 12 }
			]
		));

		// their ledger has already been slashed.
		assert_eq!(Staking::ledger(11.into()).unwrap().total, 900);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 1000));
		assert_eq!(Staking::ledger(11.into()).unwrap().total, 900);
	})
}

#[test]
fn staker_cannot_bail_deferred_slash() {
	// as long as SlashDeferDuration is less than BondingDuration, this should not be possible.
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		mock::start_active_era(1);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		let exposure = Staking::eras_stakers(active_era(), &11);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);

		// now we chill
		assert_ok!(Staking::chill(RuntimeOrigin::signed(101)));
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 500));

		assert_eq!(CurrentEra::<Test>::get().unwrap(), 1);
		assert_eq!(active_era(), 1);

		assert_eq!(
			Ledger::<Test>::get(101).unwrap(),
			StakingLedgerInspect {
				active: 0,
				total: 500,
				stash: 101,
				legacy_claimed_rewards: bounded_vec![],
				unlocking: bounded_vec![UnlockChunk { era: 4u32, value: 500 }],
			}
		);

		// no slash yet.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		// no slash yet.
		mock::start_active_era(2);
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		assert_eq!(CurrentEra::<Test>::get().unwrap(), 2);
		assert_eq!(active_era(), 2);

		// no slash yet.
		mock::start_active_era(3);
		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		assert_eq!(CurrentEra::<Test>::get().unwrap(), 3);
		assert_eq!(active_era(), 3);

		// and cannot yet unbond:
		assert_storage_noop!(assert!(
			Staking::withdraw_unbonded(RuntimeOrigin::signed(101), 0).is_ok()
		));
		assert_eq!(
			Ledger::<Test>::get(101).unwrap().unlocking.into_inner(),
			vec![UnlockChunk { era: 4u32, value: 500 as Balance }],
		);

		// at the start of era 4, slashes from era 1 are processed,
		// after being deferred for at least 2 full eras.
		mock::start_active_era(4);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 900);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - (nominated_value / 10));

		// and the leftover of the funds can now be unbonded.
	})
}

#[test]
fn remove_deferred() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		mock::start_active_era(1);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);

		let exposure = Staking::eras_stakers(active_era(), &11);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		// deferred to start of era 3.
		let slash_fraction_one = Perbill::from_percent(10);
		on_offence_now(&[offence_from(11, None)], &[slash_fraction_one], true);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		mock::start_active_era(2);

		// reported later, but deferred to start of era 3 as well.
		System::reset_events();
		let slash_fraction_two = Perbill::from_percent(15);
		on_offence_in_era(&[offence_from(11, None)], &[slash_fraction_two], 1, true);

		assert_eq!(
			UnappliedSlashes::<Test>::iter_prefix(&3).collect::<Vec<_>>(),
			vec![
				(
					(11, slash_fraction_one, 0),
					UnappliedSlash {
						validator: 11,
						own: 100,
						others: bounded_vec![(101, 12)],
						reporter: None,
						payout: 5
					}
				),
				(
					(11, slash_fraction_two, 0),
					UnappliedSlash {
						validator: 11,
						own: 50,
						others: bounded_vec![(101, 7)],
						reporter: None,
						payout: 6
					}
				),
			]
		);

		// fails if empty
		assert_noop!(
			Staking::cancel_deferred_slash(RuntimeOrigin::root(), 1, vec![]),
			Error::<Test>::EmptyTargets
		);

		// cancel the slash with 10%.
		assert_ok!(Staking::cancel_deferred_slash(
			RuntimeOrigin::root(),
			3,
			vec![(11, slash_fraction_one, 0)]
		));
		assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&3).count(), 1);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		mock::start_active_era(3);

		assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

		// at the next blocks, slashes from era 1 are processed, 1 page a block,
		// after being deferred for 2 eras.
		roll_blocks(1);

		// the first slash for 10% was cancelled, but the 15% one not.
		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				Event::OffenceReported { validator: 11, offence_era: 1, .. },
				Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 },
				Event::SlashCancelled {
					slash_era: 3,
					slash_key: (11, fraction, 0),
					payout: 5
				},
				..,
				Event::Slashed { staker: 11, amount: 50 },
				Event::Slashed { staker: 101, amount: 7 }
			] if fraction == slash_fraction_one
		));

		let slash_10 = Perbill::from_percent(10);
		let slash_15 = slash_fraction_two;
		let initial_slash = slash_10 * nominated_value;

		let total_slash = slash_15 * nominated_value;
		let actual_slash = total_slash - initial_slash;

		// 5% slash (15 - 10) processed now.
		assert_eq!(asset::stakeable_balance::<Test>(&11), 950);
		assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - actual_slash);
	})
}

#[test]
fn remove_multi_deferred() {
	ExtBuilder::default()
		.slash_defer_duration(2)
		.validator_count(4)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);

			assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);

			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(10)], true);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(25)], true);

			on_offence_now(&[offence_from(41, None)], &[Perbill::from_percent(25)], true);

			on_offence_now(&[offence_from(51, None)], &[Perbill::from_percent(25)], true);

			// there are 5 slashes to be applied in era 3.
			assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&3).count(), 5);

			// lets cancel 3 of them.
			assert_ok!(Staking::cancel_deferred_slash(
				RuntimeOrigin::root(),
				3,
				vec![
					(11, Perbill::from_percent(10), 0),
					(11, Perbill::from_percent(25), 0),
					(51, Perbill::from_percent(25), 0),
				]
			));

			let slashes = UnappliedSlashes::<Test>::iter_prefix(&3).collect::<Vec<_>>();
			assert_eq!(slashes.len(), 2);
			// the first item in the remaining slashes belongs to validator 41.
			assert_eq!(slashes[0].0, (41, Perbill::from_percent(25), 0));
			// the second and last item in the remaining slashes belongs to validator 21.
			assert_eq!(slashes[1].0, (21, Perbill::from_percent(10), 0));
		})
}

#[test]
fn slash_kicks_validators_not_nominators_and_disables_nominator_for_kicked_validator() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);

			// pre-slash balance
			assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

			// 100 has approval for 11 as of now
			assert!(Nominators::<Test>::get(101).unwrap().targets.contains(&11));

			// 11 and 21 both have the support of 100
			let exposure_11 = Staking::eras_stakers(active_era(), &11);
			let exposure_21 = Staking::eras_stakers(active_era(), &21);

			assert_eq!(exposure_11.total, 1000 + 125);
			assert_eq!(exposure_21.total, 1000 + 375);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 0, result: Ok(7) },
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(10),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 12 },
				]
			);

			assert!(matches!(
				session_events().as_slice(),
				&[.., SessionEvent::ValidatorDisabled { validator: 11 }]
			));

			// post-slash balance
			let nominator_slash_amount_11 = 125 / 10;
			assert_eq!(asset::stakeable_balance::<Test>(&11), 900);
			assert_eq!(asset::stakeable_balance::<Test>(&101), 2000 - nominator_slash_amount_11);

			// check that validator was disabled.
			assert!(is_disabled(11));

			// actually re-bond the slashed validator
			assert_ok!(Staking::validate(RuntimeOrigin::signed(11), Default::default()));

			mock::start_active_era(2);
			let exposure_11 = Staking::eras_stakers(active_era(), &11);
			let exposure_21 = Staking::eras_stakers(active_era(), &21);

			// 11's own expo is reduced. sum of support from 11 is less (448), which is 500
			// 900 + 146
			assert!(matches!(exposure_11, Exposure { own: 900, total: 1046, .. }));
			// 1000 + 342
			assert!(matches!(exposure_21, Exposure { own: 1000, total: 1342, .. }));
			assert_eq!(500 - 146 - 342, nominator_slash_amount_11);
		});
}

#[test]
fn non_slashable_offence_disables_validator() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);

			// offence with no slash associated
			on_offence_now(&[offence_from(11, None)], &[Perbill::zero()], true);

			// it does NOT affect the nominator.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// offence that slashes 25% of the bond
			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(25)], true);

			// it DOES NOT affect the nominator.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 0, result: Ok(7) },
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(0),
						offence_era: 1
					},
					Event::OffenceReported {
						validator: 21,
						fraction: Perbill::from_percent(25),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 21, page: 0 },
					Event::Slashed { staker: 21, amount: 250 },
					Event::Slashed { staker: 101, amount: 94 }
				]
			);

			assert!(matches!(
				session_events().as_slice(),
				&[
					..,
					SessionEvent::ValidatorDisabled { validator: 11 },
					SessionEvent::ValidatorDisabled { validator: 21 },
				]
			));

			// the offence for validator 11 wasn't slashable but it is disabled
			assert!(is_disabled(11));
			// validator 21 gets disabled too
			assert!(is_disabled(21));
		});
}

#[test]
fn slashing_independent_of_disabling_validator() {
	ExtBuilder::default()
		.validator_count(5)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51]);

			let now = ActiveEra::<Test>::get().unwrap().index;

			// --- Disable without a slash ---
			// offence with no slash associated
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::zero()], now, true);

			// nomination remains untouched.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// first validator is disabled but not slashed
			assert!(is_disabled(11));

			// --- Slash without disabling ---
			// offence that slashes 50% of the bond (setup for next slash)
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(50)], now, true);

			// offence that slashes 25% of the bond but does not disable
			on_offence_in_era(&[offence_from(21, None)], &[Perbill::from_percent(25)], now, true);

			// nomination remains untouched.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// second validator is slashed but not disabled
			assert!(!is_disabled(21));
			assert!(is_disabled(11));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 0, result: Ok(5) },
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(0),
						offence_era: 1
					},
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(50),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 500 },
					Event::Slashed { staker: 101, amount: 62 },
					Event::OffenceReported {
						validator: 21,
						fraction: Perbill::from_percent(25),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 21, page: 0 },
					Event::Slashed { staker: 21, amount: 250 },
					Event::Slashed { staker: 101, amount: 94 }
				]
			);

			assert_eq!(
				session_events(),
				vec![
					SessionEvent::NewSession { session_index: 1 },
					SessionEvent::NewSession { session_index: 2 },
					SessionEvent::NewSession { session_index: 3 },
					SessionEvent::ValidatorDisabled { validator: 11 }
				]
			);
		});
}

#[test]
fn offence_threshold_doesnt_plan_new_era() {
	ExtBuilder::default()
		.validator_count(4)
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41]);

			assert_eq!(
				UpToLimitWithReEnablingDisablingStrategy::<DISABLING_LIMIT_FACTOR>::disable_limit(
					Session::validators().len()
				),
				1
			);

			// we have 4 validators and an offending validator threshold of 1/3,
			// even if the third validator commits an offence a new era should not be forced
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(50)], true);

			// 11 should be disabled because the byzantine threshold is 1
			assert!(is_disabled(11));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

			on_offence_now(&[offence_from(21, None)], &[Perbill::zero()], true);

			// 21 should not be disabled because the number of disabled validators will be above the
			// byzantine threshold
			assert!(!is_disabled(21));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

			on_offence_now(&[offence_from(31, None)], &[Perbill::zero()], true);

			// same for 31
			assert!(!is_disabled(31));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
		});
}

#[test]
fn disabled_validators_are_kept_disabled_for_whole_era() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);
			assert_eq!(<Test as Config>::SessionsPerEra::get(), 3);

			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(25)], true);

			// nominations are not updated.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// validator 21 gets disabled since it got slashed
			assert!(is_disabled(21));

			advance_session();

			// disabled validators should carry-on through all sessions in the era
			assert!(is_disabled(21));

			// validator 11 commits an offence
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(25)], true);

			// nominations are not updated.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			advance_session();

			// and both are disabled in the last session of the era
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			mock::start_active_era(2);

			// when a new era starts disabled validators get cleared
			assert!(!is_disabled(11));
			assert!(!is_disabled(21));
		});
}

#[test]
fn claim_reward_at_the_last_era_and_no_double_claim_and_invalid_claim() {
	// should check that:
	// * rewards get paid until history_depth for both validators and nominators
	// * an invalid era to claim doesn't update last_reward
	// * double claim of one era fails
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		// Consumed weight for all payout_stakers dispatches that fail
		let err_weight = <Test as Config>::WeightInfo::payout_stakers_alive_staked(0);

		let init_balance_11 = asset::total_balance::<Test>(&11);
		let init_balance_101 = asset::total_balance::<Test>(&101);

		let part_for_11 = Perbill::from_rational::<u32>(1000, 1125);
		let part_for_101 = Perbill::from_rational::<u32>(125, 1125);

		// Check state
		Payee::<Test>::insert(11, RewardDestination::Account(11));
		Payee::<Test>::insert(101, RewardDestination::Account(101));

		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		// Compute total payout now for whole duration as other parameter won't change
		let validator_payout_0 = validator_payout_for(time_per_era());

		mock::start_active_era(1);

		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		// Increase total token issuance to affect the total payout.
		let _ = Balances::deposit_creating(&999, 1_000_000_000);

		// Compute total payout now for whole duration as other parameter won't change
		let total_payout_1 = validator_payout_for(time_per_era());
		assert!(total_payout_1 != validator_payout_0);

		mock::start_active_era(2);

		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		// Increase total token issuance to affect the total payout.
		let _ = Balances::deposit_creating(&999, 1_000_000_000);
		// Compute total payout now for whole duration as other parameter won't change
		let total_payout_2 = validator_payout_for(time_per_era());
		assert!(total_payout_2 != validator_payout_0);
		assert!(total_payout_2 != total_payout_1);

		mock::start_active_era(HistoryDepth::get() + 1);

		let active_era = active_era();

		// This is the latest planned era in staking, not the active era
		let current_era = CurrentEra::<Test>::get().unwrap();

		// Last kept is 1:
		assert!(current_era - HistoryDepth::get() == 1);
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 0, 0),
			// Fail: Era out of history
			Error::<Test>::InvalidEraToReward.with_weight(err_weight)
		);
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, 0));
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 2, 0));
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 2, 0),
			// Fail: Double claim
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, active_era, 0),
			// Fail: Era not finished yet
			Error::<Test>::InvalidEraToReward.with_weight(err_weight)
		);

		// Era 0 can't be rewarded anymore and current era can't be rewarded yet
		// only era 1 and 2 can be rewarded.

		assert_eq!(
			asset::total_balance::<Test>(&11),
			init_balance_11 + part_for_11 * (total_payout_1 + total_payout_2),
		);
		assert_eq!(
			asset::total_balance::<Test>(&101),
			init_balance_101 + part_for_101 * (total_payout_1 + total_payout_2),
		);
	});
}

#[test]
fn zero_slash_keeps_nominators() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);

			assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(0)], true);

			assert_eq!(asset::stakeable_balance::<Test>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<Test>(&101), 2000);

			// 11 is not removed but disabled
			assert!(Validators::<Test>::iter().any(|(stash, _)| stash == 11));
			assert!(is_disabled(11));
			// and their nominations are kept.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);
		});
}

#[test]
fn test_nominators_over_max_exposure_page_size_are_rewarded() {
	ExtBuilder::default().build_and_execute(|| {
		// bond one nominator more than the max exposure page size to validator 11.
		for i in 0..=MaxExposurePageSize::get() {
			let stash = 10_000 + i as AccountId;
			let balance = 10_000 + i as Balance;
			asset::set_stakeable_balance::<Test>(&stash, balance);
			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(stash),
				balance,
				RewardDestination::Stash
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(stash), vec![11]));
		}
		mock::start_active_era(1);

		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		// compute and ensure the reward amount is greater than zero.
		let _ = validator_payout_for(time_per_era());

		mock::start_active_era(2);
		mock::make_all_reward_payment(1);

		// Assert nominators from 1 to Max are rewarded
		let mut i: u32 = 0;
		while i < MaxExposurePageSize::get() {
			let stash = 10_000 + i as AccountId;
			let balance = 10_000 + i as Balance;
			assert!(asset::stakeable_balance::<Test>(&stash) > balance);
			i += 1;
		}

		// Assert overflowing nominators from page 1 are also rewarded
		let stash = 10_000 + i as AccountId;
		assert!(asset::stakeable_balance::<Test>(&stash) > (10_000 + i) as Balance);
	});
}

#[test]
fn test_nominators_are_rewarded_for_all_exposure_page() {
	ExtBuilder::default().build_and_execute(|| {
		// 3 pages of exposure
		let nominator_count = 2 * MaxExposurePageSize::get() + 1;

		for i in 0..nominator_count {
			let stash = 10_000 + i as AccountId;
			let balance = 10_000 + i as Balance;
			asset::set_stakeable_balance::<Test>(&stash, balance);
			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(stash),
				balance,
				RewardDestination::Stash
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(stash), vec![11]));
		}
		mock::start_active_era(1);

		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		// compute and ensure the reward amount is greater than zero.
		let _ = validator_payout_for(time_per_era());

		mock::start_active_era(2);
		mock::make_all_reward_payment(1);

		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 3);

		// Assert all nominators are rewarded according to their stake
		for i in 0..nominator_count {
			// balance of the nominator after the reward payout.
			let current_balance = asset::stakeable_balance::<Test>(&((10000 + i) as AccountId));
			// balance of the nominator in the previous iteration.
			let previous_balance =
				asset::stakeable_balance::<Test>(&((10000 + i - 1) as AccountId));
			// balance before the reward.
			let original_balance = 10_000 + i as Balance;

			assert!(current_balance > original_balance);
			// since the stake of the nominator is increasing for each iteration, the final balance
			// after the reward should also be higher than the previous iteration.
			assert!(current_balance > previous_balance);
		}
	});
}

#[test]
fn test_multi_page_payout_stakers_by_page() {
	// Test that payout_stakers work in general and that it pays the correct amount of reward.
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		let balance = 1000;
		// Track the exposure of the validator and all nominators.
		let mut total_exposure = balance;
		// Create a validator:
		bond_validator(11, balance); // Default(64)
		assert_eq!(Validators::<Test>::count(), 1);

		// Create nominators, targeting stash of validators
		for i in 0..100 {
			let bond_amount = balance + i as Balance;
			bond_nominator(1000 + i, bond_amount, vec![11]);
			// with multi page reward payout, payout exposure is same as total exposure.
			total_exposure += bond_amount;
		}

		mock::start_active_era(1);
		Staking::reward_by_ids(vec![(11, 1)]);

		// Since `MaxExposurePageSize = 64`, there are two pages of validator exposure.
		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 2);

		// compute and ensure the reward amount is greater than zero.
		let payout = validator_payout_for(time_per_era());
		mock::start_active_era(2);

		// verify the exposures are calculated correctly.
		let actual_exposure_0 = Eras::<Test>::get_paged_exposure(1, &11, 0).unwrap();
		assert_eq!(actual_exposure_0.total(), total_exposure);
		assert_eq!(actual_exposure_0.own(), 1000);
		assert_eq!(actual_exposure_0.others().len(), 64);
		let actual_exposure_1 = Eras::<Test>::get_paged_exposure(1, &11, 1).unwrap();
		assert_eq!(actual_exposure_1.total(), total_exposure);
		// own stake is only included once in the first page
		assert_eq!(actual_exposure_1.own(), 0);
		assert_eq!(actual_exposure_1.others().len(), 100 - 64);

		let pre_payout_total_issuance = pallet_balances::TotalIssuance::<Test>::get();
		RewardOnUnbalanceWasCalled::set(false);
		System::reset_events();

		let controller_balance_before_p0_payout = asset::stakeable_balance::<Test>(&11);
		// Payout rewards for first exposure page
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, 0));

		// verify `Rewarded` events are being executed
		assert!(matches!(
			staking_events_since_last_call().as_slice(),
			&[
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: Some(1) },
				..,
				Event::Rewarded { stash: 1063, dest: RewardDestination::Stash, amount: 111 },
				Event::Rewarded { stash: 1064, dest: RewardDestination::Stash, amount: 111 },
			]
		));

		let controller_balance_after_p0_payout = asset::stakeable_balance::<Test>(&11);

		// verify rewards have been paid out but still some left
		assert!(pallet_balances::TotalIssuance::<Test>::get() > pre_payout_total_issuance);
		assert!(pallet_balances::TotalIssuance::<Test>::get() < pre_payout_total_issuance + payout);

		// verify the validator has been rewarded
		assert!(controller_balance_after_p0_payout > controller_balance_before_p0_payout);

		// Payout the second and last page of nominators
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, 1));

		// verify `Rewarded` events are being executed for the second page.
		let events = staking_events_since_last_call();
		assert!(matches!(
			events.as_slice(),
			&[
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 1, next: None },
				Event::Rewarded { stash: 1065, dest: RewardDestination::Stash, amount: 111 },
				Event::Rewarded { stash: 1066, dest: RewardDestination::Stash, amount: 111 },
				..
			]
		));
		// verify the validator was not rewarded the second time
		assert_eq!(asset::stakeable_balance::<Test>(&11), controller_balance_after_p0_payout);

		// verify all rewards have been paid out
		assert_eq_error_rate!(
			pallet_balances::TotalIssuance::<Test>::get(),
			pre_payout_total_issuance + payout,
			2
		);
		assert!(RewardOnUnbalanceWasCalled::get());

		// Top 64 nominators of validator 11 automatically paid out, including the validator
		assert!(asset::stakeable_balance::<Test>(&11) > balance);
		for i in 0..100 {
			assert!(asset::stakeable_balance::<Test>(&(1000 + i)) > balance + i as Balance);
		}

		// verify we no longer track rewards in `legacy_claimed_rewards` vec
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![]
			}
		);

		// verify rewards are tracked to prevent double claims
		for page in 0..Eras::<Test>::exposure_page_count(1, &11) {
			assert_eq!(Eras::<Test>::is_rewards_claimed(1, &11, page), true);
		}

		for i in 3..16 {
			Staking::reward_by_ids(vec![(11, 1)]);

			// compute and ensure the reward amount is greater than zero.
			let payout = validator_payout_for(time_per_era());
			let pre_payout_total_issuance = pallet_balances::TotalIssuance::<Test>::get();

			mock::start_active_era(i);
			RewardOnUnbalanceWasCalled::set(false);
			mock::make_all_reward_payment(i - 1);
			assert_eq_error_rate!(
				pallet_balances::TotalIssuance::<Test>::get(),
				pre_payout_total_issuance + payout,
				2
			);
			assert!(RewardOnUnbalanceWasCalled::get());

			// verify we track rewards for each era and page
			for page in 0..Eras::<Test>::exposure_page_count(i - 1, &11) {
				assert_eq!(Eras::<Test>::is_rewards_claimed(i - 1, &11, page), true);
			}
		}

		assert_eq!(ErasClaimedRewards::<Test>::get(14, &11), vec![0, 1]);

		let last_era = 99;
		let history_depth = HistoryDepth::get();
		let last_reward_era = last_era - 1;
		let first_claimable_reward_era = last_era - history_depth;
		for i in 16..=last_era {
			Staking::reward_by_ids(vec![(11, 1)]);
			// compute and ensure the reward amount is greater than zero.
			let _ = validator_payout_for(time_per_era());
			mock::start_active_era(i);
		}

		// verify we clean up history as we go
		for era in 0..15 {
			assert_eq!(ErasClaimedRewards::<Test>::get(era, &11), Vec::<sp_staking::Page>::new());
		}

		// verify only page 0 is marked as claimed
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			first_claimable_reward_era,
			0
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(first_claimable_reward_era, &11), vec![0]);

		// verify page 0 and 1 are marked as claimed
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			first_claimable_reward_era,
			1
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(first_claimable_reward_era, &11), vec![0, 1]);

		// verify only page 0 is marked as claimed
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			last_reward_era,
			0
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(last_reward_era, &11), vec![0]);

		// verify page 0 and 1 are marked as claimed
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			last_reward_era,
			1
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(last_reward_era, &11), vec![0, 1]);

		// Out of order claims works.
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 69, 0));
		assert_eq!(ErasClaimedRewards::<Test>::get(69, &11), vec![0]);
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 23, 1));
		assert_eq!(ErasClaimedRewards::<Test>::get(23, &11), vec![1]);
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 42, 0));
		assert_eq!(ErasClaimedRewards::<Test>::get(42, &11), vec![0]);
	});
}

#[test]
fn test_multi_page_payout_stakers_backward_compatible() {
	// Test that payout_stakers work in general and that it pays the correct amount of reward.
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		let balance = 1000;
		// Track the exposure of the validator and all nominators.
		let mut total_exposure = balance;
		// Create a validator:
		bond_validator(11, balance); // Default(64)
		assert_eq!(Validators::<Test>::count(), 1);

		let err_weight = <Test as Config>::WeightInfo::payout_stakers_alive_staked(0);

		// Create nominators, targeting stash of validators
		for i in 0..100 {
			let bond_amount = balance + i as Balance;
			bond_nominator(1000 + i, bond_amount, vec![11]);
			// with multi page reward payout, payout exposure is same as total exposure.
			total_exposure += bond_amount;
		}

		mock::start_active_era(1);
		Staking::reward_by_ids(vec![(11, 1)]);

		// Since `MaxExposurePageSize = 64`, there are two pages of validator exposure.
		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 2);

		// compute and ensure the reward amount is greater than zero.
		let payout = validator_payout_for(time_per_era());
		mock::start_active_era(2);

		// verify the exposures are calculated correctly.
		let actual_exposure_0 = Eras::<Test>::get_paged_exposure(1, &11, 0).unwrap();
		assert_eq!(actual_exposure_0.total(), total_exposure);
		assert_eq!(actual_exposure_0.own(), 1000);
		assert_eq!(actual_exposure_0.others().len(), 64);
		let actual_exposure_1 = Eras::<Test>::get_paged_exposure(1, &11, 1).unwrap();
		assert_eq!(actual_exposure_1.total(), total_exposure);
		// own stake is only included once in the first page
		assert_eq!(actual_exposure_1.own(), 0);
		assert_eq!(actual_exposure_1.others().len(), 100 - 64);

		let pre_payout_total_issuance = pallet_balances::TotalIssuance::<Test>::get();
		RewardOnUnbalanceWasCalled::set(false);

		let controller_balance_before_p0_payout = asset::stakeable_balance::<Test>(&11);
		// Payout rewards for first exposure page
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1));
		// page 0 is claimed
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, 0),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		let controller_balance_after_p0_payout = asset::stakeable_balance::<Test>(&11);

		// verify rewards have been paid out but still some left
		assert!(pallet_balances::TotalIssuance::<Test>::get() > pre_payout_total_issuance);
		assert!(pallet_balances::TotalIssuance::<Test>::get() < pre_payout_total_issuance + payout);

		// verify the validator has been rewarded
		assert!(controller_balance_after_p0_payout > controller_balance_before_p0_payout);

		// This should payout the second and last page of nominators
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1));

		// cannot claim any more pages
		assert_noop!(
			Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		// verify the validator was not rewarded the second time
		assert_eq!(asset::stakeable_balance::<Test>(&11), controller_balance_after_p0_payout);

		// verify all rewards have been paid out
		assert_eq_error_rate!(
			pallet_balances::TotalIssuance::<Test>::get(),
			pre_payout_total_issuance + payout,
			2
		);
		assert!(RewardOnUnbalanceWasCalled::get());

		// verify all nominators of validator 11 are paid out, including the validator
		// Validator payout goes to controller.
		assert!(asset::stakeable_balance::<Test>(&11) > balance);
		for i in 0..100 {
			assert!(asset::stakeable_balance::<Test>(&(1000 + i)) > balance + i as Balance);
		}

		// verify we no longer track rewards in `legacy_claimed_rewards` vec
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![]
			}
		);

		// verify rewards are tracked to prevent double claims
		for page in 0..Eras::<Test>::exposure_page_count(1, &11) {
			assert_eq!(Eras::<Test>::is_rewards_claimed(1, &11, page), true);
		}

		for i in 3..16 {
			Staking::reward_by_ids(vec![(11, 1)]);

			// compute and ensure the reward amount is greater than zero.
			let payout = validator_payout_for(time_per_era());
			let pre_payout_total_issuance = pallet_balances::TotalIssuance::<Test>::get();

			mock::start_active_era(i);
			RewardOnUnbalanceWasCalled::set(false);
			mock::make_all_reward_payment(i - 1);
			assert_eq_error_rate!(
				pallet_balances::TotalIssuance::<Test>::get(),
				pre_payout_total_issuance + payout,
				2
			);
			assert!(RewardOnUnbalanceWasCalled::get());

			// verify we track rewards for each era and page
			for page in 0..Eras::<Test>::exposure_page_count(i - 1, &11) {
				assert_eq!(Eras::<Test>::is_rewards_claimed(i - 1, &11, page), true);
			}
		}

		assert_eq!(ErasClaimedRewards::<Test>::get(14, &11), vec![0, 1]);

		let last_era = 99;
		let history_depth = HistoryDepth::get();
		let last_reward_era = last_era - 1;
		let first_claimable_reward_era = last_era - history_depth;
		for i in 16..=last_era {
			Staking::reward_by_ids(vec![(11, 1)]);
			// compute and ensure the reward amount is greater than zero.
			let _ = validator_payout_for(time_per_era());
			mock::start_active_era(i);
		}

		// verify we clean up history as we go
		for era in 0..15 {
			assert_eq!(ErasClaimedRewards::<Test>::get(era, &11), Vec::<sp_staking::Page>::new());
		}

		// verify only page 0 is marked as claimed
		assert_ok!(Staking::payout_stakers(
			RuntimeOrigin::signed(1337),
			11,
			first_claimable_reward_era
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(first_claimable_reward_era, &11), vec![0]);

		// verify page 0 and 1 are marked as claimed
		assert_ok!(Staking::payout_stakers(
			RuntimeOrigin::signed(1337),
			11,
			first_claimable_reward_era,
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(first_claimable_reward_era, &11), vec![0, 1]);

		// change order and verify only page 1 is marked as claimed
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			last_reward_era,
			1
		));
		assert_eq!(ErasClaimedRewards::<Test>::get(last_reward_era, &11), vec![1]);

		// verify page 0 is claimed even when explicit page is not passed
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, last_reward_era,));

		assert_eq!(ErasClaimedRewards::<Test>::get(last_reward_era, &11), vec![1, 0]);

		// cannot claim any more pages
		assert_noop!(
			Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, last_reward_era),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		// Create 4 nominator pages
		for i in 100..200 {
			let bond_amount = balance + i as Balance;
			bond_nominator(1000 + i, bond_amount, vec![11]);
		}

		let test_era = last_era + 1;
		mock::start_active_era(test_era);

		Staking::reward_by_ids(vec![(11, 1)]);
		// compute and ensure the reward amount is greater than zero.
		let _ = validator_payout_for(time_per_era());
		mock::start_active_era(test_era + 1);

		// Out of order claims works.
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, test_era, 2));
		assert_eq!(ErasClaimedRewards::<Test>::get(test_era, &11), vec![2]);

		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, test_era));
		assert_eq!(ErasClaimedRewards::<Test>::get(test_era, &11), vec![2, 0]);

		// cannot claim page 2 again
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, test_era, 2),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, test_era));
		assert_eq!(ErasClaimedRewards::<Test>::get(test_era, &11), vec![2, 0, 1]);

		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, test_era));
		assert_eq!(ErasClaimedRewards::<Test>::get(test_era, &11), vec![2, 0, 1, 3]);
	});
}

#[test]
fn test_page_count_and_size() {
	// Test that payout_stakers work in general and that it pays the correct amount of reward.
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		let balance = 1000;
		// Track the exposure of the validator and all nominators.
		// Create a validator:
		bond_validator(11, balance); // Default(64)
		assert_eq!(Validators::<Test>::count(), 1);

		// Create nominators, targeting stash of validators
		for i in 0..100 {
			let bond_amount = balance + i as Balance;
			bond_nominator(1000 + i, bond_amount, vec![11]);
		}

		mock::start_active_era(1);

		// Since max exposure page size is 64, 2 pages of nominators are created.
		assert_eq!(MaxExposurePageSize::get(), 64);
		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 2);

		// first page has 64 nominators
		assert_eq!(Eras::<Test>::get_paged_exposure(1, &11, 0).unwrap().others().len(), 64);
		// second page has 36 nominators
		assert_eq!(Eras::<Test>::get_paged_exposure(1, &11, 1).unwrap().others().len(), 36);

		// now lets decrease page size
		MaxExposurePageSize::set(32);
		mock::start_active_era(2);
		// now we expect 4 pages.
		assert_eq!(Eras::<Test>::exposure_page_count(2, &11), 4);
		// first 3 pages have 32 nominators each
		assert_eq!(Eras::<Test>::get_paged_exposure(2, &11, 0).unwrap().others().len(), 32);
		assert_eq!(Eras::<Test>::get_paged_exposure(2, &11, 1).unwrap().others().len(), 32);
		assert_eq!(Eras::<Test>::get_paged_exposure(2, &11, 2).unwrap().others().len(), 32);
		assert_eq!(Eras::<Test>::get_paged_exposure(2, &11, 3).unwrap().others().len(), 4);

		// now lets decrease page size even more
		MaxExposurePageSize::set(5);
		mock::start_active_era(3);

		// now we expect the max 20 pages (100/5).
		assert_eq!(Eras::<Test>::exposure_page_count(3, &11), 20);
	});
}

#[test]
fn payout_stakers_handles_basic_errors() {
	// Here we will test payouts handle all errors.
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// Consumed weight for all payout_stakers dispatches that fail
		let err_weight = <Test as Config>::WeightInfo::payout_stakers_alive_staked(0);

		// Same setup as the test above
		let balance = 1000;
		bond_validator(11, balance); // Default(64)

		// Create nominators, targeting stash
		for i in 0..100 {
			bond_nominator(1000 + i, balance + i as Balance, vec![11]);
		}

		mock::start_active_era(1);
		Staking::reward_by_ids(vec![(11, 1)]);

		// compute and ensure the reward amount is greater than zero.
		let _ = validator_payout_for(time_per_era());

		mock::start_active_era(2);

		// Wrong Era, too big
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 2, 0),
			Error::<Test>::InvalidEraToReward.with_weight(err_weight)
		);
		// Wrong Staker
		assert_noop!(
			Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 10, 1, 0),
			Error::<Test>::NotStash.with_weight(err_weight)
		);

		let last_era = 99;
		for i in 3..=last_era {
			Staking::reward_by_ids(vec![(11, 1)]);
			// compute and ensure the reward amount is greater than zero.
			let _ = validator_payout_for(time_per_era());
			mock::start_active_era(i);
		}

		let history_depth = HistoryDepth::get();
		let expected_last_reward_era = last_era - 1;
		let expected_start_reward_era = last_era - history_depth;

		// We are at era last_era=99. Given history_depth=80, we should be able
		// to payout era starting from expected_start_reward_era=19 through
		// expected_last_reward_era=98 (80 total eras), but not 18 or 99.
		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_start_reward_era - 1,
				0
			),
			Error::<Test>::InvalidEraToReward.with_weight(err_weight)
		);
		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_last_reward_era + 1,
				0
			),
			Error::<Test>::InvalidEraToReward.with_weight(err_weight)
		);
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			expected_start_reward_era,
			0
		));
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			expected_last_reward_era,
			0
		));

		// can call page 1
		assert_ok!(Staking::payout_stakers_by_page(
			RuntimeOrigin::signed(1337),
			11,
			expected_last_reward_era,
			1
		));

		// Can't claim again
		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_start_reward_era,
				0
			),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_last_reward_era,
				0
			),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_last_reward_era,
				1
			),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);

		// invalid page
		assert_noop!(
			Staking::payout_stakers_by_page(
				RuntimeOrigin::signed(1337),
				11,
				expected_last_reward_era,
				2
			),
			Error::<Test>::InvalidPage.with_weight(err_weight)
		);
	});
}

#[test]
fn test_commission_paid_across_pages() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		let balance = 1;
		let commission = 50;
		// Create a validator:
		bond_validator(11, balance);
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(11),
			ValidatorPrefs { commission: Perbill::from_percent(commission), blocked: false }
		));
		assert_eq!(Validators::<Test>::count(), 1);

		// Create nominators, targeting stash of validators
		for i in 0..200 {
			let bond_amount = balance + i as Balance;
			bond_nominator(1000 + i, bond_amount, vec![11]);
		}

		mock::start_active_era(1);
		Staking::reward_by_ids(vec![(11, 1)]);

		// Since `MaxExposurePageSize = 64`, there are four pages of validator
		// exposure.
		assert_eq!(Eras::<Test>::exposure_page_count(1, &11), 4);

		// compute and ensure the reward amount is greater than zero.
		let payout = validator_payout_for(time_per_era());
		mock::start_active_era(2);

		let initial_balance = asset::stakeable_balance::<Test>(&11);
		// Payout rewards for first exposure page
		assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, 0));

		let controller_balance_after_p0_payout = asset::stakeable_balance::<Test>(&11);

		// some commission is paid
		assert!(initial_balance < controller_balance_after_p0_payout);

		// payout all pages
		for i in 1..4 {
			let before_balance = asset::stakeable_balance::<Test>(&11);
			assert_ok!(Staking::payout_stakers_by_page(RuntimeOrigin::signed(1337), 11, 1, i));
			let after_balance = asset::stakeable_balance::<Test>(&11);
			// some commission is paid for every page
			assert!(before_balance < after_balance);
		}

		assert_eq_error_rate!(
			asset::stakeable_balance::<Test>(&11),
			initial_balance + payout / 2,
			1,
		);
	});
}

#[test]
fn payout_stakers_handles_weight_refund() {
	// Note: this test relies on the assumption that `payout_stakers_alive_staked` is solely used by
	// `payout_stakers` to calculate the weight of each payout op.
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		let max_nom_rewarded = MaxExposurePageSize::get();
		// Make sure the configured value is meaningful for our use.
		assert!(max_nom_rewarded >= 4);
		let half_max_nom_rewarded = max_nom_rewarded / 2;
		// Sanity check our max and half max nominator quantities.
		assert!(half_max_nom_rewarded > 0);
		assert!(max_nom_rewarded > half_max_nom_rewarded);

		let max_nom_rewarded_weight =
			<Test as Config>::WeightInfo::payout_stakers_alive_staked(max_nom_rewarded);
		let half_max_nom_rewarded_weight =
			<Test as Config>::WeightInfo::payout_stakers_alive_staked(half_max_nom_rewarded);
		let zero_nom_payouts_weight = <Test as Config>::WeightInfo::payout_stakers_alive_staked(0);
		assert!(zero_nom_payouts_weight.any_gt(Weight::zero()));
		assert!(half_max_nom_rewarded_weight.any_gt(zero_nom_payouts_weight));
		assert!(max_nom_rewarded_weight.any_gt(half_max_nom_rewarded_weight));

		let balance = 1000;
		bond_validator(11, balance);

		// Era 1
		start_active_era(1);

		// Reward just the validator.
		Staking::reward_by_ids(vec![(11, 1)]);

		// Add some `half_max_nom_rewarded` nominators who will start backing the validator in the
		// next era.
		for i in 0..half_max_nom_rewarded {
			bond_nominator((1000 + i).into(), balance + i as Balance, vec![11]);
		}

		// Era 2
		start_active_era(2);

		// Collect payouts when there are no nominators
		let call = TestCall::Staking(StakingCall::payout_stakers_by_page {
			validator_stash: 11,
			era: 1,
			page: 0,
		});
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(20));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), zero_nom_payouts_weight);

		// The validator is not rewarded in this era; so there will be zero payouts to claim for
		// this era.

		// Era 3
		start_active_era(3);

		// Collect payouts for an era where the validator did not receive any points.
		let call = TestCall::Staking(StakingCall::payout_stakers_by_page {
			validator_stash: 11,
			era: 2,
			page: 0,
		});
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(20));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), zero_nom_payouts_weight);

		// Reward the validator and its nominators.
		Staking::reward_by_ids(vec![(11, 1)]);

		// Era 4
		start_active_era(4);

		// Collect payouts when the validator has `half_max_nom_rewarded` nominators.
		let call = TestCall::Staking(StakingCall::payout_stakers_by_page {
			validator_stash: 11,
			era: 3,
			page: 0,
		});
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(20));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), half_max_nom_rewarded_weight);

		// Add enough nominators so that we are at the limit. They will be active nominators
		// in the next era.
		for i in half_max_nom_rewarded..max_nom_rewarded {
			bond_nominator((1000 + i).into(), balance + i as Balance, vec![11]);
		}

		// Era 5
		start_active_era(5);
		// We now have `max_nom_rewarded` nominators actively nominating our validator.

		// Reward the validator so we can collect for everyone in the next era.
		Staking::reward_by_ids(vec![(11, 1)]);

		// Era 6
		start_active_era(6);

		// Collect payouts when the validator had `half_max_nom_rewarded` nominators.
		let call = TestCall::Staking(StakingCall::payout_stakers_by_page {
			validator_stash: 11,
			era: 5,
			page: 0,
		});
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(20));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), max_nom_rewarded_weight);

		// Try and collect payouts for an era that has already been collected.
		let call = TestCall::Staking(StakingCall::payout_stakers_by_page {
			validator_stash: 11,
			era: 5,
			page: 0,
		});
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(20));
		assert!(result.is_err());
		// When there is an error the consumed weight == weight when there are 0 nominator payouts.
		assert_eq!(extract_actual_weight(&result, &info), zero_nom_payouts_weight);
	});
}

#[test]
fn bond_during_era_does_not_populate_legacy_claimed_rewards() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// Era = None
		bond_validator(9, 1000);
		assert_eq!(
			Staking::ledger(9.into()).unwrap(),
			StakingLedgerInspect {
				stash: 9,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		mock::start_active_era(5);
		bond_validator(11, 1000);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// make sure only era up to history depth is stored
		let current_era = 99;
		mock::start_active_era(current_era);
		bond_validator(13, 1000);
		assert_eq!(
			Staking::ledger(13.into()).unwrap(),
			StakingLedgerInspect {
				stash: 13,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: Default::default(),
			}
		);
	});
}

#[test]
#[ignore]
fn offences_weight_calculated_correctly() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		// On offence with zero offenders: 4 Reads, 1 Write
		let zero_offence_weight =
			<Test as frame_system::Config>::DbWeight::get().reads_writes(4, 1);
		assert_eq!(
			Staking::on_offence(&[], &[Perbill::from_percent(50)], 0),
			zero_offence_weight
		);

		// On Offence with N offenders, Unapplied: 4 Reads, 1 Write + 4 Reads, 5 Writes
		let n_offence_unapplied_weight = <Test as frame_system::Config>::DbWeight::get()
			.reads_writes(4, 1) +
			<Test as frame_system::Config>::DbWeight::get().reads_writes(4, 5);

		let offenders: Vec<
			OffenceDetails<
				<Test as frame_system::Config>::AccountId,
				pallet_session::historical::IdentificationTuple<Test>,
			>,
		> = (1..10)
			.map(|i| OffenceDetails {
				offender: (i, ()),
				reporters: vec![],
			})
			.collect();
		assert_eq!(
			Staking::on_offence(
				&offenders,
				&[Perbill::from_percent(50)],
				0,
			),
			n_offence_unapplied_weight
		);

		// On Offence with one offenders, Applied
		let one_offender = [offence_from(11, Some(1))];

		let n = 1; // Number of offenders
		let rw = 3 + 3 * n; // rw reads and writes
		let one_offence_unapplied_weight =
			<Test as frame_system::Config>::DbWeight::get().reads_writes(4, 1)
		 +
			<Test as frame_system::Config>::DbWeight::get().reads_writes(rw, rw)
			// One `slash_cost`
			+ <Test as frame_system::Config>::DbWeight::get().reads_writes(6, 5)
			// `slash_cost` * nominators (1)
			+ <Test as frame_system::Config>::DbWeight::get().reads_writes(6, 5)
			// `reward_cost` * reporters (1)
			+ <Test as frame_system::Config>::DbWeight::get().reads_writes(2, 2)
		;

		assert_eq!(
			Staking::on_offence(
				&one_offender,
				&[Perbill::from_percent(50)],
				0,
			),
			one_offence_unapplied_weight
		);
	});
}

#[test]
fn cannot_rebond_to_lower_than_ed() {
	ExtBuilder::default()
		.existential_deposit(11)
		.balance_factor(11)
		.build_and_execute(|| {
			// initial stuff.
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: 11 * 1000,
					active: 11 * 1000,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// unbond all of it. must be chilled first.
			assert_ok!(Staking::chill(RuntimeOrigin::signed(21)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(21), 11 * 1000));
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: 11 * 1000,
					active: 0,
					unlocking: bounded_vec![UnlockChunk { value: 11 * 1000, era: 3 }],
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// now bond a wee bit more
			assert_noop!(
				Staking::rebond(RuntimeOrigin::signed(21), 5),
				Error::<Test>::InsufficientBond
			);
		})
}

#[test]
fn cannot_bond_extra_to_lower_than_ed() {
	ExtBuilder::default()
		.existential_deposit(11)
		.balance_factor(11)
		.build_and_execute(|| {
			// initial stuff.
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: 11 * 1000,
					active: 11 * 1000,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// unbond all of it. must be chilled first.
			assert_ok!(Staking::chill(RuntimeOrigin::signed(21)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(21), 11 * 1000));
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: 11 * 1000,
					active: 0,
					unlocking: bounded_vec![UnlockChunk { value: 11 * 1000, era: 3 }],
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// now bond a wee bit more
			assert_noop!(
				Staking::bond_extra(RuntimeOrigin::signed(21), 5),
				Error::<Test>::InsufficientBond,
			);
		})
}

#[test]
fn do_not_die_when_active_is_ed() {
	let ed = 10;
	ExtBuilder::default()
		.existential_deposit(ed)
		.balance_factor(ed)
		.build_and_execute(|| {
			// given
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: 1000 * ed,
					active: 1000 * ed,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// when unbond all of it except ed.
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(21), 999 * ed));
			start_active_era(3);
			assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(21), 100));

			// then
			assert_eq!(
				Staking::ledger(21.into()).unwrap(),
				StakingLedgerInspect {
					stash: 21,
					total: ed,
					active: ed,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);
		})
}

#[test]
fn restricted_accounts_can_only_withdraw() {
	ExtBuilder::default().build_and_execute(|| {
		start_active_era(1);
		// alice is a non blacklisted account.
		let alice = 301;
		let _ = Balances::make_free_balance_be(&alice, 500);
		// alice can bond
		assert_ok!(Staking::bond(RuntimeOrigin::signed(alice), 100, RewardDestination::Staked));
		// and bob is a blacklisted account
		let bob = 302;
		let _ = Balances::make_free_balance_be(&bob, 500);
		restrict(&bob);

		// Bob cannot bond
		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(bob), 100, RewardDestination::Staked,),
			Error::<Test>::Restricted
		);

		// alice is blacklisted now and cannot bond anymore
		restrict(&alice);
		assert_noop!(
			Staking::bond_extra(RuntimeOrigin::signed(alice), 100),
			Error::<Test>::Restricted
		);
		// but she can unbond her existing bond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));

		// she cannot rebond the unbonded amount
		start_active_era(2);
		assert_noop!(Staking::rebond(RuntimeOrigin::signed(alice), 50), Error::<Test>::Restricted);

		// move to era when alice fund can be withdrawn
		start_active_era(4);
		// alice can withdraw now
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(alice), 0));
		// she still cannot bond
		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(alice), 100, RewardDestination::Staked,),
			Error::<Test>::Restricted
		);

		// bob is removed from restrict list
		remove_from_restrict_list(&bob);
		// bob can bond now
		assert_ok!(Staking::bond(RuntimeOrigin::signed(bob), 100, RewardDestination::Staked));
		// and bond extra
		assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(bob), 100));

		start_active_era(6);
		// unbond also works.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(bob), 100));
		// bob can withdraw as well.
		start_active_era(9);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(bob), 0));
	})
}

#[test]
fn permissionless_withdraw_overstake() {
	ExtBuilder::default().build_and_execute(|| {
		// Given Alice, Bob and Charlie with some stake.
		let alice = 301;
		let bob = 302;
		let charlie = 303;
		let _ = Balances::make_free_balance_be(&alice, 500);
		let _ = Balances::make_free_balance_be(&bob, 500);
		let _ = Balances::make_free_balance_be(&charlie, 500);
		assert_ok!(Staking::bond(RuntimeOrigin::signed(alice), 100, RewardDestination::Staked));
		assert_ok!(Staking::bond(RuntimeOrigin::signed(bob), 100, RewardDestination::Staked));
		assert_ok!(Staking::bond(RuntimeOrigin::signed(charlie), 100, RewardDestination::Staked));

		// WHEN: charlie is partially unbonding.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(charlie), 90));
		let charlie_ledger = StakingLedger::<Test>::get(StakingAccount::Stash(charlie)).unwrap();

		// AND: alice and charlie ledger having higher value than actual stake.
		Ledger::<Test>::insert(alice, StakingLedger::<Test>::new(alice, 200));
		Ledger::<Test>::insert(
			charlie,
			StakingLedger { stash: charlie, total: 200, active: 200 - 90, ..charlie_ledger },
		);

		// THEN overstake can be permissionlessly withdrawn.
		System::reset_events();

		// Alice stake is corrected.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice).unwrap(),
			Stake { total: 200, active: 200 }
		);
		assert_ok!(Staking::withdraw_overstake(RuntimeOrigin::signed(1), alice));
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice).unwrap(),
			Stake { total: 100, active: 100 }
		);

		// Charlie who is partially withdrawing also gets their stake corrected.
		assert_eq!(
			<Staking as StakingInterface>::stake(&charlie).unwrap(),
			Stake { total: 200, active: 110 }
		);
		assert_ok!(Staking::withdraw_overstake(RuntimeOrigin::signed(1), charlie));
		assert_eq!(
			<Staking as StakingInterface>::stake(&charlie).unwrap(),
			Stake { total: 200 - 100, active: 110 - 100 }
		);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::Withdrawn { stash: alice, amount: 200 - 100 },
				Event::Withdrawn { stash: charlie, amount: 200 - 100 }
			]
		);

		// but Bob ledger is fine and that cannot be withdrawn.
		assert_noop!(
			Staking::withdraw_overstake(RuntimeOrigin::signed(1), bob),
			Error::<Test>::BoundNotMet
		);
	});
}

#[test]
#[should_panic]
fn count_check_works() {
	ExtBuilder::default().build_and_execute(|| {
		// We should never insert into the validators or nominators map directly as this will
		// not keep track of the count. This test should panic as we verify the count is accurate
		// after every test using the `post_checks` in `mock`.
		Validators::<Test>::insert(987654321, ValidatorPrefs::default());
		Nominators::<Test>::insert(
			987654321,
			Nominations {
				targets: Default::default(),
				submitted_in: Default::default(),
				suppressed: false,
			},
		);
	})
}

#[test]
#[should_panic = "called `Result::unwrap()` on an `Err` value: Other(\"number of entries in payee storage items does not match the number of bonded ledgers\")"]
fn check_payee_invariant1_works() {
	// A bonded ledger should always have an assigned `Payee` This test should panic as we verify
	// that a bad state will panic due to the `try_state` checks in the `post_checks` in `mock`.
	ExtBuilder::default().build_and_execute(|| {
		let rogue_ledger = StakingLedger::<Test>::new(123456, 20);
		Ledger::<Test>::insert(123456, rogue_ledger);
	})
}

#[test]
#[should_panic = "called `Result::unwrap()` on an `Err` value: Other(\"number of entries in payee storage items does not match the number of bonded ledgers\")"]
fn check_payee_invariant2_works() {
	// The number of entries in both `Payee` and of bonded staking ledgers should match. This test
	// should panic as we verify that a bad state will panic due to the `try_state` checks in the
	// `post_checks` in `mock`.
	ExtBuilder::default().build_and_execute(|| {
		Payee::<Test>::insert(1111, RewardDestination::Staked);
	})
}

#[test]
fn min_bond_checks_work() {
	ExtBuilder::default()
		.existential_deposit(100)
		.balance_factor(100)
		.min_nominator_bond(1_000)
		.min_validator_bond(1_500)
		.build_and_execute(|| {
			// 500 is not enough for any role
			assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 500, RewardDestination::Stash));
			assert_noop!(
				Staking::nominate(RuntimeOrigin::signed(3), vec![1]),
				Error::<Test>::InsufficientBond
			);
			assert_noop!(
				Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()),
				Error::<Test>::InsufficientBond,
			);

			// 1000 is enough for nominator
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(3), 500));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![1]));
			assert_noop!(
				Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()),
				Error::<Test>::InsufficientBond,
			);

			// 1500 is enough for validator
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(3), 500));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![1]));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()));

			// Can't unbond anything as validator
			assert_noop!(
				Staking::unbond(RuntimeOrigin::signed(3), 500),
				Error::<Test>::InsufficientBond
			);

			// Once they are a nominator, they can unbond 500
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(3), vec![1]));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(3), 500));
			assert_noop!(
				Staking::unbond(RuntimeOrigin::signed(3), 500),
				Error::<Test>::InsufficientBond
			);

			// Once they are chilled they can unbond everything
			assert_ok!(Staking::chill(RuntimeOrigin::signed(3)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(3), 1000));
		})
}

#[test]
fn chill_other_works() {
	ExtBuilder::default()
		.existential_deposit(100)
		.balance_factor(100)
		.min_nominator_bond(1_000)
		.min_validator_bond(1_500)
		.build_and_execute(|| {
			let initial_validators = Validators::<Test>::count();
			let initial_nominators = Nominators::<Test>::count();
			for i in 0..15 {
				let a = 4 * i;
				let b = 4 * i + 2;
				let c = 4 * i + 3;
				asset::set_stakeable_balance::<Test>(&a, 100_000);
				asset::set_stakeable_balance::<Test>(&b, 100_000);
				asset::set_stakeable_balance::<Test>(&c, 100_000);

				// Nominator
				assert_ok!(Staking::bond(RuntimeOrigin::signed(a), 1000, RewardDestination::Stash));
				assert_ok!(Staking::nominate(RuntimeOrigin::signed(a), vec![1]));

				// Validator
				assert_ok!(Staking::bond(RuntimeOrigin::signed(b), 1500, RewardDestination::Stash));
				assert_ok!(Staking::validate(RuntimeOrigin::signed(b), ValidatorPrefs::default()));
			}

			// To chill other users, we need to:
			// * Set a minimum bond amount
			// * Set a limit
			// * Set a threshold
			//
			// If any of these are missing, we do not have enough information to allow the
			// `chill_other` to succeed from one user to another.
			//
			// Out of 8 possible cases, only one will allow the use of `chill_other`, which is
			// when all 3 conditions are met.

			// 1. No limits whatsoever
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
			));

			// Can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 2. Change only the minimum bonds.
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Set(1_500),
				ConfigOp::Set(2_000),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 3. Add nominator/validator count limits, but no other threshold.
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Set(10),
				ConfigOp::Set(10),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 4. Add chil threshold, but no other limits
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Set(Percent::from_percent(75)),
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 5. Add bond and count limits, but no threshold
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Set(1_500),
				ConfigOp::Set(2_000),
				ConfigOp::Set(10),
				ConfigOp::Set(10),
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Remove,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 6. Add bond and threshold limits, but no count limits
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Set(Percent::from_percent(75)),
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 7. Add count limits and a chill threshold, but no bond limits
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Remove,
				ConfigOp::Remove,
				ConfigOp::Set(10),
				ConfigOp::Set(10),
				ConfigOp::Set(Percent::from_percent(75)),
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// Still can't chill these users
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 2),
				Error::<Test>::CannotChillOther
			);

			// 8. Add all limits
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Set(1_500),
				ConfigOp::Set(2_000),
				ConfigOp::Set(10),
				ConfigOp::Set(10),
				ConfigOp::Set(Percent::from_percent(75)),
				ConfigOp::Noop,
				ConfigOp::Noop,
			));

			// 16 people total because tests start with 2 active one
			assert_eq!(Nominators::<Test>::count(), 15 + initial_nominators);
			assert_eq!(Validators::<Test>::count(), 15 + initial_validators);

			// Users can now be chilled down to 7 people, so we try to remove 9 of them (starting
			// with 16)
			for i in 6..15 {
				let b = 4 * i;
				let d = 4 * i + 2;
				assert_ok!(Staking::chill_other(RuntimeOrigin::signed(1337), b));
				assert_eq!(*staking_events().last().unwrap(), Event::Chilled { stash: b });
				assert_ok!(Staking::chill_other(RuntimeOrigin::signed(1337), d));
				assert_eq!(*staking_events().last().unwrap(), Event::Chilled { stash: d });
			}

			// chill a nominator. Limit is not reached, not chill-able
			assert_eq!(Nominators::<Test>::count(), 7);
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1337), 0),
				Error::<Test>::CannotChillOther
			);
			// chill a validator. Limit is reached, chill-able.
			assert_eq!(Validators::<Test>::count(), 9);
			assert_ok!(Staking::chill_other(RuntimeOrigin::signed(1337), 2));
		})
}

#[test]
fn capped_stakers_works() {
	ExtBuilder::default().build_and_execute(|| {
		let validator_count = Validators::<Test>::count();
		assert_eq!(validator_count, 3);
		let nominator_count = Nominators::<Test>::count();
		assert_eq!(nominator_count, 1);

		// Change the maximums
		let max = 10;
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Set(10),
			ConfigOp::Set(10),
			ConfigOp::Set(max),
			ConfigOp::Set(max),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Noop,
		));

		// can create `max - validator_count` validators
		let mut some_existing_validator = AccountId::default();
		for i in 0..max - validator_count {
			let (_, controller) = testing_utils::create_stash_controller::<Test>(
				i + 10_000_000,
				100,
				RewardDestination::Stash,
			)
			.unwrap();
			assert_ok!(Staking::validate(
				RuntimeOrigin::signed(controller),
				ValidatorPrefs::default()
			));
			some_existing_validator = controller;
		}

		// but no more
		let (_, last_validator) =
			testing_utils::create_stash_controller::<Test>(1337, 100, RewardDestination::Stash)
				.unwrap();

		assert_noop!(
			Staking::validate(RuntimeOrigin::signed(last_validator), ValidatorPrefs::default()),
			Error::<Test>::TooManyValidators,
		);

		// same with nominators
		let mut some_existing_nominator = AccountId::default();
		for i in 0..max - nominator_count {
			let (_, controller) = testing_utils::create_stash_controller::<Test>(
				i + 20_000_000,
				100,
				RewardDestination::Stash,
			)
			.unwrap();
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(controller), vec![1]));
			some_existing_nominator = controller;
		}

		// one more is too many.
		let (_, last_nominator) = testing_utils::create_stash_controller::<Test>(
			30_000_000,
			100,
			RewardDestination::Stash,
		)
		.unwrap();
		assert_noop!(
			Staking::nominate(RuntimeOrigin::signed(last_nominator), vec![1]),
			Error::<Test>::TooManyNominators
		);

		// Re-nominate works fine
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(some_existing_nominator), vec![1]));
		// Re-validate works fine
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(some_existing_validator),
			ValidatorPrefs::default()
		));

		// No problem when we set to `None` again
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(last_nominator), vec![1]));
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(last_validator),
			ValidatorPrefs::default()
		));
	})
}

#[test]
fn min_commission_works() {
	ExtBuilder::default().build_and_execute(|| {
		// account 11 controls the stash of itself.
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(11),
			ValidatorPrefs { commission: Perbill::from_percent(5), blocked: false }
		));

		// event emitted should be correct
		assert_eq!(
			*staking_events().last().unwrap(),
			Event::ValidatorPrefsSet {
				stash: 11,
				prefs: ValidatorPrefs { commission: Perbill::from_percent(5), blocked: false }
			}
		);

		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Set(Perbill::from_percent(10)),
			ConfigOp::Noop,
		));

		// can't make it less than 10 now
		assert_noop!(
			Staking::validate(
				RuntimeOrigin::signed(11),
				ValidatorPrefs { commission: Perbill::from_percent(5), blocked: false }
			),
			Error::<Test>::CommissionTooLow
		);

		// can only change to higher.
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(11),
			ValidatorPrefs { commission: Perbill::from_percent(10), blocked: false }
		));

		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(11),
			ValidatorPrefs { commission: Perbill::from_percent(15), blocked: false }
		));
	})
}

#[test]
#[should_panic]
#[cfg(debug_assertions)]
fn change_of_absolute_max_nominations() {
	use frame_election_provider_support::ElectionDataProvider;
	ExtBuilder::default()
		.add_staker(61, 61, 10, StakerStatus::Nominator(vec![1]))
		.add_staker(71, 71, 10, StakerStatus::Nominator(vec![1, 2, 3]))
		.balance_factor(10)
		.build_and_execute(|| {
			// pre-condition
			assert_eq!(AbsoluteMaxNominations::get(), 16);

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(101, 2), (71, 3), (61, 1)]
			);

			// default bounds are unbounded.
			let bounds = DataProviderBounds::default();

			// 3 validators and 3 nominators
			assert_eq!(Staking::electing_voters(bounds, 0).unwrap().len(), 3 + 3);

			// abrupt change from 16 to 4, everyone should be fine.
			AbsoluteMaxNominations::set(4);

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(101, 2), (71, 3), (61, 1)]
			);
			assert_eq!(Staking::electing_voters(bounds, 0).unwrap().len(), 3 + 3);

			// No one can be chilled on account of non-decodable keys.
			for k in Nominators::<Test>::iter_keys() {
				assert_noop!(
					Staking::chill_other(RuntimeOrigin::signed(1), k),
					Error::<Test>::CannotChillOther
				);
			}

			// abrupt change from 4 to 3, everyone should be fine.
			AbsoluteMaxNominations::set(3);

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(101, 2), (71, 3), (61, 1)]
			);
			assert_eq!(Staking::electing_voters(bounds, 0).unwrap().len(), 3 + 3);

			// As before, no one can be chilled on account of non-decodable keys.
			for k in Nominators::<Test>::iter_keys() {
				assert_noop!(
					Staking::chill_other(RuntimeOrigin::signed(1), k),
					Error::<Test>::CannotChillOther
				);
			}

			// abrupt change from 3 to 2, this should cause some nominators to be non-decodable, and
			// thus non-existent unless they update.
			AbsoluteMaxNominations::set(2);

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(101, 2), (61, 1)]
			);

			// 101 and 61 still cannot be chilled by someone else.
			for k in [101, 61].iter() {
				assert_noop!(
					Staking::chill_other(RuntimeOrigin::signed(1), *k),
					Error::<Test>::CannotChillOther
				);
			}

			// 71 is still in storage..
			assert!(Nominators::<Test>::contains_key(71));
			// but its value cannot be decoded and default is returned.
			assert!(Nominators::<Test>::get(71).is_none());

			assert_eq!(Staking::electing_voters(bounds, 0).unwrap().len(), 3 + 2);
			assert!(Nominators::<Test>::contains_key(101));

			// abrupt change from 2 to 1, this should cause some nominators to be non-decodable, and
			// thus non-existent unless they update.
			AbsoluteMaxNominations::set(1);

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(61, 1)]
			);

			// 61 *still* cannot be chilled by someone else.
			assert_noop!(
				Staking::chill_other(RuntimeOrigin::signed(1), 61),
				Error::<Test>::CannotChillOther
			);

			assert!(Nominators::<Test>::contains_key(71));
			assert!(Nominators::<Test>::contains_key(61));
			assert!(Nominators::<Test>::get(71).is_none());
			assert!(Nominators::<Test>::get(61).is_some());
			assert_eq!(Staking::electing_voters(bounds, 0).unwrap().len(), 3 + 1);

			// now one of them can revive themselves by re-nominating to a proper value.
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(71), vec![1]));
			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(71, 1), (61, 1)]
			);

			// or they can be chilled by any account.
			assert!(Nominators::<Test>::contains_key(101));
			assert!(Nominators::<Test>::get(101).is_none());
			assert_ok!(Staking::chill_other(RuntimeOrigin::signed(71), 101));
			assert_eq!(*staking_events().last().unwrap(), Event::Chilled { stash: 101 });
			assert!(!Nominators::<Test>::contains_key(101));
			assert!(Nominators::<Test>::get(101).is_none());
		})
}

#[test]
fn nomination_quota_max_changes_decoding() {
	use frame_election_provider_support::ElectionDataProvider;
	ExtBuilder::default()
		.add_staker(60, 61, 10, StakerStatus::Nominator(vec![1]))
		.add_staker(70, 71, 10, StakerStatus::Nominator(vec![1, 2, 3]))
		.add_staker(30, 330, 10, StakerStatus::Nominator(vec![1, 2, 3, 4]))
		.add_staker(50, 550, 10, StakerStatus::Nominator(vec![1, 2, 3, 4]))
		.balance_factor(11)
		.build_and_execute(|| {
			// pre-condition.
			assert_eq!(MaxNominationsOf::<Test>::get(), 16);

			let unbonded_election = DataProviderBounds::default();

			assert_eq!(
				Nominators::<Test>::iter()
					.map(|(k, n)| (k, n.targets.len()))
					.collect::<Vec<_>>(),
				vec![(70, 3), (101, 2), (50, 4), (30, 4), (60, 1)]
			);
			// 4 validators and 4 nominators
			assert_eq!(Staking::electing_voters(unbonded_election, 0).unwrap().len(), 4 + 4);
		});
}

#[test]
fn api_nominations_quota_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(Staking::api_nominations_quota(10), MaxNominationsOf::<Test>::get());
		assert_eq!(Staking::api_nominations_quota(333), MaxNominationsOf::<Test>::get());
		assert_eq!(Staking::api_nominations_quota(222), 2);
		assert_eq!(Staking::api_nominations_quota(111), 1);
	})
}

mod sorted_list_provider {
	use super::*;
	use frame_election_provider_support::SortedListProvider;

	#[test]
	fn re_nominate_does_not_change_counters_or_list() {
		ExtBuilder::default().nominate(true).build_and_execute(|| {
			// given
			let pre_insert_voter_count =
				(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
			assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

			assert_eq!(
				<Test as Config>::VoterList::iter().collect::<Vec<_>>(),
				vec![11, 21, 31, 101]
			);

			// when account 101 renominates
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![41]));

			// then counts don't change
			assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
			// and the list is the same
			assert_eq!(
				<Test as Config>::VoterList::iter().collect::<Vec<_>>(),
				vec![11, 21, 31, 101]
			);
		});
	}

	#[test]
	fn re_validate_does_not_change_counters_or_list() {
		ExtBuilder::default().nominate(false).build_and_execute(|| {
			// given
			let pre_insert_voter_count =
				(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
			assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

			assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);

			// when account 11 re-validates
			assert_ok!(Staking::validate(RuntimeOrigin::signed(11), Default::default()));

			// then counts don't change
			assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
			// and the list is the same
			assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);
		});
	}
}

#[test]
fn force_apply_min_commission_works() {
	let prefs = |c| ValidatorPrefs { commission: Perbill::from_percent(c), blocked: false };
	let validators = || Validators::<Test>::iter().collect::<Vec<_>>();
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::validate(RuntimeOrigin::signed(31), prefs(10)));
		assert_ok!(Staking::validate(RuntimeOrigin::signed(21), prefs(5)));

		// Given
		assert_eq!(validators(), vec![(31, prefs(10)), (21, prefs(5)), (11, prefs(0))]);
		MinCommission::<Test>::set(Perbill::from_percent(5));

		// When applying to a commission greater than min
		assert_ok!(Staking::force_apply_min_commission(RuntimeOrigin::signed(1), 31));
		// Then the commission is not changed
		assert_eq!(validators(), vec![(31, prefs(10)), (21, prefs(5)), (11, prefs(0))]);

		// When applying to a commission that is equal to min
		assert_ok!(Staking::force_apply_min_commission(RuntimeOrigin::signed(1), 21));
		// Then the commission is not changed
		assert_eq!(validators(), vec![(31, prefs(10)), (21, prefs(5)), (11, prefs(0))]);

		// When applying to a commission that is less than the min
		assert_ok!(Staking::force_apply_min_commission(RuntimeOrigin::signed(1), 11));
		// Then the commission is bumped to the min
		assert_eq!(validators(), vec![(31, prefs(10)), (21, prefs(5)), (11, prefs(5))]);

		// When applying commission to a validator that doesn't exist then storage is not altered
		assert_noop!(
			Staking::force_apply_min_commission(RuntimeOrigin::signed(1), 420),
			Error::<Test>::NotStash
		);
	});
}

#[test]
fn proportional_slash_stop_slashing_if_remaining_zero() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		let c = |era, value| UnlockChunk::<Balance> { era, value };

		// we have some chunks, but they are not affected.
		let unlocking = bounded_vec![c(1, 10), c(2, 10)];

		// Given
		let mut ledger = StakingLedger::<Test>::new(123, 20);
		ledger.total = 40;
		ledger.unlocking = unlocking;

		assert_eq!(BondingDuration::get(), 3);

		// should not slash more than the amount requested, by accidentally slashing the first
		// chunk.
		assert_eq!(ledger.slash(18, 1, 0), 18);
	});
}

#[test]
fn proportional_ledger_slash_works() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		let c = |era, value| UnlockChunk::<Balance> { era, value };
		// Given
		let mut ledger = StakingLedger::<Test>::new(123, 10);
		assert_eq!(BondingDuration::get(), 3);

		// When we slash a ledger with no unlocking chunks
		assert_eq!(ledger.slash(5, 1, 0), 5);
		// Then
		assert_eq!(ledger.total, 5);
		assert_eq!(ledger.active, 5);
		assert_eq!(LedgerSlashPerEra::get().0, 5);
		assert_eq!(LedgerSlashPerEra::get().1, Default::default());

		// When we slash a ledger with no unlocking chunks and the slash amount is greater then the
		// total
		assert_eq!(ledger.slash(11, 1, 0), 5);
		// Then
		assert_eq!(ledger.total, 0);
		assert_eq!(ledger.active, 0);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, Default::default());

		// Given
		ledger.unlocking = bounded_vec![c(4, 10), c(5, 10)];
		ledger.total = 2 * 10;
		ledger.active = 0;
		// When all the chunks overlap with the slash eras
		assert_eq!(ledger.slash(20, 0, 0), 20);
		// Then
		assert_eq!(ledger.unlocking, vec![]);
		assert_eq!(ledger.total, 0);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(4, 0), (5, 0)]));

		// Given
		ledger.unlocking = bounded_vec![c(4, 100), c(5, 100), c(6, 100), c(7, 100)];
		ledger.total = 4 * 100;
		ledger.active = 0;
		// When the first 2 chunks don't overlap with the affected range of unlock eras.
		assert_eq!(ledger.slash(140, 0, 3), 140);
		// Then
		assert_eq!(ledger.unlocking, vec![c(4, 100), c(5, 100), c(6, 30), c(7, 30)]);
		assert_eq!(ledger.total, 4 * 100 - 140);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(6, 30), (7, 30)]));

		// Given
		ledger.unlocking = bounded_vec![c(4, 100), c(5, 100), c(6, 100), c(7, 100)];
		ledger.total = 4 * 100;
		ledger.active = 0;
		// When the first 2 chunks don't overlap with the affected range of unlock eras.
		assert_eq!(ledger.slash(15, 0, 3), 15);
		// Then
		assert_eq!(ledger.unlocking, vec![c(4, 100), c(5, 100), c(6, 100 - 8), c(7, 100 - 7)]);
		assert_eq!(ledger.total, 4 * 100 - 15);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(6, 92), (7, 93)]));

		// Given
		ledger.unlocking = bounded_vec![c(4, 40), c(5, 100), c(6, 10), c(7, 250)];
		ledger.active = 500;
		// 900
		ledger.total = 40 + 10 + 100 + 250 + 500;
		// When we have a partial slash that touches all chunks
		assert_eq!(ledger.slash(900 / 2, 0, 0), 450);
		// Then
		assert_eq!(ledger.active, 500 / 2);
		assert_eq!(
			ledger.unlocking,
			vec![c(4, 40 / 2), c(5, 100 / 2), c(6, 10 / 2), c(7, 250 / 2)]
		);
		assert_eq!(ledger.total, 900 / 2);
		assert_eq!(LedgerSlashPerEra::get().0, 500 / 2);
		assert_eq!(
			LedgerSlashPerEra::get().1,
			BTreeMap::from([(4, 40 / 2), (5, 100 / 2), (6, 10 / 2), (7, 250 / 2)])
		);

		// slash 1/4th with not chunk.
		ledger.unlocking = bounded_vec![];
		ledger.active = 500;
		ledger.total = 500;
		// When we have a partial slash that touches all chunks
		assert_eq!(ledger.slash(500 / 4, 0, 0), 500 / 4);
		// Then
		assert_eq!(ledger.active, 3 * 500 / 4);
		assert_eq!(ledger.unlocking, vec![]);
		assert_eq!(ledger.total, ledger.active);
		assert_eq!(LedgerSlashPerEra::get().0, 3 * 500 / 4);
		assert_eq!(LedgerSlashPerEra::get().1, Default::default());

		// Given we have the same as above,
		ledger.unlocking = bounded_vec![c(4, 40), c(5, 100), c(6, 10), c(7, 250)];
		ledger.active = 500;
		ledger.total = 40 + 10 + 100 + 250 + 500; // 900
		assert_eq!(ledger.total, 900);
		// When we have a higher min balance
		assert_eq!(
			ledger.slash(
				900 / 2,
				25, /* min balance - chunks with era 0 & 2 will be slashed to <=25, causing it
					 * to get swept */
				0
			),
			450
		);
		assert_eq!(ledger.active, 500 / 2);
		// the last chunk was not slashed 50% like all the rest, because some other earlier chunks
		// got dusted.
		assert_eq!(ledger.unlocking, vec![c(5, 100 / 2), c(7, 150)]);
		assert_eq!(ledger.total, 900 / 2);
		assert_eq!(LedgerSlashPerEra::get().0, 500 / 2);
		assert_eq!(
			LedgerSlashPerEra::get().1,
			BTreeMap::from([(4, 0), (5, 100 / 2), (6, 0), (7, 150)])
		);

		// Given
		// slash order --------------------NA--------2----------0----------1----
		ledger.unlocking = bounded_vec![c(4, 40), c(5, 100), c(6, 10), c(7, 250)];
		ledger.active = 500;
		ledger.total = 40 + 10 + 100 + 250 + 500; // 900
		assert_eq!(
			ledger.slash(
				500 + 10 + 250 + 100 / 2, // active + era 6 + era 7 + era 5 / 2
				0,
				3 /* slash era 6 first, so the affected parts are era 6, era 7 and
				   * ledge.active. This will cause the affected to go to zero, and then we will
				   * start slashing older chunks */
			),
			500 + 250 + 10 + 100 / 2
		);
		// Then
		assert_eq!(ledger.active, 0);
		assert_eq!(ledger.unlocking, vec![c(4, 40), c(5, 100 / 2)]);
		assert_eq!(ledger.total, 90);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(5, 100 / 2), (6, 0), (7, 0)]));

		// Given
		// iteration order------------------NA---------2----------0----------1----
		ledger.unlocking = bounded_vec![c(4, 100), c(5, 100), c(6, 100), c(7, 100)];
		ledger.active = 100;
		ledger.total = 5 * 100;
		// When
		assert_eq!(
			ledger.slash(
				351, // active + era 6 + era 7 + era 5 / 2 + 1
				50,  // min balance - everything slashed below 50 will get dusted
				3    /* slash era 3+3 first, so the affected parts are era 6, era 7 and
					  * ledge.active. This will cause the affected to go to zero, and then we
					  * will start slashing older chunks */
			),
			400
		);
		// Then
		assert_eq!(ledger.active, 0);
		assert_eq!(ledger.unlocking, vec![c(4, 100)]);
		assert_eq!(ledger.total, 100);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(5, 0), (6, 0), (7, 0)]));

		// Tests for saturating arithmetic

		// Given
		let slash = u64::MAX as Balance * 2;
		// The value of the other parts of ledger that will get slashed
		let value = slash - (10 * 4);

		ledger.active = 10;
		ledger.unlocking = bounded_vec![c(4, 10), c(5, 10), c(6, 10), c(7, value)];
		ledger.total = value + 40;
		// When
		let slash_amount = ledger.slash(slash, 0, 0);
		assert_eq_error_rate!(slash_amount, slash, 5);
		// Then
		assert_eq!(ledger.active, 0); // slash of 9
		assert_eq!(ledger.unlocking, vec![]);
		assert_eq!(ledger.total, 0);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(LedgerSlashPerEra::get().1, BTreeMap::from([(4, 0), (5, 0), (6, 0), (7, 0)]));

		// Given
		use sp_runtime::PerThing as _;
		let slash = u64::MAX as Balance * 2;
		let value = u64::MAX as Balance * 2;
		let unit = 100;
		// slash * value that will saturate
		assert!(slash.checked_mul(value).is_none());
		// but slash * unit won't.
		assert!(slash.checked_mul(unit).is_some());
		ledger.unlocking = bounded_vec![c(4, unit), c(5, value), c(6, unit), c(7, unit)];
		//--------------------------------------note value^^^
		ledger.active = unit;
		ledger.total = unit * 4 + value;
		// When
		assert_eq!(ledger.slash(slash, 0, 0), slash);
		// Then
		// The amount slashed out of `unit`
		let affected_balance = value + unit * 4;
		let ratio = Perquintill::from_rational_with_rounding(slash, affected_balance, Rounding::Up)
			.unwrap();
		// `unit` after the slash is applied
		let unit_slashed = {
			let unit_slash = ratio.mul_ceil(unit);
			unit - unit_slash
		};
		let value_slashed = {
			let value_slash = ratio.mul_ceil(value);
			value - value_slash
		};
		assert_eq!(ledger.active, unit_slashed);
		assert_eq!(ledger.unlocking, vec![c(5, value_slashed), c(7, 32)]);
		assert_eq!(ledger.total, value_slashed + 32);
		assert_eq!(LedgerSlashPerEra::get().0, 0);
		assert_eq!(
			LedgerSlashPerEra::get().1,
			BTreeMap::from([(4, 0), (5, value_slashed), (6, 0), (7, 32)])
		);
	});
}

#[test]
fn reducing_max_unlocking_chunks_abrupt() {
	// Concern is on validators only
	// By Default 11, 10 are stash and ctlr and 21,20
	ExtBuilder::default().build_and_execute(|| {
		// given a staker at era=10 and MaxUnlockChunks set to 2
		MaxUnlockingChunks::set(2);
		start_active_era(10);
		assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 300, RewardDestination::Staked));
		assert!(matches!(Staking::ledger(3.into()), Ok(_)));

		// when staker unbonds
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(3), 20));

		// then an unlocking chunk is added at `current_era + bonding_duration`
		// => 10 + 3 = 13
		let expected_unlocking: BoundedVec<UnlockChunk<Balance>, MaxUnlockingChunks> =
			bounded_vec![UnlockChunk { value: 20 as Balance, era: 13 as EraIndex }];
		assert!(matches!(Staking::ledger(3.into()),
			Ok(StakingLedger {
				unlocking,
				..
			}) if unlocking==expected_unlocking));

		// when staker unbonds at next era
		start_active_era(11);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(3), 50));
		// then another unlock chunk is added
		let expected_unlocking: BoundedVec<UnlockChunk<Balance>, MaxUnlockingChunks> =
			bounded_vec![UnlockChunk { value: 20, era: 13 }, UnlockChunk { value: 50, era: 14 }];
		assert!(matches!(Staking::ledger(3.into()),
			Ok(StakingLedger {
				unlocking,
				..
			}) if unlocking==expected_unlocking));

		// when staker unbonds further
		start_active_era(12);
		// then further unbonding not possible
		assert_noop!(Staking::unbond(RuntimeOrigin::signed(3), 20), Error::<Test>::NoMoreChunks);

		// when max unlocking chunks is reduced abruptly to a low value
		MaxUnlockingChunks::set(1);
		// then unbond, rebond ops are blocked with ledger in corrupt state
		assert_noop!(Staking::unbond(RuntimeOrigin::signed(3), 20), Error::<Test>::NotController);
		assert_noop!(Staking::rebond(RuntimeOrigin::signed(3), 100), Error::<Test>::NotController);

		// reset the ledger corruption
		MaxUnlockingChunks::set(2);
	})
}

#[test]
fn cannot_set_unsupported_validator_count() {
	ExtBuilder::default().build_and_execute(|| {
		MaxValidatorSet::set(50);
		MaxWinnersPerPage::set(50);
		// set validator count works
		assert_ok!(Staking::set_validator_count(RuntimeOrigin::root(), 30));
		assert_ok!(Staking::set_validator_count(RuntimeOrigin::root(), 50));
		// setting validator count above 100 does not work
		assert_noop!(
			Staking::set_validator_count(RuntimeOrigin::root(), 51),
			Error::<Test>::TooManyValidators,
		);
	})
}

#[test]
fn increase_validator_count_errors() {
	ExtBuilder::default().build_and_execute(|| {
		MaxValidatorSet::set(50);
		MaxWinnersPerPage::set(50);
		assert_ok!(Staking::set_validator_count(RuntimeOrigin::root(), 40));

		// increase works
		assert_ok!(Staking::increase_validator_count(RuntimeOrigin::root(), 6));
		assert_eq!(ValidatorCount::<Test>::get(), 46);

		// errors
		assert_noop!(
			Staking::increase_validator_count(RuntimeOrigin::root(), 5),
			Error::<Test>::TooManyValidators,
		);
	})
}

#[test]
fn scale_validator_count_errors() {
	ExtBuilder::default().build_and_execute(|| {
		MaxValidatorSet::set(50);
		MaxWinnersPerPage::set(50);
		assert_ok!(Staking::set_validator_count(RuntimeOrigin::root(), 20));

		// scale value works
		assert_ok!(Staking::scale_validator_count(
			RuntimeOrigin::root(),
			Percent::from_percent(200)
		));
		assert_eq!(ValidatorCount::<Test>::get(), 40);

		// errors
		assert_noop!(
			Staking::scale_validator_count(RuntimeOrigin::root(), Percent::from_percent(126)),
			Error::<Test>::TooManyValidators,
		);
	})
}

#[test]
fn set_min_commission_works_with_admin_origin() {
	ExtBuilder::default().build_and_execute(|| {
		// no minimum commission set initially
		assert_eq!(MinCommission::<Test>::get(), Zero::zero());

		// root can set min commission
		assert_ok!(Staking::set_min_commission(RuntimeOrigin::root(), Perbill::from_percent(10)));

		assert_eq!(MinCommission::<Test>::get(), Perbill::from_percent(10));

		// Non privileged origin can not set min_commission
		assert_noop!(
			Staking::set_min_commission(RuntimeOrigin::signed(2), Perbill::from_percent(15)),
			BadOrigin
		);

		// Admin Origin can set min commission
		assert_ok!(Staking::set_min_commission(
			RuntimeOrigin::signed(1),
			Perbill::from_percent(15),
		));

		// setting commission below min_commission fails
		assert_noop!(
			Staking::validate(
				RuntimeOrigin::signed(11),
				ValidatorPrefs { commission: Perbill::from_percent(14), blocked: false }
			),
			Error::<Test>::CommissionTooLow
		);

		// setting commission >= min_commission works
		assert_ok!(Staking::validate(
			RuntimeOrigin::signed(11),
			ValidatorPrefs { commission: Perbill::from_percent(15), blocked: false }
		));
	})
}

#[test]
fn can_page_exposure() {
	let mut others: Vec<IndividualExposure<AccountId, Balance>> = vec![];
	let mut total_stake: Balance = 0;
	// 19 nominators
	for i in 1..20 {
		let individual_stake: Balance = 100 * i as Balance;
		others.push(IndividualExposure { who: i, value: individual_stake });
		total_stake += individual_stake;
	}
	let own_stake: Balance = 500;
	total_stake += own_stake;
	assert_eq!(total_stake, 19_500);
	// build full exposure set
	let exposure: Exposure<AccountId, Balance> =
		Exposure { total: total_stake, own: own_stake, others };

	// when
	let (exposure_metadata, exposure_page): (
		PagedExposureMetadata<Balance>,
		Vec<ExposurePage<AccountId, Balance>>,
	) = exposure.clone().into_pages(3);

	// then
	// 7 pages of nominators.
	assert_eq!(exposure_page.len(), 7);
	assert_eq!(exposure_metadata.page_count, 7);
	// first page stake = 100 + 200 + 300
	assert!(matches!(exposure_page[0], ExposurePage { page_total: 600, .. }));
	// second page stake = 0 + 400 + 500 + 600
	assert!(matches!(exposure_page[1], ExposurePage { page_total: 1500, .. }));
	// verify overview has the total
	assert_eq!(exposure_metadata.total, 19_500);
	// verify total stake is same as in the original exposure.
	assert_eq!(
		exposure_page.iter().map(|a| a.page_total).reduce(|a, b| a + b).unwrap(),
		19_500 - exposure_metadata.own
	);
	// verify own stake is correct
	assert_eq!(exposure_metadata.own, 500);
	// verify number of nominators are same as in the original exposure.
	assert_eq!(exposure_page.iter().map(|a| a.others.len()).reduce(|a, b| a + b).unwrap(), 19);
	assert_eq!(exposure_metadata.nominator_count, 19);
}

#[test]
fn should_retain_era_info_only_upto_history_depth() {
	ExtBuilder::default().build_and_execute(|| {
		// remove existing exposure
		Pallet::<Test>::clear_era_information(0);
		let validator_stash = 10;

		for era in 0..4 {
			ErasClaimedRewards::<Test>::insert(era, &validator_stash, vec![0, 1, 2]);
			for page in 0..3 {
				ErasStakersPaged::<Test>::insert(
					(era, &validator_stash, page),
					ExposurePage { page_total: 100, others: vec![] },
				);
			}
		}

		for i in 0..4 {
			// Count of entries remaining in ErasClaimedRewards = total - cleared_count
			assert_eq!(ErasClaimedRewards::<Test>::iter().count(), (4 - i));
			// 1 claimed_rewards entry for each era
			assert_eq!(ErasClaimedRewards::<Test>::iter_prefix(i as EraIndex).count(), 1);
			// 3 entries (pages) for each era
			assert_eq!(ErasStakersPaged::<Test>::iter_prefix((i as EraIndex,)).count(), 3);

			// when clear era info
			Pallet::<Test>::clear_era_information(i as EraIndex);

			// then all era entries are cleared
			assert_eq!(ErasClaimedRewards::<Test>::iter_prefix(i as EraIndex).count(), 0);
			assert_eq!(ErasStakersPaged::<Test>::iter_prefix((i as EraIndex,)).count(), 0);
		}
	});
}

#[test]
fn test_runtime_api_pending_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN
		let err_weight = <Test as Config>::WeightInfo::payout_stakers_alive_staked(0);
		let stake = 100;

		// validator with non-paged exposure, rewards marked in legacy claimed rewards.
		let validator_one = 301;
		// validator with non-paged exposure, rewards marked in paged claimed rewards.
		let validator_two = 302;
		// validator with paged exposure.
		let validator_three = 303;

		// Set staker
		for v in validator_one..=validator_three {
			let _ = asset::set_stakeable_balance::<Test>(&v, stake);
			assert_ok!(Staking::bond(RuntimeOrigin::signed(v), stake, RewardDestination::Staked));
		}

		// Add reward points
		let reward = EraRewardPoints::<AccountId> {
			total: 1,
			individual: vec![(validator_one, 1), (validator_two, 1), (validator_three, 1)]
				.into_iter()
				.collect(),
		};
		ErasRewardPoints::<Test>::insert(0, reward);

		// build exposure
		let mut individual_exposures: Vec<IndividualExposure<AccountId, Balance>> = vec![];
		for i in 0..=MaxExposurePageSize::get() {
			individual_exposures.push(IndividualExposure { who: i.into(), value: stake });
		}
		let exposure = Exposure::<AccountId, Balance> {
			total: stake * (MaxExposurePageSize::get() as Balance + 2),
			own: stake,
			others: individual_exposures,
		};

		// add exposure for validators
		Eras::<Test>::upsert_exposure(0, &validator_one, exposure.clone());
		Eras::<Test>::upsert_exposure(0, &validator_two, exposure.clone());

		// add some reward to be distributed
		ErasValidatorReward::<Test>::insert(0, 1000);

		// SCENARIO: Validator with paged exposure (two pages).
		// validators have not claimed rewards, so pending rewards is true.
		assert!(Eras::<Test>::pending_rewards(0, &validator_one));
		assert!(Eras::<Test>::pending_rewards(0, &validator_two));
		// and payout works
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), validator_one, 0));
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), validator_two, 0));
		// validators have two pages of exposure, so pending rewards is still true.
		assert!(Eras::<Test>::pending_rewards(0, &validator_one));
		assert!(Eras::<Test>::pending_rewards(0, &validator_two));
		// payout again only for validator one
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), validator_one, 0));
		// now pending rewards is false for validator one
		assert!(!Eras::<Test>::pending_rewards(0, &validator_one));
		// and payout fails for validator one
		assert_noop!(
			Staking::payout_stakers(RuntimeOrigin::signed(1337), validator_one, 0),
			Error::<Test>::AlreadyClaimed.with_weight(err_weight)
		);
		// while pending reward is true for validator two
		assert!(Eras::<Test>::pending_rewards(0, &validator_two));
		// and payout works again for validator two.
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), validator_two, 0));
	});
}

mod staking_interface {
	use frame_support::storage::with_storage_layer;
	use sp_staking::StakingInterface;

	use super::*;

	#[test]
	fn force_unstake_with_slash_works() {
		ExtBuilder::default().build_and_execute(|| {
			// without slash
			let _ = with_storage_layer::<(), _, _>(|| {
				// bond an account, can unstake
				assert_eq!(Staking::bonded(&11), Some(11));
				assert_ok!(<Staking as StakingInterface>::force_unstake(11));
				Err(DispatchError::from("revert"))
			});

			// bond again and add a slash, still can unstake.
			assert_eq!(Staking::bonded(&11), Some(11));
			add_slash(&11);
			assert_ok!(<Staking as StakingInterface>::force_unstake(11));
		});
	}

	#[test]
	fn do_withdraw_unbonded_with_wrong_slash_spans_works_as_expected() {
		ExtBuilder::default().build_and_execute(|| {
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(100)], true);

			assert_eq!(Staking::bonded(&11), Some(11));

			assert_noop!(
				Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0),
				Error::<Test>::IncorrectSlashingSpans
			);

			let num_slashing_spans =
				SlashingSpans::<Test>::get(&11).map_or(0, |s| s.iter().count());
			assert_ok!(Staking::withdraw_unbonded(
				RuntimeOrigin::signed(11),
				num_slashing_spans as u32
			));
		});
	}

	#[test]
	fn do_withdraw_unbonded_can_kill_stash_with_existential_deposit_zero() {
		ExtBuilder::default()
			.existential_deposit(0)
			.nominate(false)
			.build_and_execute(|| {
				// Initial state of 11
				assert_eq!(Staking::bonded(&11), Some(11));
				assert_eq!(
					Staking::ledger(11.into()).unwrap(),
					StakingLedgerInspect {
						stash: 11,
						total: 1000,
						active: 1000,
						unlocking: Default::default(),
						legacy_claimed_rewards: bounded_vec![],
					}
				);
				assert_eq!(
					Staking::eras_stakers(active_era(), &11),
					Exposure { total: 1000, own: 1000, others: vec![] }
				);

				// Unbond all of the funds in stash.
				Staking::chill(RuntimeOrigin::signed(11)).unwrap();
				Staking::unbond(RuntimeOrigin::signed(11), 1000).unwrap();
				assert_eq!(
					Staking::ledger(11.into()).unwrap(),
					StakingLedgerInspect {
						stash: 11,
						total: 1000,
						active: 0,
						unlocking: bounded_vec![UnlockChunk { value: 1000, era: 3 }],
						legacy_claimed_rewards: bounded_vec![],
					},
				);

				// trigger future era.
				mock::start_active_era(3);

				// withdraw unbonded
				assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));

				// empty stash has been reaped
				assert!(!<Ledger<Test>>::contains_key(&11));
				assert!(!<Bonded<Test>>::contains_key(&11));
				assert!(!<Validators<Test>>::contains_key(&11));
				assert!(!<Payee<Test>>::contains_key(&11));
				// lock is removed.
				assert_eq!(asset::staked::<Test>(&11), 0);
			});
	}

	#[test]
	fn status() {
		ExtBuilder::default().build_and_execute(|| {
			// stash of a validator is identified as a validator
			assert_eq!(Staking::status(&11).unwrap(), StakerStatus::Validator);
			// .. but not the controller.
			assert!(Staking::status(&10).is_err());

			// stash of nominator is identified as a nominator
			assert_eq!(Staking::status(&101).unwrap(), StakerStatus::Nominator(vec![11, 21]));
			// .. but not the controller.
			assert!(Staking::status(&100).is_err());

			// stash of chilled is identified as a chilled
			assert_eq!(Staking::status(&41).unwrap(), StakerStatus::Idle);
			// .. but not the controller.
			assert!(Staking::status(&40).is_err());

			// random other account.
			assert!(Staking::status(&42).is_err());
		})
	}
}

mod staking_unchecked {
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
				on_offence_now(&[offence_from(11, None)], &[slash_percent], true);

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
				on_offence_now(&[offence_from(11, None)], &[slash_percent], true);

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
}

mod ledger {
	use super::*;

	#[test]
	fn paired_account_works() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(10),
				100,
				RewardDestination::Account(10)
			));

			assert_eq!(<Bonded<Test>>::get(&10), Some(10));
			assert_eq!(
				StakingLedger::<Test>::paired_account(StakingAccount::Controller(10)),
				Some(10)
			);
			assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(10)), Some(10));

			assert_eq!(<Bonded<Test>>::get(&42), None);
			assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Controller(42)), None);
			assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(42)), None);

			// bond manually stash with different controller. This is deprecated but the migration
			// has not been complete yet (controller: 100, stash: 200)
			assert_ok!(bond_controller_stash(100, 200));
			assert_eq!(<Bonded<Test>>::get(&200), Some(100));
			assert_eq!(
				StakingLedger::<Test>::paired_account(StakingAccount::Controller(100)),
				Some(200)
			);
			assert_eq!(
				StakingLedger::<Test>::paired_account(StakingAccount::Stash(200)),
				Some(100)
			);
		})
	}

	#[test]
	fn get_ledger_works() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			// stash does not exist
			assert!(StakingLedger::<Test>::get(StakingAccount::Stash(42)).is_err());

			// bonded and paired
			assert_eq!(<Bonded<Test>>::get(&11), Some(11));

			match StakingLedger::<Test>::get(StakingAccount::Stash(11)) {
				Ok(ledger) => {
					assert_eq!(ledger.controller(), Some(11));
					assert_eq!(ledger.stash, 11);
				},
				Err(_) => panic!("staking ledger must exist"),
			};

			// bond manually stash with different controller. This is deprecated but the migration
			// has not been complete yet (controller: 100, stash: 200)
			assert_ok!(bond_controller_stash(100, 200));
			assert_eq!(<Bonded<Test>>::get(&200), Some(100));

			match StakingLedger::<Test>::get(StakingAccount::Stash(200)) {
				Ok(ledger) => {
					assert_eq!(ledger.controller(), Some(100));
					assert_eq!(ledger.stash, 200);
				},
				Err(_) => panic!("staking ledger must exist"),
			};

			match StakingLedger::<Test>::get(StakingAccount::Controller(100)) {
				Ok(ledger) => {
					assert_eq!(ledger.controller(), Some(100));
					assert_eq!(ledger.stash, 200);
				},
				Err(_) => panic!("staking ledger must exist"),
			};
		})
	}

	#[test]
	fn get_ledger_bad_state_fails() {
		ExtBuilder::default().has_stakers(false).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// Case 1: double bonded but not corrupted:
			// stash 444 has controller 555:
			assert_eq!(Bonded::<Test>::get(444), Some(555));
			assert_eq!(Ledger::<Test>::get(555).unwrap().stash, 444);

			// stash 444 is also a controller of 333:
			assert_eq!(Bonded::<Test>::get(333), Some(444));
			assert_eq!(
				StakingLedger::<Test>::paired_account(StakingAccount::Stash(333)),
				Some(444)
			);
			assert_eq!(Ledger::<Test>::get(444).unwrap().stash, 333);

			// although 444 is double bonded (it is a controller and a stash of different ledgers),
			// we can safely retrieve the ledger and mutate it since the correct ledger is
			// returned.
			let ledger_result = StakingLedger::<Test>::get(StakingAccount::Stash(444));
			assert_eq!(ledger_result.unwrap().stash, 444); // correct ledger.

			let ledger_result = StakingLedger::<Test>::get(StakingAccount::Controller(444));
			assert_eq!(ledger_result.unwrap().stash, 333); // correct ledger.

			// fetching ledger 333 by its stash works.
			let ledger_result = StakingLedger::<Test>::get(StakingAccount::Stash(333));
			assert_eq!(ledger_result.unwrap().stash, 333);

			// Case 2: corrupted ledger bonding.
			// in this case, we simulate what happens when fetching a ledger by stash returns a
			// ledger with a different stash. when this happens, we return an error instead of the
			// ledger to prevent ledger mutations.
			let mut ledger = Ledger::<Test>::get(444).unwrap();
			assert_eq!(ledger.stash, 333);
			ledger.stash = 444;
			Ledger::<Test>::insert(444, ledger);

			// now, we are prevented from fetching the ledger by stash from 1. It's associated
			// controller (2) is now bonding a ledger with a different stash (2, not 1).
			assert!(StakingLedger::<Test>::get(StakingAccount::Stash(333)).is_err());
		})
	}

	#[test]
	fn bond_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
			assert!(<Bonded<Test>>::get(&42).is_none());

			let mut ledger: StakingLedger<Test> = StakingLedger::default_from(42);
			let reward_dest = RewardDestination::Account(10);

			assert_ok!(ledger.clone().bond(reward_dest));
			assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
			assert!(<Bonded<Test>>::get(&42).is_some());
			assert_eq!(<Payee<Test>>::get(&42), Some(reward_dest));

			// cannot bond again.
			assert!(ledger.clone().bond(reward_dest).is_err());

			// once bonded, update works as expected.
			ledger.legacy_claimed_rewards = bounded_vec![1];
			assert_ok!(ledger.update());
		})
	}

	#[test]
	fn bond_controller_cannot_be_stash_works() {
		ExtBuilder::default().build_and_execute(|| {
			let (stash, controller) = testing_utils::create_unique_stash_controller::<Test>(
				0,
				10,
				RewardDestination::Staked,
				false,
			)
			.unwrap();

			assert_eq!(Bonded::<Test>::get(stash), Some(controller));
			assert_eq!(Ledger::<Test>::get(controller).map(|l| l.stash), Some(stash));

			// existing controller should not be able become a stash.
			assert_noop!(
				Staking::bond(RuntimeOrigin::signed(controller), 10, RewardDestination::Staked),
				Error::<Test>::AlreadyPaired,
			);
		})
	}

	#[test]
	fn is_bonded_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
			assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Controller(42)));

			// adds entry to Bonded without Ledger pair (should not happen).
			<Bonded<Test>>::insert(42, 42);
			assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Controller(42)));

			assert_eq!(<Bonded<Test>>::get(&11), Some(11));
			assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Stash(11)));
			assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Controller(11)));

			<Bonded<Test>>::remove(42); // ensures try-state checks pass.
		})
	}

	#[test]
	#[allow(deprecated)]
	fn set_payee_errors_on_controller_destination() {
		ExtBuilder::default().build_and_execute(|| {
			Payee::<Test>::insert(11, RewardDestination::Staked);
			assert_noop!(
				Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Controller),
				Error::<Test>::ControllerDeprecated
			);
			assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Staked));
		})
	}

	#[test]
	#[allow(deprecated)]
	fn update_payee_migration_works() {
		ExtBuilder::default().build_and_execute(|| {
			// migrate a `Controller` variant to `Account` variant.
			Payee::<Test>::insert(11, RewardDestination::Controller);
			assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Controller));
			assert_ok!(Staking::update_payee(RuntimeOrigin::signed(11), 11));
			assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Account(11)));

			// Do not migrate a variant if not `Controller`.
			Payee::<Test>::insert(21, RewardDestination::Stash);
			assert_eq!(Payee::<Test>::get(&21), Some(RewardDestination::Stash));
			assert_noop!(
				Staking::update_payee(RuntimeOrigin::signed(11), 21),
				Error::<Test>::NotController
			);
			assert_eq!(Payee::<Test>::get(&21), Some(RewardDestination::Stash));
		})
	}

	#[test]
	fn deprecate_controller_batch_works_full_weight() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			// Given:

			let start = 1001;
			let mut controllers: Vec<_> = vec![];
			for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
				let ctlr: u64 = n.into();
				let stash: u64 = (n + 10000).into();

				Ledger::<Test>::insert(
					ctlr,
					StakingLedger {
						controller: None,
						total: (10 + ctlr).into(),
						active: (10 + ctlr).into(),
						..StakingLedger::default_from(stash)
					},
				);
				Bonded::<Test>::insert(stash, ctlr);
				Payee::<Test>::insert(stash, RewardDestination::Staked);

				controllers.push(ctlr);
			}

			// When:

			let bounded_controllers: BoundedVec<
				_,
				<Test as Config>::MaxControllersInDeprecationBatch,
			> = BoundedVec::try_from(controllers).unwrap();

			// Only `AdminOrigin` can sign.
			assert_noop!(
				Staking::deprecate_controller_batch(
					RuntimeOrigin::signed(2),
					bounded_controllers.clone()
				),
				BadOrigin
			);

			let result =
				Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
			assert_ok!(result);
			assert_eq!(
				result.unwrap().actual_weight.unwrap(),
				<Test as Config>::WeightInfo::deprecate_controller_batch(
					<Test as Config>::MaxControllersInDeprecationBatch::get()
				)
			);

			// Then:

			for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
				let ctlr: u64 = n.into();
				let stash: u64 = (n + 10000).into();

				// Ledger no longer keyed by controller.
				assert_eq!(Ledger::<Test>::get(ctlr), None);
				// Bonded now maps to the stash.
				assert_eq!(Bonded::<Test>::get(stash), Some(stash));

				// Ledger is now keyed by stash.
				let ledger_updated = Ledger::<Test>::get(stash).unwrap();
				assert_eq!(ledger_updated.stash, stash);

				// Check `active` and `total` values match the original ledger set by controller.
				assert_eq!(ledger_updated.active, (10 + ctlr).into());
				assert_eq!(ledger_updated.total, (10 + ctlr).into());
			}
		})
	}

	#[test]
	fn deprecate_controller_batch_works_half_weight() {
		ExtBuilder::default().build_and_execute(|| {
			// Given:

			let start = 1001;
			let mut controllers: Vec<_> = vec![];
			for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
				let ctlr: u64 = n.into();

				// Only half of entries are unique pairs.
				let stash: u64 = if n % 2 == 0 { (n + 10000).into() } else { ctlr };

				Ledger::<Test>::insert(
					ctlr,
					StakingLedger { controller: None, ..StakingLedger::default_from(stash) },
				);
				Bonded::<Test>::insert(stash, ctlr);
				Payee::<Test>::insert(stash, RewardDestination::Staked);

				controllers.push(ctlr);
			}

			// When:
			let bounded_controllers: BoundedVec<
				_,
				<Test as Config>::MaxControllersInDeprecationBatch,
			> = BoundedVec::try_from(controllers.clone()).unwrap();

			let result =
				Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
			assert_ok!(result);
			assert_eq!(
				result.unwrap().actual_weight.unwrap(),
				<Test as Config>::WeightInfo::deprecate_controller_batch(controllers.len() as u32)
			);

			// Then:

			for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
				let unique_pair = n % 2 == 0;
				let ctlr: u64 = n.into();
				let stash: u64 = if unique_pair { (n + 10000).into() } else { ctlr };

				// Side effect of migration for unique pair.
				if unique_pair {
					assert_eq!(Ledger::<Test>::get(ctlr), None);
				}
				// Bonded maps to the stash.
				assert_eq!(Bonded::<Test>::get(stash), Some(stash));

				// Ledger is keyed by stash.
				let ledger_updated = Ledger::<Test>::get(stash).unwrap();
				assert_eq!(ledger_updated.stash, stash);
			}
		})
	}

	#[test]
	fn deprecate_controller_batch_skips_unmigrated_controller_payees() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			// Given:

			let stash: u64 = 1000;
			let ctlr: u64 = 1001;

			Ledger::<Test>::insert(
				ctlr,
				StakingLedger { controller: None, ..StakingLedger::default_from(stash) },
			);
			Bonded::<Test>::insert(stash, ctlr);
			#[allow(deprecated)]
			Payee::<Test>::insert(stash, RewardDestination::Controller);

			// When:

			let bounded_controllers: BoundedVec<
				_,
				<Test as Config>::MaxControllersInDeprecationBatch,
			> = BoundedVec::try_from(vec![ctlr]).unwrap();

			let result =
				Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
			assert_ok!(result);
			assert_eq!(
				result.unwrap().actual_weight.unwrap(),
				<Test as Config>::WeightInfo::deprecate_controller_batch(1 as u32)
			);

			// Then:

			// Esure deprecation did not happen.
			assert_eq!(Ledger::<Test>::get(ctlr).is_some(), true);

			// Bonded still keyed by controller.
			assert_eq!(Bonded::<Test>::get(stash), Some(ctlr));

			// Ledger is still keyed by controller.
			let ledger_updated = Ledger::<Test>::get(ctlr).unwrap();
			assert_eq!(ledger_updated.stash, stash);
		})
	}

	#[test]
	fn deprecate_controller_batch_with_bad_state_ok() {
		ExtBuilder::default().has_stakers(false).nominate(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// now let's deprecate all the controllers for all the existing ledgers.
			let bounded_controllers: BoundedVec<
				_,
				<Test as Config>::MaxControllersInDeprecationBatch,
			> = BoundedVec::try_from(vec![333, 444, 555, 777]).unwrap();

			assert_ok!(Staking::deprecate_controller_batch(
				RuntimeOrigin::root(),
				bounded_controllers
			));

			assert_eq!(
				*staking_events().last().unwrap(),
				Event::ControllerBatchDeprecated { failures: 0 }
			);
		})
	}

	#[test]
	fn deprecate_controller_batch_with_bad_state_failures() {
		ExtBuilder::default().has_stakers(false).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// now let's deprecate all the controllers for all the existing ledgers.
			let bounded_controllers: BoundedVec<
				_,
				<Test as Config>::MaxControllersInDeprecationBatch,
			> = BoundedVec::try_from(vec![777, 555, 444, 333]).unwrap();

			assert_ok!(Staking::deprecate_controller_batch(
				RuntimeOrigin::root(),
				bounded_controllers
			));

			assert_eq!(
				*staking_events().last().unwrap(),
				Event::ControllerBatchDeprecated { failures: 2 }
			);
		})
	}

	#[test]
	fn set_controller_with_bad_state_ok() {
		ExtBuilder::default().has_stakers(false).nominate(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// in this case, setting controller works due to the ordering of the calls.
			assert_ok!(Staking::set_controller(RuntimeOrigin::signed(333)));
			assert_ok!(Staking::set_controller(RuntimeOrigin::signed(444)));
			assert_ok!(Staking::set_controller(RuntimeOrigin::signed(555)));
		})
	}

	#[test]
	fn set_controller_with_bad_state_fails() {
		ExtBuilder::default().has_stakers(false).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// setting the controller of ledger associated with stash 555 fails since its stash is a
			// controller of another ledger.
			assert_noop!(
				Staking::set_controller(RuntimeOrigin::signed(555)),
				Error::<Test>::BadState
			);
			assert_noop!(
				Staking::set_controller(RuntimeOrigin::signed(444)),
				Error::<Test>::BadState
			);
			assert_ok!(Staking::set_controller(RuntimeOrigin::signed(333)));
		})
	}
}

mod ledger_recovery {
	use super::*;

	#[test]
	fn inspect_recovery_ledger_simple_works() {
		ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// non corrupted ledger.
			assert_eq!(Staking::inspect_bond_state(&11).unwrap(), LedgerIntegrityState::Ok);

			// non bonded stash.
			assert!(Bonded::<Test>::get(&1111).is_none());
			assert!(Staking::inspect_bond_state(&1111).is_err());

			// double bonded but not corrupted.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		})
	}

	#[test]
	fn inspect_recovery_ledger_corupted_killed_works() {
		ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			let lock_333_before = asset::staked::<Test>(&333);

			// get into corrupted and killed ledger state by killing a corrupted ledger:
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			// kill(333)
			// (444, 444) -> corrupted and None.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// 333 is corrupted since it's controller is linking 444 ledger.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
			// 444 however is OK.
			assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

			// kill the corrupted ledger that is associated with stash 333.
			assert_ok!(StakingLedger::<Test>::kill(&333));

			// 333 bond is no more but it returns `BadState` because the lock on this stash is
			// still set (see checks below).
			assert_eq!(Staking::inspect_bond_state(&333), Err(Error::<Test>::BadState));
			// now the *other* ledger associated with 444 has been corrupted and killed (None).
			assert_eq!(
				Staking::inspect_bond_state(&444),
				Ok(LedgerIntegrityState::CorruptedKilled)
			);

			// side effects on 333 - ledger, bonded, payee, lock should be completely empty.
			// however, 333 lock remains.
			assert_eq!(asset::staked::<Test>(&333), lock_333_before); // NOK
			assert!(Bonded::<Test>::get(&333).is_none()); // OK
			assert!(Payee::<Test>::get(&333).is_none()); // OK
			assert!(Ledger::<Test>::get(&444).is_none()); // OK

			// side effects on 444 - ledger, bonded, payee, lock should remain be intact.
			// however, 444 lock was removed.
			assert_eq!(asset::staked::<Test>(&444), 0); // NOK
			assert!(Bonded::<Test>::get(&444).is_some()); // OK
			assert!(Payee::<Test>::get(&444).is_some()); // OK
			assert!(Ledger::<Test>::get(&555).is_none()); // NOK

			assert!(Staking::do_try_state(System::block_number()).is_err());
		})
	}

	#[test]
	fn inspect_recovery_ledger_corupted_killed_other_works() {
		ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			let lock_333_before = asset::staked::<Test>(&333);

			// get into corrupted and killed ledger state by killing a corrupted ledger:
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			// kill(444)
			// (333, 444) -> corrupted and None
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// 333 is corrupted since it's controller is linking 444 ledger.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
			// 444 however is OK.
			assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

			// kill the *other* ledger that is double bonded but not corrupted.
			assert_ok!(StakingLedger::<Test>::kill(&444));

			// now 333 is corrupted and None through the *other* ledger being killed.
			assert_eq!(
				Staking::inspect_bond_state(&333).unwrap(),
				LedgerIntegrityState::CorruptedKilled,
			);
			// 444 is cleaned and not a stash anymore; no lock left behind.
			assert_eq!(Ledger::<Test>::get(&444), None);
			assert_eq!(Staking::inspect_bond_state(&444), Err(Error::<Test>::NotStash));

			// side effects on 333 - ledger, bonded, payee, lock should be intact.
			assert_eq!(asset::staked::<Test>(&333), lock_333_before); // OK
			assert_eq!(Bonded::<Test>::get(&333), Some(444)); // OK
			assert!(Payee::<Test>::get(&333).is_some());
			// however, ledger associated with its controller was killed.
			assert!(Ledger::<Test>::get(&444).is_none()); // NOK

			// side effects on 444 - ledger, bonded, payee, lock should be completely removed.
			assert_eq!(asset::staked::<Test>(&444), 0); // OK
			assert!(Bonded::<Test>::get(&444).is_none()); // OK
			assert!(Payee::<Test>::get(&444).is_none()); // OK
			assert!(Ledger::<Test>::get(&555).is_none()); // OK

			assert!(Staking::do_try_state(System::block_number()).is_err());
		})
	}

	#[test]
	fn inspect_recovery_ledger_lock_corrupted_works() {
		ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// get into lock corrupted ledger state by bond_extra on a ledger that is double bonded
			// with a corrupted ledger.
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			//  bond_extra(333, 10) -> lock corrupted on 444
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);
			bond_extra_no_checks(&333, 10);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// 333 is corrupted since it's controller is linking 444 ledger.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
			// 444 ledger is not corrupted but locks got out of sync.
			assert_eq!(
				Staking::inspect_bond_state(&444).unwrap(),
				LedgerIntegrityState::LockCorrupted
			);
		})
	}

	// Corrupted ledger restore.
	//
	// * Double bonded and corrupted ledger.
	#[test]
	fn restore_ledger_corrupted_works() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// get into corrupted and killed ledger state.
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// recover the ledger bonded by 333 stash.
			assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

			// try-state checks are ok now.
			assert_ok!(Staking::do_try_state(System::block_number()));
		})
	}

	// Corrupted and killed ledger restore.
	//
	// * Double bonded and corrupted ledger.
	// * Ledger killed by own controller.
	#[test]
	fn restore_ledger_corrupted_killed_works() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// ledger.total == lock
			let total_444_before_corruption = asset::staked::<Test>(&444);

			// get into corrupted and killed ledger state by killing a corrupted ledger:
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			// kill(333)
			// (444, 444) -> corrupted and None.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			// kill the corrupted ledger that is associated with stash 333.
			assert_ok!(StakingLedger::<Test>::kill(&333));

			// 333 bond is no more but it returns `BadState` because the lock on this stash is
			// still set (see checks below).
			assert_eq!(Staking::inspect_bond_state(&333), Err(Error::<Test>::BadState));
			// now the *other* ledger associated with 444 has been corrupted and killed (None).
			assert!(Staking::ledger(StakingAccount::Stash(444)).is_err());

			// try-state should fail.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// recover the ledger bonded by 333 stash.
			assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

			// for the try-state checks to pass, we also need to recover the stash 444 which is
			// corrupted too by proxy of kill(333). Currently, both the lock and the ledger of 444
			// have been cleared so we need to provide the new amount to restore the ledger.
			assert_noop!(
				Staking::restore_ledger(RuntimeOrigin::root(), 444, None, None, None),
				Error::<Test>::CannotRestoreLedger
			);

			assert_ok!(Staking::restore_ledger(
				RuntimeOrigin::root(),
				444,
				None,
				Some(total_444_before_corruption),
				None,
			));

			// try-state checks are ok now.
			assert_ok!(Staking::do_try_state(System::block_number()));
		})
	}

	// Corrupted and killed by *other* ledger restore.
	//
	// * Double bonded and corrupted ledger.
	// * Ledger killed by own controller.
	#[test]
	fn restore_ledger_corrupted_killed_other_works() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			setup_double_bonded_ledgers();

			// get into corrupted and killed ledger state by killing a corrupted ledger:
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			// kill(444)
			// (333, 444) -> corrupted and None
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// 333 is corrupted since it's controller is linking 444 ledger.
			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
			// 444 however is OK.
			assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

			// kill the *other* ledger that is double bonded but not corrupted.
			assert_ok!(StakingLedger::<Test>::kill(&444));

			// recover the ledger bonded by 333 stash.
			assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

			// 444 does not need recover in this case since it's been killed successfully.
			assert_eq!(Staking::inspect_bond_state(&444), Err(Error::<Test>::NotStash));

			// try-state checks are ok now.
			assert_ok!(Staking::do_try_state(System::block_number()));
		})
	}

	// Corrupted with bond_extra.
	//
	// * Double bonded and corrupted ledger.
	// * Corrupted ledger calls `bond_extra`
	#[test]
	fn restore_ledger_corrupted_bond_extra_works() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			setup_double_bonded_ledgers();

			let lock_333_before = asset::staked::<Test>(&333);
			let lock_444_before = asset::staked::<Test>(&444);

			// get into corrupted and killed ledger state by killing a corrupted ledger:
			// init state:
			//  (333, 444)
			//  (444, 555)
			// set_controller(444) to 444
			//  (333, 444) -> corrupted
			//  (444, 444)
			// bond_extra(444, 40) -> OK
			// bond_extra(333, 30) -> locks out of sync

			assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
			set_controller_no_checks(&444);

			// now try-state fails.
			assert!(Staking::do_try_state(System::block_number()).is_err());

			// if 444 bonds extra, the locks remain in sync.
			bond_extra_no_checks(&444, 40);
			assert_eq!(asset::staked::<Test>(&333), lock_333_before);
			assert_eq!(asset::staked::<Test>(&444), lock_444_before + 40);

			// however if 333 bonds extra, the wrong lock is updated.
			bond_extra_no_checks(&333, 30);
			assert_eq!(asset::staked::<Test>(&333), lock_444_before + 40 + 30); //not OK
			assert_eq!(asset::staked::<Test>(&444), lock_444_before + 40); // OK

			// recover the ledger bonded by 333 stash. Note that the total/lock needs to be
			// re-written since on-chain data lock has become out of sync.
			assert_ok!(Staking::restore_ledger(
				RuntimeOrigin::root(),
				333,
				None,
				Some(lock_333_before + 30),
				None
			));

			// now recover 444 that although it's not corrupted, its lock and ledger.total are out
			// of sync. in which case, we need to explicitly set the ledger's lock and amount,
			// otherwise the ledger recover will fail.
			assert_noop!(
				Staking::restore_ledger(RuntimeOrigin::root(), 444, None, None, None),
				Error::<Test>::CannotRestoreLedger
			);

			//and enforcing a new ledger lock/total on this non-corrupted ledger will work.
			assert_ok!(Staking::restore_ledger(
				RuntimeOrigin::root(),
				444,
				None,
				Some(lock_444_before + 40),
				None
			));

			// double-check that ledgers got to expected state and bond_extra done during the
			// corrupted state is part of the recovered ledgers.
			let ledger_333 = Bonded::<Test>::get(&333).and_then(Ledger::<Test>::get).unwrap();
			let ledger_444 = Bonded::<Test>::get(&444).and_then(Ledger::<Test>::get).unwrap();

			assert_eq!(ledger_333.total, lock_333_before + 30);
			assert_eq!(asset::staked::<Test>(&333), ledger_333.total);
			assert_eq!(ledger_444.total, lock_444_before + 40);
			assert_eq!(asset::staked::<Test>(&444), ledger_444.total);

			// try-state checks are ok now.
			assert_ok!(Staking::do_try_state(System::block_number()));
		})
	}
}

mod byzantine_threshold_disabling_strategy {
	use crate::tests::{DisablingStrategy, Test, UpToLimitDisablingStrategy};
	use sp_runtime::Perbill;
	use sp_staking::offence::OffenceSeverity;

	// Common test data - the stash of the offending validator, the era of the offence and the
	// active set
	const OFFENDER_ID: <Test as frame_system::Config>::AccountId = 7;
	const MAX_OFFENDER_SEVERITY: OffenceSeverity = OffenceSeverity(Perbill::from_percent(100));
	const MIN_OFFENDER_SEVERITY: OffenceSeverity = OffenceSeverity(Perbill::from_percent(0));
	const ACTIVE_SET: [<Test as pallet_session::Config>::ValidatorId; 7] = [1, 2, 3, 4, 5, 6, 7];
	const OFFENDER_VALIDATOR_IDX: u32 = 6; // the offender is with index 6 in the active set

	// todo(ank4n): Ensure there is a test that for older eras, the disabling strategy does not
	// disable the validator.

	#[test]
	fn dont_disable_beyond_byzantine_threshold() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(1, MIN_OFFENDER_SEVERITY), (2, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_none() && disabling_decision.reenable.is_none());
		});
	}

	#[test]
	fn disable_when_below_byzantine_threshold() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(1, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert_eq!(disabling_decision.disable, Some(OFFENDER_VALIDATOR_IDX));
		});
	}
}

mod disabling_strategy_with_reenabling {
	use crate::tests::{DisablingStrategy, Test, UpToLimitWithReEnablingDisablingStrategy};
	use sp_runtime::Perbill;
	use sp_staking::offence::OffenceSeverity;

	// Common test data - the stash of the offending validator, the era of the offence and the
	// active set
	const OFFENDER_ID: <Test as frame_system::Config>::AccountId = 7;
	const MAX_OFFENDER_SEVERITY: OffenceSeverity = OffenceSeverity(Perbill::from_percent(100));
	const LOW_OFFENDER_SEVERITY: OffenceSeverity = OffenceSeverity(Perbill::from_percent(0));
	const ACTIVE_SET: [<Test as pallet_session::Config>::ValidatorId; 7] = [1, 2, 3, 4, 5, 6, 7];
	const OFFENDER_VALIDATOR_IDX: u32 = 6; // the offender is with index 6 in the active set

	#[test]
	fn disable_when_below_byzantine_threshold() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(0, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			// Disable Offender and do not re-enable anyone
			assert_eq!(disabling_decision.disable, Some(OFFENDER_VALIDATOR_IDX));
			assert_eq!(disabling_decision.reenable, None);
		});
	}

	#[test]
	fn reenable_arbitrary_on_equal_severity() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(0, MAX_OFFENDER_SEVERITY), (1, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_some() && disabling_decision.reenable.is_some());
			// Disable 7 and enable 1
			assert_eq!(disabling_decision.disable.unwrap(), OFFENDER_VALIDATOR_IDX);
			assert_eq!(disabling_decision.reenable.unwrap(), 0);
		});
	}

	#[test]
	fn do_not_reenable_higher_offenders() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(0, MAX_OFFENDER_SEVERITY), (1, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					LOW_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_none() && disabling_decision.reenable.is_none());
		});
	}

	#[test]
	fn reenable_lower_offenders() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(0, LOW_OFFENDER_SEVERITY), (1, LOW_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_some() && disabling_decision.reenable.is_some());
			// Disable 7 and enable 1
			assert_eq!(disabling_decision.disable.unwrap(), OFFENDER_VALIDATOR_IDX);
			assert_eq!(disabling_decision.reenable.unwrap(), 0);
		});
	}

	#[test]
	fn reenable_lower_offenders_unordered() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled = vec![(0, MAX_OFFENDER_SEVERITY), (1, LOW_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_some() && disabling_decision.reenable.is_some());
			// Disable 7 and enable 1
			assert_eq!(disabling_decision.disable.unwrap(), OFFENDER_VALIDATOR_IDX);
			assert_eq!(disabling_decision.reenable.unwrap(), 1);
		});
	}

	#[test]
	fn update_severity() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled =
				vec![(OFFENDER_VALIDATOR_IDX, LOW_OFFENDER_SEVERITY), (0, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_some() && disabling_decision.reenable.is_none());
			// Disable 7 "again" AKA update their severity
			assert_eq!(disabling_decision.disable.unwrap(), OFFENDER_VALIDATOR_IDX);
		});
	}

	#[test]
	fn update_cannot_lower_severity() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled =
				vec![(OFFENDER_VALIDATOR_IDX, MAX_OFFENDER_SEVERITY), (0, MAX_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					LOW_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_none() && disabling_decision.reenable.is_none());
		});
	}

	#[test]
	fn no_accidental_reenablement_on_repeated_offence() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initially_disabled =
				vec![(OFFENDER_VALIDATOR_IDX, MAX_OFFENDER_SEVERITY), (0, LOW_OFFENDER_SEVERITY)];
			pallet_session::Validators::<Test>::put(ACTIVE_SET.to_vec());

			let disabling_decision =
				<UpToLimitWithReEnablingDisablingStrategy as DisablingStrategy<Test>>::decision(
					&OFFENDER_ID,
					MAX_OFFENDER_SEVERITY,
					&initially_disabled,
				);

			assert!(disabling_decision.disable.is_none() && disabling_decision.reenable.is_none());
		});
	}
}

#[test]
fn reenable_lower_offenders_mock() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);

			// offence with a low slash
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)], true);
			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(20)], true);

			// it does NOT affect the nominator.
			assert_eq!(Staking::nominators(101).unwrap().targets, vec![11, 21]);

			// both validators should be disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// offence with a higher slash
			on_offence_now(&[offence_from(31, None)], &[Perbill::from_percent(50)], true);

			// First offender is no longer disabled
			assert!(!is_disabled(11));
			// Mid offender is still disabled
			assert!(is_disabled(21));
			// New offender is disabled
			assert!(is_disabled(31));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 0, result: Ok(7) },
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(10),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 12 },
					Event::OffenceReported {
						validator: 21,
						fraction: Perbill::from_percent(20),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 21, page: 0 },
					Event::Slashed { staker: 21, amount: 200 },
					Event::Slashed { staker: 101, amount: 75 },
					Event::OffenceReported {
						validator: 31,
						fraction: Perbill::from_percent(50),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 31, page: 0 },
					Event::Slashed { staker: 31, amount: 250 },
				]
			);

			assert!(matches!(
				session_events().as_slice(),
				&[
					..,
					SessionEvent::ValidatorDisabled { validator: 11 },
					SessionEvent::ValidatorDisabled { validator: 21 },
					SessionEvent::ValidatorDisabled { validator: 31 },
					SessionEvent::ValidatorReenabled { validator: 11 },
				]
			));
		});
}

#[test]
fn do_not_reenable_higher_offenders_mock() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.build_and_execute(|| {
			mock::start_active_era(1);
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);

			// offence with a major slash
			on_offence_now(
				&[offence_from(11, None), offence_from(21, None), offence_from(31, None)],
				&[Perbill::from_percent(50), Perbill::from_percent(50), Perbill::from_percent(10)],
				true,
			);

			// both validators should be disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// New offender is not disabled as limit is reached and his prio is lower
			assert!(!is_disabled(31));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 0, result: Ok(7) },
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(50),
						offence_era: 1
					},
					Event::OffenceReported {
						validator: 21,
						fraction: Perbill::from_percent(50),
						offence_era: 1
					},
					Event::OffenceReported {
						validator: 31,
						fraction: Perbill::from_percent(10),
						offence_era: 1
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 31, page: 0 },
					Event::Slashed { staker: 31, amount: 50 },
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 21, page: 0 },
					Event::Slashed { staker: 21, amount: 500 },
					Event::Slashed { staker: 101, amount: 187 },
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 500 },
					Event::Slashed { staker: 101, amount: 62 },
				]
			);

			assert!(matches!(
				session_events().as_slice(),
				&[
					..,
					SessionEvent::ValidatorDisabled { validator: 11 },
					SessionEvent::ValidatorDisabled { validator: 21 },
				]
			));
		});
}

#[cfg(all(feature = "try-runtime", test))]
mod migration_tests {
	use super::*;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use migrations::{v15, v16};

	#[test]
	fn migrate_v15_to_v16_with_try_runtime() {
		ExtBuilder::default().validator_count(7).build_and_execute(|| {
			// Initial setup: Create old `DisabledValidators` in the form of `Vec<u32>`
			let old_disabled_validators = vec![1u32, 2u32];
			v15::DisabledValidators::<Test>::put(old_disabled_validators.clone());

			// Run pre-upgrade checks
			let pre_upgrade_result = v16::VersionUncheckedMigrateV15ToV16::<Test>::pre_upgrade();
			assert!(pre_upgrade_result.is_ok());
			let pre_upgrade_state = pre_upgrade_result.unwrap();

			// Run the migration
			v16::VersionUncheckedMigrateV15ToV16::<Test>::on_runtime_upgrade();

			// Run post-upgrade checks
			let post_upgrade_result =
				v16::VersionUncheckedMigrateV15ToV16::<Test>::post_upgrade(pre_upgrade_state);
			assert!(post_upgrade_result.is_ok());
		});
	}
}

mod hold_migration {
	use super::*;
	use sp_staking::{Stake, StakingInterface};

	#[test]
	fn ledger_update_creates_hold() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			// GIVEN alice who is a nominator with old currency
			let alice = 300;
			bond_nominator(alice, 1000, vec![11]);
			assert_eq!(asset::staked::<Test>(&alice), 1000);
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
			// migrate alice currency to legacy locks
			testing_utils::migrate_to_old_currency::<Test>(alice);
			// no more holds
			assert_eq!(asset::staked::<Test>(&alice), 0);
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: 1000, active: 1000 })
			);

			// any ledger mutation should create a hold
			hypothetically!({
				// give some extra balance to alice.
				let _ = asset::mint_into_existing::<Test>(&alice, 100);

				// WHEN new fund is bonded to ledger.
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(alice), 100));

				// THEN new hold is created
				assert_eq!(asset::staked::<Test>(&alice), 1000 + 100);
				assert_eq!(
					<Staking as StakingInterface>::stake(&alice),
					Ok(Stake { total: 1100, active: 1100 })
				);

				// old locked balance is untouched
				assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
			});

			hypothetically!({
				// WHEN new fund is unbonded from ledger.
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));

				// THEN hold is updated.
				assert_eq!(asset::staked::<Test>(&alice), 1000);
				assert_eq!(
					<Staking as StakingInterface>::stake(&alice),
					Ok(Stake { total: 1000, active: 900 })
				);

				// old locked balance is untouched
				assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
			});

			// WHEN alice currency is migrated.
			assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

			// THEN hold is updated.
			assert_eq!(asset::staked::<Test>(&alice), 1000);
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: 1000, active: 1000 })
			);

			// ensure cannot migrate again.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
				Error::<Test>::AlreadyMigrated
			);

			// locked balance is removed
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		});
	}

	#[test]
	fn migrate_removes_old_lock() {
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			// GIVEN alice who is a nominator with old currency
			let alice = 300;
			bond_nominator(alice, 1000, vec![11]);
			testing_utils::migrate_to_old_currency::<Test>(alice);
			assert_eq!(asset::staked::<Test>(&alice), 0);
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
			let pre_migrate_consumer = System::consumers(&alice);
			System::reset_events();

			// WHEN alice currency is migrated.
			assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

			// THEN
			// the extra consumer from old code is removed.
			assert_eq!(System::consumers(&alice), pre_migrate_consumer - 1);
			// ensure no lock
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
			// ensure stake and hold are same.
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: 1000, active: 1000 })
			);
			assert_eq!(asset::staked::<Test>(&alice), 1000);
			// ensure events are emitted.
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::CurrencyMigrated { stash: alice, force_withdraw: 0 }]
			);

			// ensure cannot migrate again.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
				Error::<Test>::AlreadyMigrated
			);
		});
	}
	#[test]
	fn cannot_hold_all_stake() {
		// When there is not enough funds to hold all stake, part of the stake if force withdrawn.
		// At end of the migration, the stake and hold should be same.
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			// GIVEN alice who is a nominator with old currency.
			let alice = 300;
			let stake = 1000;
			bond_nominator(alice, stake, vec![11]);
			testing_utils::migrate_to_old_currency::<Test>(alice);
			assert_eq!(asset::staked::<Test>(&alice), 0);
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), stake);
			// ledger has 1000 staked.
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: stake, active: stake })
			);

			// Get rid of the extra ED to emulate all their balance including ED is staked.
			assert_ok!(Balances::transfer_allow_death(
				RuntimeOrigin::signed(alice),
				10,
				ExistentialDeposit::get()
			));

			let expected_force_withdraw = ExistentialDeposit::get();

			// ledger mutation would fail in this case before migration because of failing hold.
			assert_noop!(
				Staking::unbond(RuntimeOrigin::signed(alice), 100),
				Error::<Test>::NotEnoughFunds
			);

			// clear events
			System::reset_events();

			// WHEN alice currency is migrated.
			assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

			// THEN
			let expected_hold = stake - expected_force_withdraw;
			// ensure no lock
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
			// ensure stake and hold are same.
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: expected_hold, active: expected_hold })
			);
			assert_eq!(asset::staked::<Test>(&alice), expected_hold);
			// ensure events are emitted.
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::CurrencyMigrated {
					stash: alice,
					force_withdraw: expected_force_withdraw
				}]
			);

			// ensure cannot migrate again.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
				Error::<Test>::AlreadyMigrated
			);

			// unbond works after migration.
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));
		});
	}

	#[test]
	fn virtual_staker_consumer_provider_dec() {
		// Ensure virtual stakers consumer and provider count is decremented.
		ExtBuilder::default().has_stakers(true).build_and_execute(|| {
			// 200 virtual bonds
			bond_virtual_nominator(200, 201, 500, vec![11, 21]);

			// previously the virtual nominator had a provider inc by the delegation system as
			// well as a consumer by this pallet.
			System::inc_providers(&200);
			System::inc_consumers(&200).expect("has provider, can consume");

			hypothetically!({
				// migrate 200
				assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), 200));

				// ensure account does not exist in system anymore.
				assert_eq!(System::consumers(&200), 0);
				assert_eq!(System::providers(&200), 0);
				assert!(!System::account_exists(&200));

				// ensure cannot migrate again.
				assert_noop!(
					Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
					Error::<Test>::AlreadyMigrated
				);
			});

			hypothetically!({
				// 200 has an erroneously extra provider
				System::inc_providers(&200);

				// causes migration to fail.
				assert_noop!(
					Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
					Error::<Test>::BadState
				);
			});

			// 200 is funded for more than ED by a random account.
			assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(999), 200, 10));

			// it has an extra provider now.
			assert_eq!(System::providers(&200), 2);

			// migrate 200
			assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), 200));

			// 1 provider is left, consumers is 0.
			assert_eq!(System::providers(&200), 1);
			assert_eq!(System::consumers(&200), 0);

			// ensure cannot migrate again.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
				Error::<Test>::AlreadyMigrated
			);
		});
	}
}

mod paged_slashing {
	use super::*;
	use crate::slashing::OffenceRecord;

	#[test]
	fn offence_processed_in_multi_block() {
		// Ensure each page is processed only once.
		ExtBuilder::default()
			.has_stakers(false)
			.slash_defer_duration(3)
			.build_and_execute(|| {
				let base_stake = 1000;

				// Create a validator:
				bond_validator(11, base_stake);
				assert_eq!(Validators::<Test>::count(), 1);

				// Track the total exposure of 11.
				let mut exposure_counter = base_stake;

				// Exposure page size is 64, hence it creates 4 pages of exposure.
				let expected_page_count = 4;
				for i in 0..200 {
					let bond_amount = base_stake + i as Balance;
					bond_nominator(1000 + i, bond_amount, vec![11]);
					// with multi page reward payout, payout exposure is same as total exposure.
					exposure_counter += bond_amount;
				}

				mock::start_active_era(1);

				assert_eq!(
					ErasStakersOverview::<Test>::get(1, 11).expect("exposure should exist"),
					PagedExposureMetadata {
						total: exposure_counter,
						own: base_stake,
						page_count: expected_page_count,
						nominator_count: 200,
					}
				);

				mock::start_active_era(2);
				System::reset_events();

				// report an offence for 11 in era 1.
				on_offence_in_era(
					&[offence_from(11, None)],
					&[Perbill::from_percent(10)],
					1,
					false,
				);

				// ensure offence is queued.
				assert_eq!(
					staking_events_since_last_call().as_slice(),
					vec![Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(10),
						offence_era: 1
					}]
				);

				// ensure offence queue has items.
				assert_eq!(
					OffenceQueue::<Test>::get(1, 11).unwrap(),
					slashing::OffenceRecord {
						reporter: None,
						reported_era: 2,
						// first page to be marked for processing.
						exposure_page: expected_page_count - 1,
						slash_fraction: Perbill::from_percent(10),
						prior_slash_fraction: Perbill::zero(),
					}
				);

				// The offence era is noted in the queue.
				assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![1]);

				// ensure Processing offence is empty yet.
				assert_eq!(ProcessingOffence::<Test>::get(), None);

				// ensure no unapplied slashes for era 4 (offence_era + slash_defer_duration).
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 0);

				// Checkpoint 1: advancing to next block will compute the first page of slash.
				roll_blocks(1);

				// ensure the last page of offence is processed.
				// (offence is processed in reverse order of pages)
				assert_eq!(
					staking_events_since_last_call().as_slice(),
					vec![Event::SlashComputed {
						offence_era: 1,
						slash_era: 4,
						offender: 11,
						page: expected_page_count - 1
					},]
				);

				// offender is removed from offence queue
				assert_eq!(OffenceQueue::<Test>::get(1, 11), None);

				// offence era is removed from queue.
				assert_eq!(OffenceQueueEras::<Test>::get(), None);

				// this offence is not completely processed yet, so it should be in processing.
				assert_eq!(
					ProcessingOffence::<Test>::get(),
					Some((
						1,
						11,
						OffenceRecord {
							reporter: None,
							reported_era: 2,
							// page 3 is processed, next page to be processed is 2.
							exposure_page: 2,
							slash_fraction: Perbill::from_percent(10),
							prior_slash_fraction: Perbill::zero(),
						}
					))
				);

				// unapplied slashes for era 4.
				let slashes = UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>();
				// only one unapplied slash exists.
				assert_eq!(slashes.len(), 1);
				let (slash_key, unapplied_slash) = &slashes[0];
				// this is a unique key to ensure unapplied slash is not overwritten for multiple
				// offence by offender in the same era.
				assert_eq!(*slash_key, (11, Perbill::from_percent(10), expected_page_count - 1));

				// validator own stake is only included in the first page. Since this is page 3,
				// only nominators are slashed.
				assert_eq!(unapplied_slash.own, 0);
				assert_eq!(unapplied_slash.validator, 11);
				assert_eq!(unapplied_slash.others.len(), 200 % 64);

				// Checkpoint 2: advancing to next block will compute the second page of slash.
				roll_blocks(1);

				// offence queue still empty
				assert_eq!(OffenceQueue::<Test>::get(1, 11), None);
				assert_eq!(OffenceQueueEras::<Test>::get(), None);

				// processing offence points to next page.
				assert_eq!(
					ProcessingOffence::<Test>::get(),
					Some((
						1,
						11,
						OffenceRecord {
							reporter: None,
							reported_era: 2,
							// page 2 is processed, next page to be processed is 1.
							exposure_page: 1,
							slash_fraction: Perbill::from_percent(10),
							prior_slash_fraction: Perbill::zero(),
						}
					))
				);

				// there are two unapplied slashes for era 4.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 2);

				// ensure the last page of offence is processed.
				// (offence is processed in reverse order of pages)
				assert_eq!(
					staking_events_since_last_call().as_slice(),
					vec![Event::SlashComputed {
						offence_era: 1,
						slash_era: 4,
						offender: 11,
						page: expected_page_count - 2
					},]
				);

				// Checkpoint 3: advancing to two more blocks will complete the processing of the
				// reported offence
				roll_blocks(2);

				// no processing offence.
				assert!(ProcessingOffence::<Test>::get().is_none());
				// total of 4 unapplied slash.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 4);

				// Checkpoint 4: lets verify the application of slashes in multiple blocks.
				// advance to era 4.
				mock::start_active_era(4);
				// slashes are not applied just yet. From next blocks, they will be applied.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 4);

				// advance to next block.
				roll_blocks(1);
				// 1 slash is applied.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 3);

				// advance two blocks.
				roll_blocks(2);
				// 2 more slashes are applied.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 1);

				// advance one more block.
				roll_blocks(1);
				// all slashes are applied.
				assert_eq!(UnappliedSlashes::<Test>::iter_prefix(&4).collect::<Vec<_>>().len(), 0);

				// ensure all stakers are slashed correctly.
				assert_eq!(asset::staked::<Test>(&11), 1000 - 100);

				for i in 0..200 {
					let original_stake = 1000 + i as Balance;
					let expected_slash = Perbill::from_percent(10) * original_stake;
					assert_eq!(asset::staked::<Test>(&(1000 + i)), original_stake - expected_slash);
				}
			})
	}

	#[test]
	fn offence_discarded_correctly() {
		ExtBuilder::default().slash_defer_duration(3).build_and_execute(|| {
			start_active_era(2);

			// Scenario 1: 11 commits an offence in era 2.
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(10)], 2, false);

			// offence is queued, not processed yet.
			let queued_offence_one = OffenceQueue::<Test>::get(2, 11).unwrap();
			assert_eq!(queued_offence_one.slash_fraction, Perbill::from_percent(10));
			assert_eq!(queued_offence_one.prior_slash_fraction, Perbill::zero());
			assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![2]);

			// Scenario 1A: 11 commits a second offence in era 2 with **lower** slash fraction than
			// the previous offence.
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(5)], 2, false);

			// the second offence is discarded. No change in the queue.
			assert_eq!(OffenceQueue::<Test>::get(2, 11).unwrap(), queued_offence_one);

			// Scenario 1B: 11 commits a second offence in era 2 with **higher** slash fraction than
			// the previous offence.
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(15)], 2, false);

			// the second offence overwrites the first offence.
			let overwritten_offence = OffenceQueue::<Test>::get(2, 11).unwrap();
			assert!(overwritten_offence.slash_fraction > queued_offence_one.slash_fraction);
			assert_eq!(overwritten_offence.slash_fraction, Perbill::from_percent(15));
			assert_eq!(overwritten_offence.prior_slash_fraction, Perbill::zero());
			assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![2]);

			// Scenario 2: 11 commits another offence in era 2, but after the previous offence is
			// processed.
			roll_blocks(1);
			assert!(OffenceQueue::<Test>::get(2, 11).is_none());
			assert!(OffenceQueueEras::<Test>::get().is_none());
			// unapplied slash is created for the offence.
			assert!(UnappliedSlashes::<Test>::contains_key(
				2 + 3,
				(11, Perbill::from_percent(15), 0)
			));

			// Scenario 2A: offence has **lower** slash fraction than the previous offence.
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(14)], 2, false);
			// offence is discarded.
			assert!(OffenceQueue::<Test>::get(2, 11).is_none());
			assert!(OffenceQueueEras::<Test>::get().is_none());

			// Scenario 2B: offence has **higher** slash fraction than the previous offence.
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(16)], 2, false);
			// process offence
			roll_blocks(1);
			// there are now two slash records for 11, for era 5, with the newer one only slashing
			// the diff between slash fractions of 16 and 15.
			let slash_one =
				UnappliedSlashes::<Test>::get(2 + 3, (11, Perbill::from_percent(15), 0)).unwrap();
			let slash_two =
				UnappliedSlashes::<Test>::get(2 + 3, (11, Perbill::from_percent(16), 0)).unwrap();
			assert!(slash_one.own > slash_two.own);
		});
	}

	#[test]
	fn offence_eras_queued_correctly() {
		ExtBuilder::default().build_and_execute(|| {
			// 11 and 21 are validators.
			assert_eq!(Staking::status(&11).unwrap(), StakerStatus::Validator);
			assert_eq!(Staking::status(&21).unwrap(), StakerStatus::Validator);

			start_active_era(2);

			// 11 and 21 commits offence in era 2.
			on_offence_in_era(
				&[offence_from(11, None), offence_from(21, None)],
				&[Perbill::from_percent(10), Perbill::from_percent(20)],
				2,
				false,
			);

			// 11 and 21 commits offence in era 1 but reported after the era 2 offence.
			on_offence_in_era(
				&[offence_from(11, None), offence_from(21, None)],
				&[Perbill::from_percent(10), Perbill::from_percent(20)],
				1,
				false,
			);

			// queued offence eras are sorted.
			assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![1, 2]);

			// next two blocks, the offence in era 1 is processed.
			roll_blocks(2);

			// only era 2 is left in the queue.
			assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![2]);

			// next block, the offence in era 2 is processed.
			roll_blocks(1);

			// era still exist in the queue.
			assert_eq!(OffenceQueueEras::<Test>::get().unwrap(), vec![2]);

			// next block, the era 2 is processed.
			roll_blocks(1);

			// queue is empty.
			assert_eq!(OffenceQueueEras::<Test>::get(), None);
		});
	}
	#[test]
	fn non_deferred_slash_applied_instantly() {
		ExtBuilder::default().build_and_execute(|| {
			mock::start_active_era(2);
			let validator_stake = asset::staked::<Test>(&11);
			let slash_fraction = Perbill::from_percent(10);
			let expected_slash = slash_fraction * validator_stake;
			System::reset_events();

			// report an offence for 11 in era 1.
			on_offence_in_era(&[offence_from(11, None)], &[slash_fraction], 1, false);

			// ensure offence is queued.
			assert_eq!(
				staking_events_since_last_call().as_slice(),
				vec![Event::OffenceReported {
					validator: 11,
					fraction: Perbill::from_percent(10),
					offence_era: 1
				}]
			);

			// process offence
			roll_blocks(1);

			// ensure slash is computed and applied.
			assert_eq!(
				staking_events_since_last_call().as_slice(),
				vec![
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: expected_slash },
					// this is the nominator of 11.
					Event::Slashed { staker: 101, amount: 12 },
				]
			);

			// ensure validator is slashed.
			assert_eq!(asset::staked::<Test>(&11), validator_stake - expected_slash);
		});
	}

	#[test]
	fn validator_with_no_exposure_slashed() {
		ExtBuilder::default().build_and_execute(|| {
			let validator_stake = asset::staked::<Test>(&11);
			let slash_fraction = Perbill::from_percent(10);
			let expected_slash = slash_fraction * validator_stake;

			// only 101 nominates 11, lets remove them.
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![21]));

			start_active_era(2);
			// ensure validator has no exposure.
			assert_eq!(ErasStakersOverview::<Test>::get(2, 11).unwrap().page_count, 0,);

			// clear events
			System::reset_events();

			// report an offence for 11.
			on_offence_now(&[offence_from(11, None)], &[slash_fraction], true);

			// ensure validator is slashed.
			assert_eq!(asset::staked::<Test>(&11), validator_stake - expected_slash);
			assert_eq!(
				staking_events_since_last_call().as_slice(),
				vec![
					Event::OffenceReported {
						offence_era: 2,
						validator: 11,
						fraction: slash_fraction
					},
					Event::SlashComputed { offence_era: 2, slash_era: 2, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: expected_slash },
				]
			);
		});
	}
}
 */

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
use crate::{session_rotation::Eras, slashing};
use pallet_staking_async_rc_client as rc_client;
use sp_runtime::{Perquintill, Rounding};
use sp_staking::StakingInterface;

#[test]
fn nominators_also_get_slashed_pro_rata() {
	ExtBuilder::default()
		.validator_count(4)
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			let initial_exposure = Staking::eras_stakers(active_era(), &11);
			assert_eq!(
				initial_exposure,
				Exposure {
					total: 1250,
					own: 1000,
					others: vec![IndividualExposure { who: 101, value: 250 }]
				}
			);

			// staked values;
			let nominator_stake = Staking::ledger(101.into()).unwrap().active;
			let nominator_balance = asset::stakeable_balance::<Test>(&101);
			let validator_stake = Staking::ledger(11.into()).unwrap().active;
			let validator_balance = asset::stakeable_balance::<Test>(&11);
			let exposed_stake = initial_exposure.total;
			let exposed_validator = initial_exposure.own;
			let exposed_nominator = initial_exposure.others.first().unwrap().value;

			// register a slash for 11 with 10%.
			add_slash(11);
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(10)
				}]
			);

			// roll one block until it is applied
			assert_eq!(SlashDeferDuration::get(), 0);
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 25 }
				]
			);

			// both stakes must have been decreased.
			assert!(Staking::ledger(101.into()).unwrap().active < nominator_stake);
			assert!(Staking::ledger(11.into()).unwrap().active < validator_stake);

			let slash_amount = Perbill::from_percent(10) * exposed_stake;
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
			assert_eq!(
				asset::stakeable_balance::<Test>(&101), // free balance
				nominator_balance - nominator_share,
			);
			assert_eq!(
				asset::stakeable_balance::<Test>(&11), // free balance
				validator_balance - validator_share,
			);
		});
}

#[test]
fn slashing_performed_according_exposure() {
	// This test checks that slashing is performed according the exposure (or more precisely,
	// historical exposure), not the current balance.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq!(Staking::eras_stakers(active_era(), &11).own, 1000);

		// Handle an offence with a historical exposure.
		add_slash_with_percent(11, 50);
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::OffenceReported {
				offence_era: 1,
				validator: 11,
				fraction: Perbill::from_percent(50)
			}]
		);

		// roll one block until it is applied
		assert_eq!(SlashDeferDuration::get(), 0);

		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
				Event::Slashed { staker: 11, amount: 500 },
			]
		);

		// The stash account should be slashed for 250 (50% of 500).
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000 / 2);
	});
}

#[test]
fn offence_doesnt_force_new_era() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(ForceEra::<T>::get(), Forcing::NotForcing);
		add_slash(11);
		assert_eq!(ForceEra::<T>::get(), Forcing::NotForcing);
	});
}

#[test]
fn offence_ensures_new_era_without_clobbering() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::force_new_era_always(RuntimeOrigin::root()));
		assert_eq!(ForceEra::<T>::get(), Forcing::ForceAlways);

		add_slash(11);

		assert_eq!(ForceEra::<T>::get(), Forcing::ForceAlways);
	});
}

#[test]
fn add_slash_works() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq_uvec!(session_validators(), vec![11, 21]);

		add_slash(11);
		// roll to apply the slash
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(10)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
				Event::Slashed { staker: 11, amount: 100 },
			]
		);

		// no one is chilled, FYI
		assert!(Validators::<T>::contains_key(11));
	});
}

#[test]
fn only_first_reporter_receive_the_slice() {
	// This test verifies that the first reporter of the offence receive their slice from the
	// slashed amount.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// The reporters' reward is calculated from the total exposure.
		assert_eq!(Staking::eras_stakers(active_era(), &11).total, 1000);

		let initial_balance_1 = asset::total_balance::<T>(&1);
		let initial_balance_2 = asset::total_balance::<T>(&2);

		<Staking as rc_client::AHStakingInterface>::on_new_offences(
			session_mock::Session::current_index(),
			vec![rc_client::Offence {
				offender: 11,
				reporters: vec![1, 2],
				slash_fraction: Perbill::from_percent(50),
			}],
		);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(50)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
				Event::Slashed { staker: 11, amount: 500 },
			]
		);

		let reward = 500 / 20;
		assert_eq!(asset::total_balance::<T>(&1), initial_balance_1 + reward);
		// second reporter got nothing
		assert_eq!(asset::total_balance::<T>(&2), initial_balance_2);
	});
}

#[test]
fn subsequent_reports_in_same_span_pay_out_less() {
	// This test verifies that the reporters of the offence receive their slice from the slashed
	// amount, but less and less if they submit multiple reports in one span.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// The reporters' reward is calculated from the total exposure.
		let initial_balance = 1000;

		assert_eq!(Staking::eras_stakers(active_era(), &11).total, initial_balance);
		let initial_balance_1 = asset::total_balance::<T>(&1);

		<Staking as rc_client::AHStakingInterface>::on_new_offences(
			session_mock::Session::current_index(),
			vec![rc_client::Offence {
				offender: 11,
				reporters: vec![1],
				slash_fraction: Perbill::from_percent(20),
			}],
		);
		Session::roll_next();

		// F1 * (reward_proportion * slash - 0)
		// 50% * (10% * initial_balance * 20%)
		let reward = (initial_balance / 5) / 20;
		assert_eq!(reward, 10);
		assert_eq!(asset::total_balance::<T>(&1), initial_balance_1 + reward);

		<Staking as rc_client::AHStakingInterface>::on_new_offences(
			session_mock::Session::current_index(),
			vec![rc_client::Offence {
				offender: 11,
				reporters: vec![1],
				slash_fraction: Perbill::from_percent(50),
			}],
		);
		Session::roll_next();

		let prior_payout = reward;
		// F1 * (reward_proportion * slash - prior_payout)
		// 50% * (10% * (initial_balance / 2) - prior_payout)
		let reward = ((initial_balance / 20) - prior_payout) / 2;
		assert_eq!(reward, 20);
		assert_eq!(asset::total_balance::<T>(&1), initial_balance_1 + prior_payout + reward);
	});
}

#[test]
fn deferred_slashes_are_deferred() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		let exposure = Staking::eras_stakers(active_era(), &11);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		// only 1 page of exposure, so slashes will be applied in one block.
		assert_eq!(Eras::<T>::exposure_page_count(1, &11), 1);

		add_slash(11);
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::OffenceReported {
				offence_era: 1,
				validator: 11,
				fraction: Perbill::from_percent(10)
			}]
		);

		// slash computed in the next block
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 },]
		);

		// nominations are not removed regardless of the deferring.
		assert_eq!(Nominators::<T>::get(101).unwrap().targets, vec![11, 21]);

		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		Session::roll_until_active_era(2);
		// no slash applied
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::SessionRotated { starting_session: 4, active_era: 1, planned_era: 2 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 2 },
				Event::EraPaid { era_index: 1, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 6, active_era: 2, planned_era: 2 }
			]
		);

		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		// the slashes for era 1 will start applying in era 3, to end before era 4.
		Session::roll_until_active_era(3);
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::SessionRotated { starting_session: 7, active_era: 2, planned_era: 3 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 8, active_era: 2, planned_era: 3 },
				Event::EraPaid { era_index: 2, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 9, active_era: 3, planned_era: 3 }
			]
		);

		// Slashes not applied yet. Will apply in the next block after era starts.
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		// trigger slashing by advancing block.
		Session::roll_next();

		assert_eq!(asset::stakeable_balance::<T>(&11), 900);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500 - (nominated_value / 10));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::Slashed { staker: 11, amount: 100 },
				Event::Slashed { staker: 101, amount: 25 }
			]
		);
	})
}

#[test]
fn retroactive_deferred_slashes_two_eras_before() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		assert_eq!(BondingDuration::get(), 3);
		assert_eq!(Nominators::<T>::get(101).unwrap().targets, vec![11, 21]);

		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// slash for era 1 detected in era 2, defer for 2, apply in era 3.
		add_slash_in_era(11, 1);
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::OffenceReported {
				offence_era: 1,
				validator: 11,
				fraction: Perbill::from_percent(10)
			}]
		);

		// computed in next block, but not applied
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 }]
		);

		Session::roll_until_active_era(3);
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::SessionRotated { starting_session: 7, active_era: 2, planned_era: 3 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 8, active_era: 2, planned_era: 3 },
				Event::EraPaid { era_index: 2, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 9, active_era: 3, planned_era: 3 }
			]
		);

		// Slashes not applied yet. Will apply in the next block after era starts.
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::Slashed { staker: 11, amount: 100 },
				Event::Slashed { staker: 101, amount: 25 }
			]
		);
	})
}

#[test]
fn retroactive_deferred_slashes_one_before() {
	ExtBuilder::default()
		.slash_defer_duration(2)
		.nominate(false)
		.build_and_execute(|| {
			assert_eq!(BondingDuration::get(), 3);

			// unbond at slash era.
			Session::roll_until_active_era(2);

			assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 100));

			Session::roll_until_active_era(3);
			// ignore all events thus far
			let _ = staking_events_since_last_call();

			add_slash_in_era(11, 2);
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::OffenceReported {
					offence_era: 2,
					validator: 11,
					fraction: Perbill::from_percent(10)
				}]
			);

			// computed in next block, but not applied
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::SlashComputed { offence_era: 2, slash_era: 4, offender: 11, page: 0 }]
			);

			Session::roll_until_active_era(4);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 10, active_era: 3, planned_era: 4 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 11, active_era: 3, planned_era: 4 },
					Event::EraPaid { era_index: 3, validator_payout: 7500, remainder: 7500 },
					Event::SessionRotated { starting_session: 12, active_era: 4, planned_era: 4 }
				]
			);

			// no slash applied yet
			assert_eq!(Staking::ledger(11.into()).unwrap().total, 1000);

			// slash happens at next blocks.
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::Slashed { staker: 11, amount: 100 }]
			);

			// their ledger has already been slashed.
			assert_eq!(Staking::ledger(11.into()).unwrap().total, 900);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 1000));
			assert_eq!(Staking::ledger(11.into()).unwrap().total, 900);
		})
}

#[test]
fn invulnerables_are_not_slashed() {
	// For invulnerable validators no slashing is performed.
	ExtBuilder::default()
		.invulnerables(vec![11])
		.nominate(false)
		.build_and_execute(|| {
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&21), 1000);

			let initial_balance = Staking::slashable_balance_of(&21);

			// slash both
			add_slash(11);
			add_slash(21);
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::OffenceReported {
						offence_era: 1,
						validator: 21,
						fraction: Perbill::from_percent(10)
					},
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 21, page: 0 },
					Event::Slashed { staker: 21, amount: 100 }
				]
			);

			// The validator 11 hasn't been slashed, but 21 has been.
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			// 1000 - (0.1 * initial_balance)
			assert_eq!(asset::stakeable_balance::<T>(&21), 1000 - (initial_balance / 10));
		});
}

#[test]
fn dont_slash_if_fraction_is_zero() {
	// Don't slash if the fraction is zero.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);

		add_slash_with_percent(11, 0);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::OffenceReported { offence_era: 1, validator: 11, fraction: Zero::zero() }]
		);

		// The validator hasn't been slashed. The new era is not forced.
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(ForceEra::<T>::get(), Forcing::NotForcing);
	});
}

#[test]
fn only_slash_for_max_in_era() {
	// multiple slashes within one era are only applied if it is more than any previous slash in the
	// same era.
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);

		add_slash_with_percent(11, 50);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(50)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
				Event::Slashed { staker: 11, amount: 500 }
			]
		);

		// The validator has been slashed and has been force-chilled.
		assert_eq!(asset::stakeable_balance::<T>(&11), 500);

		add_slash_with_percent(11, 25);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::OffenceReported {
				offence_era: 1,
				validator: 11,
				fraction: Perbill::from_percent(25)
			},]
		);

		// The validator has not been slashed additionally.
		assert_eq!(asset::stakeable_balance::<T>(&11), 500);

		// now slash for more than 50
		add_slash_with_percent(11, 60);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(60)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
				Event::Slashed { staker: 11, amount: 100 }
			]
		);

		// The validator got slashed 10% more.
		assert_eq!(asset::stakeable_balance::<T>(&11), 400);
	})
}

#[test]
fn garbage_collection_after_slashing() {
	// ensures that `SlashingSpans` and `SpanSlash` of an account is removed after reaping.
	ExtBuilder::default()
		.existential_deposit(2)
		.balance_factor(2)
		.build_and_execute(|| {
			assert_eq!(asset::stakeable_balance::<T>(&11), 2000);

			add_slash_with_percent(11, 10);
			Session::roll_next();

			assert_eq!(asset::stakeable_balance::<T>(&11), 2000 - 200);
			assert!(SlashingSpans::<T>::get(&11).is_some());
			assert_eq!(SpanSlash::<T>::get(&(11, 0)).amount(), &200);

			add_slash_with_percent(11, 100);
			Session::roll_next();

			// validator and nominator slash in era are garbage-collected by era change,
			// so we don't test those here.

			assert_eq!(asset::stakeable_balance::<T>(&11), 0);
			// Non staked balance is not touched.
			assert_eq!(asset::total_balance::<T>(&11), ExistentialDeposit::get());

			let slashing_spans = SlashingSpans::<T>::get(&11).unwrap();
			assert_eq!(slashing_spans.iter().count(), 2);

			// reap_stash respects num_slashing_spans so that weight is accurate
			assert_noop!(
				Staking::reap_stash(RuntimeOrigin::signed(20), 11, 0),
				Error::<T>::IncorrectSlashingSpans
			);
			assert_ok!(Staking::reap_stash(RuntimeOrigin::signed(20), 11, 2));

			assert!(SlashingSpans::<T>::get(&11).is_none());
			assert_eq!(SpanSlash::<T>::get(&(11, 0)).amount(), &0);
		})
}

#[test]
fn garbage_collection_on_window_pruning() {
	// ensures that `ValidatorSlashInEra` and `NominatorSlashInEra` are cleared after
	// `BondingDuration`.
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		let now = active_era();

		let exposure = Staking::eras_stakers(now, &11);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);
		let nominated_value = exposure.others.iter().find(|o| o.who == 101).unwrap().value;

		add_slash(11);
		Session::roll_next();

		assert_eq!(asset::stakeable_balance::<T>(&11), 900);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500 - (nominated_value / 10));

		assert!(ValidatorSlashInEra::<T>::get(&now, &11).is_some());
		assert!(NominatorSlashInEra::<T>::get(&now, &101).is_some());

		// + 1 because we have to exit the bonding window.
		for era in (0..(BondingDuration::get() + 1)).map(|offset| offset + now + 1) {
			assert!(ValidatorSlashInEra::<T>::get(&now, &11).is_some());
			assert!(NominatorSlashInEra::<T>::get(&now, &101).is_some());

			Session::roll_until_active_era(era);
		}

		assert!(ValidatorSlashInEra::<T>::get(&now, &11).is_none());
		assert!(NominatorSlashInEra::<T>::get(&now, &101).is_none());
	})
}

#[test]
fn slashing_nominators_by_span_max() {
	ExtBuilder::default().build_and_execute(|| {
		Session::roll_until_active_era(3);

		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&21), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);
		assert_eq!(Staking::slashable_balance_of(&21), 1000);

		let exposure_11 = Staking::eras_stakers(active_era(), &11);
		let exposure_21 = Staking::eras_stakers(active_era(), &21);
		let nominated_value_11 = exposure_11.others.iter().find(|o| o.who == 101).unwrap().value;
		let nominated_value_21 = exposure_21.others.iter().find(|o| o.who == 101).unwrap().value;

		add_slash_in_era(11, 2);
		Session::roll_next();

		assert_eq!(asset::stakeable_balance::<T>(&11), 900);

		let slash_1_amount = Perbill::from_percent(10) * nominated_value_11;
		assert_eq!(asset::stakeable_balance::<T>(&101), 500 - slash_1_amount);

		let expected_spans = vec![
			slashing::SlashingSpan { index: 1, start: 4, length: None },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		let get_span = |account| SlashingSpans::<T>::get(&account).unwrap();

		assert_eq!(get_span(11).iter().collect::<Vec<_>>(), expected_spans);
		assert_eq!(get_span(101).iter().collect::<Vec<_>>(), expected_spans);

		// second slash: higher era, higher value, same span.
		add_slash_in_era_with_value(21, 3, Perbill::from_percent(30));
		Session::roll_next();

		// 11 was not further slashed, but 21 and 101 were.
		assert_eq!(asset::stakeable_balance::<T>(&11), 900);
		assert_eq!(asset::stakeable_balance::<T>(&21), 700);

		let slash_2_amount = Perbill::from_percent(30) * nominated_value_21;
		assert!(slash_2_amount > slash_1_amount);

		// only the maximum slash in a single span is taken.
		assert_eq!(asset::stakeable_balance::<T>(&101), 500 - slash_2_amount);

		// third slash: in same era and on same validator as first, higher in-era value, but lower
		// slash value than slash 2.
		add_slash_in_era_with_value(11, 2, Perbill::from_percent(20));
		Session::roll_next();

		// 11 was further slashed, but 21 and 101 were not.
		assert_eq!(asset::stakeable_balance::<T>(&11), 800);
		assert_eq!(asset::stakeable_balance::<T>(&21), 700);

		let slash_3_amount = Perbill::from_percent(20) * nominated_value_21;
		assert!(slash_3_amount < slash_2_amount);
		assert!(slash_3_amount > slash_1_amount);

		// only the maximum slash in a single span is taken.
		assert_eq!(asset::stakeable_balance::<T>(&101), 500 - slash_2_amount);
	});
}

#[test]
fn slashes_are_summed_across_spans() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		Session::roll_until_active_era(3);

		assert_eq!(asset::stakeable_balance::<T>(&21), 1000);
		assert_eq!(Staking::slashable_balance_of(&21), 1000);

		let get_span = |account| SlashingSpans::<T>::get(&account).unwrap();

		add_slash(21);
		Session::roll_next();

		let expected_spans = vec![
			slashing::SlashingSpan { index: 1, start: 4, length: None },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		assert_eq!(get_span(21).iter().collect::<Vec<_>>(), expected_spans);
		assert_eq!(asset::stakeable_balance::<T>(&21), 900);
		assert_eq!(Staking::slashable_balance_of(&21), 900);

		Session::roll_until_active_era(4);
		add_slash(21);
		Session::roll_next();

		let expected_spans = vec![
			slashing::SlashingSpan { index: 2, start: 5, length: None },
			slashing::SlashingSpan { index: 1, start: 4, length: Some(1) },
			slashing::SlashingSpan { index: 0, start: 0, length: Some(4) },
		];

		assert_eq!(get_span(21).iter().collect::<Vec<_>>(), expected_spans);
		assert_eq!(asset::stakeable_balance::<T>(&21), 810);
	});
}

#[test]
fn staker_cannot_bail_deferred_slash() {
	// as long as SlashDeferDuration is less than BondingDuration, this should not be possible.
	ExtBuilder::default()
		.slash_defer_duration(2)
		.bonding_duration(3)
		.build_and_execute(|| {
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500);

			add_slash(11);
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::OffenceReported {
						offence_era: 1,
						validator: 11,
						fraction: Perbill::from_percent(10)
					},
					Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 }
				]
			);

			// now we chill
			assert_ok!(Staking::chill(RuntimeOrigin::signed(101)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 500));

			assert_eq!(CurrentEra::<T>::get().unwrap(), 1);
			assert_eq!(active_era(), 1);

			assert_eq!(
				Ledger::<T>::get(101).unwrap(),
				StakingLedgerInspect {
					active: 0,
					total: 500,
					stash: 101,
					unlocking: bounded_vec![UnlockChunk { era: 4u32, value: 500 }],
				}
			);

			// no slash yet.
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500);

			// no slash yet.
			Session::roll_until_active_era(2);
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500);

			// no slash yet.
			Session::roll_until_active_era(3);
			let _ = staking_events_since_last_call();
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500);
			assert_eq!(CurrentEra::<T>::get().unwrap(), 3);
			assert_eq!(active_era(), 3);

			// and cannot yet unbond:
			assert_storage_noop!(assert!(Staking::withdraw_unbonded(
				RuntimeOrigin::signed(101),
				0
			)
			.is_ok()));

			// first block of era 3, slashes are applied.
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 25 }
				]
			);

			assert_eq!(asset::stakeable_balance::<T>(&11), 900);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500 - 25);

			// and the leftover of the funds can now be unbonded.
		})
}

#[test]
fn remove_deferred() {
	ExtBuilder::default().slash_defer_duration(2).build_and_execute(|| {
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		// deferred to start of era 3.
		add_slash(11);
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(10)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 }
			]
		);

		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::stakeable_balance::<T>(&101), 500);

		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();
		// reported later, but deferred to start of era 3 as well.
		add_slash_in_era_with_value(11, 1, Perbill::from_percent(15));
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(15)
				},
				Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 }
			]
		);

		assert_eq!(
			UnappliedSlashes::<T>::iter_prefix(&3).collect::<Vec<_>>(),
			vec![
				(
					(11, Perbill::from_percent(10), 0),
					UnappliedSlash {
						validator: 11,
						own: 100,
						others: bounded_vec![(101, 25)],
						reporter: None,
						payout: 6
					}
				),
				(
					(11, Perbill::from_percent(15), 0),
					UnappliedSlash {
						validator: 11,
						own: 50,
						others: bounded_vec![(101, 12)],
						reporter: None,
						payout: 6
					}
				),
			]
		);

		// fails if empty
		assert_noop!(
			Staking::cancel_deferred_slash(RuntimeOrigin::root(), 1, vec![]),
			Error::<T>::EmptyTargets
		);

		// cancel the slash with 10%.
		assert_ok!(Staking::cancel_deferred_slash(
			RuntimeOrigin::root(),
			3,
			vec![(11, Perbill::from_percent(10), 0)]
		));
		assert_eq!(UnappliedSlashes::<T>::iter_prefix(&3).count(), 1);
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SlashCancelled {
				slash_era: 3,
				slash_key: (11, Perbill::from_percent(10), 0),
				payout: 6
			}]
		);

		// apply the one with 15%.
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		Session::roll_next();
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::Slashed { staker: 11, amount: 50 },
				Event::Slashed { staker: 101, amount: 12 }
			]
		);
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
			assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
			assert_eq!(asset::stakeable_balance::<T>(&101), 500);

			add_slash_with_percent(11, 10);
			add_slash_with_percent(21, 10);
			add_slash_with_percent(41, 25);
			Session::roll_next();
			Session::roll_next();
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::OffenceReported {
						offence_era: 1,
						validator: 11,
						fraction: Perbill::from_percent(10)
					},
					Event::OffenceReported {
						offence_era: 1,
						validator: 21,
						fraction: Perbill::from_percent(10)
					},
					Event::OffenceReported {
						offence_era: 1,
						validator: 41,
						fraction: Perbill::from_percent(25)
					},
					Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 41, page: 0 },
					Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 21, page: 0 },
					Event::SlashComputed { offence_era: 1, slash_era: 3, offender: 11, page: 0 },
				]
			);

			// there are 3 slashes to be applied in era 3.
			assert_eq!(UnappliedSlashes::<T>::iter_prefix(&3).count(), 3);

			// lets cancel 2 of them.
			assert_ok!(Staking::cancel_deferred_slash(
				RuntimeOrigin::root(),
				3,
				vec![(11, Perbill::from_percent(10), 0), (21, Perbill::from_percent(10), 0),]
			));

			let slashes = UnappliedSlashes::<T>::iter_prefix(&3).collect::<Vec<_>>();
			assert_eq!(slashes.len(), 1);
		})
}

#[test]
fn proportional_slash_stop_slashing_if_remaining_zero() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		let c = |era, value| UnlockChunk::<Balance> { era, value };

		// we have some chunks, but they are not affected.
		let unlocking = bounded_vec![c(1, 10), c(2, 10)];

		// Given
		let mut ledger = StakingLedger::<T>::new(123, 20);
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
		let mut ledger = StakingLedger::<T>::new(123, 10);
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
				assert_eq!(Validators::<T>::count(), 1);

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

				Session::roll_until_active_era(2);
				let _ = staking_events_since_last_call();

				assert_eq!(
					ErasStakersOverview::<T>::get(2, 11).expect("exposure should exist"),
					PagedExposureMetadata {
						total: exposure_counter,
						own: base_stake,
						page_count: expected_page_count,
						nominator_count: 200,
					}
				);

				// report an offence for 11 in era 2.
				add_slash(11);

				// ensure offence is queued.
				assert_eq!(
					staking_events_since_last_call(),
					vec![Event::OffenceReported {
						validator: 11,
						fraction: Perbill::from_percent(10),
						offence_era: 2
					}]
				);

				// ensure offence queue has items.
				assert_eq!(
					OffenceQueue::<T>::get(2, 11).unwrap(),
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
				assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![2]);

				// ensure Processing offence is empty yet.
				assert_eq!(ProcessingOffence::<T>::get(), None);

				// ensure no unapplied slashes for era 5 (offence_era + slash_defer_duration).
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 0);

				// Checkpoint 1: advancing to next block will compute the first page of slash.
				Session::roll_next();

				// ensure the last page of offence is processed.
				// (offence is processed in reverse order of pages)
				assert_eq!(
					staking_events_since_last_call().as_slice(),
					vec![Event::SlashComputed {
						offence_era: 2,
						slash_era: 5,
						offender: 11,
						page: expected_page_count - 1
					},]
				);

				// offender is removed from offence queue
				assert_eq!(OffenceQueue::<T>::get(2, 11), None);

				// offence era is removed from queue.
				assert_eq!(OffenceQueueEras::<T>::get(), None);

				// this offence is not completely processed yet, so it should be in processing.
				assert_eq!(
					ProcessingOffence::<T>::get(),
					Some((
						2,
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

				// unapplied slashes for era 5.
				let slashes = UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>();

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
				Session::roll_next();

				// offence queue still empty
				assert_eq!(OffenceQueue::<T>::get(2, 11), None);
				assert_eq!(OffenceQueueEras::<T>::get(), None);

				// processing offence points to next page.
				assert_eq!(
					ProcessingOffence::<T>::get(),
					Some((
						2,
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
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 2);

				// ensure the last page of offence is processed.
				// (offence is processed in reverse order of pages)
				assert_eq!(
					staking_events_since_last_call(),
					vec![Event::SlashComputed {
						offence_era: 2,
						slash_era: 5,
						offender: 11,
						page: expected_page_count - 2
					},]
				);

				// Checkpoint 3: advancing to two more blocks will complete the processing of the
				// reported offence
				Session::roll_next();
				Session::roll_next();

				// no processing offence.
				assert!(ProcessingOffence::<T>::get().is_none());
				// total of 4 unapplied slash.
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 4);

				// Checkpoint 4: lets verify the application of slashes in multiple blocks.
				// advance to era 4.
				Session::roll_until_active_era(5);
				// slashes are not applied just yet. From next blocks, they will be applied.
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 4);

				// advance to next block.
				Session::roll_next();
				// 1 slash is applied.
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 3);

				// advance two blocks.
				Session::roll_next();
				Session::roll_next();
				// 2 more slashes are applied.
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 1);

				// advance one more block.
				Session::roll_next();
				// all slashes are applied.
				assert_eq!(UnappliedSlashes::<T>::iter_prefix(&5).collect::<Vec<_>>().len(), 0);

				// ensure all stakers are slashed correctly.
				assert_eq!(asset::staked::<T>(&11), 1000 - 100);

				for i in 0..200 {
					let original_stake = 1000 + i as Balance;
					let expected_slash = Perbill::from_percent(10) * original_stake;
					assert_eq!(asset::staked::<T>(&(1000 + i)), original_stake - expected_slash);
				}
			})
	}

	#[test]
	fn offence_discarded_correctly() {
		ExtBuilder::default().slash_defer_duration(3).build_and_execute(|| {
			Session::roll_until_active_era(2);
			let _ = staking_events_since_last_call();

			// Scenario 1: 11 commits an offence in era 2.
			add_slash(11);

			// offence is queued, not processed yet.
			let queued_offence_one = OffenceQueue::<T>::get(2, 11).unwrap();
			assert_eq!(queued_offence_one.slash_fraction, Perbill::from_percent(10));
			assert_eq!(queued_offence_one.prior_slash_fraction, Perbill::zero());
			assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![2]);

			// Scenario 1A: 11 commits a second offence in era 2 with **lower** slash fraction than
			// the previous offence.
			add_slash_with_percent(11, 5);

			// the second offence is discarded. No change in the queue.
			assert_eq!(OffenceQueue::<T>::get(2, 11).unwrap(), queued_offence_one);

			// Scenario 1B: 11 commits a second offence in era 2 with **higher** slash fraction than
			// the previous offence.
			add_slash_with_percent(11, 15);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::OffenceReported {
						offence_era: 2,
						validator: 11,
						fraction: Perbill::from_percent(10)
					},
					Event::OffenceReported {
						offence_era: 2,
						validator: 11,
						fraction: Perbill::from_percent(5)
					},
					Event::OffenceReported {
						offence_era: 2,
						validator: 11,
						fraction: Perbill::from_percent(15)
					}
				]
			);

			// the second offence overwrites the first offence.
			let overwritten_offence = OffenceQueue::<T>::get(2, 11).unwrap();
			assert!(overwritten_offence.slash_fraction > queued_offence_one.slash_fraction);
			assert_eq!(overwritten_offence.slash_fraction, Perbill::from_percent(15));
			assert_eq!(overwritten_offence.prior_slash_fraction, Perbill::zero());
			assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![2]);

			// Scenario 2: 11 commits another offence in era 2, but after the previous offence is
			// processed.
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::SlashComputed { offence_era: 2, slash_era: 5, offender: 11, page: 0 }]
			);

			assert!(OffenceQueue::<T>::get(2, 11).is_none());
			assert!(OffenceQueueEras::<T>::get().is_none());
			// unapplied slash is created for the offence.
			assert!(UnappliedSlashes::<T>::contains_key(2 + 3, (11, Perbill::from_percent(15), 0)));

			// Scenario 2A: offence has **lower** slash fraction than the previous offence.
			add_slash_with_percent(11, 14);
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::OffenceReported {
					offence_era: 2,
					validator: 11,
					fraction: Perbill::from_percent(14)
				},]
			);

			// offence is discarded.
			assert!(OffenceQueue::<T>::get(2, 11).is_none());
			assert!(OffenceQueueEras::<T>::get().is_none());

			// Scenario 2B: offence has **higher** slash fraction than the previous offence.
			add_slash_with_percent(11, 16);
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::OffenceReported {
					offence_era: 2,
					validator: 11,
					fraction: Perbill::from_percent(16)
				},]
			);

			// process offence
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::SlashComputed { offence_era: 2, slash_era: 5, offender: 11, page: 0 }]
			);

			// there are now two slash records for 11, for era 5, with the newer one only slashing
			// the diff between slash fractions of 16 and 15.
			let slash_one =
				UnappliedSlashes::<T>::get(2 + 3, (11, Perbill::from_percent(15), 0)).unwrap();
			let slash_two =
				UnappliedSlashes::<T>::get(2 + 3, (11, Perbill::from_percent(16), 0)).unwrap();
			assert!(slash_one.own > slash_two.own);
		});
	}

	#[test]
	fn offence_eras_queued_correctly() {
		ExtBuilder::default().build_and_execute(|| {
			// 11 and 21 are validators.
			assert_eq!(Staking::status(&11).unwrap(), StakerStatus::Validator);
			assert_eq!(Staking::status(&21).unwrap(), StakerStatus::Validator);

			Session::roll_until_active_era(2);

			// 11 and 21 commits offence in era 2.
			add_slash_in_era(11, 2);
			add_slash_in_era(21, 2);

			// 11 and 21 commits offence in era 1 but reported after the era 2 offence.
			add_slash_in_era(11, 1);
			add_slash_in_era(21, 1);

			// queued offence eras are sorted.
			assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![1, 2]);

			// next two blocks, the offence in era 1 is processed.
			Session::roll_next();
			Session::roll_next();

			// only era 2 is left in the queue.
			assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![2]);

			// next block, the offence in era 2 is processed.
			Session::roll_next();

			// era still exist in the queue.
			assert_eq!(OffenceQueueEras::<T>::get().unwrap(), vec![2]);

			// next block, the era 2 is processed.
			Session::roll_next();

			// queue is empty.
			assert_eq!(OffenceQueueEras::<T>::get(), None);
		});
	}

	#[test]
	fn non_deferred_slash_applied_instantly() {
		ExtBuilder::default().build_and_execute(|| {
			Session::roll_until_active_era(2);

			let validator_stake = asset::staked::<T>(&11);
			let slash_fraction = Perbill::from_percent(10);
			let expected_slash = slash_fraction * validator_stake;
			let _ = staking_events_since_last_call();

			// report an offence for 11 in era 1.
			add_slash_in_era_with_value(11, 1, slash_fraction);

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
			Session::roll_next();

			// ensure slash is computed and applied.
			assert_eq!(
				staking_events_since_last_call().as_slice(),
				vec![
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: expected_slash },
					// this is the nominator of 11.
					Event::Slashed { staker: 101, amount: 25 },
				]
			);

			// ensure validator is slashed.
			assert_eq!(asset::staked::<T>(&11), validator_stake - expected_slash);
		});
	}

	#[test]
	fn validator_with_no_exposure_slashed() {
		ExtBuilder::default().build_and_execute(|| {
			let validator_stake = asset::staked::<T>(&11);
			let slash_fraction = Perbill::from_percent(10);
			let expected_slash = slash_fraction * validator_stake;

			// only 101 nominates 11, lets remove them.
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![21]));

			Session::roll_until_active_era(2);

			// ensure validator has no exposure.
			assert_eq!(ErasStakersOverview::<T>::get(2, 11).unwrap().page_count, 0,);

			// clear events
			let _ = staking_events_since_last_call();

			// report an offence for 11.
			add_slash_with_percent(11, 10);
			Session::roll_next();

			// ensure validator is slashed.
			assert_eq!(asset::staked::<T>(&11), validator_stake - expected_slash);
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

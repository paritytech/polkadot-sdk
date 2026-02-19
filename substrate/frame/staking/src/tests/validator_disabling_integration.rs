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
fn reenable_lower_offenders() {
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
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(10)]);
			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(20)]);

			// it does NOT affect the nominator.
			assert_eq!(Staking::nominators(101).unwrap().targets, vec![11, 21]);

			// both validators should be disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// offence with a higher slash
			on_offence_now(&[offence_from(31, None)], &[Perbill::from_percent(50)]);

			// First offender is no longer disabled
			assert!(!is_disabled(11));
			// Mid offender is still disabled
			assert!(is_disabled(21));
			// New offender is disabled
			assert!(is_disabled(31));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::SlashReported {
						validator: 11,
						fraction: Perbill::from_percent(10),
						slash_era: 1
					},
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 12 },
					Event::SlashReported {
						validator: 21,
						fraction: Perbill::from_percent(20),
						slash_era: 1
					},
					Event::Slashed { staker: 21, amount: 200 },
					Event::Slashed { staker: 101, amount: 75 },
					Event::SlashReported {
						validator: 31,
						fraction: Perbill::from_percent(50),
						slash_era: 1
					},
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
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(50)]);
			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(50)]);

			// both validators should be disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// offence with a minor slash
			on_offence_now(&[offence_from(31, None)], &[Perbill::from_percent(10)]);

			// First and second offenders are still disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));
			// New offender is not disabled as limit is reached and his prio is lower
			assert!(!is_disabled(31));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::SlashReported {
						validator: 11,
						fraction: Perbill::from_percent(50),
						slash_era: 1
					},
					Event::Slashed { staker: 11, amount: 500 },
					Event::Slashed { staker: 101, amount: 62 },
					Event::SlashReported {
						validator: 21,
						fraction: Perbill::from_percent(50),
						slash_era: 1
					},
					Event::Slashed { staker: 21, amount: 500 },
					Event::Slashed { staker: 101, amount: 187 },
					Event::SlashReported {
						validator: 31,
						fraction: Perbill::from_percent(10),
						slash_era: 1
					},
					Event::Slashed { staker: 31, amount: 50 },
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

#[test]
fn clear_disabled_only_on_era_change() {
	ExtBuilder::default()
		.validator_count(7)
		.set_status(41, StakerStatus::Validator)
		.set_status(51, StakerStatus::Validator)
		.set_status(201, StakerStatus::Validator)
		.set_status(202, StakerStatus::Validator)
		.session_per_era(3)
		.build_and_execute(|| {
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41, 51, 201, 202]);

			// offence with a major slash
			on_offence_now(
				&[offence_from(11, None), offence_from(21, None)],
				&[Perbill::from_percent(50), Perbill::from_percent(50)],
			);

			// both validators should be disabled
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// progress session and check if disablement is retained
			start_session(2);
			assert!(is_disabled(11));
			assert!(is_disabled(21));

			// progress era (3 sessions per era) and clear disablement
			start_session(3);
			assert!(!is_disabled(11));
			assert!(!is_disabled(21));
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

			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(0)]);

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
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(0)], 1);

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
			);

			// the validator doesn't get disabled again
			assert!(Validators::<Test>::iter().any(|(stash, _)| stash == 11));
			assert!(!is_disabled(11));
			// and we are still not forcing a new era
			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
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
			on_offence_now(&[offence_from(11, None)], &[Perbill::zero()]);

			// it does NOT affect the nominator.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// offence that slashes 25% of the bond
			on_offence_now(&[offence_from(21, None)], &[Perbill::from_percent(25)]);

			// it DOES NOT affect the nominator.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::SlashReported {
						validator: 11,
						fraction: Perbill::from_percent(0),
						slash_era: 1
					},
					Event::SlashReported {
						validator: 21,
						fraction: Perbill::from_percent(25),
						slash_era: 1
					},
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
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::zero()], now);

			// nomination remains untouched.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// first validator is disabled
			assert!(is_disabled(11));

			// --- Slash without disabling (because limit reached) ---
			// offence that slashes 50% of the bond (setup for next slash)
			on_offence_in_era(&[offence_from(11, None)], &[Perbill::from_percent(50)], now);

			// offence that slashes 25% of the bond but does not disable
			on_offence_in_era(&[offence_from(21, None)], &[Perbill::from_percent(25)], now);

			// nomination remains untouched.
			assert_eq!(Nominators::<Test>::get(101).unwrap().targets, vec![11, 21]);

			// second validator is slashed but not disabled
			assert!(!is_disabled(21));
			assert!(is_disabled(11));

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::StakersElected,
					Event::EraPaid { era_index: 0, validator_payout: 11075, remainder: 33225 },
					Event::SlashReported {
						validator: 11,
						fraction: Perbill::from_percent(0),
						slash_era: 1
					},
					Event::SlashReported {
						validator: 11,
						fraction: Perbill::from_percent(50),
						slash_era: 1
					},
					Event::Slashed { staker: 11, amount: 500 },
					Event::Slashed { staker: 101, amount: 62 },
					Event::SlashReported {
						validator: 21,
						fraction: Perbill::from_percent(25),
						slash_era: 1
					},
					Event::Slashed { staker: 21, amount: 250 },
					Event::Slashed { staker: 101, amount: 94 }
				]
			);

			assert_eq!(
				session_events(),
				vec![
					SessionEvent::NewSession { session_index: 1 },
					SessionEvent::NewQueued,
					SessionEvent::NewSession { session_index: 2 },
					SessionEvent::NewSession { session_index: 3 },
					SessionEvent::ValidatorDisabled { validator: 11 }
				]
			);
		});
}

#[test]
fn offence_threshold_doesnt_force_new_era() {
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

			// we have 4 validators and an offending validator threshold of 1,
			// even if two validators commit an offence a new era should not be forced
			on_offence_now(&[offence_from(11, None)], &[Perbill::from_percent(50)]);

			// 11 should be disabled because the byzantine threshold is 1
			assert!(is_disabled(11));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

			on_offence_now(&[offence_from(21, None)], &[Perbill::zero()]);

			// 21 should not be disabled because the number of disabled validators will be above
			// the byzantine threshold
			assert!(!is_disabled(21));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);

			on_offence_now(&[offence_from(31, None)], &[Perbill::zero()]);

			// same for 31
			assert!(!is_disabled(31));

			assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
		});
}

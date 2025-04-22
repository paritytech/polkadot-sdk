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

use crate::session_rotation::Eras;

use super::*;

#[test]
fn forcing_force_none() {
	ExtBuilder::default().build_and_execute(|| {
		ForceEra::<T>::put(Forcing::ForceNone);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 4, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 6, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 1 }]
		);
	});
}

#[test]
fn forcing_no_forcing_default() {
	ExtBuilder::default().build_and_execute(|| {
		// default value, setting it again just for read-ability.
		ForceEra::<T>::put(Forcing::NotForcing);

		Session::roll_until_active_era(2);
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
	});
}

#[test]
fn forcing_force_always() {
	ExtBuilder::default()
		.session_per_era(6)
		.no_flush_events()
		.build_and_execute(|| {
			// initial events thus far, without `ForceAlways` set.
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

			// but with it set..
			ForceEra::<T>::put(Forcing::ForceAlways);

			Session::roll_until_active_era(2);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					// we immediately plan a new era as soon as the first session report comes in
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					// by now it is given to mock session, and is buffered
					Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 7500, remainder: 7500 },
					// and by now it is activated. Note how the validator payout is less, since the
					// era duration is less. Note that we immediately plan the next era as well.
					Event::SessionRotated { starting_session: 9, active_era: 2, planned_era: 3 }
				]
			);
		});
}

#[test]
fn forcing_force_new() {
	ExtBuilder::default()
		.session_per_era(6)
		.no_flush_events()
		.build_and_execute(|| {
			// initial events thus far, without `ForceAlways` set.
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

			// but with it set..
			ForceEra::<T>::put(Forcing::ForceNew);

			// one era happens quicker
			Session::roll_until_active_era(2);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					// we immediately plan a new era as soon as the first session report comes in
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					// by now it is given to mock session, and is buffered
					Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 7500, remainder: 7500 },
					// and by now it is activated. Note how the validator payout is less, since the
					// era duration is less.
					Event::SessionRotated { starting_session: 9, active_era: 2, planned_era: 2 }
				]
			);

			// And the next era goes back to normal.
			Session::roll_until_active_era(3);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 10, active_era: 2, planned_era: 2 },
					Event::SessionRotated { starting_session: 11, active_era: 2, planned_era: 2 },
					Event::SessionRotated { starting_session: 12, active_era: 2, planned_era: 2 },
					Event::SessionRotated { starting_session: 13, active_era: 2, planned_era: 3 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 14, active_era: 2, planned_era: 3 },
					Event::EraPaid { era_index: 2, validator_payout: 15000, remainder: 15000 },
					Event::SessionRotated { starting_session: 15, active_era: 3, planned_era: 3 }
				]
			);
		});
}

#[test]
#[should_panic]
fn activation_timestamp_when_no_planned_era() {
	// maybe not needed, as we have the id check
	todo!("what if we receive an activation timestamp when there is no planned era?");
}

#[test]
#[should_panic]
fn activation_timestamp_when_era_planning_not_complete() {
	// maybe not needed, as we have the id check
	todo!("what if we receive an activation timestamp when the era planning (election) is not complete?");
}

#[test]
#[should_panic]
fn max_era_duration_safety_guard() {
	todo!("a safety guard that ensures that there is an upper bound on how long an era duration can be. Should prevent us from parabolic inflation in case of some crazy bug.");
}

#[test]
fn era_cleanup_history_depth_works() {
	// TODO: try-state for it
	ExtBuilder::default().build_and_execute(|| {
		// when we go forward to `HistoryDepth - 1`
		assert_eq!(active_era(), 1);

		Session::roll_until_active_era(HistoryDepth::get() - 1);
		assert!(matches!(
			&staking_events_since_last_call()[..],
			&[
				..,
				Event::SessionRotated { starting_session: 236, active_era: 78, planned_era: 79 },
				Event::EraPaid { era_index: 78, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 237, active_era: 79, planned_era: 79 }
			]
		));
		assert_ok!(Eras::<T>::era_present(1));
		assert_ok!(Eras::<T>::era_present(2));
		// ..
		assert_ok!(Eras::<T>::era_present(HistoryDepth::get() - 1));

		Session::roll_until_active_era(HistoryDepth::get());
		assert_ok!(Eras::<T>::era_present(1));
		assert_ok!(Eras::<T>::era_present(2));
		// ..
		assert_ok!(Eras::<T>::era_present(HistoryDepth::get()));

		// then first era info should have been deleted
		Session::roll_until_active_era(HistoryDepth::get() + 1);
		assert_ok!(Eras::<T>::era_present(1));
		assert_ok!(Eras::<T>::era_present(2));
		// ..
		assert_ok!(Eras::<T>::era_present(HistoryDepth::get() + 1));

		Session::roll_until_active_era(HistoryDepth::get() + 2);
		assert_ok!(Eras::<T>::era_absent(1));
		assert_ok!(Eras::<T>::era_present(2));
		assert_ok!(Eras::<T>::era_present(3));
		// ..
		assert_ok!(Eras::<T>::era_present(HistoryDepth::get() + 2));
	});
}

mod inflation {
	use super::*;

	#[test]
	fn max_staked_rewards_default_not_set_works() {
		ExtBuilder::default().build_and_execute(|| {
			let default_stakers_payout = validator_payout_for(time_per_era());
			assert!(default_stakers_payout > 0);

			assert_eq!(<MaxStakedRewards<Test>>::get(), None);

			Session::roll_until_active_era(2);

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

			// the final stakers reward is the same as the reward before applied the cap.
			assert_eq!(ErasValidatorReward::<Test>::get(0).unwrap(), default_stakers_payout);
		})
	}

	#[test]
	fn max_staked_rewards_default_equal_100() {
		ExtBuilder::default().build_and_execute(|| {
			let default_stakers_payout = validator_payout_for(time_per_era());
			assert!(default_stakers_payout > 0);
			<MaxStakedRewards<Test>>::set(Some(Percent::from_parts(100)));

			Session::roll_until_active_era(2);

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

			// the final stakers reward is the same as the reward before applied the cap.
			assert_eq!(ErasValidatorReward::<Test>::get(0).unwrap(), default_stakers_payout);
		});
	}

	#[test]
	fn max_staked_rewards_works() {
		ExtBuilder::default().nominate(true).build_and_execute(|| {
			// sets new max staked rewards through set_staking_configs.
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(Percent::from_percent(10)),
			));

			assert_eq!(<MaxStakedRewards<Test>>::get(), Some(Percent::from_percent(10)));

			// check validators account state.
			assert_eq!(Session::validators().len(), 2);
			assert!(Session::validators().contains(&11) & Session::validators().contains(&21));

			// balance of the mock treasury account is 0
			assert_eq!(RewardRemainderUnbalanced::get(), 0);

			Session::roll_until_active_era(2);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 4, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 1500, remainder: 13500 },
					Event::SessionRotated { starting_session: 6, active_era: 2, planned_era: 2 }
				]
			);

			let treasury_payout = RewardRemainderUnbalanced::get();
			let validators_payout = ErasValidatorReward::<Test>::get(1).unwrap();
			let total_payout = treasury_payout + validators_payout;

			// total payout is the same
			assert_eq!(total_payout, total_payout_for(time_per_era()));
			// validators get only 10%
			assert_eq!(validators_payout, Percent::from_percent(10) * total_payout);
			// treasury gets 90%
			assert_eq!(treasury_payout, Percent::from_percent(90) * total_payout);
		})
	}
}

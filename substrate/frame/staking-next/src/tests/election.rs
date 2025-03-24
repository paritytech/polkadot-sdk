use super::*;

#[test]
fn election_provider_returning_err() {
	todo!();
}

#[test]
fn planning_era_offset_less_works() {
	// same as `basic_setup_sessions_per_era`, but notice how `PagedElectionProceeded` happens
	// one session later, and planning era is incremented one session later
	ExtBuilder::default()
		.session_per_era(6)
		.planning_era_offset(0)
		.no_flush_events()
		.build_and_execute(|| {
			// this essentially makes the session duration 7, because the mock session will buffer
			// for one session before activating the era.
			assert_eq!(Session::current_index(), 7);
			assert_eq!(active_era(), 1);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 1, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 2, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 3, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 4, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 5, active_era: 0, planned_era: 1 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 6, active_era: 0, planned_era: 1 },
					Event::EraPaid { era_index: 0, validator_payout: 17500, remainder: 17500 },
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 1 }
				]
			);

			Session::roll_until_active_era(2);
			assert_eq!(Session::current_index(), 14);
			assert_eq!(active_era(), 2);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 9, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 10, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 11, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 12, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 13, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 17500, remainder: 17500 },
					Event::SessionRotated { starting_session: 14, active_era: 2, planned_era: 2 }
				]
			);
		});
}

#[test]
fn planning_era_offset_more_works() {
	ExtBuilder::default()
		.session_per_era(6)
		.planning_era_offset(2)
		.no_flush_events()
		.build_and_execute(|| {
			// This effectively makes the era one session shorter.
			assert_eq!(Session::current_index(), 5);
			assert_eq!(active_era(), 1);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 1, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 2, active_era: 0, planned_era: 0 },
					Event::SessionRotated { starting_session: 3, active_era: 0, planned_era: 1 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 4, active_era: 0, planned_era: 1 },
					Event::EraPaid { era_index: 0, validator_payout: 12500, remainder: 12500 },
					Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 1 }
				]
			);

			Session::roll_until_active_era(2);
			assert_eq!(Session::current_index(), 10);
			assert_eq!(active_era(), 2);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 6, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) },
					Event::SessionRotated { starting_session: 9, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 12500, remainder: 12500 },
					Event::SessionRotated { starting_session: 10, active_era: 2, planned_era: 2 }
				]
			);
		});
}

#[test]
fn less_than_needed_candidates_works() {
	ExtBuilder::default()
		.minimum_validator_count(1)
		.validator_count(4)
		.nominate(false)
		.build_and_execute(|| {
			assert_eq_uvec!(Session::validators(), vec![31, 21, 11]);
			Session::roll_until_active_era(2);

			// Previous set is selected.
			assert_eq_uvec!(Session::validators(), vec![31, 21, 11]);

			// Only has self votes.
			assert!(ErasStakersPaged::<Test>::iter_prefix_values((active_era(),))
				.all(|exposure| exposure.others.is_empty()));
		});
}

#[test]
fn no_candidate_emergency_condition() {
	ExtBuilder::default()
		.validator_count(15)
		.set_status(41, StakerStatus::Validator)
		.nominate(false)
		.planning_era_offset(0)
		.build_and_execute(|| {
			// initial validators
			assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41]);
			let prefs = ValidatorPrefs { commission: Perbill::one(), ..Default::default() };
			Validators::<Test>::insert(11, prefs.clone());

			// set the minimum validator count.
			MinimumValidatorCount::<Test>::put(11);

			// try to go to next era
			Session::roll_until_active_era(2);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 1 },
					Event::SessionRotated { starting_session: 6, active_era: 1, planned_era: 2 },
					Event::PagedElectionProceeded { page: 0, result: Ok(4) },
					Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 2 },
					Event::EraPaid { era_index: 1, validator_payout: 10000, remainder: 10000 },
					Event::SessionRotated { starting_session: 8, active_era: 2, planned_era: 2 }
				]
			);

			todo!();

			// // in fact chill 11
			// let res = Staking::chill(RuntimeOrigin::signed(11));
			// assert_ok!(res);

			// let current_era = CurrentEra::<Test>::get();

			// // try trigger new era
			// mock::roll_to_block(21);
			// assert_eq!(*staking_events().last().unwrap(), Event::StakingElectionFailed);
			// // No new era is created
			// assert_eq!(current_era, CurrentEra::<Test>::get());

			// // Go to far further session to see if validator have changed
			// mock::roll_to_block(100);

			// // Previous ones are elected. chill is not effective in active era (as era hasn't
			// // changed)
			// assert_eq_uvec!(Session::validators(), vec![11, 21, 31, 41]);
			// // The chill is still pending.
			// assert!(!Validators::<Test>::contains_key(11));
			// // No new era is created.
			// assert_eq!(current_era, CurrentEra::<Test>::get());
		});
}

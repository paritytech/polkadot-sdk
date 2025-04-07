use super::*;

#[test]
fn unexpected_activation_timestamp() {
	todo!()
}

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
	todo!()
}

#[test]
fn forcing_force_always() {
	todo!()
}

#[test]
fn forcing_force_new() {
	todo!()
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

mod election_provider {
	use super::*;

	#[test]
	fn new_era_elects_correct_number_of_validators() {
		ExtBuilder::default().nominate(true).validator_count(1).build_and_execute(|| {
			assert_eq!(ValidatorCount::<Test>::get(), 1);
			assert_eq!(session_validators().len(), 1);
		})
	}
}

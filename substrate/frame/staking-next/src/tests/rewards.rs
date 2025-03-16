use super::*;

#[test]
fn rewards_with_nominator_should_work() {
	ExtBuilder::default().nominate(true).session_per_era(3).build_and_execute(|| {
		let init_balance_11 = asset::total_balance::<Test>(&11);
		let init_balance_21 = asset::total_balance::<Test>(&21);
		let init_balance_101 = asset::total_balance::<Test>(&101);

		// Set payees
		Payee::<Test>::insert(11, RewardDestination::Account(11));
		Payee::<Test>::insert(21, RewardDestination::Account(21));
		Payee::<Test>::insert(101, RewardDestination::Account(101));

		Pallet::<Test>::reward_by_ids(vec![(11, 50)]);
		Pallet::<Test>::reward_by_ids(vec![(11, 50)]);
		// This is the second validator of the current elected set.
		Pallet::<Test>::reward_by_ids(vec![(21, 50)]);

		// Compute total payout now for whole duration of the session.
		let validator_payout_0 = validator_payout_for(time_per_era());
		let maximum_payout = total_payout_for(time_per_era());

		assert_eq_uvec!(Session::validators(), vec![11, 21]);

		assert_eq!(asset::total_balance::<Test>(&11), init_balance_11);
		assert_eq!(asset::total_balance::<Test>(&21), init_balance_21);
		assert_eq!(asset::total_balance::<Test>(&101), init_balance_101);
		assert_eq!(
			ErasRewardPoints::<Test>::get(active_era()),
			EraRewardPoints {
				total: 50 * 3,
				individual: vec![(11, 100), (21, 50)].into_iter().collect(),
			}
		);
		let part_for_11 = Perbill::from_rational::<u32>(1000, 1250);
		let part_for_21 = Perbill::from_rational::<u32>(1000, 1250);
		let part_for_101_from_11 = Perbill::from_rational::<u32>(250, 1250);
		let part_for_101_from_21 = Perbill::from_rational::<u32>(250, 1250);

		Session::roll_until_active_era(2);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::SessionRotated { starting_session: 4, active_era: 1, planned_era: 2 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 2 },
				Event::EraPaid {
					era_index: 1,
					validator_payout: validator_payout_0,
					remainder: maximum_payout - validator_payout_0
				},
				Event::SessionRotated { starting_session: 6, active_era: 2, planned_era: 2 }
			]
		);
		assert_eq!(mock::RewardRemainderUnbalanced::get(), maximum_payout - validator_payout_0);

		// make note of total issuance before rewards.
		let pre_issuance = asset::total_issuance::<Test>();

		mock::make_all_reward_payment(1);
		assert_eq!(
			mock::staking_events_since_last_call(),
			vec![
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: None },
				Event::Rewarded { stash: 11, dest: RewardDestination::Account(11), amount: 4000 },
				Event::Rewarded { stash: 101, dest: RewardDestination::Account(101), amount: 1000 },
				Event::PayoutStarted { era_index: 1, validator_stash: 21, page: 0, next: None },
				Event::Rewarded { stash: 21, dest: RewardDestination::Account(21), amount: 2000 },
				Event::Rewarded { stash: 101, dest: RewardDestination::Account(101), amount: 500 }
			]
		);

		// total issuance should have increased
		let post_issuance = asset::total_issuance::<Test>();
		assert_eq!(post_issuance, pre_issuance + validator_payout_0);

		assert_eq_error_rate!(
			asset::total_balance::<Test>(&11),
			init_balance_11 + part_for_11 * validator_payout_0 * 2 / 3,
			2,
		);
		assert_eq_error_rate!(
			asset::total_balance::<Test>(&21),
			init_balance_21 + part_for_21 * validator_payout_0 * 1 / 3,
			2,
		);
		assert_eq_error_rate!(
			asset::total_balance::<Test>(&101),
			init_balance_101 +
				part_for_101_from_11 * validator_payout_0 * 2 / 3 +
				part_for_101_from_21 * validator_payout_0 * 1 / 3,
			2
		);

		assert_eq_uvec!(Session::validators(), vec![11, 21]);
		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);

		// Compute total payout now for whole duration as other parameter won't change
		let total_payout_1 = validator_payout_for(time_per_era());

		Session::roll_until_active_era(3);

		assert_eq!(
			mock::RewardRemainderUnbalanced::get(),
			maximum_payout * 2 - validator_payout_0 - total_payout_1,
		);
		assert_eq!(
			mock::staking_events_since_last_call(),
			vec![
				Event::SessionRotated { starting_session: 7, active_era: 2, planned_era: 3 },
				Event::PagedElectionProceeded { page: 0, result: Ok(2) },
				Event::SessionRotated { starting_session: 8, active_era: 2, planned_era: 3 },
				Event::EraPaid { era_index: 2, validator_payout: 7500, remainder: 7500 },
				Event::SessionRotated { starting_session: 9, active_era: 3, planned_era: 3 }
			]
		);

		mock::make_all_reward_payment(2);
		assert_eq!(asset::total_issuance::<Test>(), post_issuance + total_payout_1);

		assert_eq_error_rate!(
			asset::total_balance::<Test>(&11),
			init_balance_11 + part_for_11 * (validator_payout_0 * 2 / 3 + total_payout_1),
			2,
		);
		assert_eq_error_rate!(
			asset::total_balance::<Test>(&21),
			init_balance_21 + part_for_21 * validator_payout_0 * 1 / 3,
			2,
		);
		assert_eq_error_rate!(
			asset::total_balance::<Test>(&101),
			init_balance_101 +
				part_for_101_from_11 * (validator_payout_0 * 2 / 3 + total_payout_1) +
				part_for_101_from_21 * validator_payout_0 * 1 / 3,
			2
		);
	});
}

#[test]
fn rewards_no_nominator_should_work() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq_uvec!(Session::validators(), vec![11, 21]);

		// with no backers
		assert_eq!(
			era_exposures(1),
			vec![
				(11, Exposure { total: 1000, own: 1000, others: vec![] }),
				(21, Exposure { total: 1000, own: 1000, others: vec![] })
			]
		);

		// give them some points
		reward_all_elected();

		// go to next active era
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// payout era 1
		make_all_reward_payment(1);

		// payout works
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: None },
				Event::Rewarded { stash: 11, dest: RewardDestination::Staked, amount: 3750 },
				Event::PayoutStarted { era_index: 1, validator_stash: 21, page: 0, next: None },
				Event::Rewarded { stash: 21, dest: RewardDestination::Staked, amount: 3750 }
			]
		);
	});
}

#[test]
fn nominating_and_rewards_should_work() {
	ExtBuilder::default()
		.nominate(false)
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			// initial validators, note that 41 has more stake than 11
			assert_eq_uvec!(Session::validators(), vec![41, 21]);

			// bond two monitors, both favouring 11
			bond_nominator(1, 5000, vec![11, 41]);
			bond_virtual_nominator(3, 333, 5000, vec![11]);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::Bonded { stash: 1, amount: 5000 },
					Event::Bonded { stash: 3, amount: 5000 },
				]
			);

			// reward our two winning validators
			Pallet::<Test>::reward_by_ids(vec![(41, 1)]);
			Pallet::<Test>::reward_by_ids(vec![(21, 1)]);

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

			// 11 now has more votes
			assert_eq_uvec!(Session::validators(), vec![11, 41]);
			assert_eq!(ErasStakersPaged::<Test>::iter_prefix_values((active_era(),)).count(), 2);
			assert_eq!(
				Staking::eras_stakers(active_era(), &11),
				Exposure {
					total: 7500,
					own: 1000,
					others: vec![
						IndividualExposure { who: 1, value: 1500 },
						IndividualExposure { who: 3, value: 5000 }
					]
				}
			);
			assert_eq!(
				Staking::eras_stakers(active_era(), &41),
				Exposure {
					total: 7500,
					own: 4000,
					others: vec![IndividualExposure { who: 1, value: 3500 }]
				}
			);

			// payout era 1, in which 21 and 41 were validators with no nominators.
			mock::make_all_reward_payment(1);
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PayoutStarted { era_index: 1, validator_stash: 21, page: 0, next: None },
					Event::Rewarded { stash: 21, dest: RewardDestination::Staked, amount: 3750 },
					Event::PayoutStarted { era_index: 1, validator_stash: 41, page: 0, next: None },
					Event::Rewarded { stash: 41, dest: RewardDestination::Staked, amount: 3750 }
				]
			);

			reward_all_elected();
			Session::roll_until_active_era(3);
			// ignore session rotation events, we've seen them before.
			let _ = staking_events_since_last_call();

			// for era 2 we had a nominator too, who is rewarded.
			mock::make_all_reward_payment(2);

			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PayoutStarted { era_index: 2, validator_stash: 11, page: 0, next: None },
					Event::Rewarded { stash: 11, dest: RewardDestination::Staked, amount: 500 },
					Event::Rewarded { stash: 1, dest: RewardDestination::Stash, amount: 750 },
					Event::Rewarded {
						stash: 3,
						dest: RewardDestination::Account(333),
						amount: 2500
					},
					Event::PayoutStarted { era_index: 2, validator_stash: 41, page: 0, next: None },
					Event::Rewarded { stash: 41, dest: RewardDestination::Staked, amount: 2000 },
					Event::Rewarded { stash: 1, dest: RewardDestination::Stash, amount: 1750 }
				]
			);
		});
}

#[test]
fn reward_destination_staked() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// initial conditions
		assert!(Session::validators().contains(&11));
		assert_eq!(Staking::payee(11.into()), Some(RewardDestination::Staked));
		assert_eq!(asset::total_balance::<Test>(&11), 1001);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);

		// reward era 1 and payout at era 2
		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		mock::make_all_reward_payment(1);
		assert_eq!(ErasClaimedRewards::<Test>::get(1, &11), vec![0]);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: None },
				Event::Rewarded { stash: 11, dest: RewardDestination::Staked, amount: 7500 }
			]
		);

		// ledger must have been increased
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 8500,
				active: 8500,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);
		// balance also updated
		assert_eq!(asset::total_balance::<Test>(&11), 1001 + 7500);
	});
}

#[test]
fn reward_destination_stash() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// initial conditions
		assert!(Session::validators().contains(&11));
		assert_ok!(Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Stash));
		assert_eq!(asset::total_balance::<Test>(&11), 1001);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);

		// reward era 1 and payout at era 2
		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		mock::make_all_reward_payment(1);
		assert_eq!(ErasClaimedRewards::<Test>::get(1, &11), vec![0]);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: None },
				Event::Rewarded { stash: 11, dest: RewardDestination::Stash, amount: 7500 }
			]
		);

		// ledger same, balance increased
		assert_eq!(asset::total_balance::<Test>(&11), 1001 + 7500);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);
	});
}

#[test]
fn reward_destination_account() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// initial conditions
		assert!(Session::validators().contains(&11));
		assert_ok!(Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Account(7)));

		assert_eq!(asset::total_balance::<Test>(&11), 1001);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);

		// reward era 1 and payout at era 2
		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		mock::make_all_reward_payment(1);
		assert_eq!(ErasClaimedRewards::<Test>::get(1, &11), vec![0]);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				Event::PayoutStarted { era_index: 1, validator_stash: 11, page: 0, next: None },
				Event::Rewarded { stash: 11, dest: RewardDestination::Account(7), amount: 7500 }
			]
		);

		// balance and ledger the same, 7 is unded
		assert_eq!(asset::total_balance::<Test>(&11), 1001);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				legacy_claimed_rewards: Default::default(),
				unlocking: Default::default(),
			}
		);
		assert_eq!(asset::total_balance::<Test>(&7), 7500);
	});
}

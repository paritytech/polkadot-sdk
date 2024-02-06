use super::*;
use frame_election_provider_support::ElectionDataProvider;

#[test]
fn targets_2sec_block() {
	let mut validators = 1000;
	while <Test as Config>::WeightInfo::get_npos_targets(validators).all_lt(Weight::from_parts(
		2u64 * frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
		u64::MAX,
	)) {
		validators += 1;
	}

	println!("Can create a snapshot of {} validators in 2sec block", validators);
}

#[test]
fn voters_2sec_block() {
	// we assume a network only wants up to 1000 validators in most cases, thus having 2000
	// candidates is as high as it gets.
	let validators = 2000;
	let mut nominators = 1000;

	while <Test as Config>::WeightInfo::get_npos_voters(validators, nominators).all_lt(
		Weight::from_parts(
			2u64 * frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
			u64::MAX,
		),
	) {
		nominators += 1;
	}

	println!(
		"Can create a snapshot of {} nominators [{} validators, each 1 slashing] in 2sec block",
		nominators, validators
	);
}

#[test]
fn set_minimum_active_stake_is_correct() {
	ExtBuilder::default()
		.nominate(false)
		.add_staker(61, 61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21]))
		.add_staker(71, 71, 10, StakerStatus::<AccountId>::Nominator(vec![21]))
		.add_staker(81, 81, 50, StakerStatus::<AccountId>::Nominator(vec![21]))
		.build_and_execute(|| {
			// default bounds are unbounded.
			assert_ok!(<Staking as ElectionDataProvider>::electing_voters(
				DataProviderBounds::default()
			));
			assert_eq!(MinimumActiveStake::<Test>::get(), 10);

			// remove staker with lower bond by limiting the number of voters and check
			// `MinimumActiveStake` again after electing voters.
			let bounds = ElectionBoundsBuilder::default().voters_count(5.into()).build();
			assert_ok!(<Staking as ElectionDataProvider>::electing_voters(bounds.voters));
			assert_eq!(MinimumActiveStake::<Test>::get(), 50);
		});
}

#[test]
fn set_minimum_active_stake_lower_bond_works() {
	// if there are no voters, minimum active stake is zero (should not happen).
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// default bounds are unbounded.
		assert_ok!(<Staking as ElectionDataProvider>::electing_voters(
			DataProviderBounds::default()
		));
		assert_eq!(<Test as Config>::VoterList::count(), 0);
		assert_eq!(MinimumActiveStake::<Test>::get(), 0);
	});

	// lower non-zero active stake below `MinNominatorBond` is the minimum active stake if
	// it is selected as part of the npos voters.
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		assert_eq!(MinNominatorBond::<Test>::get(), 1);
		assert_eq!(<Test as Config>::VoterList::count(), 4);

		assert_ok!(Staking::bond(RuntimeOrigin::signed(4), 5, RewardDestination::Staked,));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(4), vec![1]));
		assert_eq!(<Test as Config>::VoterList::count(), 5);

		let voters_before =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default())
				.unwrap();
		assert_eq!(MinimumActiveStake::<Test>::get(), 5);

		// update minimum nominator bond.
		MinNominatorBond::<Test>::set(10);
		assert_eq!(MinNominatorBond::<Test>::get(), 10);
		// voter list still considers nominator 4 for voting, even though its active stake is
		// lower than `MinNominatorBond`.
		assert_eq!(<Test as Config>::VoterList::count(), 5);

		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default())
				.unwrap();
		assert_eq!(voters_before, voters);

		// minimum active stake is lower than `MinNominatorBond`.
		assert_eq!(MinimumActiveStake::<Test>::get(), 5);
	});
}

#[test]
fn set_minimum_active_bond_corrupt_state() {
	ExtBuilder::default()
		.has_stakers(true)
		.nominate(true)
		.add_staker(61, 61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21]))
		.build_and_execute(|| {
			assert_eq!(Staking::weight_of(&101), 500);
			let voters =
				<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default())
					.unwrap();
			assert_eq!(voters.len(), 5);
			assert_eq!(MinimumActiveStake::<Test>::get(), 500);

			assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 200));
			start_active_era(10);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 100));
			start_active_era(20);

			// corrupt ledger state by lowering max unlocking chunks bounds.
			MaxUnlockingChunks::set(1);

			let voters =
				<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default())
					.unwrap();
			// number of returned voters decreases since ledger entry of stash 101 is now
			// corrupt.
			assert_eq!(voters.len(), 4);
			// minimum active stake does not take into consideration the corrupt entry.
			assert_eq!(MinimumActiveStake::<Test>::get(), 2_000);

			// voter weight of corrupted ledger entry is 0.
			assert_eq!(Staking::weight_of(&101), 0);

			// reset max unlocking chunks for try_state to pass.
			MaxUnlockingChunks::set(32);
		})
}

#[test]
fn voters_include_self_vote() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// default bounds are unbounded.
		assert!(<Validators<Test>>::iter().map(|(x, _)| x).all(|v| Staking::electing_voters(
			DataProviderBounds::default()
		)
		.unwrap()
		.into_iter()
		.any(|(w, _, t)| { v == w && t[0] == w })))
	})
}

// Tests the criteria that in `ElectionDataProvider::voters` function, we try to get at most
// `maybe_max_len` voters, and if some of them end up being skipped, we iterate at most `2 *
// maybe_max_len`.
#[test]
#[should_panic]
fn only_iterates_max_2_times_max_allowed_len() {
	ExtBuilder::default()
		.nominate(false)
		// the best way to invalidate a bunch of nominators is to have them nominate a lot of
		// ppl, but then lower the MaxNomination limit.
		.add_staker(61, 61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
		.add_staker(71, 71, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
		.add_staker(81, 81, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
		.build_and_execute(|| {
			let bounds_builder = ElectionBoundsBuilder::default();
			// all voters ordered by stake,
			assert_eq!(
				<Test as Config>::VoterList::iter().collect::<Vec<_>>(),
				vec![61, 71, 81, 11, 21, 31]
			);

			AbsoluteMaxNominations::set(2);

			// we want 2 voters now, and in maximum we allow 4 iterations. This is what happens:
			// 61 is pruned;
			// 71 is pruned;
			// 81 is pruned;
			// 11 is taken;
			// we finish since the 2x limit is reached.
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(2.into()).build().voters)
					.unwrap()
					.iter()
					.map(|(stash, _, _)| stash)
					.copied()
					.collect::<Vec<_>>(),
				vec![11],
			);
		});
}

#[test]
fn respects_snapshot_count_limits() {
	ExtBuilder::default()
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			// sum of all nominators who'd be voters (1), plus the self-votes (4).
			assert_eq!(<Test as Config>::VoterList::count(), 5);

			let bounds_builder = ElectionBoundsBuilder::default();

			// if voter count limit is less..
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(1.into()).build().voters)
					.unwrap()
					.len(),
				1
			);

			// if voter count limit is equal..
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(5.into()).build().voters)
					.unwrap()
					.len(),
				5
			);

			// if voter count limit is more.
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(55.into()).build().voters)
					.unwrap()
					.len(),
				5
			);

			// if target count limit is more..
			assert_eq!(
				Staking::electable_targets(bounds_builder.targets_count(6.into()).build().targets)
					.unwrap()
					.len(),
				4
			);

			// if target count limit is equal..
			assert_eq!(
				Staking::electable_targets(bounds_builder.targets_count(4.into()).build().targets)
					.unwrap()
					.len(),
				4
			);

			// if target limit count is less, then we return an error.
			assert_eq!(
				Staking::electable_targets(bounds_builder.targets_count(1.into()).build().targets)
					.unwrap_err(),
				"Target snapshot too big"
			);
		});
}

#[test]
fn respects_snapshot_size_limits() {
	ExtBuilder::default().build_and_execute(|| {
		// voters: set size bounds that allows only for 1 voter.
		let bounds = ElectionBoundsBuilder::default().voters_size(26.into()).build();
		let elected = Staking::electing_voters(bounds.voters).unwrap();
		assert!(elected.encoded_size() == 26 as usize);
		let prev_len = elected.len();

		// larger size bounds means more quota for voters.
		let bounds = ElectionBoundsBuilder::default().voters_size(100.into()).build();
		let elected = Staking::electing_voters(bounds.voters).unwrap();
		assert!(elected.encoded_size() <= 100 as usize);
		assert!(elected.len() > 1 && elected.len() > prev_len);

		// targets: set size bounds that allows for only one target to fit in the snapshot.
		let bounds = ElectionBoundsBuilder::default().targets_size(10.into()).build();
		let elected = Staking::electable_targets(bounds.targets).unwrap();
		assert!(elected.encoded_size() == 9 as usize);
		let prev_len = elected.len();

		// larger size bounds means more space for targets.
		let bounds = ElectionBoundsBuilder::default().targets_size(100.into()).build();
		let elected = Staking::electable_targets(bounds.targets).unwrap();
		assert!(elected.encoded_size() <= 100 as usize);
		assert!(elected.len() > 1 && elected.len() > prev_len);
	});
}

#[test]
fn nomination_quota_checks_at_nominate_works() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// stash bond of 222 has a nomination quota of 2 targets.
		bond(61, 222);
		assert_eq!(Staking::api_nominations_quota(222), 2);

		// nominating with targets below the nomination quota works.
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(61), vec![11]));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(61), vec![11, 12]));

		// nominating with targets above the nomination quota returns error.
		assert_noop!(
			Staking::nominate(RuntimeOrigin::signed(61), vec![11, 12, 13]),
			Error::<Test>::TooManyTargets
		);
	});
}

#[test]
fn lazy_quota_npos_voters_works_above_quota() {
	ExtBuilder::default()
		.nominate(false)
		.add_staker(
			61,
			60,
			300, // 300 bond has 16 nomination quota.
			StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]),
		)
		.build_and_execute(|| {
			// unbond 78 from stash 60 so that it's bonded balance is 222, which has a lower
			// nomination quota than at nomination time (max 2 targets).
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(61), 78));
			assert_eq!(Staking::api_nominations_quota(300 - 78), 2);

			// even through 61 has nomination quota of 2 at the time of the election, all the
			// nominations (5) will be used.
			assert_eq!(
				Staking::electing_voters(DataProviderBounds::default())
					.unwrap()
					.iter()
					.map(|(stash, _, targets)| (*stash, targets.len()))
					.collect::<Vec<_>>(),
				vec![(11, 1), (21, 1), (31, 1), (61, 5)],
			);
		});
}

#[test]
fn nominations_quota_limits_size_work() {
	ExtBuilder::default()
		.nominate(false)
		.add_staker(
			71,
			70,
			333,
			StakerStatus::<AccountId>::Nominator(vec![16, 15, 14, 13, 12, 11, 10]),
		)
		.build_and_execute(|| {
			// nominations of controller 70 won't be added due to voter size limit exceeded.
			let bounds = ElectionBoundsBuilder::default().voters_size(100.into()).build();
			assert_eq!(
				Staking::electing_voters(bounds.voters)
					.unwrap()
					.iter()
					.map(|(stash, _, targets)| (*stash, targets.len()))
					.collect::<Vec<_>>(),
				vec![(11, 1), (21, 1), (31, 1)],
			);

			assert_eq!(
				*staking_events().last().unwrap(),
				Event::SnapshotVotersSizeExceeded { size: 75 }
			);

			// however, if the election voter size bounds were largers, the snapshot would
			// include the electing voters of 70.
			let bounds = ElectionBoundsBuilder::default().voters_size(1_000.into()).build();
			assert_eq!(
				Staking::electing_voters(bounds.voters)
					.unwrap()
					.iter()
					.map(|(stash, _, targets)| (*stash, targets.len()))
					.collect::<Vec<_>>(),
				vec![(11, 1), (21, 1), (31, 1), (71, 7)],
			);
		});
}

#[test]
fn estimate_next_election_works() {
	ExtBuilder::default().session_per_era(5).period(5).build_and_execute(|| {
		// first session is always length 0.
		for b in 1..20 {
			run_to_block(b);
			assert_eq!(Staking::next_election_prediction(System::block_number()), 20);
		}

		// election
		run_to_block(20);
		assert_eq!(Staking::next_election_prediction(System::block_number()), 45);
		assert_eq!(staking_events().len(), 1);
		assert_eq!(*staking_events().last().unwrap(), Event::StakersElected);

		for b in 21..45 {
			run_to_block(b);
			assert_eq!(Staking::next_election_prediction(System::block_number()), 45);
		}

		// election
		run_to_block(45);
		assert_eq!(Staking::next_election_prediction(System::block_number()), 70);
		assert_eq!(staking_events().len(), 3);
		assert_eq!(*staking_events().last().unwrap(), Event::StakersElected);

		Staking::force_no_eras(RuntimeOrigin::root()).unwrap();
		assert_eq!(Staking::next_election_prediction(System::block_number()), u64::MAX);

		Staking::force_new_era_always(RuntimeOrigin::root()).unwrap();
		assert_eq!(Staking::next_election_prediction(System::block_number()), 45 + 5);

		Staking::force_new_era(RuntimeOrigin::root()).unwrap();
		assert_eq!(Staking::next_election_prediction(System::block_number()), 45 + 5);

		// Do a fail election
		MinimumValidatorCount::<Test>::put(1000);
		run_to_block(50);
		// Election: failed, next session is a new election
		assert_eq!(Staking::next_election_prediction(System::block_number()), 50 + 5);
		// The new era is still forced until a new era is planned.
		assert_eq!(ForceEra::<Test>::get(), Forcing::ForceNew);

		MinimumValidatorCount::<Test>::put(2);
		run_to_block(55);
		assert_eq!(Staking::next_election_prediction(System::block_number()), 55 + 25);
		assert_eq!(staking_events().len(), 10);
		assert_eq!(
			*staking_events().last().unwrap(),
			Event::ForceEra { mode: Forcing::NotForcing }
		);
		assert_eq!(
			*staking_events().get(staking_events().len() - 2).unwrap(),
			Event::StakersElected
		);
		// The new era has been planned, forcing is changed from `ForceNew` to `NotForcing`.
		assert_eq!(ForceEra::<Test>::get(), Forcing::NotForcing);
	})
}

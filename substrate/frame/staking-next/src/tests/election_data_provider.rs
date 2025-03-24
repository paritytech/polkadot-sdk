use super::*;
use frame_election_provider_support::ElectionDataProvider;

#[test]
fn set_minimum_active_stake_is_correct() {
	ExtBuilder::default()
		.nominate(false)
		.add_staker(61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21]))
		.add_staker(71, 10, StakerStatus::<AccountId>::Nominator(vec![21]))
		.add_staker(81, 50, StakerStatus::<AccountId>::Nominator(vec![21]))
		.build_and_execute(|| {
			// default bounds are unbounded.
			assert_ok!(<Staking as ElectionDataProvider>::electing_voters(
				DataProviderBounds::default(),
				0
			));
			assert_eq!(MinimumActiveStake::<Test>::get(), 10);

			// remove staker with lower bond by limiting the number of voters and check
			// `MinimumActiveStake` again after electing voters.
			let bounds = ElectionBoundsBuilder::default().voters_count(5.into()).build();
			assert_ok!(<Staking as ElectionDataProvider>::electing_voters(bounds.voters, 0));
			assert_eq!(MinimumActiveStake::<Test>::get(), 50);
		});
}

#[test]
fn set_minimum_active_stake_lower_bond_works() {
	// lower non-zero active stake below `MinNominatorBond` is the minimum active stake if
	// it is selected as part of the npos voters.
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		assert_eq!(MinNominatorBond::<Test>::get(), 1);
		assert_eq!(<Test as Config>::VoterList::count(), 4);

		assert_ok!(Staking::bond(RuntimeOrigin::signed(4), 5, RewardDestination::Staked,));
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(4), vec![1]));
		assert_eq!(<Test as Config>::VoterList::count(), 5);

		let voters_before =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
				.unwrap();
		assert_eq!(MinimumActiveStake::<Test>::get(), 5);

		// update minimum nominator bond.
		MinNominatorBond::<Test>::set(10);
		assert_eq!(MinNominatorBond::<Test>::get(), 10);
		// voter list still considers nominator 4 for voting, even though its active stake is
		// lower than `MinNominatorBond`.
		assert_eq!(<Test as Config>::VoterList::count(), 5);

		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
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
		.add_staker(61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21]))
		.build_and_execute(|| {
			assert_eq!(Staking::weight_of(&101), 500);
			let voters = <Staking as ElectionDataProvider>::electing_voters(
				DataProviderBounds::default(),
				0,
			)
			.unwrap();
			assert_eq!(voters.len(), 5);
			assert_eq!(MinimumActiveStake::<Test>::get(), 500);

			Session::roll_until_active_era(10);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 200));
			Session::roll_until_active_era(20);
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(101), 100));

			// corrupt ledger state by lowering max unlocking chunks bounds.
			MaxUnlockingChunks::set(1);

			let voters = <Staking as ElectionDataProvider>::electing_voters(
				DataProviderBounds::default(),
				0,
			)
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
			DataProviderBounds::default(),
			0
		)
		.unwrap()
		.into_iter()
		.any(|(w, _, t)| { v == w && t[0] == w })))
	})
}

#[test]
#[should_panic]
#[cfg(debug_assertions)]
fn only_iterates_max_2_times_max_allowed_len() {
	ExtBuilder::default()
		.nominate(false)
		// the best way to invalidate a bunch of nominators is to have them nominate a lot of
		// ppl, but then lower the MaxNomination limit.
		.add_staker(61, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
		.add_staker(71, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
		.add_staker(81, 2_000, StakerStatus::<AccountId>::Nominator(vec![21, 22, 23, 24, 25]))
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
				Staking::electing_voters(bounds_builder.voters_count(2.into()).build().voters, 0)
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
				Staking::electing_voters(bounds_builder.voters_count(1.into()).build().voters, 0)
					.unwrap()
					.len(),
				1
			);

			// if voter count limit is equal..
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(5.into()).build().voters, 0)
					.unwrap()
					.len(),
				5
			);

			// if voter count limit is more.
			assert_eq!(
				Staking::electing_voters(bounds_builder.voters_count(55.into()).build().voters, 0)
					.unwrap()
					.len(),
				5
			);

			// if target count limit is more..
			assert_eq!(
				Staking::electable_targets(
					bounds_builder.targets_count(6.into()).build().targets,
					0,
				)
				.unwrap()
				.len(),
				4
			);

			// if target count limit is equal..
			assert_eq!(
				Staking::electable_targets(
					bounds_builder.targets_count(4.into()).build().targets,
					0,
				)
				.unwrap()
				.len(),
				4
			);

			// if target limit count is less, then we return an error.
			assert_eq!(
				Staking::electable_targets(
					bounds_builder.targets_count(1.into()).build().targets,
					0
				)
				.unwrap()
				.len(),
				1,
			);
		});
}

#[test]
fn respects_snapshot_size_limits() {
	ExtBuilder::default().build_and_execute(|| {
		// voters: set size bounds that allows only for 1 voter.
		let bounds = ElectionBoundsBuilder::default().voters_size(26.into()).build();
		let elected = Staking::electing_voters(bounds.voters, 0).unwrap();
		assert!(elected.encoded_size() == 26 as usize);
		let prev_len = elected.len();

		// larger size bounds means more quota for voters.
		let bounds = ElectionBoundsBuilder::default().voters_size(100.into()).build();
		let elected = Staking::electing_voters(bounds.voters, 0).unwrap();
		assert!(elected.encoded_size() <= 100 as usize);
		assert!(elected.len() > 1 && elected.len() > prev_len);

		// targets: set size bounds that allows for only one target to fit in the snapshot.
		let bounds = ElectionBoundsBuilder::default().targets_size(10.into()).build();
		let elected = Staking::electable_targets(bounds.targets, 0).unwrap();
		assert!(elected.encoded_size() == 9 as usize);
		let prev_len = elected.len();

		// larger size bounds means more space for targets.
		let bounds = ElectionBoundsBuilder::default().targets_size(100.into()).build();
		let elected = Staking::electable_targets(bounds.targets, 0).unwrap();
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
				Staking::electing_voters(DataProviderBounds::default(), 0)
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
			333,
			StakerStatus::<AccountId>::Nominator(vec![16, 15, 14, 13, 12, 11, 10]),
		)
		.build_and_execute(|| {
			// nominations of controller 70 won't be added due to voter size limit exceeded.
			let bounds = ElectionBoundsBuilder::default().voters_size(100.into()).build();
			assert_eq!(
				Staking::electing_voters(bounds.voters, 0)
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

			// however, if the election voter size bounds were larger, the snapshot would
			// include the electing voters of 70.
			let bounds = ElectionBoundsBuilder::default().voters_size(1_000.into()).build();
			assert_eq!(
				Staking::electing_voters(bounds.voters, 0)
					.unwrap()
					.iter()
					.map(|(stash, _, targets)| (*stash, targets.len()))
					.collect::<Vec<_>>(),
				vec![(11, 1), (21, 1), (31, 1), (71, 7)],
			);
		});
}

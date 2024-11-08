// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

use crate::{mock::*, *};
use frame_support::{assert_ok, testing_prelude::*};
use substrate_test_utils::assert_eq_uvec;

use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, ElectionDataProvider, SortedListProvider,
};
use sp_staking::StakingInterface;

#[test]
fn add_electable_stashes_work() {
	ExtBuilder::default().build_and_execute(|| {
		MaxValidatorSet::set(5);
		assert_eq!(MaxValidatorSet::get(), 5);
		assert!(ElectableStashes::<Test>::get().is_empty());

		// adds stashes without duplicates, do not overflow bounds.
		assert_ok!(Staking::add_electables(bounded_vec![1, 2, 3]));
		assert_eq!(
			ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
			vec![1, 2, 3]
		);

		// adds with duplicates which are deduplicated implicitly, no not overflow bounds.
		assert_ok!(Staking::add_electables(bounded_vec![1, 2, 4]));
		assert_eq!(
			ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
			vec![1, 2, 3, 4]
		);

		// adds stashes so that bounds are overflown, fails and internal state changes so that
		// all slots are filled.
		assert!(Staking::add_electables(bounded_vec![6, 7, 8, 9, 10]).is_err());
		assert_eq!(
			ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
			vec![1, 2, 3, 4, 6]
		);
	})
}

mod paged_on_initialize {
	use super::*;

	#[test]
	fn single_page_election_works() {
		ExtBuilder::default()
			// set desired targets to 3.
			.validator_count(3)
			.build_and_execute(|| {
				// single page election provider.
				assert_eq!(
					<<Test as Config>::ElectionProvider as ElectionProvider>::Pages::get(),
					1
				);

				let next_election = <Staking as ElectionDataProvider>::next_election_prediction(
					System::block_number(),
				);

				// single page.
				let pages: BlockNumber = Staking::election_pages().into();
				assert_eq!(pages, 1);

				// genesis validators.
				assert_eq!(current_era(), 0);
				assert_eq_uvec!(Session::validators(), vec![11, 21, 31]);

				// force unstake of 31 to ensure the election results of the next era are
				// different than genesis.
				assert_ok!(Staking::force_unstake(RuntimeOrigin::root(), 31, 0));

				let expected_elected = Validators::<Test>::iter_keys()
					.filter(|x| Staking::status(x) == Ok(StakerStatus::Validator))
					.collect::<Vec<AccountId>>();
				//  use all registered validators as potential targets.
				ValidatorCount::<Test>::set(expected_elected.len() as u32);
				assert_eq!(expected_elected.len(), 2);

				// 1. election prep hasn't started yet, election cursor and electable stashes are
				//    not
				// set yet.
				run_to_block(next_election - pages - 1);
				assert_eq!(ElectingStartedAt::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// 2. starts preparing election at the (election_prediction - n_pages) block.
				run_to_block(next_election - pages);

				// electing started at cursor is set once the election starts to be prepared.
				assert_eq!(ElectingStartedAt::<Test>::get(), Some(next_election - pages));
				// now the electable stashes have been fetched and stored.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);

				// era is still 0.
				assert_eq!(current_era(), 0);

				// 3. progress to election block, which matches with era rotation.
				run_to_block(next_election);
				assert_eq!(current_era(), 1);
				// clears out election metadata for era.
				assert!(ElectingStartedAt::<Test>::get().is_none());
				assert!(ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>().is_empty());

				// era progresseed and electable stashes have been served to session pallet.
				assert_eq_uvec!(Session::validators(), vec![11, 21, 31]);

				// 4. in the next era, the validator set does not include 31 anymore which was
				// unstaked.
				start_active_era(2);
				assert_eq_uvec!(Session::validators(), vec![11, 21]);
			})
	}

	#[test]
	fn single_page_election_era_transition_exposures_work() {
		ExtBuilder::default()
			// set desired targets to 3.
			.validator_count(3)
			.build_and_execute(|| {
				// single page election provider.
				assert_eq!(
					<<Test as Config>::ElectionProvider as ElectionProvider>::Pages::get(),
					1
				);

				assert_eq!(current_era(), 0);

				// 3 sessions per era.
				assert_eq!(SessionsPerEra::get(), 3);

				// genesis validators and exposures.
				assert_eq!(current_era(), 0);
				assert_eq_uvec!(validator_controllers(), vec![11, 21, 31]);
				assert_eq!(
					era_exposures(current_era()),
					vec![
						(
							11,
							Exposure {
								total: 1125,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 125 }]
							}
						),
						(
							21,
							Exposure {
								total: 1375,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 375 }]
							}
						),
						(31, Exposure { total: 500, own: 500, others: vec![] })
					]
				);

				start_session(1);
				assert_eq!(current_era(), 0);
				// election haven't started yet.
				assert_eq!(ElectingStartedAt::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// progress to era rotation session.
				start_session(SessionsPerEra::get());
				assert_eq!(current_era(), 1);
				assert_eq_uvec!(Session::validators(), vec![11, 21, 31]);
				assert_eq!(
					era_exposures(current_era()),
					vec![
						(
							11,
							Exposure {
								total: 1125,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 125 }]
							}
						),
						(
							21,
							Exposure {
								total: 1375,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 375 }]
							}
						),
						(31, Exposure { total: 500, own: 500, others: vec![] })
					]
				);

				// force unstake validator 31 for next era.
				assert_ok!(Staking::force_unstake(RuntimeOrigin::root(), 31, 0));

				// progress session and rotate era.
				start_session(SessionsPerEra::get() * 2);
				assert_eq!(current_era(), 2);
				assert_eq_uvec!(Session::validators(), vec![11, 21]);

				assert_eq!(
					era_exposures(current_era()),
					vec![
						(
							11,
							Exposure {
								total: 1125,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 125 }]
							}
						),
						(
							21,
							Exposure {
								total: 1375,
								own: 1000,
								others: vec![IndividualExposure { who: 101, value: 375 }]
							}
						),
					]
				);
			})
	}

	#[test]
	fn multi_page_election_works() {
		ExtBuilder::default()
		    .add_staker(61, 61, 10, StakerStatus::Validator)
		    .add_staker(71, 71, 10, StakerStatus::Validator)
		    .add_staker(81, 81, 10, StakerStatus::Validator)
		    .add_staker(91, 91, 10, StakerStatus::Validator)
			.multi_page_election_provider(3)
            .max_winners_per_page(5)
			.build_and_execute(|| {
				// election provider has 3 pages.
				let pages: BlockNumber =
					<<Test as Config>::ElectionProvider as ElectionProvider>::Pages::get().into();
				assert_eq!(pages, 3);
                // 5 max winners per page.
                let max_winners_page = <<Test as Config>::ElectionProvider as ElectionProvider>::MaxWinnersPerPage::get();
                assert_eq!(max_winners_page, 5);

                // genesis era.
				assert_eq!(current_era(), 0);

                // confirm the genesis validators.
                assert_eq!(Session::validators(), vec![11, 21]);

				let next_election = <Staking as ElectionDataProvider>::next_election_prediction(
					System::block_number(),
				);
				assert_eq!(next_election, 10);

	            let expected_elected = Validators::<Test>::iter_keys()
					.filter(|x| Staking::status(x) == Ok(StakerStatus::Validator))
                    // mock multi page election provider takes first `max_winners_page`
                    // validators as winners.
                    .take(max_winners_page as usize)
					.collect::<Vec<AccountId>>();
				// adjust desired targets to number of winners per page.
				ValidatorCount::<Test>::set(expected_elected.len() as u32);
				assert_eq!(expected_elected.len(), 5);

				// 1. election prep hasn't started yet, election cursor and electable stashes are not
				// set yet.
				run_to_block(next_election - pages - 1);
				assert_eq!(ElectingStartedAt::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// 2. starts preparing election at the (election_prediction - n_pages) block.
                //  fetches msp (i.e. 2).
				run_to_block(next_election - pages);

				// electing started at cursor is set once the election starts to be prepared.
				assert_eq!(ElectingStartedAt::<Test>::get(), Some(next_election - pages));
				// now the electable stashes started to be fetched and stored.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);
                // exposures have been collected for all validators in the page.
                // note that the mock election provider adds one exposures per winner for
                // each page.
                for s in expected_elected.iter() {
                    // 1 page fetched, 1 `other` exposure collected per electable stash.
                    assert_eq!(Staking::eras_stakers(current_era() + 1, s).others.len(), 1);
                }

                // 3. progress one block to fetch page 1.
                run_to_block(System::block_number() + 1);
                // the electable stashes remain the same.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);
                // election cursor reamins unchanged during intermediate pages.
				assert_eq!(ElectingStartedAt::<Test>::get(), Some(next_election - pages));
                // exposures have been collected for all validators in the page.
                for s in expected_elected.iter() {
                    // 2 pages fetched, 2 `other` exposures collected per electable stash.
                    assert_eq!(Staking::eras_stakers(current_era() + 1, s).others.len(), 2);
                }

                // 4. progress one block to fetch lsp (i.e. 0).
                run_to_block(System::block_number() + 1);
                // the electable stashes remain the same.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);
                // exposures have been collected for all validators in the page.
                for s in expected_elected.iter() {
                    // 3 pages fetched, 3 `other` exposures collected per electable stash.
                    assert_eq!(Staking::eras_stakers(current_era() + 1, s).others.len(), 3);
                }
                // upon fetching page 0, the electing started at will remain in storage until the
                // era rotates.
				assert_eq!(current_era(), 0);
				assert_eq!(ElectingStartedAt::<Test>::get(), Some(next_election - pages));

                // 5. rotate era.
                start_active_era(current_era() + 1);
                // the new era validators are the expected elected stashes.
                assert_eq_uvec!(Session::validators(), expected_elected);
                // and all the metadata has been cleared up and ready for the next election.
				assert!(ElectingStartedAt::<Test>::get().is_none());
				assert!(ElectableStashes::<Test>::get().is_empty());
			})
	}
}

mod paged_snapshot {
	use super::*;

	#[test]
	fn target_snapshot_works() {
		ExtBuilder::default()
			.nominate(true)
			.set_status(41, StakerStatus::Validator)
			.set_status(51, StakerStatus::Validator)
			.set_status(101, StakerStatus::Idle)
			.build_and_execute(|| {
				// all registered validators.
				let all_targets = vec![51, 31, 41, 21, 11];
				assert_eq_uvec!(
					<Test as Config>::TargetList::iter().collect::<Vec<_>>(),
					all_targets,
				);

				// 3 targets per page.
				let bounds =
					ElectionBoundsBuilder::default().targets_count(3.into()).build().targets;

				let targets =
					<Staking as ElectionDataProvider>::electable_targets(bounds, 0).unwrap();
				assert_eq_uvec!(targets, all_targets.iter().take(3).cloned().collect::<Vec<_>>());

				// emulates a no bounds target snapshot request.
				let bounds =
					ElectionBoundsBuilder::default().targets_count(u32::MAX.into()).build().targets;

				let single_page_targets =
					<Staking as ElectionDataProvider>::electable_targets(bounds, 0).unwrap();

				// complete set of paged targets is the same as single page, no bounds set of
				// targets.
				assert_eq_uvec!(all_targets, single_page_targets);
			})
	}

	#[test]
	fn target_snaposhot_multi_page_redundant() {
		ExtBuilder::default().build_and_execute(|| {
			let all_targets = vec![31, 21, 11];
			assert_eq_uvec!(<Test as Config>::TargetList::iter().collect::<Vec<_>>(), all_targets,);

			// no bounds.
			let bounds =
				ElectionBoundsBuilder::default().targets_count(u32::MAX.into()).build().targets;

			// target snapshot supports only single-page, thus it is redundant what's the page index
			// requested.
			let snapshot = Staking::electable_targets(bounds, 0).unwrap();
			assert!(
				snapshot == all_targets &&
					snapshot == Staking::electable_targets(bounds, 1).unwrap() &&
					snapshot == Staking::electable_targets(bounds, 2).unwrap() &&
					snapshot == Staking::electable_targets(bounds, u32::MAX).unwrap(),
			);
		})
	}

	#[test]
	fn voter_snapshot_works() {
		ExtBuilder::default()
			.nominate(true)
			.set_status(51, StakerStatus::Validator)
			.set_status(41, StakerStatus::Nominator(vec![51]))
			.set_status(101, StakerStatus::Validator)
			.build_and_execute(|| {
				let bounds = ElectionBoundsBuilder::default().voters_count(3.into()).build().voters;

				assert_eq!(
					<Test as Config>::VoterList::iter().collect::<Vec<_>>(),
					vec![11, 21, 31, 41, 51, 101],
				);

				let mut all_voters = vec![];

				let voters_page_3 = <Staking as ElectionDataProvider>::electing_voters(bounds, 3)
					.unwrap()
					.into_iter()
					.map(|(a, _, _)| a)
					.collect::<Vec<_>>();
				all_voters.extend(voters_page_3.clone());

				assert_eq!(voters_page_3, vec![11, 21, 31]);

				let voters_page_2 = <Staking as ElectionDataProvider>::electing_voters(bounds, 2)
					.unwrap()
					.into_iter()
					.map(|(a, _, _)| a)
					.collect::<Vec<_>>();
				all_voters.extend(voters_page_2.clone());

				assert_eq!(voters_page_2, vec![41, 51, 101]);

				// all voters in the list have been consumed.
				assert_eq!(VoterSnapshotStatus::<Test>::get(), SnapshotStatus::Consumed);

				// thus page 1 and 0 are empty.
				assert!(<Staking as ElectionDataProvider>::electing_voters(bounds, 1)
					.unwrap()
					.is_empty());
				assert!(<Staking as ElectionDataProvider>::electing_voters(bounds, 0)
					.unwrap()
					.is_empty());

				// last page has been requested, reset the snapshot status to waiting.
				assert_eq!(VoterSnapshotStatus::<Test>::get(), SnapshotStatus::Waiting);

				// now request 1 page with bounds where all registerd voters fit. u32::MAX
				// emulates a no bounds request.
				let bounds =
					ElectionBoundsBuilder::default().voters_count(u32::MAX.into()).build().targets;

				let single_page_voters =
					<Staking as ElectionDataProvider>::electing_voters(bounds, 0)
						.unwrap()
						.into_iter()
						.map(|(a, _, _)| a)
						.collect::<Vec<_>>();

				// complete set of paged voters is the same as single page, no bounds set of
				// voters.
				assert_eq!(all_voters, single_page_voters);
			})
	}
}

mod paged_exposures {
	use super::*;

	#[test]
	fn store_stakers_info_elect_works() {
		ExtBuilder::default().exposures_page_size(2).build_and_execute(|| {
			assert_eq!(MaxExposurePageSize::get(), 2);

			let exposure_one = Exposure {
				total: 1000 + 700,
				own: 1000,
				others: vec![
					IndividualExposure { who: 101, value: 500 },
					IndividualExposure { who: 102, value: 100 },
					IndividualExposure { who: 103, value: 100 },
				],
			};

			let exposure_two = Exposure {
				total: 1000 + 1000,
				own: 1000,
				others: vec![
					IndividualExposure { who: 104, value: 500 },
					IndividualExposure { who: 105, value: 500 },
				],
			};

			let exposure_three = Exposure {
				total: 1000 + 500,
				own: 1000,
				others: vec![
					IndividualExposure { who: 110, value: 250 },
					IndividualExposure { who: 111, value: 250 },
				],
			};

			let exposures_page_one = bounded_vec![(1, exposure_one), (2, exposure_two),];
			let exposures_page_two = bounded_vec![(1, exposure_three),];

			// stores exposure page with exposures of validator 1 and 2, returns exposed validator
			// account id.
			assert_eq!(
				Pallet::<Test>::store_stakers_info(exposures_page_one, current_era()).to_vec(),
				vec![1, 2]
			);
			// Stakers overview OK for validator 1 and 2.
			assert_eq!(
				ErasStakersOverview::<Test>::get(0, &1).unwrap(),
				PagedExposureMetadata {
					total: 1700,
					own: 1000,
					nominator_count: 3,
					page_count: 2,
					last_page_empty_slots: 1,
				},
			);
			assert_eq!(
				ErasStakersOverview::<Test>::get(0, &2).unwrap(),
				PagedExposureMetadata {
					total: 2000,
					own: 1000,
					nominator_count: 2,
					page_count: 1,
					last_page_empty_slots: 0,
				},
			);

			// stores exposure page with exposures of validator 1, returns exposed validator
			// account id.
			assert_eq!(
				Pallet::<Test>::store_stakers_info(exposures_page_two, current_era()).to_vec(),
				vec![1]
			);

			// Stakers overview OK for validator 1.
			assert_eq!(
				ErasStakersOverview::<Test>::get(0, &1).unwrap(),
				PagedExposureMetadata {
					total: 2200,
					own: 1000,
					nominator_count: 5,
					page_count: 3,
					last_page_empty_slots: 1,
				},
			);

			// validator 1 has 3 paged exposures.
			assert!(
				ErasStakersPaged::<Test>::iter_prefix_values((0, &1)).count() as u32 ==
					EraInfo::<Test>::get_page_count(0, &1) &&
					EraInfo::<Test>::get_page_count(0, &1) == 3
			);
			assert!(ErasStakersPaged::<Test>::get((0, &1, 0)).is_some());
			assert!(ErasStakersPaged::<Test>::get((0, &1, 1)).is_some());
			assert!(ErasStakersPaged::<Test>::get((0, &1, 2)).is_some());
			assert!(ErasStakersPaged::<Test>::get((0, &1, 3)).is_none());

			// validator 2 has 1 paged exposures.
			assert!(ErasStakersPaged::<Test>::get((0, &2, 0)).is_some());
			assert!(ErasStakersPaged::<Test>::get((0, &2, 1)).is_none());
			assert_eq!(ErasStakersPaged::<Test>::iter_prefix_values((0, &2)).count(), 1);

			// exposures of validator 1 are the expected:
			assert_eq!(
				ErasStakersPaged::<Test>::get((0, &1, 0)).unwrap(),
				ExposurePage {
					page_total: 600,
					others: vec![
						IndividualExposure { who: 101, value: 500 },
						IndividualExposure { who: 102, value: 100 }
					]
				},
			);
			assert_eq!(
				ErasStakersPaged::<Test>::get((0, &1, 1)).unwrap(),
				ExposurePage {
					page_total: 350,
					others: vec![
						IndividualExposure { who: 103, value: 100 },
						IndividualExposure { who: 110, value: 250 }
					]
				}
			);
			assert_eq!(
				ErasStakersPaged::<Test>::get((0, &1, 2)).unwrap(),
				ExposurePage {
					page_total: 250,
					others: vec![IndividualExposure { who: 111, value: 250 }]
				}
			);

			// exposures of validator 2.
			assert_eq!(
				ErasStakersPaged::<Test>::iter_prefix_values((0, &2)).collect::<Vec<_>>(),
				vec![ExposurePage {
					page_total: 1000,
					others: vec![
						IndividualExposure { who: 104, value: 500 },
						IndividualExposure { who: 105, value: 500 }
					]
				}],
			);
		})
	}
}

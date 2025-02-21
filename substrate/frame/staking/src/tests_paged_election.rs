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
	bounds::ElectionBoundsBuilder, ElectionDataProvider, SortedListProvider, Support,
};
use sp_staking::StakingInterface;

mod electable_stashes {
	use super::*;

	#[test]
	fn add_electable_stashes_work() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			MaxValidatorSet::set(5);
			assert_eq!(MaxValidatorSet::get(), 5);
			assert!(ElectableStashes::<Test>::get().is_empty());

			// adds stashes without duplicates, do not overflow bounds.
			assert_ok!(Staking::add_electables(vec![1u64, 2, 3].into_iter()));
			assert_eq!(
				ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
				vec![1, 2, 3]
			);

			// adds with duplicates which are deduplicated implicitly, no not overflow bounds.
			assert_ok!(Staking::add_electables(vec![1u64, 2, 4].into_iter()));
			assert_eq!(
				ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
				vec![1, 2, 3, 4]
			);
		})
	}

	#[test]
	fn add_electable_stashes_overflow_works() {
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			MaxValidatorSet::set(5);
			assert_eq!(MaxValidatorSet::get(), 5);
			assert!(ElectableStashes::<Test>::get().is_empty());

			// adds stashes so that bounds are overflown, fails and internal state changes so that
			// all slots are filled. error will return the idx of the first account that was not
			// included.
			let expected_idx_not_included = 5; // stash 6.
			assert_eq!(
				Staking::add_electables(vec![1u64, 2, 3, 4, 5, 6, 7, 8].into_iter()),
				Err(expected_idx_not_included)
			);
			// the included were added to the electable stashes, despite the error.
			assert_eq!(
				ElectableStashes::<Test>::get().into_inner().into_iter().collect::<Vec<_>>(),
				vec![1, 2, 3, 4, 5]
			);
		})
	}

	#[test]
	fn overflow_electable_stashes_no_exposures_work() {
		// ensures exposures are stored only for the electable stashes that fit within the
		// electable stashes bounds in case of overflow.
		ExtBuilder::default().try_state(false).build_and_execute(|| {
			MaxValidatorSet::set(2);
			assert_eq!(MaxValidatorSet::get(), 2);
			assert!(ElectableStashes::<Test>::get().is_empty());

			// current era is 0, preparing 1.
			assert_eq!(current_era(), 0);

			let supports = to_bounded_supports(vec![
				(1, Support { total: 100, voters: vec![(10, 1_000)] }),
				(2, Support { total: 200, voters: vec![(20, 2_000)] }),
				(3, Support { total: 300, voters: vec![(30, 3_000)] }),
				(4, Support { total: 400, voters: vec![(40, 4_000)] }),
			]);

			// error due to bounds.
			let expected_not_included = 2;
			assert_eq!(Staking::do_elect_paged_inner(supports), Err(expected_not_included));

			// electable stashes have been collected to the max bounds despite the error.
			assert_eq!(ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(), vec![1, 2]);

			let exposure_exists =
				|acc, era| EraInfo::<Test>::get_full_exposure(era, &acc).total != 0;

			// exposures were only collected for electable stashes in bounds (1 and 2).
			assert!(exposure_exists(1, 1));
			assert!(exposure_exists(2, 1));
			assert!(!exposure_exists(3, 1));
			assert!(!exposure_exists(4, 1));
		})
	}
}

mod paged_on_initialize {
	use super::*;
	use frame_election_provider_support::onchain;

	#[test]
	fn single_page_election_works() {
		ExtBuilder::default()
			// set desired targets to 3.
			.validator_count(3)
			.build_and_execute(|| {
				let next_election = Staking::next_election_prediction(System::block_number());
				assert_eq!(next_election, 10);

				// single page.
				let pages: BlockNumber = Staking::election_pages().into();
				assert_eq!(pages, 1);

				// genesis validators are now in place.
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
				// not set yet.
				run_to_block(8);
				assert_eq!(NextElectionPage::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());
				assert_eq!(VoterSnapshotStatus::<Test>::get(), SnapshotStatus::Waiting);

				// try-state sanity check.
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// 2. starts preparing election at the (election_prediction - n_pages) block.
				run_to_block(9);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// electing started, but since single-page, we don't set `NextElectionPage` at all.
				assert_eq!(NextElectionPage::<Test>::get(), None);
				// now the electable stashes have been fetched and stored.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);
				assert_eq!(VoterSnapshotStatus::<Test>::get(), SnapshotStatus::Waiting);

				// era is still 0.
				assert_eq!(current_era(), 0);

				// 3. progress to election block, which matches with era rotation.
				run_to_block(10);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				assert_eq!(current_era(), 1);
				// clears out election metadata for era.
				assert!(NextElectionPage::<Test>::get().is_none());
				assert!(ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>().is_empty());
				assert_eq!(VoterSnapshotStatus::<Test>::get(), SnapshotStatus::Waiting);

				// era progressed and electable stashes have been served to session pallet.
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

				// try-state sanity check.
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				start_session(1);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				assert_eq!(current_era(), 0);
				// election haven't started yet.
				assert_eq!(NextElectionPage::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// progress to era rotation session.
				start_session(SessionsPerEra::get());
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
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
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
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

				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
			})
	}

	#[test]
	fn multi_page_election_works() {
		ExtBuilder::default()
			.add_staker(61, 61, 1000, StakerStatus::Validator)
			.add_staker(71, 71, 1000, StakerStatus::Validator)
			.add_staker(81, 81, 1000, StakerStatus::Validator)
			.add_staker(91, 91, 1000, StakerStatus::Validator)
			.multi_page_election_provider(3)
			.max_winners_per_page(5)
			.build_and_execute(|| {
				// we need this later.
				let genesis_validators = Session::validators();

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

				// try-state sanity check.
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// 1. election prep hasn't started yet, election cursor and electable stashes are
				// not set yet.
				run_to_block(6);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				assert_eq!(NextElectionPage::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// 2. starts preparing election at the (election_prediction - n_pages) block.
				//  fetches msp (i.e. 2).
				run_to_block(7);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// electing started at cursor is set once the election starts to be prepared.
				assert_eq!(NextElectionPage::<Test>::get(), Some(1));
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
				run_to_block(8);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				// the electable stashes remain the same.
				assert_eq_uvec!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					expected_elected
				);
				// election cursor moves along.
				assert_eq!(NextElectionPage::<Test>::get(), Some(0));
				// exposures have been collected for all validators in the page.
				for s in expected_elected.iter() {
					// 2 pages fetched, 2 `other` exposures collected per electable stash.
					assert_eq!(Staking::eras_stakers(current_era() + 1, s).others.len(), 2);
				}

				// 4. progress one block to fetch lsp (i.e. 0).
				run_to_block(9);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
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
				assert_eq!(NextElectionPage::<Test>::get(), None);
				assert_eq!(staking_events_since_last_call(), vec![
					Event::PagedElectionProceeded { page: 2, result: Ok(5) },
					Event::PagedElectionProceeded { page: 1, result: Ok(0) },
					Event::PagedElectionProceeded { page: 0, result: Ok(0) }
				]);

				// upon fetching page 0, the electing started will remain in storage until the
				// era rotates.
				assert_eq!(current_era(), 0);

				// Next block the era will rotate.
				run_to_block(10);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				// and all the metadata has been cleared up and ready for the next election.
				assert!(NextElectionPage::<Test>::get().is_none());
				assert!(ElectableStashes::<Test>::get().is_empty());
				// events
				assert_eq!(staking_events_since_last_call(), vec![
					Event::StakersElected
				]);
				// session validators are not updated yet, these are genesis validators
				assert_eq_uvec!(Session::validators(),  genesis_validators);

				// next session they are updated.
				advance_session();
				// the new era validators are the expected elected stashes.
				assert_eq_uvec!(Session::validators(), expected_elected);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			})
	}

	#[test]
	fn multi_page_election_with_mulit_page_exposures_rewards_work() {
		ExtBuilder::default()
			.add_staker(61, 61, 1000, StakerStatus::Validator)
			.add_staker(71, 71, 1000, StakerStatus::Validator)
            .add_staker(1, 1, 5, StakerStatus::Nominator(vec![21, 31, 71]))
            .add_staker(2, 2, 5, StakerStatus::Nominator(vec![21, 31, 71]))
            .add_staker(3, 3, 5, StakerStatus::Nominator(vec![21, 31, 71]))
			.multi_page_election_provider(3)
            .max_winners_per_page(3)
            .exposures_page_size(2)
			.build_and_execute(|| {
				// election provider has 3 pages.
				let pages: BlockNumber =
					<<Test as Config>::ElectionProvider as ElectionProvider>::Pages::get().into();
				assert_eq!(pages, 3);
                // 3 max winners per page.
                let max_winners_page = <<Test as Config>::ElectionProvider as ElectionProvider>::MaxWinnersPerPage::get();
                assert_eq!(max_winners_page, 3);

        		// setup validator payee prefs and 10% commission.
                for s in vec![21, 31, 71] {
        		    Payee::<Test>::insert(s, RewardDestination::Account(s));
                    let prefs = ValidatorPrefs { commission: Perbill::from_percent(10), ..Default::default() };
			        Validators::<Test>::insert(s, prefs.clone());
                }

                let init_balance_all = vec![21, 31, 71, 1, 2, 3].iter().fold(0, |mut acc, s| {
                    acc += asset::total_balance::<Test>(&s);
                    acc
                });

                // progress era.
				assert_eq!(current_era(), 0);
                start_active_era(1);
				assert_eq!(current_era(), 1);
                assert_eq!(Session::validators(), vec![21, 31, 71]);

                // distribute reward,
		        Pallet::<Test>::reward_by_ids(vec![(21, 50)]);
		        Pallet::<Test>::reward_by_ids(vec![(31, 50)]);
		        Pallet::<Test>::reward_by_ids(vec![(71, 50)]);

        		let total_payout = current_total_payout_for_duration(reward_time_per_era());

                start_active_era(2);

                // all the validators exposed in era 1 have two pages of exposures, since exposure
                // page size is 2.
                assert_eq!(MaxExposurePageSize::get(), 2);
                assert_eq!(EraInfo::<Test>::get_page_count(1, &21), 2);
                assert_eq!(EraInfo::<Test>::get_page_count(1, &31), 2);
                assert_eq!(EraInfo::<Test>::get_page_count(1, &71), 2);

                make_all_reward_payment(1);

                let balance_all = vec![21, 31, 71, 1, 2, 3].iter().fold(0, |mut acc, s| {
                    acc += asset::total_balance::<Test>(&s);
                    acc
                });

			    assert_eq_error_rate!(
                    total_payout,
                    balance_all - init_balance_all,
                    4
                );
            })
	}

	#[test]
	fn multi_page_election_is_graceful() {
		// demonstrate that in a multi-page election, in some of the `elect(_)` calls fail we won't
		// bail right away.
		ExtBuilder::default().multi_page_election_provider(3).build_and_execute(|| {
			// load some exact data into the election provider, some of which are error or empty.
			let correct_results = <Test as Config>::GenesisElectionProvider::elect(0);
			CustomElectionSupports::set(Some(vec![
				// page 0.
				correct_results.clone(),
				// page 1.
				Err(onchain::Error::FailedToBound),
				// page 2.
				Ok(Default::default()),
			]));

			// genesis era.
			assert_eq!(current_era(), 0);

			let next_election =
				<Staking as ElectionDataProvider>::next_election_prediction(System::block_number());
			assert_eq!(next_election, 10);

			// try-state sanity check.
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			// 1. election prep hasn't started yet, election cursor and electable stashes are
			// not set yet.
			run_to_block(6);
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
			assert_eq!(NextElectionPage::<Test>::get(), None);
			assert!(ElectableStashes::<Test>::get().is_empty());

			// 2. starts preparing election at the (election_prediction - n_pages) block.
			//  fetches lsp (i.e. 2).
			run_to_block(7);
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			// electing started at cursor is set once the election starts to be prepared.
			assert_eq!(NextElectionPage::<Test>::get(), Some(1));
			// in elect(2) we won't collect any stashes yet.
			assert!(ElectableStashes::<Test>::get().is_empty());

			// 3. progress one block to fetch page 1.
			run_to_block(8);
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			// in elect(1) we won't collect any stashes yet.
			assert!(ElectableStashes::<Test>::get().is_empty());
			// election cursor is updated
			assert_eq!(NextElectionPage::<Test>::get(), Some(0));

			// 4. progress one block to fetch mps (i.e. 0).
			run_to_block(9);
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			// some stashes come in.
			assert_eq!(
				ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
				vec![11 as AccountId, 21]
			);
			// cursor is now none
			assert_eq!(NextElectionPage::<Test>::get(), None);

			// events thus far
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::PagedElectionProceeded { page: 2, result: Ok(0) },
					Event::PagedElectionProceeded { page: 1, result: Err(0) },
					Event::PagedElectionProceeded { page: 0, result: Ok(2) }
				]
			);

			// upon fetching page 0, the electing started will remain in storage until the
			// era rotates.
			assert_eq!(current_era(), 0);

			// Next block the era will rotate.
			run_to_block(10);
			assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

			// and all the metadata has been cleared up and ready for the next election.
			assert!(NextElectionPage::<Test>::get().is_none());
			assert!(ElectableStashes::<Test>::get().is_empty());

			// and the overall staking worked fine.
			assert_eq!(staking_events_since_last_call(), vec![Event::StakersElected]);
		})
	}

	#[test]
	fn multi_page_election_fails_if_not_enough_validators() {
		// a graceful multi-page election still fails if not enough validators are provided.
		ExtBuilder::default()
			.multi_page_election_provider(3)
			.minimum_validator_count(3)
			.build_and_execute(|| {
				// load some exact data into the election provider, some of which are error or
				// empty.
				let correct_results = <Test as Config>::GenesisElectionProvider::elect(0);
				CustomElectionSupports::set(Some(vec![
					// page 0.
					correct_results.clone(),
					// page 1.
					Err(onchain::Error::FailedToBound),
					// page 2.
					Ok(Default::default()),
				]));

				// genesis era.
				assert_eq!(current_era(), 0);

				let next_election = <Staking as ElectionDataProvider>::next_election_prediction(
					System::block_number(),
				);
				assert_eq!(next_election, 10);

				// try-state sanity check.
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// 1. election prep hasn't started yet, election cursor and electable stashes are
				// not set yet.
				run_to_block(6);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));
				assert_eq!(NextElectionPage::<Test>::get(), None);
				assert!(ElectableStashes::<Test>::get().is_empty());

				// 2. starts preparing election at the (election_prediction - n_pages) block.
				//  fetches lsp (i.e. 2).
				run_to_block(7);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// electing started at cursor is set once the election starts to be prepared.
				assert_eq!(NextElectionPage::<Test>::get(), Some(1));
				// in elect(2) we won't collect any stashes yet.
				assert!(ElectableStashes::<Test>::get().is_empty());

				// 3. progress one block to fetch page 1.
				run_to_block(8);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// in elect(1) we won't collect any stashes yet.
				assert!(ElectableStashes::<Test>::get().is_empty());
				// election cursor is updated
				assert_eq!(NextElectionPage::<Test>::get(), Some(0));

				// 4. progress one block to fetch mps (i.e. 0).
				run_to_block(9);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// some stashes come in.
				assert_eq!(
					ElectableStashes::<Test>::get().into_iter().collect::<Vec<_>>(),
					vec![11 as AccountId, 21]
				);
				// cursor is now none
				assert_eq!(NextElectionPage::<Test>::get(), None);

				// events thus far
				assert_eq!(
					staking_events_since_last_call(),
					vec![
						Event::PagedElectionProceeded { page: 2, result: Ok(0) },
						Event::PagedElectionProceeded { page: 1, result: Err(0) },
						Event::PagedElectionProceeded { page: 0, result: Ok(2) }
					]
				);

				// upon fetching page 0, the electing started will remain in storage until the
				// era rotates.
				assert_eq!(current_era(), 0);

				// Next block the era will rotate.
				run_to_block(10);
				assert_ok!(Staking::ensure_snapshot_metadata_state(System::block_number()));

				// and all the metadata has been cleared up and ready for the next election.
				assert!(NextElectionPage::<Test>::get().is_none());
				assert!(ElectableStashes::<Test>::get().is_empty());

				// and the overall staking worked fine.
				assert_eq!(staking_events_since_last_call(), vec![Event::StakingElectionFailed]);
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

				// now request 1 page with bounds where all registered voters fit. u32::MAX
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

	#[test]
	#[should_panic]
	fn voter_snapshot_starts_from_msp_to_lsp() {
		todo!();
	}
}

mod paged_exposures {
	use super::*;

	#[test]
	fn genesis_collect_exposures_works() {
		ExtBuilder::default().multi_page_election_provider(3).build_and_execute(|| {
			// first, clean up all the era data and metadata to mimic a genesis election next.
			Staking::clear_era_information(current_era());

			// genesis election is single paged.
			let genesis_result = <<Test as Config>::GenesisElectionProvider>::elect(0u32).unwrap();
			let expected_exposures = Staking::collect_exposures(genesis_result.clone());

			Staking::try_plan_new_era(0u32, true);

			// expected exposures are stored for the expected genesis validators.
			for exposure in expected_exposures {
				assert_eq!(EraInfo::<Test>::get_full_exposure(0, &exposure.0), exposure.1);
			}
		})
	}

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
				PagedExposureMetadata { total: 1700, own: 1000, nominator_count: 3, page_count: 2 },
			);
			assert_eq!(
				ErasStakersOverview::<Test>::get(0, &2).unwrap(),
				PagedExposureMetadata { total: 2000, own: 1000, nominator_count: 2, page_count: 1 },
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
				PagedExposureMetadata { total: 2200, own: 1000, nominator_count: 5, page_count: 3 },
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

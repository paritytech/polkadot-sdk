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

use super::{Event as SignedEvent, *};
use crate::{
	mock::*,
	types::Pagify,
	verifier::{FeasibilityError, Verifier},
	Phase,
};
use frame_election_provider_support::{ElectionProvider, NposSolution};
use frame_support::storage::unhashed;
use sp_core::bounded_vec;
use sp_npos_elections::ElectionScore;

pub type T = Runtime;

mod calls {
	use super::*;
	use sp_runtime::{DispatchError, TokenError::FundsUnavailable};

	#[test]
	fn cannot_register_with_insufficient_balance() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			// 777 is not funded.
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(777), Default::default()),
				DispatchError::Token(FundsUnavailable)
			);
		});

		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			// 99 is funded but deposit is too high.
			assert_eq!(balances(99), (100, 0));
			SignedDepositBase::set(101);
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(99), Default::default()),
				DispatchError::Token(FundsUnavailable)
			);
		})
	}

	#[test]
	fn cannot_register_if_not_signed() {
		ExtBuilder::signed().build_and_execute(|| {
			assert!(!crate::Pallet::<T>::current_phase().is_signed());
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(99), Default::default()),
				Error::<T>::PhaseNotSigned
			);
		})
	}

	#[test]
	fn register_metadata_works() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			assert_eq!(balances(99), (100, 0));
			let score = ElectionScore { minimal_stake: 100, ..Default::default() };

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(balances(99), (95, 5));

			assert_eq!(Submissions::<Runtime>::metadata_iter(1).count(), 0);
			assert_eq!(Submissions::<Runtime>::metadata_iter(0).count(), 1);
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 5,
					fee: 1,
					pages: bounded_vec![false, false, false],
					reward: 3
				}
			);
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(99, ElectionScore { minimal_stake: 100, ..Default::default() })]
			);
			assert!(matches!(signed_events().as_slice(), &[
					SignedEvent::Registered(_, x, _),
				] if x == 99));

			// second ones submits
			assert_eq!(balances(999), (100, 0));
			let score = ElectionScore { minimal_stake: 90, ..Default::default() };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(999), score));
			assert_eq!(balances(999), (95, 5));

			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 999).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 5,
					fee: 1,
					pages: bounded_vec![false, false, false],
					reward: 3
				}
			);
			assert!(matches!(signed_events().as_slice(), &[
					SignedEvent::Registered(..),
					SignedEvent::Registered(_, x, _),
				] if x == 999));

			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![
					(999, ElectionScore { minimal_stake: 90, ..Default::default() }),
					(99, ElectionScore { minimal_stake: 100, ..Default::default() })
				]
			);
			assert_eq!(Submissions::<Runtime>::metadata_iter(1).count(), 0);
			assert_eq!(Submissions::<Runtime>::metadata_iter(0).count(), 2);

			// submit again with a new score.
			assert_noop!(
				SignedPallet::register(
					RuntimeOrigin::signed(999),
					ElectionScore { minimal_stake: 80, ..Default::default() }
				),
				Error::<T>::Duplicate,
			);
		})
	}

	#[test]
	fn page_submission_accumulates_fee() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			let score = ElectionScore { minimal_stake: 100, ..Default::default() };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));

			// fee for register is recorded.
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 5,
					fee: 1,
					pages: bounded_vec![false, false, false],
					reward: 3
				}
			);

			// fee for page submission is recorded.
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				0,
				Some(Default::default())
			));
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 6,
					fee: 2,
					pages: bounded_vec![true, false, false],
					reward: 3
				}
			);

			// another fee for page submission is recorded.
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				1,
				Some(Default::default())
			));
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 7,
					fee: 3,
					pages: bounded_vec![true, true, false],
					reward: 3
				}
			);

			// removal updates deposit but not the fee
			assert_ok!(SignedPallet::submit_page(RuntimeOrigin::signed(99), 1, None));

			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 6,
					fee: 3,
					pages: bounded_vec![true, false, false],
					reward: 3
				}
			);
		});
	}

	#[test]
	fn metadata_submission_sorted_based_on_stake() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			let score_from = |x| ElectionScore { minimal_stake: x, ..Default::default() };

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(91), score_from(100)));
			assert_eq!(*Submissions::<Runtime>::leaderboard(0), vec![(91, score_from(100))]);
			assert_eq!(balances(91), (95, 5));
			assert!(matches!(signed_events().as_slice(), &[SignedEvent::Registered(_, 91, _)]));

			// weaker one comes while we have space.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(92), score_from(90)));
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(92, score_from(90)), (91, score_from(100))]
			);
			assert_eq!(balances(92), (95, 5));
			assert!(matches!(
				signed_events().as_slice(),
				&[SignedEvent::Registered(..), SignedEvent::Registered(_, 92, _),]
			));

			// stronger one comes while we have have space.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(93), score_from(110)));
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(92, score_from(90)), (91, score_from(100)), (93, score_from(110))]
			);
			assert_eq!(balances(93), (95, 5));
			assert!(matches!(
				signed_events().as_slice(),
				&[
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Registered(_, 93, _),
				]
			));

			// weaker one comes while we don't have space.
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(94), score_from(80)),
				Error::<T>::QueueFull
			);
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(92, score_from(90)), (91, score_from(100)), (93, score_from(110))]
			);
			assert_eq!(balances(94), (100, 0));
			// no event has been emitted this time.
			assert!(matches!(
				signed_events().as_slice(),
				&[
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
				]
			));

			// stronger one comes while we don't have space. Eject the weakest
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(94), score_from(120)));
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(91, score_from(100)), (93, score_from(110)), (94, score_from(120))]
			);
			assert!(matches!(
				signed_events().as_slice(),
				&[
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Ejected(_, 92),
					SignedEvent::Registered(_, 94, _),
				]
			));
			assert_eq!(balances(94), (95, 5));
			// 92 is ejected, 1 unit of deposit is refunded, 4 units are slashed.
			// see the default `EjectGraceRatio`.
			assert_eq!(balances(92), (96, 0));

			// another stronger one comes, only replace the weakest.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(95), score_from(105)));
			assert_eq!(
				*Submissions::<Runtime>::leaderboard(0),
				vec![(95, score_from(105)), (93, score_from(110)), (94, score_from(120))]
			);
			assert_eq!(balances(95), (95, 5));
			// 91 is ejected, they get only a part of the deposit back.
			assert_eq!(balances(91), (96, 0));
			assert!(matches!(
				signed_events().as_slice(),
				&[
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Registered(..),
					SignedEvent::Ejected(..),
					SignedEvent::Registered(..),
					SignedEvent::Ejected(_, 91),
					SignedEvent::Registered(_, 95, _),
				]
			));
		})
	}

	#[test]
	fn can_bail_at_a_cost() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			let score = ElectionScore { minimal_stake: 100, ..Default::default() };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(balances(99), (95, 5));

			// not submitted, cannot bailout.
			assert_noop!(SignedPallet::bail(RuntimeOrigin::signed(999)), Error::<T>::NoSubmission);

			// can bail.
			assert_ok!(SignedPallet::bail(RuntimeOrigin::signed(99)));
			// 20% of the deposit returned, which is 1, 4 is slashed.
			assert_eq!(balances(99), (96, 0));
			assert_no_data_for(0, 99);

			assert_eq!(
				signed_events(),
				vec![Event::Registered(0, 99, score), Event::Bailed(0, 99)]
			);
		});
	}

	#[test]
	fn can_submit_pages() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			assert_noop!(
				SignedPallet::submit_page(RuntimeOrigin::signed(99), 0, Default::default()),
				Error::<T>::NotRegistered
			);

			assert_ok!(SignedPallet::register(
				RuntimeOrigin::signed(99),
				ElectionScore { minimal_stake: 100, ..Default::default() }
			));

			assert_eq!(Submissions::<Runtime>::pages_of(0, 99).count(), 0);
			assert_eq!(balances(99), (95, 5));

			// indices 0, 1, 2 are valid.
			assert_noop!(
				SignedPallet::submit_page(RuntimeOrigin::signed(99), 3, Default::default()),
				Error::<T>::BadPageIndex
			);

			// add the first page.
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				0,
				Some(Default::default())
			));
			assert_eq!(Submissions::<Runtime>::pages_of(0, 99).count(), 1);
			assert_eq!(balances(99), (94, 6));
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap().pages.into_inner(),
				vec![true, false, false]
			);

			// replace it again, nada.
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				0,
				Some(Default::default())
			));
			assert_eq!(Submissions::<Runtime>::pages_of(0, 99).count(), 1);
			assert_eq!(balances(99), (94, 6));

			// add a new one.
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				1,
				Some(Default::default())
			));
			assert_eq!(Submissions::<Runtime>::pages_of(0, 99).count(), 2);
			assert_eq!(balances(99), (93, 7));
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap().pages.into_inner(),
				vec![true, true, false]
			);

			// remove one, deposit is back.
			assert_ok!(SignedPallet::submit_page(RuntimeOrigin::signed(99), 0, None));
			assert_eq!(Submissions::<Runtime>::pages_of(0, 99).count(), 1);
			assert_eq!(balances(99), (94, 6));
			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap().pages.into_inner(),
				vec![false, true, false]
			);

			assert!(matches!(
				signed_events().as_slice(),
				&[
					SignedEvent::Registered(..),
					SignedEvent::Stored(.., 0),
					SignedEvent::Stored(.., 0),
					SignedEvent::Stored(.., 1),
					SignedEvent::Stored(.., 0),
				]
			));
		});
	}
}

mod e2e {
	use super::*;
	#[test]
	fn good_bad_evil() {
		// an extensive scenario: 3 solutions submitted, once rewarded, one slashed, and one
		// discarded.
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// an invalid, but weak solution.
			{
				let score =
					ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 100 };
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					0,
					Some(Default::default())
				));

				assert_eq!(balances(99), (94, 6));
			}

			// a valid, strong solution.
			let strong_score = {
				let paged = mine_full_solution().unwrap();
				load_signed_for_verification(999, paged.clone());
				assert_eq!(balances(999), (92, 8));
				paged.score
			};

			// an invalid, strong solution.
			{
				let mut score = strong_score;
				score.minimal_stake *= 2;
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(92), score));
				assert_eq!(balances(92), (95, 5));
				// we don't even bother to submit a page..
			}

			assert_eq!(
				Submissions::<Runtime>::leaderboard(0)
					.into_iter()
					.map(|(x, _)| x)
					.collect::<Vec<_>>(),
				vec![99, 999, 92]
			);

			assert_eq!(
				Submissions::<Runtime>::metadata_iter(0).collect::<Vec<_>>(),
				vec![
					(
						92,
						SubmissionMetadata {
							deposit: 5,
							fee: 1,
							reward: 3,
							claimed_score: ElectionScore {
								minimal_stake: 110,
								sum_stake: 130,
								sum_stake_squared: 8650
							},
							pages: bounded_vec![false, false, false]
						}
					),
					(
						999,
						SubmissionMetadata {
							deposit: 8,
							fee: 4,
							reward: 3,
							claimed_score: ElectionScore {
								minimal_stake: 55,
								sum_stake: 130,
								sum_stake_squared: 8650
							},
							pages: bounded_vec![true, true, true]
						}
					),
					(
						99,
						SubmissionMetadata {
							deposit: 6,
							fee: 2,
							reward: 3,
							claimed_score: ElectionScore {
								minimal_stake: 10,
								sum_stake: 10,
								sum_stake_squared: 100
							},
							pages: bounded_vec![true, false, false]
						}
					)
				]
			);

			roll_to_signed_validation_open();

			// 92 is slashed in 3+1 blocks, 999 becomes rewarded in 3 blocks, and 99 is discarded.
			roll_next();
			roll_next();
			roll_next();
			roll_next();

			// Check events after first solution (92) is rejected
			let events_after_first_rejection = verifier_events_since_last_call();
			assert_eq!(
				events_after_first_rejection,
				vec![
					crate::verifier::Event::Verified(2, 0),
					crate::verifier::Event::Verified(1, 0),
					crate::verifier::Event::Verified(0, 0),
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore),
				]
			);

			assert_eq!(
				Submissions::<Runtime>::leaderboard(0)
					.into_iter()
					.map(|(x, _)| x)
					.collect::<Vec<_>>(),
				vec![99, 999]
			);

			roll_next();
			roll_next();
			roll_next();

			// Check events after second solution (999) is accepted
			let events_after_acceptance = verifier_events_since_last_call();
			assert_eq!(
				events_after_acceptance,
				vec![
					crate::verifier::Event::Verified(2, 2),
					crate::verifier::Event::Verified(1, 2),
					crate::verifier::Event::Verified(0, 2),
					crate::verifier::Event::Queued(
						ElectionScore {
							minimal_stake: 55,
							sum_stake: 130,
							sum_stake_squared: 8650
						},
						None
					)
				]
			);

			assert_eq!(
				signed_events_since_last_call(),
				vec![
					Event::Registered(
						0,
						99,
						ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 100 }
					),
					Event::Stored(0, 99, 0),
					Event::Registered(
						0,
						999,
						ElectionScore {
							minimal_stake: 55,
							sum_stake: 130,
							sum_stake_squared: 8650
						}
					),
					Event::Stored(0, 999, 0),
					Event::Stored(0, 999, 1),
					Event::Stored(0, 999, 2),
					Event::Registered(
						0,
						92,
						ElectionScore {
							minimal_stake: 110,
							sum_stake: 130,
							sum_stake_squared: 8650
						}
					),
					Event::Slashed(0, 92, 5),
					Event::Rewarded(0, 999, 7),
				]
			);

			assert!(
				verifier_events_since_last_call().is_empty(),
				"No additional verifier events should occur"
			);

			// 99 is discarded -- for now they have some deposit collected, which they have to
			// manually collect next.
			assert_eq!(balances(99), (94, 6));
			// 999 has gotten their deposit back, plus fee and reward back.
			assert_eq!(balances(999), (107, 0));
			// 92 loses a part of their deposit for being ejected.
			assert_eq!(balances(92), (95, 0));

			// the data associated with 999 is already removed.
			assert_ok!(Submissions::<Runtime>::ensure_killed_with(&999, 0));
			// the data associated with 92 is already removed.
			assert_ok!(Submissions::<Runtime>::ensure_killed_with(&92, 0));
			// but not for 99
			assert!(Submissions::<Runtime>::ensure_killed_with(&99, 0).is_err());

			// we cannot cleanup just yet.
			assert_noop!(
				SignedPallet::clear_old_round_data(RuntimeOrigin::signed(99), 0, Pages::get()),
				Error::<T>::RoundNotOver
			);

			MultiBlock::rotate_round();

			// now we can delete our stuff.
			assert_ok!(SignedPallet::clear_old_round_data(
				RuntimeOrigin::signed(99),
				0,
				Pages::get()
			));
			// our stuff is gone.
			assert_ok!(Submissions::<Runtime>::ensure_killed_with(&99, 0));

			// check events.
			assert_eq!(signed_events_since_last_call(), vec![Event::Discarded(0, 99)]);

			// 99 now has their deposit returned.
			assert_eq!(balances(99), (100, 0));

			// signed pallet should be in 100% clean state.
			assert_ok!(Submissions::<Runtime>::ensure_killed(0));
		})
	}

	#[test]
	fn after_rejecting_does_not_call_verifier_start_if_no_leader_exists() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// Submit only an invalid solution (register but don't submit pages)
			let invalid_score =
				ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 100 };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), invalid_score));

			let current_round = SignedPallet::current_round();
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);

			roll_to_signed_validation_open();
			roll_next(); // one block so signed pallet will send start signal.
			roll_to_full_verification();

			// Check that rejection events were properly generated
			assert_eq!(
				verifier_events_since_last_call(),
				vec![
					crate::verifier::Event::Verified(2, 0),
					crate::verifier::Event::Verified(1, 0),
					crate::verifier::Event::Verified(0, 0),
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore),
				]
			);

			// Check that expected events were emitted for the rejection
			assert_eq!(
				signed_events(),
				vec![Event::Registered(0, 99, invalid_score), Event::Slashed(0, 99, 5),]
			);

			// Verify no-restart conditions are met
			assert!(crate::Pallet::<Runtime>::current_phase().is_signed_validation());
			assert!(!Submissions::<Runtime>::has_leader(current_round));
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// Verifier should remain idle for the rest of the signed validation phase
			while crate::Pallet::<Runtime>::current_phase().is_signed_validation() {
				roll_next();
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
			}

			roll_next();
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
		});
	}

	#[test]
	fn missing_pages_treated_as_empty() {
		// Test the scenario where a valid multi-page solution is mined but only some pages
		// are submitted.
		//
		// The key behavior being tested: missing pages should be treated as empty/default
		// rather than causing verification failures.
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			let paged = mine_full_solution().unwrap();
			let real_score = paged.score;
			let submitter = 99;

			assert!(
				paged.solution_pages.len() > 1,
				"Test requires a multi-page solution, got {} pages",
				paged.solution_pages.len()
			);

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(submitter), real_score));

			// Submit ONLY page 0 of the solution, deliberately skip pages 1 and 2.
			let first_page = paged.solution_pages.pagify(Pages::get()).next().unwrap();
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(submitter),
				first_page.0,
				Some(Box::new(first_page.1.clone()))
			));

			// Verify the metadata shows correct page submission status
			assert_eq!(
					    Submissions::<Runtime>::metadata_of(0, submitter).unwrap().pages.into_inner(),
					    vec![true, false, false]
					);

			// Verify that the solution data provider can access all pages without errors
			let page_0 = SignedPallet::get_page(0);
			let page_1 = SignedPallet::get_page(1);
			let page_2 = SignedPallet::get_page(2);

			// Page 0 should have actual data, pages 1 and 2 should be empty (default)
			assert_ne!(page_0, Default::default(), "Submitted page 0 should have data");
			assert_eq!(page_1, Default::default(), "Missing page 1 should return empty solution");
			assert_eq!(page_2, Default::default(), "Missing page 2 should return empty solution");

			// Start verification process
			roll_to_signed_validation_open();

			// The verification should proceed and treat the missing pages as empty
			roll_next(); // set status to ongoing
			roll_next(); // Process page 2 (missing, treated as empty)
			roll_next(); // Process page 1 (missing, treated as empty)
			roll_next(); // Process page 0 (submitted with real data)

			// Check only the events from this verification
			assert_eq!(
				verifier_events_since_last_call(),
				vec![
					crate::verifier::Event::Verified(2, 0), // Page 2 missing, treated as empty
					crate::verifier::Event::Verified(1, 0), // Page 1 missing, treated as empty
					crate::verifier::Event::Verified(0, 2), // Page 0 submitted with real data
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore)
				],
				"Missing pages should be treated as empty, but partial submission leads to verification failure due to score mismatch"
			);
		});
	}

	#[test]
	fn not_all_solutions_verified_signed_verification_to_unsigned() {
		// Test that in case of multiple verifications when not all of them are verified within the
		// signed validation phase, the verifier should stop and go back to
		// idle when transitioning from signed validation to unsigned phase, while keeping not yet
		// verified solutions.
		// NOTE: the signed validation phase must be a multiple of the number of pages, which
		// ensures that solutions cannot be halfway verified
		ExtBuilder::signed()
			.pages(3)
			.signed_validation_phase(3) // so that we can validate only one solution per validation phase
			.unsigned_phase(3)
			.build_and_execute(|| {
				roll_to_signed_open();

				// Submit invalid solution with high score but no pages (will be slashed)
				{
					let mut strong_score = mine_full_solution().unwrap().score;
					strong_score.minimal_stake *= 2;
					assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), strong_score));
					// Don't submit any pages - this will cause it to be slashed
				}

				// Submit good solution with all pages
				{
					let strong_solution = mine_full_solution().unwrap();
					load_signed_for_verification(999, strong_solution.clone());
				}

				let current_round = SignedPallet::current_round();
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 2);

				roll_to_signed_validation_open();
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(3));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Nothing));
				roll_next(); // one block so signed-pallet will send the start signal
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(2));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(2)));

				// Initial verifier state should have no queued solution
				assert_eq!(VerifierPallet::queued_score(), None);

				// Bad solution is the current leader
				let mut remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 99);

				// Bad solution starts verification
				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(1));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Nothing));

				// Check events after bad solution verification completes
				assert_eq!(
					verifier_events_since_last_call(),
					vec![
						crate::verifier::Event::Verified(2, 0),
						crate::verifier::Event::Verified(1, 0),
						crate::verifier::Event::Verified(0, 0),
						crate::verifier::Event::VerificationFailed(
							0,
							FeasibilityError::InvalidScore
						),
					]
				);

				// Verify phase transition to unsigned
				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(UnsignedPhase::get() - 1));

				// Check that invalid solution is slashed but good solution remains
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
				assert!(Submissions::<Runtime>::has_leader(current_round));
				remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 999); // Good solution remains

				// Check that expected events were emitted for the rejection
				assert_eq!(
					signed_events(),
					vec![
						Event::Registered(
							0,
							99,
							ElectionScore {
								minimal_stake: 110,
								sum_stake: 130,
								sum_stake_squared: 8650
							}
						),
						Event::Registered(
							0,
							999,
							ElectionScore {
								minimal_stake: 55,
								sum_stake: 130,
								sum_stake_squared: 8650
							}
						),
						Event::Stored(0, 999, 0),
						Event::Stored(0, 999, 1),
						Event::Stored(0, 999, 2),
						Event::Slashed(0, 99, 5),
					]
				);

				// Verify no additional events occurred
				assert!(
					verifier_events_since_last_call().is_empty(),
					"No additional verifier events should occur"
				);

				// Verifier should be STOPPED when transitioning to unsigned
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// Verify no solution was queued (verification was stopped, not completed)
				assert_eq!(VerifierPallet::queued_score(), None);
			});
	}

	#[test]
	fn incomplete_signed_verification_1_solution_back_to_signed_then_done() {
		ExtBuilder::signed()
			.pages(3)
			.signed_validation_phase(3)
			.are_we_done(AreWeDoneModes::BackToSigned)
			.build_and_execute(|| {
				roll_to_signed_open();

				// submit a bad solution with junk in page 2
				{
					let mut bad_solution = mine_full_solution().unwrap();
					bad_solution.solution_pages[1usize].corrupt();
					load_signed_for_verification(99, bad_solution);
				}

				// submit a good solution that will be sent to next round as we won't have enough
				// time for it.
				{
					let good_solution = mine_full_solution().unwrap();
					load_signed_for_verification(999, good_solution);
				}

				roll_to_signed_validation_open();
				let _ = signed_events_since_last_call();

				// henceforth we proceed block-by-block for better visibility of what is happening.

				// 3 blocks to reject the first one: 1 to set status to ongoing, and 2 to verify
				roll_next();
				roll_next();
				roll_next();

				assert_eq!(
					verifier_events_since_last_call(),
					vec![
						crate::verifier::Event::Verified(2, 2),
						crate::verifier::Event::VerificationFailed(
							1,
							FeasibilityError::NposElection(
								sp_npos_elections::Error::SolutionInvalidIndex
							)
						),
					]
				);
				assert_eq!(
					signed_events_since_last_call(),
					vec![crate::signed::Event::Slashed(0, 99, 8)]
				);

				// we have 1 block left in signed verification, but we cannot do anything here.
				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::SignedValidation(0));

				// we go back to signed next
				roll_next();
				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::Signed(4));

				// no one submits again, and we go to verification again
				roll_to_signed_validation_open();

				// 4 block to verify: 1 to set status, and 3 to verify
				roll_next();
				roll_next();
				roll_next();
				roll_next();

				assert_eq!(
					verifier_events_since_last_call(),
					vec![
						crate::verifier::Event::Verified(2, 2),
						crate::verifier::Event::Verified(1, 2),
						crate::verifier::Event::Verified(0, 2),
						crate::verifier::Event::Queued(
							ElectionScore {
								minimal_stake: 55,
								sum_stake: 130,
								sum_stake_squared: 8650
							},
							None
						)
					]
				);
				assert_eq!(
					signed_events_since_last_call(),
					vec![crate::signed::Event::Rewarded(0, 999, 7)]
				);

				// verifier is `Nothing`, and will remain so as signed-pallet will not start it
				// again.

				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::SignedValidation(0));
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// next block we go to done
				roll_next();
				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::Done);
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
			})
	}

	#[test]
	fn not_all_solutions_verified_signed_verification_incomplete_to_signed() {
		// Test that demonstrates early failure and no verifier restart due to insufficient blocks:
		// - SignedValidation phase of 3 blocks with 3-page solutions
		// - Bad solution with high score fails early on page 1 with invalid voter index
		// - Good solution with lower score is submitted but cannot be verified
		// - After bad solution fails, verifier is NOT restarted because there are only 2 blocks
		//   remaining and we need 3 blocks for a 3-page solution
		// - System transitions back to Signed phase with good solution still queued for next round
		ExtBuilder::signed()
			.pages(3)
			.signed_validation_phase(3) // 3 blocks for validation
			.are_we_done(crate::mock::AreWeDoneModes::BackToSigned) // Revert to signed if no solution
			.build_and_execute(|| {
				roll_to_signed_open();

				// Submit bad solution with high score that will fail early during verification
				{
					let mut bad_solution = mine_full_solution().unwrap();
					bad_solution.score.minimal_stake *= 2;
					bad_solution.solution_pages[1usize].corrupt();
					load_signed_for_verification(99, bad_solution);
				}

				// Submit good solution with lower score (all pages valid)
				{
					let good_solution = mine_full_solution().unwrap();
					load_signed_for_verification(999, good_solution.clone());
				}

				let current_round = SignedPallet::current_round();
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 2);

				roll_to_signed_validation_open();
				roll_next(); // one block so signed-pallet will send the start signal
				assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Bad solution is the current leader (higher score)
				let leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(leader.0, 99);

				// Block 1: Start verification of bad solution (page 2)
				roll_next(); // SignedValidation(2) -> SignedValidation(1)
				assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(1)));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Should verify page 2 successfully
				assert_eq!(
					verifier_events_since_last_call(),
					vec![crate::verifier::Event::Verified(2, 2)]
				);

				// Block 2: Continue verification (page 1) - bad solution should fail here
				roll_next(); // SignedValidation(1) -> SignedValidation(0)
				assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(0)));

				// Bad solution should fail early with NposElection error on page 1 verification
				assert_eq!(
					verifier_events_since_last_call(),
					vec![crate::verifier::Event::VerificationFailed(
						1,
						FeasibilityError::NposElection(
							sp_npos_elections::Error::SolutionInvalidIndex
						)
					)]
				);

				// Block 3: No more verification needed - bad solution already failed
				roll_next(); // SignedValidation(0) -> transitions to next phase

				// Check that bad solution is removed and good solution remains
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
				let remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 999); // Good solution is now leader

				// CRITICAL: Verifier should NOT restart because there are only 2 blocks remaining
				// (after using 1 block and failing early, but still need 3 blocks for a full
				// solution) Should transition back to Signed phase since verification cannot
				// restart
				assert!(matches!(MultiBlock::current_phase(), Phase::Signed(_)));
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// Check events - bad solution should be slashed
				let all_signed_events = signed_events();
				assert!(all_signed_events.iter().any(|e| matches!(e, Event::Slashed(0, 99, _))));

				// At the end of signed phase, verifier is still idle
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				roll_to_signed_validation_open();
				roll_next(); // one block so signed-pallet will send the start signal

				// Now in the next validation phase, the good solution starts verification
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);

				// Verify the good solution over 3 blocks
				roll_next(); // Process page 2
				roll_next(); // Process page 1
				roll_next(); // Process page 0

				// Good solution should be fully verified and accepted
				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::SignedValidation(0));
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			});
	}

	#[test]
	fn max_queue_with_single_valid_solution_at_end() {
		// Test that when the submission queue is at capacity (MaxSubmissions = 3), with only the
		// third submission being valid, the verifier processes the entire queue sequentially and
		// eventually accepts the final valid solution.

		// Set max submissions to 3 for this test
		ExtBuilder::signed()
			.max_signed_submissions(3)
			.signed_validation_phase(9) // 3 solutions * 3 pages per solution = 9 blocks needed
			.build_and_execute(|| {
				roll_to_signed_open();
				assert_full_snapshot();

				let current_round = SignedPallet::current_round();

				// Submit two invalid solutions (register but don't submit pages)
				let invalid_score1 = {
					let mut score = mine_full_solution().unwrap().score;
					score.minimal_stake *= 2; // Make it attractive but it will be invalid
					score
				};
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(91), invalid_score1));

				let invalid_score2 = {
					let mut score = mine_full_solution().unwrap().score;
					score.minimal_stake *= 3; // Make it even more attractive but still invalid
					score
				};
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(92), invalid_score2));

				// Submit one valid solution at the end
				let valid_solution = mine_full_solution().unwrap();
				load_signed_for_verification(99, valid_solution.clone());

				// Verify we have 3 submissions at max capacity
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 3);
				assert!(Submissions::<Runtime>::has_leader(current_round));

				// Move to verification phase
				roll_to_signed_validation_open();
				roll_next(); // one block so signed-pallet will send the start signal
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Process first invalid solution (91)
				roll_next(); // Process page 2
				roll_next(); // Process page 1
				roll_next(); // Process page 0 and reject solution

				// Check events after first solution (91) is rejected
				let events_first_solution = verifier_events_since_last_call();
				assert_eq!(
					events_first_solution,
					vec![
						crate::verifier::Event::Verified(2, 0),
						crate::verifier::Event::Verified(1, 0),
						crate::verifier::Event::Verified(0, 0),
						crate::verifier::Event::VerificationFailed(
							0,
							FeasibilityError::InvalidScore
						),
					]
				);

				// Verify first solution was slashed and removed
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 2);
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Process second invalid solution (92)
				roll_next(); // Process page 2
				roll_next(); // Process page 1
				roll_next(); // Process page 0 and reject solution

				// Check events after second solution (92) is rejected
				assert_eq!(
					verifier_events_since_last_call(),
					vec![
						crate::verifier::Event::Verified(2, 0),
						crate::verifier::Event::Verified(1, 0),
						crate::verifier::Event::Verified(0, 0),
						crate::verifier::Event::VerificationFailed(
							0,
							FeasibilityError::InvalidScore
						),
					]
				);

				// Verify second solution was slashed and removed
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Verify the last remaining solution is our valid one
				let leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(leader.0, 99);

				// Process the final valid solution
				roll_next(); // Process page 2 of valid solution
				roll_next(); // Process page 1 of valid solution
				roll_next(); // Process page 0 of valid solution and accept it

				// Check events after valid solution (99) is accepted
				assert_eq!(
					verifier_events_since_last_call(),
					vec![
						crate::verifier::Event::Verified(2, 2),
						crate::verifier::Event::Verified(1, 2),
						crate::verifier::Event::Verified(0, 2),
						crate::verifier::Event::Queued(valid_solution.score, None),
					]
				);

				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::SignedValidation(0));
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// Check that all expected events were emitted in the correct order
				assert_eq!(
					signed_events(),
					vec![
						Event::Registered(0, 91, invalid_score1),
						Event::Registered(0, 92, invalid_score2),
						Event::Registered(0, 99, valid_solution.score),
						Event::Stored(0, 99, 0),
						Event::Stored(0, 99, 1),
						Event::Stored(0, 99, 2),
						Event::Slashed(0, 92, 5),
						Event::Slashed(0, 91, 5),
						Event::Rewarded(0, 99, 7),
					]
				);

				// finally done
				roll_next();
				assert_eq!(crate::Pallet::<T>::current_phase(), Phase::Done);
				// verifier has done nothing
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
				assert!(
					verifier_events_since_last_call().is_empty(),
					"No additional verifier events should occur"
				);
			});
	}
}

mod invulnerables {
	use super::*;

	fn make_invulnerable(who: AccountId) {
		SignedPallet::set_invulnerables(RuntimeOrigin::root(), vec![who]).unwrap();
	}

	#[test]
	fn set_invulnerables_requires_admin_origin() {
		ExtBuilder::signed().build_and_execute(|| {
			// Should fail with non-admin origin
			assert_noop!(
				SignedPallet::set_invulnerables(RuntimeOrigin::signed(1), vec![99]),
				sp_runtime::DispatchError::BadOrigin
			);

			// Should succeed with admin origin (root)
			assert_ok!(SignedPallet::set_invulnerables(RuntimeOrigin::root(), vec![99]));
		});
	}

	#[test]
	fn set_invulnerables_too_many_fails() {
		ExtBuilder::signed().build_and_execute(|| {
			// Try to set more than 16 invulnerables (ConstU32<16> limit)
			let too_many: Vec<AccountId> = (1..=17).collect();
			assert_noop!(
				SignedPallet::set_invulnerables(RuntimeOrigin::root(), too_many),
				Error::<Runtime>::TooManyInvulnerables
			);
		});
	}

	#[test]
	fn invulnerable_pays_different_deposit_independent_of_pages() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			make_invulnerable(99);
			assert_eq!(balances(99), (100, 0));
			assert_eq!(<Runtime as crate::signed::Config>::InvulnerableDeposit::get(), 7);

			let score = ElectionScore { minimal_stake: 100, ..Default::default() };

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(balances(99), (93, 7));

			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 7,
					// ^^ fixed deposit
					fee: 1,
					// ^^ fee accumulates
					pages: bounded_vec![false, false, false],
					reward: 3
				}
			);

			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				0,
				Some(Default::default())
			));

			assert_eq!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 7,
					// ^^ deposit still fixed at 7
					fee: 2,
					// ^^ fee accumulates
					pages: bounded_vec![true, false, false],
					reward: 3
				}
			);

			// Submit additional pages to verify deposit remains fixed
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				1,
				Some(Default::default())
			));
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				2,
				Some(Default::default())
			));

			// Verify final state after all pages are submitted
			let final_metadata = Submissions::<Runtime>::metadata_of(0, 99).unwrap();
			assert_eq!(
				final_metadata,
				SubmissionMetadata {
					claimed_score: score,
					deposit: 7,
					// ^^ deposit still fixed at 7 even after submitting all pages
					fee: 4,
					// ^^ fee accumulates: register(1) + page0(1) + page1(1) + page2(1) = 4 total
					pages: bounded_vec![true, true, true],
					// ^^ all pages submitted
					reward: 3
				}
			);

			// Balance should remain the same - only deposit is held, fees tracked in metadata
			assert_eq!(balances(99), (93, 7));
		})
	}

	#[test]
	fn multiple_invulnerables_all_get_fixed_deposit() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// Set multiple invulnerables
			SignedPallet::set_invulnerables(RuntimeOrigin::root(), vec![99, 98, 97]).unwrap();

			let score = ElectionScore { minimal_stake: 100, ..Default::default() };

			// All should pay the same fixed deposit
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(98), score));
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(97), score));

			// All should have paid exactly InvulnerableDeposit (7)
			assert_eq!(balances(99), (93, 7));
			assert_eq!(balances(98), (93, 7));
			assert_eq!(balances(97), (93, 7));
		});
	}

	#[test]
	fn ejected_invulnerable_gets_deposit_back() {
		ExtBuilder::signed().max_signed_submissions(2).build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();
			make_invulnerable(99);

			// by default, we pay back 20% of the discarded deposit back
			assert_eq!(
				<Runtime as crate::signed::Config>::EjectGraceRatio::get(),
				Perbill::from_percent(20)
			);

			// submit 99 as invulnerable
			assert_ok!(SignedPallet::register(
				RuntimeOrigin::signed(99),
				ElectionScore { minimal_stake: 100, ..Default::default() }
			));
			assert!(matches!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata { deposit: 7, .. }
			));

			// submit 98 as normal
			assert_ok!(SignedPallet::register(
				RuntimeOrigin::signed(98),
				ElectionScore { minimal_stake: 101, ..Default::default() }
			));
			assert!(matches!(
				Submissions::<Runtime>::metadata_of(0, 98).unwrap(),
				SubmissionMetadata { deposit: 5, .. }
			));
			let _ = signed_events_since_last_call();
			assert_eq!(balances(99), (93, 7));
			assert_eq!(balances(98), (95, 5));

			// submit 97 and 96 with higher scores, eject both of the previous ones
			assert_ok!(SignedPallet::register(
				RuntimeOrigin::signed(97),
				ElectionScore { minimal_stake: 200, ..Default::default() }
			));
			assert_ok!(SignedPallet::register(
				RuntimeOrigin::signed(96),
				ElectionScore { minimal_stake: 201, ..Default::default() }
			));

			assert_eq!(
				signed_events_since_last_call(),
				vec![
					Event::Ejected(0, 99),
					Event::Registered(
						0,
						97,
						ElectionScore { minimal_stake: 200, sum_stake: 0, sum_stake_squared: 0 }
					),
					Event::Ejected(0, 98),
					Event::Registered(
						0,
						96,
						ElectionScore { minimal_stake: 201, sum_stake: 0, sum_stake_squared: 0 }
					)
				]
			);

			// 99 gets everything back
			assert_eq!(balances(99), (100, 0));
			// 98 gets 20% x 5 = 1 back
			assert_eq!(balances(98), (96, 0));
		})
	}

	#[test]
	fn discarded_invulnerable_gets_fee_and_deposit_back() {
		ExtBuilder::signed().build_and_execute(|| {
			make_invulnerable(99);

			roll_to_signed_open();

			// a weak, discarded solution from 99
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));
			assert!(matches!(
				Submissions::<Runtime>::metadata_of(0, 99).unwrap(),
				SubmissionMetadata { deposit: 7, fee: 1, .. }
			));
			// note: we don't actually collect the tx-fee in the tests
			assert_eq!(balances(99), (93, 7));

			// a valid, strong solution.
			let paged = mine_full_solution().unwrap();
			load_signed_for_verification(98, paged.clone());
			let _ = signed_events_since_last_call();

			roll_to_signed_validation_open();
			roll_next();
			roll_to_full_verification();

			assert_eq!(
				verifier_events_since_last_call(),
				vec![
					crate::verifier::Event::Verified(2, 2),
					crate::verifier::Event::Verified(1, 2),
					crate::verifier::Event::Verified(0, 2),
					crate::verifier::Event::Queued(
						ElectionScore {
							minimal_stake: 55,
							sum_stake: 130,
							sum_stake_squared: 8650
						},
						None
					)
				]
			);
			assert_eq!(signed_events_since_last_call(), vec![Event::Rewarded(0, 98, 7)]);

			// not relevant: signed will not start verification again
			roll_next();
			assert!(verifier_events_since_last_call().is_empty());
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// fast-forward to round being over
			roll_to_done();
			MultiBlock::rotate_round();

			// now we can delete our stuff.
			assert_ok!(SignedPallet::clear_old_round_data(
				RuntimeOrigin::signed(99),
				0,
				Pages::get()
			));
			assert_eq!(signed_events_since_last_call(), vec![Event::Discarded(0, 99)]);
			// full deposit is returned + tx-fee
			assert_eq!(balances(99), (101, 0));
		})
	}

	#[test]
	fn removing_from_invulnerables_affects_future_submissions() {
		ExtBuilder::signed().build_and_execute(|| {
			// Initially make invulnerable
			make_invulnerable(99);

			roll_to_signed_open();
			assert_full_snapshot();

			let score = ElectionScore { minimal_stake: 100, ..Default::default() };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(Submissions::<Runtime>::metadata_of(0, 99).unwrap().deposit, 7);

			// Clear round and remove from invulnerables
			roll_to_done();
			MultiBlock::rotate_round();
			SignedPallet::set_invulnerables(RuntimeOrigin::root(), vec![]).unwrap();

			// Start new election
			assert_ok!(MultiBlock::start());

			// Now roll to signed open for the new round
			roll_to_signed_open();
			assert_full_snapshot();
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			let new_deposit =
				Submissions::<Runtime>::metadata_of(MultiBlock::round(), 99).unwrap().deposit;
			assert_eq!(new_deposit, 5); // Should not be fixed deposit anymore
		});
	}
}

mod defensive_tests {
	use super::*;

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "Defensive failure has been triggered!")]
	fn missing_leader_storage_triggers_defensive() {
		// Call Verifier::start and mutate storage to delete the score of the leader.
		// This creates the scenario where score becomes unavailable during verification
		ExtBuilder::signed().build_and_execute(|| {
			// Setup a leader first
			roll_to_signed_open();
			assert_eq!(balances(99), (100, 0));
			let score = ElectionScore { minimal_stake: 100, ..Default::default() };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(balances(99), (95, 5)); // deposit taken

			// Submit a solution page to have something to verify
			assert_ok!(SignedPallet::submit_page(RuntimeOrigin::signed(99), 0, Default::default()));

			assert_ok!(<VerifierPallet as AsynchronousVerifier>::start());

			roll_next(); // Process page 2
			roll_next(); // Process page 1

			assert_eq!(
				signed_events_since_last_call(),
				vec![
					Event::Registered(
						0,
						99,
						ElectionScore { minimal_stake: 100, sum_stake: 0, sum_stake_squared: 0 }
					),
					Event::Stored(0, 99, 0)
				]
			);

			// Now mutate storage to delete the score of the leader.
			let current_round = SignedPallet::current_round();

			// Delete the score storage
			let full_key =
				crate::signed::pallet::SortedScores::<Runtime>::hashed_key_for(current_round);
			unhashed::kill(&full_key);

			// Complete verification - this should trigger score unavailable detection
			roll_next();
		});
	}

	#[test]
	#[should_panic(expected = "Defensive failure has been triggered!")]
	fn get_score_defensive_when_no_leader() {
		// Test that get_score() triggers defensive failure when no leader exists
		ExtBuilder::signed().build_and_execute(|| {
			// Ensure we're in signed phase but no submissions exist
			roll_to_signed_open();

			// Verify no leader exists
			assert_eq!(Submissions::<Runtime>::leader(SignedPallet::current_round()), None);

			// get_score should trigger defensive failure when no leader exists
			let _score = SignedPallet::get_score();
		});
	}

	#[test]
	#[should_panic(expected = "Defensive failure has been triggered!")]
	fn get_page_defensive_when_no_leader() {
		// Test that get_page() triggers defensive failure when no leader exists
		ExtBuilder::signed().build_and_execute(|| {
			// Ensure we're in signed phase but no submissions exist
			roll_to_signed_open();

			// Verify no leader exists
			assert_eq!(Submissions::<Runtime>::leader(SignedPallet::current_round()), None);

			// get_page should trigger defensive failure when no leader exists
			let _page = SignedPallet::get_page(0);
		});
	}
}

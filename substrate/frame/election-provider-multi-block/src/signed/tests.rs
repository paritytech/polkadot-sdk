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

			// 92 is slashed in 3 blocks, 999 becomes rewarded in 3 blocks, , and 99 is discarded.
			roll_next();
			roll_next();
			roll_next();

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

			assert_eq!(
				verifier_events(),
				vec![
					crate::verifier::Event::Verified(2, 0),
					crate::verifier::Event::Verified(1, 0),
					crate::verifier::Event::Verified(0, 0),
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore),
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
			assert_eq!(signed_events_since_last_call(), vec![Event::Discarded(1, 99)]);

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

			roll_to_full_verification();

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

			// Check that expected events were emitted for the rejection
			assert_eq!(
				signed_events(),
				vec![Event::Registered(0, 99, invalid_score), Event::Slashed(0, 99, 5),]
			);

			roll_next();
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
		});
	}

	#[test]
	fn after_accepting_one_solution_verifier_is_idle_if_no_leader_exists() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// Load and verify a good solution
			let good_solution = mine_full_solution().unwrap();
			load_signed_for_verification_and_start_and_roll_to_verified(
				99,
				good_solution.clone(),
				0,
			);

			assert_eq!(
				signed_events(),
				vec![
					Event::Registered(0, 99, good_solution.score),
					Event::Stored(0, 99, 0),
					Event::Stored(0, 99, 1),
					Event::Stored(0, 99, 2),
					Event::Rewarded(0, 99, 7),
				]
			);

			// Check verifier events
			assert_eq!(
				verifier_events(),
				vec![
					crate::verifier::Event::Verified(2, 2),
					crate::verifier::Event::Verified(1, 2),
					crate::verifier::Event::Verified(0, 2),
					crate::verifier::Event::Queued(good_solution.score, None),
				]
			);

			roll_to_done();
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
			roll_next(); // Process page 2 (missing, treated as empty)
			roll_next(); // Process page 1 (missing, treated as empty)
			roll_next(); // Process page 0 (submitted with real data)

			// Check that verification handled the missing pages gracefully.
			// Missing pages should be treated as empty pages without errors.
			assert_eq!(
				verifier_events(),
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
			.unsigned_phase(1)
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
				assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Initial verifier state should have no queued solution
				assert_eq!(VerifierPallet::queued_score(), None);

				// Bad solution is the current leader
				let mut remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 99);

				// Bad solution starts verification
				roll_next(); // SignedValidation(2) -> SignedValidation(1)
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next(); // SignedValidation(1) -> SignedValidation(0)
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next(); // SignedValidation(0) -> Unsigned(4) (verification of bad solution complete,
				 // verification of the 2nd solution hasn't started yet)

				// Verify phase transition to unsigned
				assert!(matches!(MultiBlock::current_phase(), Phase::Unsigned(_)));

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
				assert_eq!(
					verifier_events(),
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

				// Verifier should be STOPPED when transitioning to unsigned
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// Verify no solution was queued (verification was stopped, not completed)
				assert_eq!(VerifierPallet::queued_score(), None);
			});
	}

	#[test]
	fn not_all_solutions_verified_signed_verification_incomplete_to_signed() {
		// Test that in case of multiple verifications when not all of them are verified within the
		// signed validation phase, the verifier should stop and go back to
		// idle when transitioning from signed validation to signed phase, while keeping not yet
		// verified solutions.
		// NOTE: the signed validation phase must be a multiple of the number of pages, which
		// ensures that solutions cannot be halfway verified
		ExtBuilder::signed()
			.pages(3)
			.signed_validation_phase(3) // so that we can validate only one solution per validation phase
			.are_we_done(crate::mock::AreWeDoneModes::BackToSigned) // Revert to signed if no solution
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
				assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)));
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Initial verifier state should have no queued solution
				assert_eq!(VerifierPallet::queued_score(), None);

				// Bad solution is the current leader
				let mut remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 99);

				// Bad solution starts verification
				roll_next(); // SignedValidation(2) -> SignedValidation(1)
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next(); // SignedValidation(1) -> SignedValidation(0)
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				roll_next(); // SignedValidation(0) -> Unsigned(4) (verification of bad solution complete,
				 // verification of the 2nd solution hasn't started yet)

				// Verify phase transition back to signed
				assert!(matches!(MultiBlock::current_phase(), Phase::Signed(_)));

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
				assert_eq!(
					verifier_events(),
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

				roll_to_last_signed();

				// at the end of the signed phase, the verifier remains idle
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);
				assert_eq!(VerifierPallet::queued_score(), None);

				roll_to_signed_validation_open();

				// now the verification of the good solution starts
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));
				assert_eq!(VerifierPallet::queued_score(), None);
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
				remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
				assert_eq!(remaining_leader.0, 999); // Good solution remains

				roll_next(); // processes page 2 of good solution
				roll_next(); // processes page 1 of good solution
				roll_next(); // processes page 0 of good solution

				// Check verifier status - should be Nothing after completion
				assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

				// Check good solution is fully verified and removed from queue
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
				assert!(!Submissions::<Runtime>::has_leader(current_round));
			});
	}

	#[test]
	fn max_queue_with_single_valid_solution_at_end() {
		// Test that when the submission queue is at capacity (MaxSubmissions = 3), with only the
		// third submission being valid, the verifier processes the entire queue sequentially and
		// eventually accepts the final valid solution.

		// Set max submissions to 3 for this test
		SignedMaxSubmissions::set(3);
		ExtBuilder::signed()
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
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Process first invalid solution (91)
				roll_next(); // Process page 2
				roll_next(); // Process page 1
				roll_next(); // Process page 0 and reject solution

				// Verify first solution was slashed and removed
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 2);
				assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

				// Process second invalid solution (92)
				roll_next(); // Process page 2
				roll_next(); // Process page 1
				roll_next(); // Process page 0 and reject solution

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

				// Roll until done and check final state
				roll_to_done();
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

				// Verify verifier events show all verifications
				assert_eq!(
					verifier_events(),
					vec![
						crate::verifier::Event::Verified(2, 0),
						crate::verifier::Event::Verified(1, 0),
						crate::verifier::Event::Verified(0, 0),
						crate::verifier::Event::VerificationFailed(
							0,
							FeasibilityError::InvalidScore
						),
						crate::verifier::Event::Verified(2, 0),
						crate::verifier::Event::Verified(1, 0),
						crate::verifier::Event::Verified(0, 0),
						crate::verifier::Event::VerificationFailed(
							0,
							FeasibilityError::InvalidScore
						),
						crate::verifier::Event::Verified(2, 2),
						crate::verifier::Event::Verified(1, 2),
						crate::verifier::Event::Verified(0, 2),
						crate::verifier::Event::Queued(valid_solution.score, None),
					]
				);
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

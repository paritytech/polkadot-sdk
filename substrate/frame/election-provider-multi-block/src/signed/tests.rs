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
	fn after_rejecting_calls_verifier_start_again_if_leader_exists() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

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

			// Go to signed validation phase
			roll_to_signed_validation_open();
			assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

			roll_next(); // Block 1
			roll_next(); // Block 2
			roll_next(); // Block 3: invalid solution (99) gets slashed for having no pages

			// Check that invalid solution is slashed but good solution remains
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
			assert!(Submissions::<Runtime>::has_leader(current_round));
			let remaining_leader = Submissions::<Runtime>::leader(current_round).unwrap();
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
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore),
				]
			);

			// Check verifier status - should still be ongoing.
			assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

			roll_next(); // Block 4: processes page 2 of good solution
			roll_next(); // Block 5: processes page 1 of good solution
			roll_next(); // Block 6: processes page 0 of good solution

			// Check verifier status - should be Nothing after completion
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// Check good solution is fully verified and removed from queue
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			assert!(!Submissions::<Runtime>::has_leader(current_round));
		});
	}

	#[test]
	fn after_rejecting_does_not_call_verifier_start_if_no_leader_exists() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// Submit only an invalid solution
			let score = ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 100 };
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(99),
				0,
				Some(Default::default())
			));

			// Verify we have exactly one submission
			let current_round = SignedPallet::current_round();
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
			assert!(Submissions::<Runtime>::has_leader(current_round));

			roll_to_signed_validation_open();

			assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

			roll_next(); // Block 1: processes page 2 (empty)
			roll_next(); // Block 2: processes page 1  (empty)
			roll_next(); // Block 3: processes page 0 (the only submitted page)

			// After 3 blocks, the invalid solution should be processed and discarded
			// Verify no-restart conditions are met
			assert!(crate::Pallet::<Runtime>::current_phase().is_signed_validation());
			assert!(!Submissions::<Runtime>::has_leader(current_round));
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// Check that expected events were emitted for the rejection
			assert_eq!(
				signed_events(),
				vec![
					Event::Registered(
						0,
						99,
						ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 100 }
					),
					Event::Stored(0, 99, 0),
					Event::Slashed(0, 99, 6),
				]
			);
			assert_eq!(
				verifier_events(),
				vec![
					crate::verifier::Event::Verified(2, 0),
					crate::verifier::Event::Verified(1, 0),
					crate::verifier::Event::Verified(0, 0),
					crate::verifier::Event::VerificationFailed(0, FeasibilityError::InvalidScore),
				]
			);
		});
	}

	#[test]
	fn after_accepting_one_solution_verifier_is_idle_if_no_leader_exists() {
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			// Submit only one good solution that will be accepted
			let good_solution = mine_full_solution().unwrap();
			load_signed_for_verification(99, good_solution.clone());

			// Verify we have exactly one submission
			let current_round = SignedPallet::current_round();
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 1);
			assert!(Submissions::<Runtime>::has_leader(current_round));

			// Move to verification phase
			roll_to_signed_validation_open();
			assert!(matches!(VerifierPallet::status(), crate::verifier::Status::Ongoing(_)));

			// The good solution will be processed and accepted over 3 blocks
			roll_next(); // Block 1: processes page 2
			roll_next(); // Block 2: processes page 1
			roll_next(); // Block 3: processes page 0 and solution gets queued/rewarded

			// After acceptance, verify no-restart conditions are met
			assert!(crate::Pallet::<Runtime>::current_phase().is_signed_validation());
			assert!(!Submissions::<Runtime>::has_leader(current_round));
			assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// Verify verifier stays idle until we reach Done phase (no restart)
			while !MultiBlock::current_phase().is_done() {
				roll_next();
				assert_eq!(
					VerifierPallet::status(),
					crate::verifier::Status::Nothing,
					"Verifier should remain idle until Done phase"
				);
				assert!(!Submissions::<Runtime>::has_leader(current_round));
				assert_eq!(Submissions::<Runtime>::submitters_count(current_round), 0);
			}

			// Verify we reached Done phase with verifier still idle
			assert_eq!(MultiBlock::current_phase(), Phase::Done);
			assert_eq!(VerifierPallet::status(), crate::verifier::Status::Nothing);

			// Check that expected events were emitted for the acceptance
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
			assert_eq!(
				verifier_events(),
				vec![
					crate::verifier::Event::Verified(2, 2),
					crate::verifier::Event::Verified(1, 2),
					crate::verifier::Event::Verified(0, 2),
					crate::verifier::Event::Queued(good_solution.score, None),
				]
			);
		});
	}

	#[test]
	fn missing_pages_treated_as_empty() {
		// Test the scenario where a valid multi-page solution is mined but only some pages
		// are submitted.
		//
		// The key behavior being tested: missing pages should be treated as empty/default
		// rather than causing DataUnavailable errors or verification failures.
		ExtBuilder::signed().build_and_execute(|| {
			roll_to_signed_open();
			assert_full_snapshot();

			let paged = mine_full_solution().unwrap();
			let real_score = paged.score;
			let submitter = 99;
			let initial_balance = Balances::free_balance(&submitter);

			assert!(
				paged.solution_pages.len() > 1,
				"Test requires a multi-page solution, got {} pages",
				paged.solution_pages.len()
			);

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(submitter), real_score));
			assert_eq!(
				balances(submitter),
				(initial_balance - SignedDepositBase::get(), SignedDepositBase::get())
			);

			// Submit ONLY page 0 of the solution, deliberately skip pages 1 and 2.
			let first_page = paged.solution_pages.pagify(Pages::get()).next().unwrap();
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(submitter),
				first_page.0,
				Some(Box::new(first_page.1.clone()))
			));

			// Verify the metadata shows correct page submission status
			let metadata = Submissions::<Runtime>::metadata_of(0, submitter).unwrap();
			let pages_status = metadata.pages.into_inner();
			assert_eq!(pages_status[0], true, "Page 0 should be submitted");
			assert_eq!(pages_status[1], false, "Page 1 should not be submitted");
			assert_eq!(pages_status[2], false, "Page 2 should not be submitted");

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
			// Note that even with missing pages, crate::verifier::Event::VerificationDataUnavailable(_) should not be emitted
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
}

mod defensive_tests {
	use super::*;

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "Defensive failure has been triggered!")]
	fn verification_data_unavailable_score_slashes_submitter() {
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

			// Should have slashed the deposit (5 units)
			assert_eq!(balances(99), (95, 0));

			// Should have emitted the expected sequence of events
			assert_eq!(
				signed_events(),
				vec![
					SignedEvent::Registered(
						0,
						99,
						ElectionScore { minimal_stake: 100, sum_stake: 0, sum_stake_squared: 0 }
					),
					SignedEvent::Stored(0, 99, 0),
					SignedEvent::Slashed(0, 99, 5)
				]
			);
		});
	}
}

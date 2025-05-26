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
use crate::{mock::*, verifier::FeasibilityError};
use sp_core::bounded_vec;

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
				singed_events_since_last_call(),
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
			assert_eq!(singed_events_since_last_call(), vec![Event::Discarded(1, 99)]);

			// 99 now has their deposit returned.
			assert_eq!(balances(99), (100, 0));

			// signed pallet should be in 100% clean state.
			assert_ok!(Submissions::<Runtime>::ensure_killed(0));
		})
	}
}

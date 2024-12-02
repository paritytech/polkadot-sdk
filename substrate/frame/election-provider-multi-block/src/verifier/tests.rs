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

use super::*;
use crate::{
	mock::*,
	verifier::{impls::pallet::*, SolutionDataProvider},
	Phase,
};
use frame_support::testing_prelude::*;
use sp_npos_elections::ElectionScore;
use sp_runtime::Perbill;
use substrate_test_utils::assert_eq_uvec;

mod e2e {
	use super::*;

	use crate::{signed::pallet::Submissions, Snapshot};
	use frame_election_provider_support::ElectionProvider;

	#[test]
	fn single_page_works() {
		ExtBuilder::default()
			.pages(1)
			.signed_max_submissions(2)
			.unsigned_phase(0)
			.build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				assert_ok!(Snapshot::<T>::ensure());

				let submitted_solution = mine_full().unwrap();
				let claimed_score = submitted_solution.score;

				// 99 registers and submits solution page.
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), claimed_score));
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					0,
					Some(submitted_solution.solution_pages[0].clone())
				));

				// async verfier is idle.
				assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);
				// no queued validated score at this point.
				assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);
				// no queued backings or variant solutions in storage yet.
				assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);
				assert!(QueuedSolutionY::<T>::iter_keys().count() == 0);
				assert!(QueuedSolutionBackings::<T>::iter_keys().count() == 0);

				// roll to signed validated phase to start validating the queued submission.
				let phase_transition = calculate_phases();
				roll_to(*phase_transition.get("validate").unwrap() - 1);
				// one block before signed validations is signed phase.
				assert_eq!(current_phase(), Phase::Signed);
				// no verifier events yet.
				assert!(verifier_events().is_empty());

				// next block is signed validation.
				roll_one();
				let validation_started = System::block_number();
				assert_eq!(current_phase(), Phase::SignedValidation(validation_started));

				// async verfier verified single page.
				assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(0));
				// no queued validated score at this point.
				assert_eq!(QueuedSolution::<T>::queued_score(), None);
				// invalid variant starts to be set.
				assert_eq!(QueuedSolution::<T>::invalid(), SolutionPointer::Y);
				assert_eq_uvec!(QueuedSolutionY::<T>::iter_keys().collect::<Vec<_>>(), vec![0]);
				// the other variant remains empty.
				assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);
				assert!(QueuedSolutionBackings::<T>::iter_keys().count() > 0);

				// queued solution still none.
				assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);

				// roll one to finalize validation.
				roll_one();
				assert_eq!(current_phase(), Phase::SignedValidation(validation_started));

				// valid solution has been queued, we're gold.
				assert_eq!(QueuedSolution::<Runtime>::queued_score(), Some(claimed_score));
				// validation finished successfully, thus the async verifier is off.
				assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);

				// set phase to export to call elect.
				set_phase_to(Phase::Export(0));

				// now confirm that calling `elect` maps to the expected solution backings that were
				// successful submitted by 99.
				let expected_supports =
					Pallet::<T>::feasibility_check(submitted_solution.solution_pages[0].clone(), 0)
						.unwrap();
				assert_eq!(MultiPhase::elect(0).unwrap(), expected_supports);

				// check all events.
				assert_eq!(
					verifier_events(),
					vec![
						Event::VerificationStarted {
							claimed_score: ElectionScore {
								minimal_stake: 10,
								sum_stake: 50,
								sum_stake_squared: 666
							}
						},
						Event::Verified { page: 0, backers: 4 },
						Event::Queued {
							score: ElectionScore {
								minimal_stake: 10,
								sum_stake: 50,
								sum_stake_squared: 666,
							},
							old_score: None,
						}
					]
				);
			})
	}

	// Test case:
	// * max submissions are registered;
	// * one submitter submits all pages;
	// * first submission validation is valid (checked with async verifier).
	// * election is successful with highest submission's score.
	#[test]
	fn multi_page_works() {
		let (mut ext, pool) =
			ExtBuilder::default().pages(3).signed_max_submissions(2).build_offchainify(1);

		ext.execute_with(|| {
			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// snapshot should exist now.
			assert_ok!(Snapshot::<T>::ensure());

			// 99 registers with good submission and submits all pages.
			let submitted_solution = mine_full().unwrap();
			let claimed_score = submitted_solution.score;

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), claimed_score));
			// note: page submission order does not matter.
			for page in 0..=CorePallet::<T>::msp() {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					page,
					Some(submitted_solution.solution_pages[page as usize].clone())
				));
			}

			// 10 registers with default submission.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(10), Default::default()));

			// async verfier is idle.
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);
			// no queued validated score at this point.
			assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);
			// no queued backings or variant solutions in storage yet.
			assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);
			assert!(QueuedSolutionY::<T>::iter_keys().count() == 0);
			assert!(QueuedSolutionBackings::<T>::iter_keys().count() == 0);

			// roll to signed validated phase to start validating the queued submission.
			let phase_transition = calculate_phases();
			roll_to_with_ocw(*phase_transition.get("validate").unwrap() - 1, Some(pool.clone()));
			// one block before signed validations is signed phase.
			assert_eq!(current_phase(), Phase::Signed);
			// no verifier events yet.
			assert!(verifier_events().is_empty());

			// next block is signed validation.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(current_phase(), Phase::SignedValidation(System::block_number()));

			// the async verifier has started verifying the best stored submit. It verifies the
			// score and the msp page.
			assert_eq!(
				verifier_events(),
				vec![
					Event::VerificationStarted { claimed_score },
					Event::Verified { page: 2, backers: 2 }
				]
			);
			// it wraps the page index 2 since it's signaled to start verifying page 2 in the next
			// block.
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(2));

			// the invalid queued solution variant starts to be filled as the async verifier starts
			// processing the pages.
			assert_eq!(QueuedSolution::<T>::invalid(), SolutionPointer::Y);
			assert_eq_uvec!(QueuedSolutionY::<T>::iter_keys().collect::<Vec<_>>(), vec![2]);
			// solution backings start to get populated.
			assert_eq_uvec!(QueuedSolutionBackings::<T>::iter_keys().collect::<Vec<_>>(), vec![2]);
			// the other variant remains empty.
			assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);

			// still no queued score though, since not all pages have been verified.
			assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);

			// progress block to verify page 1.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(1));

			assert_eq!(verifier_events().pop().unwrap(), Event::Verified { page: 1, backers: 2 });

			assert_eq_uvec!(QueuedSolutionY::<T>::iter_keys().collect::<Vec<_>>(), vec![1, 2]);
			assert_eq_uvec!(
				QueuedSolutionBackings::<T>::iter_keys().collect::<Vec<_>>(),
				vec![1, 2]
			);
			assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);

			// progress block to verify last page (lsp).
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(0));

			// queued solution has all the pages.
			assert_eq_uvec!(QueuedSolutionY::<T>::iter_keys().collect::<Vec<_>>(), vec![0, 1, 2]);
			assert_eq!(QueuedSolutionY::<T>::iter_keys().count() as u32, Pages::get());
			// queued solution does not exist yet, the validation will be finalized in the next
			// block.
			assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);

			// next block will finalize the solution and wrap the signed validation phase up.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);

			// now the queued score matches that of the claimed_score from the accepted submission.
			assert_eq!(QueuedSolution::<Runtime>::queued_score(), Some(claimed_score));
			// the valid variant pointer has changed.
			assert_eq!(QueuedSolution::<T>::valid(), SolutionPointer::Y);
			assert_eq!(QueuedSolution::<T>::invalid(), SolutionPointer::X);
			// and the solution backings have been cleared, since they are not required anymore
			// after the solution has been validated and accepted.
			assert!(QueuedSolutionBackings::<T>::iter_keys().count() == 0);

			// check all events.
			assert_eq!(
				verifier_events(),
				vec![
					Event::Verified { page: 0, backers: 5 },
					Event::Queued {
						score: ElectionScore {
							minimal_stake: 50,
							sum_stake: 310,
							sum_stake_squared: 19650
						},
						old_score: None,
					}
				]
			);

			// now a submission has been accepted, next block transitions to unsigned phase.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(current_phase(), Phase::Unsigned(System::block_number()));

			// progress to the export phase without the OCW (thus no unsigned submissions). now we
			// can fetch the election pages from the signed submission.
			roll_to(*phase_transition.get("export").unwrap());
			assert_eq!(current_phase(), Phase::Export(System::block_number()));

			// now confirm that calling `elect` maps to the expected solution backings that were
			// successful submitted by 99. note: order matters, elect must be called from msp to
			// lsp.
			for page in (0..=CorePallet::<T>::msp()).rev() {
				let expected_supports = Pallet::<T>::feasibility_check(
					submitted_solution.solution_pages[page as usize].clone(),
					page,
				)
				.unwrap();
				assert_eq!(MultiPhase::elect(page).unwrap(), expected_supports);
			}
		})
	}

	// Test case:
	// * max submissions are registered;
	// * first submission verification fails.
	// * second best submission is successful.
	#[test]
	fn e2e_first_submission_verification_continues_works() {
		let (mut ext, pool) = ExtBuilder::default()
			.unsigned_phase(0)
			.pages(2)
			.signed_max_submissions(3)
			.build_offchainify(1);

		ext.execute_with(|| {
			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// snapshot should exist now.
			assert_ok!(Snapshot::<T>::ensure());

			// 99 registers with good submission and submits all pages. the registed claimed score
			// has the wrong score, so the verification will fail.
			let submitted_solution = mine_full().unwrap();
			let ok_claimed_score = submitted_solution.score;
			let wrong_claimed_score = ElectionScore {
				minimal_stake: u128::MAX,
				sum_stake: u128::MAX,
				sum_stake_squared: u128::MAX,
			};
			assert!(ok_claimed_score < wrong_claimed_score);

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), wrong_claimed_score));
			// note: page submission order does not matter.
			for page in 0..=CorePallet::<T>::msp() {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					page,
					Some(submitted_solution.solution_pages[page as usize].clone())
				));
			}

			// 10 registers with the correct score and submission pages.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(10), ok_claimed_score));
			// note: page submission order does not matter.
			for page in 0..=CorePallet::<T>::msp() {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(10),
					page,
					Some(submitted_solution.solution_pages[page as usize].clone())
				));
			}

			// The 99 submission will be the first to be verified since it has registed with a
			// higher score.
			let mut sorted_submissions = Submissions::<T>::scores_for(current_round());
			assert_eq!(sorted_submissions.pop().unwrap(), (99, wrong_claimed_score));
			assert_eq!(sorted_submissions.pop().unwrap(), (10, ok_claimed_score));

			// async verfier is idle.
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);
			// no queued validated score at this point.
			assert_eq!(QueuedSolution::<Runtime>::queued_score(), None);
			// no queued backings or variant solutions in storage yet.
			assert!(QueuedSolutionX::<T>::iter_keys().count() == 0);
			assert!(QueuedSolutionY::<T>::iter_keys().count() == 0);
			assert!(QueuedSolutionBackings::<T>::iter_keys().count() == 0);

			// roll to signed validated phase to start validating the queued submission.
			let phase_transition = calculate_phases();
			roll_to_with_ocw(*phase_transition.get("validate").unwrap() - 1, Some(pool.clone()));
			// one block before signed validations is signed phase.
			assert_eq!(current_phase(), Phase::Signed);
			// no verifier events yet.
			assert!(verifier_events().is_empty());

			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);

			// next block is signed validation.
			roll_one_with_ocw(Some(pool.clone()));

			let started_validation_at = System::block_number();
			assert_eq!(current_phase(), Phase::SignedValidation(started_validation_at));

			// verification started and first page has been verified.
			assert_eq!(
				verifier_events(),
				[
					Event::VerificationStarted { claimed_score: wrong_claimed_score },
					Event::Verified { page: 1, backers: 4 }
				]
			);

			// verify second page.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(current_phase(), Phase::SignedValidation(started_validation_at));

			assert_eq!(verifier_events(), [Event::Verified { page: 0, backers: 4 }]);

			// one more block to finalize the verification, which fails.
			roll_one_with_ocw(Some(pool.clone()));

			assert_eq!(
				verifier_events(),
				[Event::FinalizeVerificationFailed { error: FeasibilityError::InvalidScore }]
			);

			// start verifying second submitted solution.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(
				verifier_events(),
				[
					Event::VerificationStarted { claimed_score: ok_claimed_score },
					Event::Verified { page: 1, backers: 4 }
				]
			);

			// verify page 2.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(verifier_events(), [Event::Verified { page: 0, backers: 4 }],);

			// verification finalization also worked as expected.
			roll_one_with_ocw(Some(pool.clone()));
			assert_eq!(
				verifier_events(),
				[Event::Queued { score: ok_claimed_score, old_score: None }]
			);

			// progress to the export phase without the OCW (thus no unsigned submissions). now we
			// can fetch the election pages from the signed submission.
			roll_to(*phase_transition.get("export").unwrap());
			assert_eq!(current_phase(), Phase::Export(System::block_number()));

			// now confirm that calling `elect` maps to the expected solution backings that were
			// successful submitted by 100. note: order matters, elect must be called from msp to
			// lsp.
			for page in (0..=CorePallet::<T>::msp()).rev() {
				let expected_supports = Pallet::<T>::feasibility_check(
					submitted_solution.solution_pages[page as usize].clone(),
					page,
				)
				.unwrap();
				assert_eq!(MultiPhase::elect(page).unwrap(), expected_supports);
			}
		})
	}

	// Test case:
	// * max submissions are registered;
	// * all submissions validation fail;
	// * no unsigned phase, thus election round fails.
	#[test]
	fn e2e_all_submissions_verification_final_fail() {
		let (mut ext, pool) = ExtBuilder::default()
			.unsigned_phase(0)
			.pages(2)
			.signed_max_submissions(3)
			.build_offchainify(1);

		ext.execute_with(|| {
			let phase_transition = calculate_phases();
			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// snapshot should exist now.
			assert_ok!(Snapshot::<T>::ensure());

			// 99 registers with good submission and submits all pages. the registed claimed score
			// has the wrong score, so the verification will fail.
			let submitted_solution = mine_full().unwrap();
			let ok_claimed_score = submitted_solution.score;
			let wrong_claimed_score = ElectionScore {
				minimal_stake: u128::MAX,
				sum_stake: u128::MAX,
				sum_stake_squared: u128::MAX,
			};
			assert!(ok_claimed_score < wrong_claimed_score);

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), wrong_claimed_score));
			// note: page submission order does not matter.
			for page in 0..=CorePallet::<T>::msp() {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					page,
					Some(submitted_solution.solution_pages[page as usize].clone())
				));
			}

			// 10 also submits with incorrect score and submission pages.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(10), wrong_claimed_score));
			// note: page submission order does not matter.
			for page in 0..=CorePallet::<T>::msp() {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(10),
					page,
					Some(submitted_solution.solution_pages[page as usize].clone())
				));
			}

			// no submissino is valid, thus both will fail during signed validation phase.
			roll_to(*phase_transition.get("unsigned").unwrap() - 1);
			assert!(current_phase().is_signed_validation());

			assert_eq!(
				verifier_events(),
				[
					Event::VerificationStarted { claimed_score: wrong_claimed_score },
					Event::Verified { page: 1, backers: 4 },
					Event::Verified { page: 0, backers: 4 },
					Event::FinalizeVerificationFailed { error: FeasibilityError::InvalidScore },
					Event::VerificationStarted { claimed_score: wrong_claimed_score },
					Event::Verified { page: 1, backers: 4 },
					Event::Verified { page: 0, backers: 4 },
					Event::FinalizeVerificationFailed { error: FeasibilityError::InvalidScore }
				],
			);

			// progress to the export phase without the OCW (thus no unsigned submissions).
			roll_to(*phase_transition.get("export").unwrap() + 100);
			assert_eq!(current_phase(), Phase::Export(System::block_number()));

			// a good solution was not found, error when calling elect and end up in emergency
			// phase.
			assert!(MultiPhase::elect(CorePallet::<T>::msp()).is_err());
		})
	}
}

mod solution {
	use super::*;

	#[test]
	fn variant_flipping_works() {
		ExtBuilder::default().core_try_state(false).build_and_execute(|| {
			assert!(QueuedSolution::<T>::valid() != QueuedSolution::<T>::invalid());

			let valid_before = QueuedSolution::<T>::valid();
			let invalid_before = valid_before.other();

			set_phase_to(Phase::Unsigned(0));
			Pallet::<T>::force_valid_solution(0, Default::default(), Default::default());

			// variant has flipped.
			assert_eq!(QueuedSolution::<T>::valid(), invalid_before);
			assert_eq!(QueuedSolution::<T>::invalid(), valid_before);
		})
	}
}

mod feasibility_check {
	use super::*;

	#[test]
	fn ensure_score_quality_works() {
		ExtBuilder::default()
			.solution_improvements_threshold(Perbill::from_percent(10))
			.build_and_execute(|| {
				assert_eq!(MinimumScore::<T>::get(), Default::default());
				assert!(<Pallet<T> as Verifier>::queued_score().is_none());

				// if minimum score is not set and there's no queued score, any score has quality.
				assert_ok!(Pallet::<T>::ensure_score_quality(ElectionScore {
					minimal_stake: 1,
					sum_stake: 1,
					sum_stake_squared: 1
				}));

				// if minimum score is set, the score being evaluated must be higher than the
				// minimum score.
				MinimumScore::<T>::set(
					ElectionScore { minimal_stake: 10, sum_stake: 20, sum_stake_squared: 300 }
						.into(),
				);

				// score is not higher than minimum score.
				assert_err!(
					Pallet::<T>::ensure_score_quality(ElectionScore {
						minimal_stake: 1,
						sum_stake: 1,
						sum_stake_squared: 1,
					}),
					FeasibilityError::ScoreTooLow
				);

				// if score improves the current one by the minimum solution improvement, we're
				// gold.
				assert_ok!(Pallet::<T>::ensure_score_quality(ElectionScore {
					minimal_stake: 11,
					sum_stake: 22,
					sum_stake_squared: 300
				}));
			})
	}

	#[test]
	fn winner_indices_page_in_bounds() {
		ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			let mut solution = mine_full().unwrap();
			assert_eq!(crate::Snapshot::<Runtime>::targets().unwrap().len(), 8);

			// swap all votes from 3 to 4 to invalidate index 4.
			solution.solution_pages[0]
				.votes1
				.iter_mut()
				.filter(|(_, t)| *t == TargetIndex::from(3u16))
				.for_each(|(_, t)| *t += 1);

			assert_noop!(
				VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0),
				FeasibilityError::InvalidVote,
			);
		})
	}

	#[test]
	fn targets_not_in_snapshot() {
		ExtBuilder::default().build_and_execute(|| {
			roll_to_phase(Phase::Off);

			crate::Snapshot::<Runtime>::kill();
			assert_eq!(crate::Snapshot::<Runtime>::targets(), None);

			assert_noop!(
				VerifierPallet::feasibility_check(TestNposSolution::default(), 0),
				FeasibilityError::SnapshotUnavailable,
			);
		})
	}

	#[test]
	fn voters_not_in_snapshot() {
		ExtBuilder::default().build_and_execute(|| {
			roll_to_phase(Phase::Signed);

			let _ = crate::PagedVoterSnapshot::<Runtime>::clear(u32::MAX, None);

			assert_eq!(crate::Snapshot::<Runtime>::targets().unwrap().len(), 8);
			assert_eq!(crate::Snapshot::<Runtime>::voters(0), None);

			assert_noop!(
				VerifierPallet::feasibility_check(TestNposSolution::default(), 0),
				FeasibilityError::SnapshotUnavailable,
			);
		})
	}
}

mod sync_verifier {
	use super::*;

	mod verify_synchronous {
		use super::*;

		#[test]
		fn given_better_solution_stores_provided_page_as_valid_solution() {
			ExtBuilder::default().pages(1).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full().unwrap();

				// empty solution storage items before verification
				assert!(<VerifierPallet as Verifier>::next_missing_solution_page().is_some());
				assert!(QueuedSolutionBackings::<Runtime>::get(0).is_none());
				assert!(match QueuedSolution::<Runtime>::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::get(0),
					SolutionPointer::Y => QueuedSolutionY::<T>::get(0),
				}
				.is_none());

				set_phase_to(Phase::Unsigned(0));
				assert_ok!(<VerifierPallet as Verifier>::verify_synchronous(
					solution.solution_pages[0].clone(),
					solution.score,
					0,
				));

				// solution storage items filled after verification
				assert!(QueuedSolutionBackings::<Runtime>::get(0).is_some());
				assert!(match QueuedSolution::<Runtime>::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::get(0),
					SolutionPointer::Y => QueuedSolutionY::<T>::get(0),
				}
				.is_some());
			})
		}

		#[test]
		fn returns_error_if_score_quality_is_lower_than_expected() {
			ExtBuilder::default().pages(1).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full().unwrap();

				set_phase_to(Phase::Unsigned(0));

				// a solution already stored
				let score =
					ElectionScore { minimal_stake: u128::max_value(), ..Default::default() };
				Pallet::<T>::force_valid_solution(0, Default::default(), score);

				assert_err!(
					<VerifierPallet as Verifier>::verify_synchronous(
						solution.solution_pages[0].clone(),
						solution.score,
						0,
					),
					FeasibilityError::ScoreTooLow
				);
			})
		}

		#[test]
		fn returns_error_if_solution_fails_feasibility_check() {
			ExtBuilder::default().build_and_execute(|| {
				roll_to_phase(Phase::Signed);

				let solution = mine_full().unwrap();
				let _ = crate::PagedVoterSnapshot::<Runtime>::clear(u32::MAX, None);
				assert_err!(
					<VerifierPallet as Verifier>::verify_synchronous(
						solution.solution_pages[0].clone(),
						solution.score,
						0,
					),
					FeasibilityError::SnapshotUnavailable
				);
			})
		}

		#[test]
		fn returns_error_if_computed_score_is_different_than_provided() {
			ExtBuilder::default().build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full().unwrap();
				assert_err!(
					<VerifierPallet as Verifier>::verify_synchronous(
						solution.solution_pages[0].clone(),
						solution.score,
						0,
					),
					FeasibilityError::InvalidScore
				);
			})
		}
	}

	#[test]
	fn next_missing_solution_works() {
		ExtBuilder::default().core_try_state(false).build_and_execute(|| {
			let supports: SupportsOf<Pallet<T>> = Default::default();
			let msp = crate::Pallet::<T>::msp();
			assert!(msp == <T as crate::Config>::Pages::get() - 1 && msp == 2);

			// enforce unsigned phase.
			set_phase_to(Phase::Unsigned(System::block_number()));

			// msp page is the next missing.
			assert_eq!(<VerifierPallet as Verifier>::next_missing_solution_page(), Some(msp));

			// X is the current valid solution, let's work with it.
			assert_eq!(QueuedSolution::<T>::valid(), SolutionPointer::X);

			// set msp and check the next missing page again.
			QueuedSolution::<T>::set_page(msp, supports.clone());
			assert_eq!(<VerifierPallet as Verifier>::next_missing_solution_page(), Some(msp - 1));

			QueuedSolution::<T>::set_page(msp - 1, supports.clone());
			assert_eq!(<VerifierPallet as Verifier>::next_missing_solution_page(), Some(0));

			// set last page, missing page after is None as solution is complete.
			QueuedSolution::<T>::set_page(0, supports.clone());
			assert_eq!(<VerifierPallet as Verifier>::next_missing_solution_page(), None);
		})
	}
}

mod async_verifier {
	use super::*;

	#[test]
	fn async_verifier_simple_works() {
		ExtBuilder::default().build_and_execute(|| {})
	}

	mod force_finalize_verification {
		use frame_support::assert_err;
		use sp_npos_elections::ElectionScore;

		use super::{AsyncVerifier, VerifierPallet, *};

		#[test]
		fn failed_score_computation_returns_underlying_error() {
			ExtBuilder::default().build_and_execute(|| {
				let claimed_score: ElectionScore = Default::default();
				assert_err!(
					<VerifierPallet as AsyncVerifier>::force_finalize_verification(claimed_score),
					FeasibilityError::Incomplete
				);
			});
		}

		#[test]
		fn final_score_differs_from_claimed_score() {
			ExtBuilder::default().pages(1).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));

				set_phase_to(Phase::Unsigned(0));
				Pallet::<T>::force_valid_solution(0, Default::default(), Default::default());

				let claimed_score: ElectionScore =
					ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };
				assert_err!(
					<VerifierPallet as AsyncVerifier>::force_finalize_verification(claimed_score),
					FeasibilityError::InvalidScore
				);
			});
		}

		#[test]
		fn winner_count_differs_from_desired_targets() {
			ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full().unwrap();
				let supports =
					VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0);
				assert!(supports.is_ok());
				let supports = supports.unwrap();

				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), solution.score));
				set_phase_to(Phase::Unsigned(0));
				Pallet::<T>::force_valid_solution(0, supports, solution.score);

				// setting desired targets value to a lower one, to make the solution verification
				// fail
				self::DesiredTargets::set(Ok(1));
				assert_eq!(crate::Snapshot::<Runtime>::desired_targets(), Some(1));

				// just to make sure there's more winners in the current solution than desired
				// targets
				let winner_count = QueuedSolution::<Runtime>::compute_current_score()
					.map(|(_, winner_count)| winner_count)
					.unwrap();
				assert_eq!(winner_count, 2);

				assert_err!(
					<VerifierPallet as AsyncVerifier>::force_finalize_verification(solution.score),
					FeasibilityError::WrongWinnerCount
				);
			});
		}

		#[test]
		fn force_verification_without_pages_fails() {
			ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full().unwrap();
				let supports =
					VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0);
				assert!(supports.is_ok());

				set_phase_to(Phase::Unsigned(0));
				Pallet::<T>::force_valid_solution(0, supports.unwrap(), solution.score);

				assert_ok!(<VerifierPallet as AsyncVerifier>::force_finalize_verification(
					solution.score
				));

				// stored score is the submitted one
				assert_eq!(QueuedSolution::<Runtime>::queued_score(), Some(solution.score));
			});
		}
	}

	#[test]
	fn stopping_the_verification_cleans_storage_items() {
		ExtBuilder::default().pages(1).build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));

			set_phase_to(Phase::Unsigned(0));
			Pallet::<T>::force_valid_solution(0, Default::default(), Default::default());

			<VerifierPallet as AsyncVerifier>::stop();

			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);
			// invalid pointer has been cleared.
			assert_eq!(QueuedSolution::<T>::invalid(), SolutionPointer::X);
			assert_eq!(QueuedSolutionX::<T>::iter().count(), 0);
			assert_eq!(QueuedSolutionBackings::<T>::iter().count(), 0);
		});
	}

	mod verification_start {
		use super::*;
		use crate::signed::pallet::Submissions;

		#[test]
		fn fails_if_verification_is_ongoing() {
			ExtBuilder::default().build_and_execute(|| {
				set_phase_to(Phase::SignedValidation(System::block_number()));
				<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));
				assert_err!(<VerifierPallet as AsyncVerifier>::start(), "verification ongoing");
			});
		}

		#[test]
		fn reports_result_rejection_no_metadata_fails() {
			ExtBuilder::default()
				.minimum_score(ElectionScore {
					minimal_stake: 100,
					sum_stake: 100,
					sum_stake_squared: 100,
				})
				.solution_improvements_threshold(Perbill::from_percent(10))
				.build_and_execute(|| {
					<VerifierPallet as AsyncVerifier>::set_status(Status::Nothing);

					// no score in sorted scores yet.
					assert!(<SignedPallet as SolutionDataProvider>::get_score().is_none());
					assert!(Submissions::<T>::scores_for(current_round()).is_empty());

					let low_score =
						ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };

					// force insert score and `None` metadata.
					Submissions::<T>::insert_score_and_metadata(
						current_round(),
						1,
						Some(low_score),
						None,
					);

					// low_score has been added to the sorted scores.
					assert_eq!(
						<SignedPallet as SolutionDataProvider>::get_score(),
						Some(low_score)
					);
					assert!(Submissions::<T>::scores_for(current_round()).len() == 1);
					// metadata is None.
					assert_eq!(
						Submissions::<T>::take_leader_score(current_round()),
						Some((1, None))
					);
					// will defensive panic since submission metadata does not exist.
					let _ = <VerifierPallet as AsyncVerifier>::start();
				})
		}

		#[test]
		fn reports_result_rejection_works() {
			ExtBuilder::default()
				.minimum_score(ElectionScore {
					minimal_stake: 100,
					sum_stake: 100,
					sum_stake_squared: 100,
				})
				.solution_improvements_threshold(Perbill::from_percent(10))
				.build_and_execute(|| {
					<VerifierPallet as AsyncVerifier>::set_status(Status::Nothing);

					// no score in sorted scores or leader yet.
					assert!(<SignedPallet as SolutionDataProvider>::get_score().is_none());
					assert!(Submissions::<T>::scores_for(current_round()).is_empty());
					assert_eq!(Submissions::<T>::take_leader_score(current_round()), None);

					let low_score =
						ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };

					let metadata = Submissions::submission_metadata_from(
						low_score,
						Default::default(),
						Default::default(),
						Default::default(),
					);

					// force insert score and metadata.
					Submissions::<T>::insert_score_and_metadata(
						current_round(),
						1,
						Some(low_score),
						Some(metadata),
					);

					// low_score has been added to the sorted scores.
					assert_eq!(
						<SignedPallet as SolutionDataProvider>::get_score(),
						Some(low_score)
					);
					assert!(Submissions::<T>::scores_for(current_round()).len() == 1);

					// insert a score lower than minimum score.
					assert_ok!(<VerifierPallet as AsyncVerifier>::start());

					// score too low event submitted.
					assert_eq!(
						verifier_events(),
						vec![Event::<T>::VerificationFailed {
							page: 2,
							error: FeasibilityError::ScoreTooLow,
						}]
					);
				})
		}
	}
}

mod hooks {
	use super::*;
	use crate::signed::pallet::Submissions;

	#[test]
	fn on_initialize_ongoing_fails() {
		ExtBuilder::default().pages(1).build_and_execute(|| {
			let round = current_round();
			let score = ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };
			let metadata = Submissions::submission_metadata_from(
				score,
				Default::default(),
				Default::default(),
				Default::default(),
			);
			Submissions::<T>::insert_score_and_metadata(
				round,
				1,
				Some(score),
				Some(metadata.clone()),
			);

			// force ongoing status and validate phase.
			set_phase_to(Phase::SignedValidation(System::block_number()));
			<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(0));

			// no events yet.
			assert!(verifier_events().is_empty());

			// progress the block.
			roll_one();

			assert_eq!(
				verifier_events(),
				vec![Event::FinalizeVerificationFailed { error: FeasibilityError::Incomplete },]
			);
		});
	}
}

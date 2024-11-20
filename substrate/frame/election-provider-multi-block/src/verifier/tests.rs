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

use crate::{
	mock::*,
	verifier::{impls::pallet::*, SolutionDataProvider, *},
	Phase,
};
use frame_support::{assert_err, assert_noop, assert_ok, testing_prelude::*, StorageMap};
use sp_npos_elections::ElectionScore;
use sp_runtime::Perbill;

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

			// if minimum score is set, the score being evaluated must be higher than the minimum
			// score.
			MinimumScore::<T>::set(
				ElectionScore { minimal_stake: 10, sum_stake: 20, sum_stake_squared: 300 }.into(),
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

			// if score improves the current one by the minimum solution improvement, we're gold.
			assert_ok!(Pallet::<T>::ensure_score_quality(ElectionScore {
				minimal_stake: 11,
				sum_stake: 22,
				sum_stake_squared: 300
			}));
		})
}

mod solution {
	use super::*;

	#[test]
	fn variant_flipping_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(QueuedSolution::<T>::valid() != QueuedSolution::<T>::invalid());

			let valid_before = QueuedSolution::<T>::valid();
			let invalid_before = valid_before.other();

			let mock_score = ElectionScore { minimal_stake: 10, ..Default::default() };

			// queue solution and flip variant.
			QueuedSolution::<T>::finalize_solution(mock_score);

			// solution has been queued
			assert_eq!(QueuedSolution::<T>::queued_score().unwrap(), mock_score);
			// variant has flipped.
			assert_eq!(QueuedSolution::<T>::valid(), invalid_before);
			assert_eq!(QueuedSolution::<T>::invalid(), valid_before);
		})
	}
}

mod feasibility_check {
	use super::*;

	#[test]
	fn winner_indices_page_in_bounds() {
		ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			let mut solution = mine_full(1).unwrap();
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

	#[test]
	fn desired_targets_not_in_snapshot() {
		ExtBuilder::default().no_desired_targets().build_and_execute(|| {
			set_phase_to(Phase::Signed);
			assert_err!(
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
				let solution = mine_full(0).unwrap();

				// empty solution storage items before verification
				assert!(<VerifierPallet as Verifier>::next_missing_solution_page().is_some());
				assert!(QueuedSolutionBackings::<Runtime>::get(0).is_none());
				assert!(match QueuedSolution::<Runtime>::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::get(0),
					SolutionPointer::Y => QueuedSolutionY::<T>::get(0),
				}
				.is_none());

				assert_ok!(<VerifierPallet as Verifier>::verify_synchronous(
					solution.solution_pages[0].clone(),
					solution.score,
					0,
				));

				// solution storage items filled after verification
				assert!(QueuedSolutionBackings::<Runtime>::get(0).is_some());
				assert_eq!(<VerifierPallet as Verifier>::next_missing_solution_page(), None);
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

				// a solution already stored
				let score =
					ElectionScore { minimal_stake: u128::max_value(), ..Default::default() };
				QueuedSolution::<T>::finalize_solution(score);

				let solution = mine_full(0).unwrap();
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

				let solution = mine_full(0).unwrap();
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
				let solution = mine_full(0).unwrap();
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

			// run to snapshot phase to reset `RemainingUnsignedPages`.
			roll_to_phase(Phase::Snapshot(crate::Pallet::<T>::lsp()));

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
				QueuedSolution::<T>::set_page(0, Default::default());

				let claimed_score: ElectionScore = Default::default();
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
				let solution = mine_full(0).unwrap();
				let supports =
					VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0);
				assert!(supports.is_ok());
				let supports = supports.unwrap();

				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), solution.score));
				QueuedSolution::<T>::set_page(0, supports);

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
		fn valid_score_results_with_solution_finalized() {
			ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let solution = mine_full(0).unwrap();
				let supports =
					VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0);
				assert!(supports.is_ok());
				let supports = supports.unwrap();

				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), solution.score));
				QueuedSolution::<T>::set_page(0, supports);

				// no stored score so far
				assert!(QueuedSolution::<Runtime>::queued_score().is_none());

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
		ExtBuilder::default().build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));
			QueuedSolution::<T>::set_page(0, Default::default());

			assert_ne!(
				QueuedSolutionX::<Runtime>::iter().count() +
					QueuedSolutionY::<Runtime>::iter().count(),
				0
			);
			assert_ne!(QueuedSolutionBackings::<Runtime>::iter().count(), 0);

			<VerifierPallet as AsyncVerifier>::stop();

			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);
			assert_eq!(QueuedSolutionX::<Runtime>::iter().count(), 0);
			assert_eq!(QueuedSolutionY::<Runtime>::iter().count(), 0);
			assert_eq!(QueuedSolutionBackings::<Runtime>::iter().count(), 0);
		});
	}

	mod verification_start {
		use super::*;
		use crate::signed::pallet::Submissions;

		#[test]
		fn fails_if_verification_is_ongoing() {
			ExtBuilder::default().build_and_execute(|| {
				<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));
				assert_err!(<VerifierPallet as AsyncVerifier>::start(), "verification ongoing");
			});
		}

		#[test]
		#[should_panic(expected = "unexpected: selected leader without active submissions.")]
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
						vec![Event::<T>::VerificationFailed(2, FeasibilityError::ScoreTooLow,)]
					);
				})
		}

		#[test]
		fn given_better_score_sets_verification_status_to_ongoing() {
			ExtBuilder::default().build_and_execute(|| {
				roll_to_phase(Phase::Signed);
				let msp = crate::Pallet::<T>::msp();

				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));
				assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Nothing);

				assert_ok!(<VerifierPallet as AsyncVerifier>::start());

				assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(msp));
			});
		}
	}
}

mod hooks {
	use super::*;
	use crate::signed::pallet::Submissions;
	use frame_support::traits::Hooks;

	#[test]
	fn on_initialize_status_nothing_returns_default_value() {
		ExtBuilder::default().build_and_execute(|| {
			<VerifierPallet as AsyncVerifier>::set_status(Status::Nothing);
			assert_eq!(VerifierPallet::on_initialize(0), Default::default());
		});
	}

	#[test]
	fn on_initialize_solution_infeasible() {
		ExtBuilder::default().build_and_execute(|| {
			// solution insertion
			let round = current_round();
			let score = ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };
			let metadata = Submissions::submission_metadata_from(
				score,
				Default::default(),
				Default::default(),
				Default::default(),
			);
			Submissions::<T>::insert_score_and_metadata(round, 1, Some(score), Some(metadata));

			<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));
			assert_eq!(<VerifierPallet as AsyncVerifier>::status(), Status::Ongoing(0));

			assert_eq!(VerifierPallet::on_initialize(0), Default::default());

			// TODO: zebedeusz - for some reason events list is empty even though the event deposit
			// is executed
			assert_eq!(
				verifier_events(),
				vec![Event::<T>::VerificationFailed(0, FeasibilityError::ScoreTooLow)]
			);
			assert_eq!(VerificationStatus::<T>::get(), Status::Nothing);
		});
	}

	#[test]
	fn on_initialize_feasible_solution_but_score_invalid() {
		ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
			// solution insertion
			let round = current_round();
			let score = ElectionScore { minimal_stake: 10, sum_stake: 10, sum_stake_squared: 10 };
			let metadata = Submissions::submission_metadata_from(
				score,
				Default::default(),
				Default::default(),
				Default::default(),
			);
			Submissions::<T>::insert_score_and_metadata(round, 1, Some(score), Some(metadata));

			// needed for targets to exist in the snapshot
			roll_to_phase(Phase::Signed);
			let _ = mine_full(0).unwrap();

			<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));

			assert_eq!(VerifierPallet::on_initialize(0), Default::default());

			assert_eq!(
				verifier_events(),
				vec![
					Event::<T>::Verified(0, 0),
					Event::<T>::VerificationFailed(0, FeasibilityError::InvalidScore)
				]
			);
			assert!(QueuedSolution::<Runtime>::queued_score().is_none());
			assert_eq!(VerificationStatus::<T>::get(), Status::Nothing);
		});
	}

	#[test]
	fn on_initialize_feasible_solution_and_valid_score() {
		ExtBuilder::default().pages(1).desired_targets(2).build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			let solution = mine_full(0).unwrap();
			let supports = VerifierPallet::feasibility_check(solution.solution_pages[0].clone(), 0);
			assert!(supports.is_ok());

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), solution.score));
			QueuedSolution::<T>::set_page(0, supports.unwrap());

			assert_ok!(<VerifierPallet as AsyncVerifier>::force_finalize_verification(
				solution.score
			));

			<VerifierPallet as AsyncVerifier>::set_status(Status::Ongoing(0));

			assert_eq!(VerifierPallet::on_initialize(0), Default::default());

			let events = verifier_events();
			assert!(events.len() > 0);
			assert_eq!(
				events.get(0),
				Some(&Event::<T>::Queued(solution.score, Some(solution.score)))
			);
			assert_eq!(VerificationStatus::<T>::get(), Status::Nothing);
		});
	}
}

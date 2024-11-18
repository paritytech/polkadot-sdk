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
	verifier::{impls::pallet::*, *},
	Phase,
};
use frame_support::{assert_err, assert_noop, assert_ok};
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
}

mod sync_verifier {
	use super::*;

	#[test]
	fn sync_verifier_simple_works() {
		ExtBuilder::default().build_and_execute(|| {})
	}

	#[test]
	fn next_missing_solution_works() {
		ExtBuilder::default().build_and_execute(|| {
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
}

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

use super::*;
use crate::{mock::*, Phase, Verifier};
use frame_support::{assert_noop, assert_ok};
use sp_npos_elections::ElectionScore;

mod calls {
	use super::*;
	use sp_core::bounded_vec;

	#[test]
	fn register_works() {
		ExtBuilder::default().build_and_execute(|| {
			roll_to_phase(Phase::Signed);
			assert_ok!(assert_snapshots());

			assert_eq!(balances(99), (100, 0));
			let score = ElectionScore { minimal_stake: 100, ..Default::default() };

			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), score));
			assert_eq!(balances(99), (90, 10));

			assert_eq!(
				Submissions::<T>::metadata(current_rount(), &99).unwrap(),
				SubmissionMetadata {
					claimed_score: score,
					deposit: 10,
					pages: bounded_vec![false, false, false],
				}
			);

			// duplicate submission for the same round fails.
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(99), score),
				Error::<T>::DuplicateRegister,
			);

			// if claimed score if below the minimum score, submission will fail.
			<VerifierPallet as Verifier>::set_minimum_score(ElectionScore {
				minimal_stake: 20,
				..Default::default()
			});

			let low_score = ElectionScore { minimal_stake: 10, ..Default::default() };
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(97), low_score),
				Error::<T>::SubmissionScoreTooLow,
			);
		})
	}

	#[test]
	fn register_sorted_works() {
		ExtBuilder::default().build_and_execute(|| {})
	}
}

mod e2e {
	use super::*;
	use crate::{mock::*, Phase};
	use frame_support::{testing_prelude::*, BoundedVec};

	type MaxSubmissions = <Runtime as Config>::MaxSubmissions;

	mod simple_e2e_works {
		use super::*;

		#[test]
		fn submit_solution_happy_path_works() {
			ExtBuilder::default().build_and_execute(|| {
				// TODO: check events
				roll_to_phase(Phase::Signed);

				let current_round = MultiPhase::current_round();
				assert!(Submissions::<Runtime>::metadata(current_round, &10).is_none());

				let claimed_score = ElectionScore::default();

				// register submission
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(10), claimed_score,));

				// metadata and claimed scores have been stored as expected.
				assert_eq!(
					Submissions::<Runtime>::metadata(current_round, &10),
					Some(SubmissionMetadata {
						claimed_score,
						deposit: 10,
						pages: bounded_vec![false, false, false],
					})
				);
				let expected_scores: BoundedVec<(AccountId, ElectionScore), MaxSubmissions> =
					bounded_vec![(10, claimed_score)];
				assert_eq!(Submissions::<Runtime>::scores_for(current_round), expected_scores);

				// submit all pages of a noop solution;
				let solution = TestNposSolution::default();
				for page in (0..=MultiPhase::msp()).into_iter().rev() {
					assert_ok!(SignedPallet::submit_page(
						RuntimeOrigin::signed(10),
						page,
						Some(solution.clone())
					));

					assert_eq!(
						Submissions::<Runtime>::submission_for(10, current_round, page),
						Some(solution.clone())
					);
				}
			})
		}
	}
}

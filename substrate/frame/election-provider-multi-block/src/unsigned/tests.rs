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
use crate::{
	mock::*,
	unsigned::{miner::*, pallet::Config as UnsignedConfig},
	PagedTargetSnapshot, PagedVoterSnapshot, Phase, Snapshot, Verifier,
};

use frame_election_provider_support::ElectionProvider;
use frame_support::{assert_noop, assert_ok};

mod calls {
	use super::*;

	#[test]
	fn unsigned_submission_works() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(0);
		ext.execute_with(|| {
			// election predicted at 30.
			assert_eq!(election_prediction(), 30);

			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// progress through unsigned phase just before the election.
			roll_to_with_ocw(29, Some(pool.clone()));

			// successful submission events for all 3 pages, as expected.
			assert_eq!(
				unsigned_events(),
				[
					Event::UnsignedSolutionSubmitted { at: 19, page: 2 },
					Event::UnsignedSolutionSubmitted { at: 20, page: 1 },
					Event::UnsignedSolutionSubmitted { at: 21, page: 0 }
				]
			);
			// now, solution exists.
			assert!(<VerifierPallet as Verifier>::queued_score().is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(1).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(0).is_some());

			// roll to election prediction bn.
			roll_to_with_ocw(election_prediction(), Some(pool.clone()));

			// still in unsigned phase (after unsigned submissions have been submitted and before
			// the election happened).
			assert!(current_phase().is_unsigned());

			// elect() works as expected.
			assert!(call_elect().is_ok());

			assert_eq!(current_phase(), Phase::Off);
		})
	}

	#[test]
	fn unsigned_submission_no_snapshot() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
			// election predicted at 30.
			assert_eq!(election_prediction(), 30);

			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// but snapshot exists.
			assert!(PagedVoterSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_some());
			assert!(PagedTargetSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_some());
			// so let's clear it.
			clear_snapshot();
			assert!(PagedVoterSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_none());
			assert!(PagedTargetSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_none());

			// progress through unsigned phase just before the election.
			roll_to_with_ocw(29, Some(pool.clone()));

			// snapshot was not available, so unsigned submissions and thus no solution queued.
			assert_eq!(unsigned_events().len(), 0);
			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// call elect (which fails) to restart the phase.
			assert!(call_elect().is_err());
			assert_eq!(current_phase(), Phase::Off);

			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// snapshot exists now.
			assert!(PagedVoterSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_some());
			assert!(PagedTargetSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_some());

			roll_to_with_ocw(election_prediction() - 1, Some(pool.clone()));

			// successful submission events for all 3 pages, as expected.
			assert_eq!(
				unsigned_events(),
				[
					Event::UnsignedSolutionSubmitted { at: 49, page: 2 },
					Event::UnsignedSolutionSubmitted { at: 50, page: 1 },
					Event::UnsignedSolutionSubmitted { at: 51, page: 0 }
				]
			);
			// now, solution exists.
			assert!(<VerifierPallet as Verifier>::queued_score().is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(1).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(0).is_some());

			// elect() works as expected.
			assert_ok!(<MultiPhase as ElectionProvider>::elect(2));
			assert_ok!(<MultiPhase as ElectionProvider>::elect(1));
			assert_ok!(<MultiPhase as ElectionProvider>::elect(0));

			assert_eq!(current_phase(), Phase::Off);
		})
	}
}

mod miner {
	use super::*;

	type OffchainSolver = <T as miner::Config>::Solver;

	#[test]
	fn snapshot_idx_based_works() {
		ExtBuilder::default().build_and_execute(|| {
			roll_to_phase(Phase::Signed);

			let mut all_voter_pages = vec![];
			let mut all_target_pages = vec![];

			for page in (0..Pages::get()).rev() {
				all_voter_pages.push(Snapshot::<T>::voters(page).unwrap());
				all_target_pages.push(Snapshot::<T>::targets().unwrap());
			}
		})
	}

	#[test]
	fn desired_targets_bounds_works() {
		ExtBuilder::default()
			.max_winners_per_page(3)
			.desired_targets(3)
			.build_and_execute(|| {
				// max winner per page == desired_targets, OK.
				compute_snapshot_checked();
				assert_ok!(mine_and_verify_all());

				// max winner per page > desired_targets, OK.
				MaxWinnersPerPage::set(4);
				compute_snapshot_checked();
				assert_ok!(mine_and_verify_all());

				// max winner per page < desired_targets, fails.
				MaxWinnersPerPage::set(2);
				compute_snapshot_checked();
				assert!(mine_and_verify_all().is_err());
			})
	}

	#[test]
	fn fetch_or_mine() {
		let (mut ext, _) = ExtBuilder::default().build_offchainify(1);

		ext.execute_with(|| {
			let msp = crate::Pallet::<T>::msp();
			assert_eq!(msp, 2);

			// no snapshot available, calling mine_paged_solution should fail.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(msp).is_none());

			assert!(OffchainWorkerMiner::<T>::fetch_or_mine(0).is_err());
			compute_snapshot_checked();

			let (full_score_2, partial_score_2, _) =
				OffchainWorkerMiner::<T>::fetch_or_mine(msp).unwrap();
			let (full_score_1, partial_score_1, _) =
				OffchainWorkerMiner::<T>::fetch_or_mine(msp - 1).unwrap();
			let (full_score_0, partial_score_0, _) =
				OffchainWorkerMiner::<T>::fetch_or_mine(0).unwrap();

			assert!(full_score_2 == full_score_1 && full_score_2 == full_score_0);
			assert!(
				full_score_2.sum_stake == full_score_1.sum_stake &&
					full_score_2.sum_stake == full_score_0.sum_stake
			);

			assert_eq!(
				partial_score_0.sum_stake + partial_score_1.sum_stake + partial_score_2.sum_stake,
				full_score_0.sum_stake
			);
		})
	}
}

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
use crate::{mock::*, PagedVoterSnapshot, Phase, Snapshot, TargetSnapshot, Verifier};

use frame_election_provider_support::ElectionProvider;
use frame_support::{assert_err, assert_ok};

mod calls {
	use super::*;

	#[test]
	fn unsigned_submission_works() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(0);
		ext.execute_with(|| {
			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// progress through unsigned phase just before the election.
			roll_to_with_ocw(election_prediction() - 1, Some(pool.clone()));

			// successful submission events for all 3 pages, as expected.
			assert_eq!(
				unsigned_events(),
				[
					Event::UnsignedSolutionSubmitted { at: 89, page: 2 },
					Event::UnsignedSolutionSubmitted { at: 90, page: 1 },
					Event::UnsignedSolutionSubmitted { at: 91, page: 0 }
				]
			);
			// now, solution exists.
			assert!(<VerifierPallet as Verifier>::queued_score().is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(1).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(0).is_some());

			// roll to election prediction bn.
			roll_to_with_ocw(election_prediction(), Some(pool.clone()));

			// now in the export phase.
			assert!(current_phase().is_export());

			// thus, elect() works as expected.
			assert!(call_elect().is_ok());

			assert_eq!(current_phase(), Phase::Off);
		})
	}

	#[test]
	fn unsigned_submission_no_snapshot() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
			roll_to_phase_with_ocw(Phase::Signed, Some(pool.clone()));

			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// but snapshot exists.
			assert!(PagedVoterSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_some());
			assert!(TargetSnapshot::<T>::get().is_some());
			// so let's clear it.
			clear_snapshot();
			assert!(PagedVoterSnapshot::<T>::get(crate::Pallet::<T>::lsp()).is_none());
			assert!(TargetSnapshot::<T>::get().is_none());

			// progress through unsigned phase just before the election.
			roll_to_with_ocw(election_prediction() - 1, Some(pool.clone()));

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
			assert!(TargetSnapshot::<T>::get().is_some());

			roll_to_with_ocw(election_prediction() - 1, Some(pool.clone()));

			// successful submission events for all 3 pages, as expected.
			assert_eq!(
				unsigned_events(),
				[
					Event::UnsignedSolutionSubmitted { at: 189, page: 2 },
					Event::UnsignedSolutionSubmitted { at: 190, page: 1 },
					Event::UnsignedSolutionSubmitted { at: 191, page: 0 }
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

	use crate::{
		miner::{Miner, MinerError, SnapshotType},
		MinerVoterOf,
	};
	use frame_support::BoundedVec;

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
			.verifier_try_state(false)
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

	#[test]
	fn mine_fails_given_less_targets_than_desired() {
		ExtBuilder::default().build_and_execute(|| {
			let all_voter_pages = Default::default();
			let round = Default::default();
			let pages = Pages::get();

			// only one target
			let mut all_targets: BoundedVec<AccountId, TargetSnapshotPerBlock> = Default::default();
			let _ = all_targets.try_push(0);

			// but two desired targets
			let desired_targets = 2;

			let solution = Miner::<Runtime>::mine_paged_solution_with_snapshot(
				&all_voter_pages,
				&all_targets,
				pages,
				round,
				desired_targets,
				false,
			);

			assert_err!(solution, MinerError::NotEnoughTargets)
		});
	}

	#[test]
	fn mine_fails_due_to_unavailable_snapshot() {
		ExtBuilder::default().build_and_execute(|| {
			let round = Default::default();
			let desired_targets = Default::default();
			let pages = Pages::get();

			// snapshot of voters for page 0 does not exist
			let all_voter_pages = Default::default();

			// but there is one target in targets snapshot
			let mut all_targets: BoundedVec<AccountId, TargetSnapshotPerBlock> = Default::default();
			let _ = all_targets.try_push(0);

			let solution = Miner::<Runtime>::mine_paged_solution_with_snapshot(
				&all_voter_pages,
				&all_targets,
				pages,
				round,
				desired_targets,
				false,
			);

			assert_err!(solution, MinerError::SnapshotUnAvailable(SnapshotType::Voters(0)))
		});
	}

	#[test]
	fn mining_done_solution_calculated() {
		ExtBuilder::default()
			.pages(1)
			.desired_targets(1)
			.snapshot_targets_page(1)
			.snapshot_voters_page(1)
			.build_and_execute(|| {
				let round = Default::default();
				let pages = Pages::get();

				let mut all_voter_pages: BoundedVec<
					BoundedVec<MinerVoterOf<Runtime>, VoterSnapshotPerBlock>,
					Pages,
				> = BoundedVec::with_bounded_capacity(pages.try_into().unwrap());

				let mut voters_page = BoundedVec::new();

				// one voter with accountId 12 that votes for validator 0
				let mut voters_votes: BoundedVec<AccountId, MaxVotesPerVoter> = BoundedVec::new();
				assert_ok!(voters_votes.try_push(0));
				let voter = (12, 1, voters_votes);

				// one voters page with the voter 12
				assert_ok!(voters_page.try_push(voter));
				assert_ok!(all_voter_pages.try_push(voters_page));

				// one election target with accountId 0
				let mut all_targets: BoundedVec<AccountId, TargetSnapshotPerBlock> =
					Default::default();
				assert_ok!(all_targets.try_push(0));

				// the election should result with one target chosen
				let desired_targets = 1;

				let solution = Miner::<Runtime>::mine_paged_solution_with_snapshot(
					&all_voter_pages,
					&all_targets,
					pages,
					round,
					desired_targets,
					false,
				);

				assert_ok!(solution.clone());
				assert_eq!(solution.unwrap().0.solution_pages.len(), 1);
			});
	}
}

mod pallet {
	use super::*;
	mod pre_dispatch_checks {
		use super::*;

		#[test]
		fn pre_dispatch_checks_fails_if_phase_is_not_usnigned() {
			ExtBuilder::default().build_and_execute(|| {
				let phases = vec![
					Phase::Signed,
					Phase::Snapshot(0),
					Phase::SignedValidation(0),
					Phase::Export(0),
					Phase::Emergency,
					Phase::Off,
				];

				for phase in phases {
					set_phase_to(phase);
					let claimed_score =
						ElectionScore { minimal_stake: 1, sum_stake: 1, sum_stake_squared: 1 };
					assert_err!(UnsignedPallet::pre_dispatch_checks(0, &claimed_score), ());
				}
			});
		}

		#[test]
		fn pre_dispatch_checks_fails_if_page_is_higher_than_msp() {
			ExtBuilder::default().core_try_state(false).build_and_execute(|| {
				set_phase_to(Phase::Unsigned(0));
				let claimed_score =
					ElectionScore { minimal_stake: 1, sum_stake: 1, sum_stake_squared: 1 };
				assert_err!(
					UnsignedPallet::pre_dispatch_checks(MultiPhase::msp() + 1, &claimed_score),
					()
				);
			});
		}

		#[test]
		fn pre_dispatch_checks_fails_if_score_quality_is_insufficient() {
			ExtBuilder::default()
				.minimum_score(ElectionScore {
					minimal_stake: 10,
					sum_stake: 10,
					sum_stake_squared: 10,
				})
				.pages(1)
				.core_try_state(false)
				.verifier_try_state(false)
				.build_and_execute(|| {
					set_phase_to(Phase::Unsigned(0));
					let claimed_score =
						ElectionScore { minimal_stake: 1, sum_stake: 1, sum_stake_squared: 1 };
					assert_err!(UnsignedPallet::pre_dispatch_checks(0, &claimed_score), ());
				});
		}

		#[test]
		fn pre_dispatch_checks_succeeds_for_correct_page_and_better_score() {
			ExtBuilder::default().core_try_state(false).build_and_execute(|| {
				set_phase_to(Phase::Unsigned(0));
				let claimed_score =
					ElectionScore { minimal_stake: 1, sum_stake: 1, sum_stake_squared: 1 };
				assert_ok!(UnsignedPallet::pre_dispatch_checks(0, &claimed_score));
			});
		}
	}

	mod do_sync_offchain_worker {
		use sp_runtime::offchain::storage::StorageValueRef;

		use super::*;

		#[test]
		fn cached_results_clean_up_at_export_phase() {
			let (mut ext, _) = ExtBuilder::default().build_offchainify(0);
			ext.execute_with(|| {
				set_phase_to(Phase::Export(0));

				let score_storage = StorageValueRef::persistent(
					&OffchainWorkerMiner::<Runtime>::OFFCHAIN_CACHED_SCORE,
				);

				// add some score to cache.
				assert_ok!(score_storage.mutate::<_, (), _>(|_| Ok(ElectionScore::default())));

				// there's something in the cache before worker run
				assert_eq!(
					StorageValueRef::persistent(
						&OffchainWorkerMiner::<Runtime>::OFFCHAIN_CACHED_SCORE,
					)
					.get::<ElectionScore>()
					.unwrap(),
					Some(ElectionScore::default())
				);

				// call sync offchain workers in Export phase will clear up the cache.
				assert_ok!(UnsignedPallet::do_sync_offchain_worker(0));

				assert_eq!(
					StorageValueRef::persistent(
						&OffchainWorkerMiner::<Runtime>::OFFCHAIN_CACHED_SCORE,
					)
					.get::<ElectionScore>()
					.unwrap(),
					None
				);
			});
		}

		#[test]
		fn worker_fails_to_mine_solution() {
			let (mut ext, _) = ExtBuilder::default().no_desired_targets().build_offchainify(0);
			ext.execute_with(|| {
				roll_to_phase(Phase::Snapshot(crate::Pallet::<T>::lsp()));
				set_phase_to(Phase::Unsigned(0));
				assert!(UnsignedPallet::do_sync_offchain_worker(0).is_err());
			});
		}

		#[test]
		fn solution_page_submitted() {
			let (mut ext, pool) = ExtBuilder::default().pages(1).build_offchainify(0);
			ext.execute_with(|| {
				assert_eq!(pool.read().transactions.iter().count(), 0);

				roll_to_phase(Phase::Signed);
				let _ = mine_full().unwrap();
				assert!(<VerifierPallet as Verifier>::next_missing_solution_page().is_some());

				set_phase_to(Phase::Unsigned(0));
				assert!(UnsignedPallet::do_sync_offchain_worker(0).is_ok());

				assert_eq!(pool.read().transactions.iter().count(), 1);
			});
		}
	}
}

mod hooks {
	use super::*;
	use frame_support::traits::Hooks;

	#[test]
	fn on_initialize_returns_default_weight_in_non_off_phases() {
		ExtBuilder::default().build_and_execute(|| {
			let phases = vec![
				Phase::Signed,
				Phase::Snapshot(0),
				Phase::SignedValidation(0),
				Phase::Unsigned(0),
				Phase::Export(0),
				Phase::Emergency,
			];

			for phase in phases {
				set_phase_to(phase);
				assert_eq!(UnsignedPallet::on_initialize(0), Default::default());
			}
		});
	}

	#[test]
	fn on_initialize_returns_specific_weight_in_off_phase() {
		ExtBuilder::default().build_and_execute(|| {
			set_phase_to(Phase::Off);
			assert_ne!(UnsignedPallet::on_initialize(0), Default::default());
			assert_eq!(UnsignedPallet::on_initialize(0), Weighter::get().reads_writes(1, 1));
		});
	}
}

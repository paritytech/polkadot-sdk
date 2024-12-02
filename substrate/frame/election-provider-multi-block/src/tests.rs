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
use crate::mock::*;

use frame_support::testing_prelude::*;

mod phase_transition {
	use super::*;

	#[test]
	fn single_page() {
		let (mut ext, _) = ExtBuilder::default()
			.pages(1)
			.signed_phase(3)
			.validate_signed_phase(1)
			.lookahead(0)
			.build_offchainify(1);

		ext.execute_with(|| {
            assert_eq!(System::block_number(), 0);
            assert_eq!(Pages::get(), 1);
            assert_eq!(<Round<T>>::get(), 0);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

			let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );
            assert_eq!(next_election, 100);

            let phase_transitions = calculate_phases();

			// tests transition phase boundaries.
            roll_to(*phase_transitions.get("snapshot").unwrap());
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(Pages::get() - 1));
            assert!(Snapshot::<T>::targets().is_some());
            assert!(Snapshot::<T>::voters(0).is_none());

            roll_to(*phase_transitions.get("signed").unwrap());
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);
            assert!(Snapshot::<T>::voters(0).is_some());

            roll_to(*phase_transitions.get("validate").unwrap());
            let start_validate = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

            roll_to(*phase_transitions.get("unsigned").unwrap());
            let start_unsigned = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

			// roll to export phase to call elect().
			roll_to_export();

			// elect() should work.
            assert_ok!(MultiPhase::elect(0));

            // one page only -- election done, go to off phase.
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);
		})
	}

	#[test]
	fn multi_page() {
		let (mut ext, _) = ExtBuilder::default()
			.pages(2)
			.signed_phase(3)
			.validate_signed_phase(1)
			.lookahead(0)
			.build_offchainify(1);

		ext.execute_with(|| {
            assert_eq!(System::block_number(), 0);
            assert_eq!(Pages::get(), 2);
            assert_eq!(<Round<T>>::get(), 0);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

			let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );
            assert_eq!(next_election, 100);

            let phase_transitions = calculate_phases();

            // two blocks for snapshot.
            roll_to(*phase_transitions.get("snapshot").unwrap());
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(Pages::get() - 1));

            roll_one();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(0));

            roll_to(*phase_transitions.get("signed").unwrap());
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

             // ensure snapshot is sound by the beginning of the signed phase.
            assert_ok!(Snapshot::<T>::ensure());

            roll_one();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

            // two blocks for validate signed.
            roll_to(*phase_transitions.get("validate").unwrap());
            let start_validate = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

            // now in unsigned until elect() is called.
            roll_one();
            roll_one();
            let start_unsigned = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned - 1));
		})
	}

	#[test]
	fn emergency_phase_works() {
		let (mut ext, _) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
			// if election fails, enters in emergency phase.
			ElectionFailure::<T>::set(ElectionFailureStrategy::Emergency);

			// force phase export for elect to be called without any solution stored.
			set_phase_to(Phase::Export(System::block_number()));

			// election will fail due to inexistent solution.
			assert!(MultiPhase::elect(Pallet::<T>::msp()).is_err());

			// thus entering in emergency phase.
			assert_eq!(<CurrentPhase<T>>::get(), Phase::Emergency);
		})
	}

	#[test]
	fn restart_after_elect_fails_works() {
		let (mut ext, _) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
        	let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );

            // if election fails, restart the election round.
            ElectionFailure::<T>::set(ElectionFailureStrategy::Restart);

			// roll to next election without ocw to prevent solution to be stored.
            roll_to(next_election);

			// election will fail due to inexistent solution.
            assert!(MultiPhase::elect(Pallet::<T>::msp()).is_err());
			// thus restarting from Off phase.
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);
        })
	}
}

mod phase_transition_errors {
	use super::*;

	#[test]
	fn snapshot_fails() {
		ExtBuilder::default().build_and_execute(|| {
			let phase_transitions = calculate_phases();

			// if election fails, enters in emergency phase.
			ElectionFailure::<T>::set(ElectionFailureStrategy::Emergency);

			// force error from data provider when fetching the electable targets.
			data_provider_errors(true);
			// roll to target snapshot block.
			roll_to(*phase_transitions.get("snapshot").unwrap());
			// data provider errors when fetching target snapshot, enters in emergency phase.
			assert_eq!(<CurrentPhase<T>>::get(), Phase::Emergency);

			Pallet::<T>::reset_round_restart();
			assert_eq!(current_phase(), Phase::Off);

			data_provider_errors(false);
			roll_to(*phase_transitions.get("snapshot").unwrap() + 2);
			// target snapshot and page msp of voters has been successfully prepared.
			assert!(Snapshot::<T>::targets().is_some());
			assert!(Snapshot::<T>::voters(MultiPhase::msp()).is_some());

			// fail the next voter page, enter in emergency phase.
			data_provider_errors(true);
			roll_one();
			assert_eq!(current_phase(), Phase::Emergency);
		})
	}

	#[test]
	fn export_fails() {
		let (mut ext, _) = ExtBuilder::default().export_limit(10).build_offchainify(1);

		ext.execute_with(|| {
			// if election fails, enters in emergency phase.
			ElectionFailure::<T>::set(ElectionFailureStrategy::Emergency);

			assert_eq!(ExportPhaseLimit::get(), 10);

			roll_to_export();
			assert!(current_phase().is_export());

			// exceed the export phase block limit without calling elect, thus failing the
			// election.
			roll_to(System::block_number() + ExportPhaseLimit::get() + 1);
			assert_eq!(current_phase(), Phase::Emergency);
		})
	}
}

mod snapshot {
	use super::*;

	use frame_support::{assert_noop, assert_ok};

	#[test]
	fn setters_getters_work() {
		ExtBuilder::default().pages(2).build_and_execute(|| {
			let t = BoundedVec::<_, _>::try_from(vec![]).unwrap();
			let v = BoundedVec::<_, _>::try_from(vec![]).unwrap();

			assert!(Snapshot::<T>::targets().is_none());
			assert!(Snapshot::<T>::voters(0).is_none());
			assert!(Snapshot::<T>::voters(1).is_none());

			Snapshot::<T>::set_targets(t.clone());
			assert!(Snapshot::<T>::targets().is_some());

			Snapshot::<T>::set_voters(0, v.clone());
			Snapshot::<T>::set_voters(1, v.clone());

			assert!(Snapshot::<T>::voters(0).is_some());
			assert!(Snapshot::<T>::voters(1).is_some());

			// ensure snapshot is sound.
			force_phase(Phase::Signed);
			assert_ok!(Snapshot::<T>::ensure());

			// force Off and clear up snapshot.
			force_phase(Phase::Off);
			Snapshot::<T>::kill();
			assert!(Snapshot::<T>::targets().is_none());
			assert!(Snapshot::<T>::voters(0).is_none());
			assert!(Snapshot::<T>::voters(1).is_none());

			assert_ok!(Snapshot::<T>::ensure());
		})
	}

	#[test]
	fn targets_voters_snapshot_boundary_checks_works() {
		ExtBuilder::default().core_try_state(false).build_and_execute(|| {
			assert_eq!(Pages::get(), 3);
			assert_eq!(MultiPhase::msp(), 2);
			assert_eq!(MultiPhase::lsp(), 0);

			assert_ok!(MultiPhase::create_targets_snapshot());

			assert_ok!(MultiPhase::create_voters_snapshot(2));
			assert_ok!(MultiPhase::create_voters_snapshot(1));
			assert_ok!(MultiPhase::create_voters_snapshot(0));

			assert_noop!(
				MultiPhase::create_voters_snapshot(3),
				ElectionError::<T>::RequestedPageExceeded
			);
			assert_noop!(
				MultiPhase::create_voters_snapshot(10),
				ElectionError::<T>::RequestedPageExceeded
			);
		})
	}

	#[test]
	fn create_targets_snapshot_bounds_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(MultiPhase::msp(), 2);

			let no_bounds = ElectionBoundsBuilder::default().build().targets;
			let all_targets =
				<MockStaking as ElectionDataProvider>::electable_targets(no_bounds, 0);
			assert_eq!(all_targets.unwrap(), Targets::get());
			assert_eq!(Targets::get().len(), 8);

			// sets max targets per page to 2.
			TargetSnapshotPerBlock::set(2);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 2);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20]);

			// sets max targets per page to 4.
			TargetSnapshotPerBlock::set(4);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 4);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20, 30, 40]);

			Snapshot::<T>::kill();

			TargetSnapshotPerBlock::set(6);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 6);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20, 30, 40, 50, 60]);

			// reset storage.
			Snapshot::<T>::kill();
		})
	}

	#[test]
	fn voters_snapshot_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(MultiPhase::msp(), 2);

			let no_bounds = ElectionBoundsBuilder::default().build().voters;
			let all_voters = <MockStaking as ElectionDataProvider>::electing_voters(no_bounds, 0);
			assert_eq!(all_voters.unwrap(), Voters::get());
			assert_eq!(Voters::get().len(), 16);

			// sets max voters per page to 7.
			VoterSnapshotPerBlock::set(7);

			let voters_page = |page: PageIndex| {
				Snapshot::<T>::voters(page)
					.unwrap()
					.iter()
					.map(|v| v.0)
					.collect::<Vec<AccountId>>()
			};

			// page `msp`.
			let result_and_count = MultiPhase::create_voters_snapshot(MultiPhase::msp());
			assert_eq!(result_and_count.unwrap(), 7);
			assert_eq!(voters_page(MultiPhase::msp()), vec![1, 2, 3, 4, 5, 6, 7]);

			let result_and_count = MultiPhase::create_voters_snapshot(1);
			assert_eq!(result_and_count.unwrap(), 7);
			assert_eq!(voters_page(1), vec![8, 10, 20, 30, 40, 50, 60]);

			// page `lsp`.
			let result_and_count = MultiPhase::create_voters_snapshot(MultiPhase::lsp());
			assert_eq!(result_and_count.unwrap(), 2);
			assert_eq!(voters_page(MultiPhase::lsp()), vec![70, 80]);

			assert_ok!(MultiPhase::create_targets_snapshot());

			force_phase(Phase::Signed);
			assert_ok!(Snapshot::<T>::ensure());
		})
	}

	#[test]
	fn try_progress_snapshot_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(Pages::get(), 3);
			assert_ok!(Snapshot::<T>::ensure());

			// no snapshot yet.
			assert!(Snapshot::<T>::targets().is_none());
			let _ = (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp())
				.rev()
				.map(|p| assert!(Snapshot::<T>::voters(p).is_none()))
				.collect::<Vec<_>>();

			roll_to_snapshot();
			assert_eq!(current_phase(), Phase::Snapshot(MultiPhase::msp()));

			// first snapshot to be generated is the (single-page) target snapshot at idx = msp.
			assert!(Snapshot::<T>::targets().is_some());
			assert!(Snapshot::<T>::voters(MultiPhase::msp()).is_none());

			let _ = (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp())
				.rev()
				.map(|p| {
					assert!(Snapshot::<T>::voters(p).is_none());

					roll_one();
					assert!(Snapshot::<T>::voters(p).is_some());
				})
				.collect::<Vec<_>>();

			assert!(current_phase().is_signed());

			// all snapshot pages are in storage.
			let _ = (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp())
				.rev()
				.map(|p| assert!(Snapshot::<T>::voters(p).is_some()))
				.collect::<Vec<_>>();

			assert_ok!(Snapshot::<T>::ensure());

			// snapshot ready, change to Phase::Signed
			roll_one();
			assert_eq!(current_phase(), Phase::Signed);
		})
	}
}

mod election_provider {
	use super::*;
	use crate::unsigned::miner::Miner;

	#[test]
	fn snapshot_to_supports_conversions_work() {
		use frame_election_provider_support::BoundedSupports;
		use sp_npos_elections::{Support, Supports};

		type VotersPerPage = <T as pallet::Config>::VoterSnapshotPerBlock;
		type TargetsPerPage = <T as pallet::Config>::TargetSnapshotPerBlock;
		type Pages = <T as pallet::Config>::Pages;

		// snapshot state across all externalities' tests.
		let all_targets: BoundedVec<AccountId, TargetsPerPage> = bounded_vec![10, 20, 30, 40];
		let all_voter_pages: BoundedVec<BoundedVec<VoterOf<Runtime>, VotersPerPage>, Pages> = bounded_vec![
			bounded_vec![
				(1, 100, bounded_vec![10, 20]),
				(2, 20, bounded_vec![30]),
				(3, 30, bounded_vec![10]),
				(10, 10, bounded_vec![10])
			],
			bounded_vec![
				(20, 20, bounded_vec![20]),
				(30, 30, bounded_vec![30]),
				(40, 40, bounded_vec![40])
			],
		];

		ExtBuilder::default()
			.pages(2)
			.snapshot_voters_page(4)
			.snapshot_targets_page(4)
			.desired_targets(2)
			.core_try_state(false)
			.build_and_execute(|| {
				assert_eq!(MultiPhase::msp(), 1);

				Snapshot::<T>::set_targets(all_targets.clone());
				Snapshot::<T>::set_voters(0, all_voter_pages[0].clone());
				Snapshot::<T>::set_voters(1, all_voter_pages[1].clone());

				let (results, _) = Miner::<T>::mine_paged_solution_with_snapshot(
					&all_voter_pages,
					&all_targets,
					Pages::get(),
					current_round(),
					Snapshot::<T>::desired_targets().unwrap(),
					false,
				)
				.unwrap();

				let supports_page_zero =
					VerifierPallet::feasibility_check(results.solution_pages[0].clone(), 0)
						.unwrap();
				let supports_page_one =
					VerifierPallet::feasibility_check(results.solution_pages[1].clone(), 1)
						.unwrap();

				let s0: Supports<AccountId> = vec![
					(10, Support { total: 90, voters: vec![(3, 30), (10, 10), (1, 50)] }),
					(20, Support { total: 50, voters: vec![(1, 50)] }),
				];
				let bs0: BoundedSupports<_, _, _> = s0.try_into().unwrap();

				let s1: Supports<AccountId> =
					vec![(20, Support { total: 20, voters: vec![(20, 20)] })];
				let bs1: BoundedSupports<_, _, _> = s1.try_into().unwrap();

				assert_eq!(supports_page_zero, bs0);
				assert_eq!(supports_page_one, bs1);

				// consume supports and checks they fit within the max backers per winner bounds.
				let _ = supports_page_zero
					.into_iter()
					.map(|p| assert!(p.1.voters.len() as u32 <= MaxBackersPerWinner::get()))
					.collect::<Vec<_>>();
				let _ = supports_page_one
					.into_iter()
					.map(|p| assert!(p.1.voters.len() as u32 <= MaxBackersPerWinner::get()))
					.collect::<Vec<_>>();
			});

		// with max_backers_winner = 1
		ExtBuilder::default()
			.pages(2)
			.snapshot_voters_page(4)
			.snapshot_targets_page(4)
			.desired_targets(2)
			.max_backers_per_winner(1)
			.core_try_state(false)
			.build_and_execute(|| {
				Snapshot::<T>::set_targets(all_targets.clone());
				Snapshot::<T>::set_voters(0, all_voter_pages[0].clone());
				Snapshot::<T>::set_voters(1, all_voter_pages[1].clone());

				let (results, _) = Miner::<T>::mine_paged_solution_with_snapshot(
					&all_voter_pages,
					&all_targets,
					Pages::get(),
					current_round(),
					Snapshot::<T>::desired_targets().unwrap(),
					false,
				)
				.unwrap();

				let supports_page_zero =
					VerifierPallet::feasibility_check(results.solution_pages[0].clone(), 0)
						.unwrap();
				let supports_page_one =
					VerifierPallet::feasibility_check(results.solution_pages[1].clone(), 1)
						.unwrap();

				// consume supports and checks they fit within the max backers per winner bounds.
				let _ = supports_page_zero
					.into_iter()
					.map(|p| assert!(p.1.voters.len() as u32 <= MaxBackersPerWinner::get()))
					.collect::<Vec<_>>();
				let _ = supports_page_one
					.into_iter()
					.map(|p| assert!(p.1.voters.len() as u32 <= MaxBackersPerWinner::get()))
					.collect::<Vec<_>>();
			})
	}
}

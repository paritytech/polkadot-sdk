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

use crate::{
	verifier::{self, Verifier},
	Config, CurrentPhase, Pallet, Phase, Snapshot,
};
use frame_benchmarking::v2::*;
use frame_election_provider_support::{ElectionDataProvider, ElectionProvider};
use frame_support::{assert_ok, pallet_prelude::*};

const SNAPSHOT_NOT_BIG_ENOUGH: &'static str = "Snapshot page is not full, you should run this \
benchmark with enough genesis stakers in staking to fill a page of voters/targets \
as per VoterSnapshotPerBlock and TargetSnapshotPerBlock. Generate at least \
2 * VoterSnapshotPerBlock) nominators and TargetSnapshotPerBlock validators. Use `dev_stakers` in \
genesis config.";

// TODO: remove unwraps from all benchmarks of this pallet -- it makes debugging via wasm harder

#[benchmarks(where T: crate::signed::Config + crate::unsigned::Config + crate::verifier::Config)]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn per_block_nothing() -> Result<(), BenchmarkError> {
		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);

		#[block]
		{
			Pallet::<T>::roll_next(false);
		}

		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn per_block_snapshot_msp() -> Result<(), BenchmarkError> {
		assert!(T::Pages::get() >= 2, "this benchmark only works in a runtime with 2 pages or more, set at least `type Pages = 2` for benchmark run");

		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get()));

		#[block]
		{
			Pallet::<T>::roll_next(false);
		}

		// we have collected the target snapshot only
		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get() - 1));
		assert_eq!(
			Snapshot::<T>::targets_decode_len().unwrap() as u32,
			T::TargetSnapshotPerBlock::get(),
			"{}",
			SNAPSHOT_NOT_BIG_ENOUGH
		);
		assert_eq!(Snapshot::<T>::voters_decode_len(T::Pages::get() - 1), None);

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn per_block_snapshot_rest() -> Result<(), BenchmarkError> {
		assert!(T::Pages::get() >= 2, "this benchmark only works in a runtime with 2 pages or more, set at least `type Pages = 2` for benchmark run");

		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		// roll to the first block of the snapshot.
		Pallet::<T>::roll_until_matches(|| {
			CurrentPhase::<T>::get() == Phase::Snapshot(T::Pages::get() - 1)
		});

		// we have collected the target snapshot only
		assert_eq!(
			Snapshot::<T>::targets_decode_len().unwrap() as u32,
			T::TargetSnapshotPerBlock::get()
		);
		// and no voters yet.
		assert_eq!(Snapshot::<T>::voters_decode_len(T::Pages::get() - 1), None);

		// take one more snapshot page.
		#[block]
		{
			Pallet::<T>::roll_next(false);
		}

		// we have now collected the first page of voters.
		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get() - 2));
		// it must be full
		assert_eq!(
			Snapshot::<T>::voters_decode_len(T::Pages::get() - 1).unwrap() as u32,
			T::VoterSnapshotPerBlock::get(),
			"{}",
			SNAPSHOT_NOT_BIG_ENOUGH
		);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn per_block_start_signed_validation() -> Result<(), BenchmarkError> {
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		Pallet::<T>::roll_until_before_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::SignedValidation(_))
		});

		assert!(CurrentPhase::<T>::get().is_signed());

		#[block]
		{
			Pallet::<T>::roll_next(false);
		}

		assert!(CurrentPhase::<T>::get().is_signed_validation());

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn export_non_terminal() -> Result<(), BenchmarkError> {
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		// submit a full solution.
		crate::Pallet::<T>::roll_to_signed_and_submit_full_solution()?;

		// fully verify it in the signed validation phase.
		assert!(T::Verifier::queued_score().is_none());
		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_))
		});

		// full solution is queued.
		assert!(T::Verifier::queued_score().is_some());
		assert_eq!(verifier::QueuedSolution::<T>::valid_iter().count() as u32, T::Pages::get());

		// Roll to Done phase to start export
		crate::Pallet::<T>::roll_until_matches(|| CurrentPhase::<T>::get().is_done());

		#[block]
		{
			// tell the data provider to do its election process for one page, while we are fully
			// ready.
			T::DataProvider::fetch_page(T::Pages::get() - 1);
		}

		// we should be in the export phase now.
		assert_eq!(CurrentPhase::<T>::get(), Phase::Export(T::Pages::get() - 2));

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn export_terminal() -> Result<(), BenchmarkError> {
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		// submit a full solution.
		crate::Pallet::<T>::roll_to_signed_and_submit_full_solution()?;

		// fully verify it in the signed validation phase.
		ensure!(T::Verifier::queued_score().is_none(), "nothing should be queued");
		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_))
		});

		// full solution is queued.
		ensure!(T::Verifier::queued_score().is_some(), "something should be queued");
		ensure!(
			verifier::QueuedSolution::<T>::valid_iter().count() as u32 == T::Pages::get(),
			"solution should be full"
		);

		// Roll to Done phase
		crate::Pallet::<T>::roll_until_matches(|| CurrentPhase::<T>::get().is_done());

		// Start export and fetch all pages except the last one
		(1..=T::Pages::get() - 1).rev().for_each(T::DataProvider::fetch_page);

		assert_eq!(CurrentPhase::<T>::get(), Phase::Export(0));

		#[block]
		{
			T::DataProvider::fetch_page(0);
		}

		// we should be in the off phase now.
		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn manage_fallback() -> Result<(), BenchmarkError> {
		// heaviest case is emergency set.
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		// roll to signed so the snapshot exists
		Pallet::<T>::roll_until_before_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Signed(_))
		});

		// set phase to emergency
		CurrentPhase::<T>::set(Phase::Emergency);
		let origin = T::ManagerOrigin::try_successful_origin()
			.map_err(|_| -> BenchmarkError { "cannot create manager origin".into() })?;
		#[block]
		{
			// fallback might decide to fail, that's okay..
			let maybe_err = Pallet::<T>::manage(origin, crate::ManagerOperation::EmergencyFallback);
			//.. but it cannot be bad origin.
			assert!(maybe_err.is_ok() || maybe_err.unwrap_err() != DispatchError::BadOrigin.into());
		}

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn admin_set() -> Result<(), BenchmarkError> {
		// heaviest case is emergency set.
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		// mine a single page solution.
		let solution = crate::Pallet::<T>::roll_to_signed_and_mine_solution(1);

		// verify to get the support.
		let (voter_pages, all_targets, desired_targets) =
			crate::unsigned::miner::OffchainWorkerMiner::<T>::fetch_snapshot(T::Pages::get())
				.map_err(|_| -> BenchmarkError { "fetch_snapshot".into() })?;
		let supports = crate::unsigned::miner::BaseMiner::<T::MinerConfig>::check_feasibility(
			&solution,
			&voter_pages,
			&all_targets,
			desired_targets,
		)
		.map_err(|_| -> BenchmarkError { "check_feasibility".into() })?;

		let single_support = supports
			.first()
			.cloned()
			.ok_or_else(|| -> BenchmarkError { "no support".into() })?;

		// set phase to emergency
		CurrentPhase::<T>::set(Phase::Emergency);

		// nothing is queued in verified just yet.
		assert!(<T::Verifier as Verifier>::queued_score().is_none());

		let origin = T::AdminOrigin::try_successful_origin()
			.map_err(|_| -> BenchmarkError { "cannot create admin origin".into() })?;
		#[block]
		{
			assert_ok!(Pallet::<T>::admin(
				origin,
				crate::AdminOperation::EmergencySetSolution(
					sp_std::boxed::Box::new(single_support),
					solution.score,
				),
			));
		}

		// something is queued now.
		assert!(<T::Verifier as Verifier>::queued_score().is_some());

		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::ExtBuilder::full().build_unchecked(),
		crate::mock::Runtime
	);
}

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

use crate::{Config, CurrentPhase, Pallet, Phase, Snapshot};
use frame_benchmarking::v2::*;
use frame_election_provider_support::ElectionDataProvider;
use frame_support::pallet_prelude::*;
const SNAPSHOT_NOT_BIG_ENOUGH: &'static str = "Snapshot page is not full, you should run this \
benchmark with enough genesis stakers in staking (DataProvider) to fill a page of voters/targets \
as per VoterSnapshotPerBlock and TargetSnapshotPerBlock. Generate at least \
2 * VoterSnapshotPerBlock) nominators and TargetSnapshotPerBlock validators";

#[benchmarks(where T: crate::signed::Config + crate::unsigned::Config + crate::verifier::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize_nothing() -> Result<(), BenchmarkError> {
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());
		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);

		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);
		Ok(())
	}

	#[benchmark]
	fn on_initialize_into_snapshot_msp() -> Result<(), BenchmarkError> {
		assert!(T::Pages::get() >= 2, "this benchmark only works in a runtime with 2 pages or more, set at least `type Pages = 2` for benchmark run");
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());
		// TODO: the results of this benchmark cause too many hits to voters bags list, why???

		// roll to next block until we are about to go into the snapshot.
		Pallet::<T>::run_until_before_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Snapshot(_))
		});

		// since we reverted the last page, we are still in phase Off.
		assert_eq!(CurrentPhase::<T>::get(), Phase::Off);

		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get() - 1));
		assert_eq!(
			Snapshot::<T>::voters_decode_len(T::Pages::get() - 1).unwrap() as u32,
			T::VoterSnapshotPerBlock::get(),
			"{}",
			SNAPSHOT_NOT_BIG_ENOUGH
		);
		assert_eq!(
			Snapshot::<T>::targets_decode_len().unwrap() as u32,
			T::TargetSnapshotPerBlock::get(),
			"{}",
			SNAPSHOT_NOT_BIG_ENOUGH
		);

		Ok(())
	}

	#[benchmark]
	fn on_initialize_into_snapshot_rest() -> Result<(), BenchmarkError> {
		assert!(T::Pages::get() >= 2, "this benchmark only works in a runtime with 2 pages or more, set at least `type Pages = 2` for benchmark run");
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());

		// roll to the first block of the snapshot.
		Pallet::<T>::roll_until_matches(|| matches!(CurrentPhase::<T>::get(), Phase::Snapshot(_)));

		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get() - 1));

		// take one more snapshot page.
		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(T::Pages::get() - 2));
		assert_eq!(
			Snapshot::<T>::voters_decode_len(T::Pages::get() - 2).unwrap() as u32,
			T::VoterSnapshotPerBlock::get(),
			"{}",
			SNAPSHOT_NOT_BIG_ENOUGH
		);
		Ok(())
	}

	#[benchmark]
	fn on_initialize_into_signed() -> Result<(), BenchmarkError> {
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());
		Pallet::<T>::run_until_before_matches(|| matches!(CurrentPhase::<T>::get(), Phase::Signed));

		assert_eq!(CurrentPhase::<T>::get(), Phase::Snapshot(0));

		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		assert_eq!(CurrentPhase::<T>::get(), Phase::Signed);

		Ok(())
	}

	#[benchmark]
	fn on_initialize_into_signed_validation() -> Result<(), BenchmarkError> {
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());
		Pallet::<T>::run_until_before_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::SignedValidation(_))
		});

		assert_eq!(CurrentPhase::<T>::get(), Phase::Signed);

		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		Ok(())
	}

	#[benchmark]
	fn on_initialize_into_unsigned() -> Result<(), BenchmarkError> {
		T::DataProvider::set_next_election(Pallet::<T>::reasonable_next_election());
		Pallet::<T>::run_until_before_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_))
		});
		assert!(matches!(CurrentPhase::<T>::get(), Phase::SignedValidation(_)));

		#[block]
		{
			Pallet::<T>::roll_next(true, false);
		}

		assert!(matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_)));
		Ok(())
	}

	#[benchmark]
	fn manage() -> Result<(), BenchmarkError> {
		#[block]
		{}
		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::ExtBuilder::full().build_unchecked(),
		crate::mock::Runtime
	);
}

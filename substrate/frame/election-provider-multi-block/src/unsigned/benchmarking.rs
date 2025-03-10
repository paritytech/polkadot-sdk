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
	unsigned::{miner::OffchainWorkerMiner, Call, Config, Pallet},
	verifier::Verifier,
	CurrentPhase, Phase,
};
use frame_benchmarking::v2::*;
use frame_election_provider_support::ElectionDataProvider;
use frame_support::{assert_ok, pallet_prelude::*};
use frame_system::RawOrigin;
use sp_std::boxed::Box;
#[benchmarks(where T: crate::Config + crate::signed::Config + crate::verifier::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn validate_unsigned() -> Result<(), BenchmarkError> {
		// TODO: for now we are not using this, maybe remove?
		// roll to unsigned phase open
		T::DataProvider::set_next_election(crate::Pallet::<T>::reasonable_next_election());
		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_))
		});
		let call: Call<T> = OffchainWorkerMiner::<T>::mine_solution(1, false)
			.map(|solution| Call::submit_unsigned { paged_solution: Box::new(solution) })
			.unwrap();

		#[block]
		{
			assert_ok!(Pallet::<T>::validate_unsigned(TransactionSource::Local, &call));
		}

		Ok(())
	}

	#[benchmark]
	fn submit_unsigned() -> Result<(), BenchmarkError> {
		// roll to unsigned phase open
		T::DataProvider::set_next_election(crate::Pallet::<T>::reasonable_next_election());
		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Unsigned(_))
		});
		// TODO: we need to better ensure that this is actually worst case
		let solution = OffchainWorkerMiner::<T>::mine_solution(1, false).unwrap();

		// nothing is queued
		assert!(T::Verifier::queued_score().is_none());
		#[block]
		{
			assert_ok!(Pallet::<T>::submit_unsigned(RawOrigin::None.into(), Box::new(solution)));
		}

		// something is queued
		assert!(T::Verifier::queued_score().is_some());
		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::ExtBuilder::full().build_unchecked(),
		crate::mock::Runtime
	);
}

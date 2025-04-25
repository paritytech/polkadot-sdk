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
	signed::{Config, Pallet, Submissions},
	types::PagedRawSolution,
	unsigned::miner::OffchainWorkerMiner,
	CurrentPhase, Phase, Round,
};
use frame_benchmarking::v2::*;
use frame_election_provider_support::ElectionProvider;
use frame_support::pallet_prelude::*;
use frame_system::RawOrigin;
use sp_npos_elections::ElectionScore;
use sp_runtime::traits::One;
use sp_std::boxed::Box;

#[benchmarks(where T: crate::Config + crate::verifier::Config + crate::unsigned::Config)]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn register_not_full() -> Result<(), BenchmarkError> {
		CurrentPhase::<T>::put(Phase::Signed(T::SignedPhase::get() - One::one()));
		let round = Round::<T>::get();
		let alice = crate::Pallet::<T>::funded_account("alice", 0);
		let score = ElectionScore::default();

		assert_eq!(Submissions::<T>::sorted_submitters(round).len(), 0);
		#[block]
		{
			Pallet::<T>::register(RawOrigin::Signed(alice).into(), score)?;
		}

		assert_eq!(Submissions::<T>::sorted_submitters(round).len(), 1);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn register_eject() -> Result<(), BenchmarkError> {
		CurrentPhase::<T>::put(Phase::Signed(T::SignedPhase::get() - One::one()));
		let round = Round::<T>::get();

		for i in 0..T::MaxSubmissions::get() {
			let submitter = crate::Pallet::<T>::funded_account("submitter", i);
			let score = ElectionScore { minimal_stake: i.into(), ..Default::default() };
			Pallet::<T>::register(RawOrigin::Signed(submitter.clone()).into(), score)?;

			// The first one, which will be ejected, has also submitted all pages
			if i == 0 {
				for p in 0..T::Pages::get() {
					let page = Some(Default::default());
					Pallet::<T>::submit_page(RawOrigin::Signed(submitter.clone()).into(), p, page)?;
				}
			}
		}

		let who = crate::Pallet::<T>::funded_account("who", 0);
		let score =
			ElectionScore { minimal_stake: T::MaxSubmissions::get().into(), ..Default::default() };

		assert_eq!(
			Submissions::<T>::sorted_submitters(round).len(),
			T::MaxSubmissions::get() as usize
		);

		#[block]
		{
			Pallet::<T>::register(RawOrigin::Signed(who).into(), score)?;
		}

		assert_eq!(
			Submissions::<T>::sorted_submitters(round).len(),
			T::MaxSubmissions::get() as usize
		);
		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn submit_page() -> Result<(), BenchmarkError> {
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Signed(_))
		});

		// mine a full solution
		let PagedRawSolution { score, solution_pages, .. } =
			OffchainWorkerMiner::<T>::mine_solution(T::Pages::get(), false).unwrap();
		let page = Some(Box::new(solution_pages[0].clone()));

		// register alice
		let alice = crate::Pallet::<T>::funded_account("alice", 0);
		Pallet::<T>::register(RawOrigin::Signed(alice.clone()).into(), score)?;

		#[block]
		{
			Pallet::<T>::submit_page(RawOrigin::Signed(alice).into(), 0, page)?;
		}

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn unset_page() -> Result<(), BenchmarkError> {
		#[cfg(test)]
		crate::mock::ElectionStart::set(sp_runtime::traits::Bounded::max_value());
		crate::Pallet::<T>::start().unwrap();

		crate::Pallet::<T>::roll_until_matches(|| {
			matches!(CurrentPhase::<T>::get(), Phase::Signed(_))
		});

		// mine a full solution
		let PagedRawSolution { score, solution_pages, .. } =
			OffchainWorkerMiner::<T>::mine_solution(T::Pages::get(), false).unwrap();
		let page = Some(Box::new(solution_pages[0].clone()));

		// register alice
		let alice = crate::Pallet::<T>::funded_account("alice", 0);
		Pallet::<T>::register(RawOrigin::Signed(alice.clone()).into(), score)?;

		// submit page
		Pallet::<T>::submit_page(RawOrigin::Signed(alice.clone()).into(), 0, page)?;

		#[block]
		{
			Pallet::<T>::submit_page(RawOrigin::Signed(alice).into(), 0, None)?;
		}

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn bail() -> Result<(), BenchmarkError> {
		CurrentPhase::<T>::put(Phase::Signed(T::SignedPhase::get() - One::one()));
		let alice = crate::Pallet::<T>::funded_account("alice", 0);

		// register alice
		let score = ElectionScore::default();
		Pallet::<T>::register(RawOrigin::Signed(alice.clone()).into(), score)?;

		// submit all pages
		for p in 0..T::Pages::get() {
			let page = Some(Default::default());
			Pallet::<T>::submit_page(RawOrigin::Signed(alice.clone()).into(), p, page)?;
		}

		#[block]
		{
			Pallet::<T>::bail(RawOrigin::Signed(alice).into())?;
		}

		Ok(())
	}

	#[benchmark(pov_mode = Measured)]
	fn clear_old_round_data(p: Linear<1, { T::Pages::get() }>) -> Result<(), BenchmarkError> {
		// set signed phase and alice ready to submit
		CurrentPhase::<T>::put(Phase::Signed(T::SignedPhase::get() - One::one()));
		let alice = crate::Pallet::<T>::funded_account("alice", 0);

		// register alice
		let score = ElectionScore::default();
		Pallet::<T>::register(RawOrigin::Signed(alice.clone()).into(), score)?;

		// submit a solution with p pages.
		for pp in 0..p {
			let page = Some(Default::default());
			Pallet::<T>::submit_page(RawOrigin::Signed(alice.clone()).into(), pp, page)?;
		}

		// force rotate to the next round.
		let prev_round = Round::<T>::get();
		crate::Pallet::<T>::rotate_round();

		#[block]
		{
			Pallet::<T>::clear_old_round_data(RawOrigin::Signed(alice).into(), prev_round, p)?;
		}

		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::ExtBuilder::signed().build_unchecked(),
		crate::mock::Runtime
	);
}

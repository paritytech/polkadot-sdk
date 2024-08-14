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

//! # Benchmarking for the Elections Multiblock Unsigned sub-pallet.

use super::*;
use crate::{
	benchmarking::helpers, signed::Config as ConfigSigned, unsigned::Config, BenchmarkingConfig,
	Config as ConfigCore, ConfigVerifier, Pallet as PalletCore, Phase,
};
use frame_system::RawOrigin;

use frame_benchmarking::v2::*;

#[benchmarks(
    where T: Config + ConfigCore + ConfigSigned + ConfigVerifier,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit_page_unsigned(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		// configs necessary to proceed with the unsigned submission.
		PalletCore::<T>::phase_transition(Phase::Unsigned(0u32.into()));

		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		// the last page (0) will also perfom a full feasibility check for all the pages in the
		// queue. For this benchmark, we want to ensure that we do not call `submit_page_unsigned`
		// on the last page, to avoid this extra step.
		assert!(T::Pages::get() >= 2);

		let (claimed_full_score, partial_score, paged_solution) =
			OffchainWorkerMiner::<T>::mine(PalletCore::<T>::msp()).map_err(|err| {
				log!(error, "mine error: {:?}", err);
				BenchmarkError::Stop("miner error")
			})?;

		#[extrinsic_call]
		_(
			RawOrigin::None,
			PalletCore::<T>::msp(),
			paged_solution,
			partial_score,
			claimed_full_score,
		);

		Ok(())
	}

	impl_benchmark_test_suite!(
		PalletUnsigned,
		crate::mock::ExtBuilder::default(),
		crate::mock::Runtime,
		exec_name = build_and_execute
	);
}

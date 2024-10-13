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
use crate::{benchmarking::helpers, BenchmarkingConfig, ConfigCore, ConfigSigned, ConfigUnsigned};
use frame_benchmarking::v2::*;

#[benchmarks(
    where T: ConfigCore + ConfigSigned + ConfigUnsigned,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn verify_page(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS,
			<T as ConfigCore>::BenchmarkingConfig::TARGETS,
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		#[block]
		{
			// TODO
			let _ = 1 + 2;
		}

		Ok(())
	}

	impl_benchmark_test_suite!(
		PalletSigned,
		crate::mock::ExtBuilder::default(),
		crate::mock::Runtime,
		exec_name = build_and_execute
	);
}

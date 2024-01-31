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

//! Parameters pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Parameters;

use frame_benchmarking::v2::*;
use frame_system::{Pallet as System, RawOrigin};
use sp_core::Get;

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn init() {
		let acc =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(acc, Default::default()));

		assert!(Salary::<T, I>::status().is_some());
	}

	impl_benchmark_test_suite! {
		Salary,
		crate::tests::new_test_ext(),
		crate::tests::Test,
	}
}

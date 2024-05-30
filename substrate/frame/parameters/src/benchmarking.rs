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
#[cfg(test)]
use crate::Pallet as Parameters;

use frame_benchmarking::v2::*;

#[benchmarks(where T::RuntimeParameters: Default)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_parameter() -> Result<(), BenchmarkError> {
		let kv = T::RuntimeParameters::default();
		let k = kv.clone().into_parts().0;

		let origin =
			T::AdminOrigin::try_successful_origin(&k).map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, kv);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Parameters,
		crate::tests::mock::new_test_ext(),
		crate::tests::mock::Runtime,
	}
}

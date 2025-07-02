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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
// use crate::{pallet as pallet_origin_and_gate, Pallet as OriginAndGate};

use frame_benchmarking::{v2::*, BenchmarkError};
use sp_runtime::traits::DispatchTransaction;

// Import mock directly instead of through module import
#[path = "./mock.rs"]
pub mod mock;
pub use mock::{Test, new_test_ext};

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks]
mod benchmarks {
	use super::*;
	#[cfg(test)]
	use crate::Pallet as OriginAndGate;
	use frame_system::RawOrigin;

	// This will measure the execution time of `set_dummy`.
	#[benchmark]
	fn set_dummy() {
		// This is the benchmark setup phase.
		// `set_dummy` is a constant time function, hence we hard-code some random value here.
		let value: T::Balance = 1000u32.into();

		#[extrinsic_call]
		set_dummy(RawOrigin::Root, value); // The execution phase is just running `set_dummy` extrinsic call

		// This is the optional benchmark verification phase, asserting certain states.
		assert_eq!(Dummy::<T>::get(), Some(value))
	}

	impl_benchmark_test_suite!(OriginAndGate, crate::mock::new_test_ext(), crate::mock::Test);
}

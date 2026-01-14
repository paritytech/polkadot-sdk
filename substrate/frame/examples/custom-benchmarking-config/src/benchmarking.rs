// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Benchmarking for `pallet-example-custom-benchmarking-config`.

// Only enable this module for benchmarking.
#![cfg(feature = "runtime-benchmarks")]

use crate::*;
use frame_benchmarking::v2::*;

pub trait BenchmarkHelper<T: Config> {
	fn initialize_and_get_id() -> u32;
}

pub trait BenchmarkConfig: pallet::Config {
	type Helper: BenchmarkHelper<Self>;
}

impl<T: Config> BenchmarkHelper<T> for () {
	fn initialize_and_get_id() -> u32 {
		let id = 1_000;

		NextId::<T>::put(id);
		Registered::<T>::remove(id);
		id
	}
}

#[benchmarks(where T: BenchmarkConfig)]
mod benchmarks {
	use super::*;
	use frame_system::RawOrigin;

	#[benchmark]
	fn register() {
		let caller: T::AccountId = whitelisted_caller();
		let id = T::Helper::initialize_and_get_id();

		#[extrinsic_call]
		register(RawOrigin::Signed(caller), id);

		assert_eq!(NextId::<T>::get(), id);
		assert_eq!(Registered::<T>::contains_key(id), true);
	}

	impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}

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

//! Benchmarking for `pallet-example-kitchensink`.

// Only enable this module for benchmarking.
#![cfg(feature = "runtime-benchmarks")]
use super::*;

#[allow(unused)]
use crate::Pallet as Kitchensink;

use frame_benchmarking::v2::*;
use frame_support::pallet_prelude::TransactionSource;
use frame_system::RawOrigin;

// To actually run this benchmark on pallet-example-kitchensink, we need to put this pallet into the
//   runtime and compile it with `runtime-benchmarks` feature. The detail procedures are
//   documented at:
//   https://docs.substrate.io/reference/how-to-guides/weights/add-benchmarks/
//
// The auto-generated weight estimate of this pallet is copied over to the `weights.rs` file.
// The exact command of how the estimate generated is printed at the top of the file.

// Details on using the benchmarks macro can be seen at:
//   https://paritytech.github.io/substrate/master/frame_benchmarking/trait.Benchmarking.html#tymethod.benchmarks
#[benchmarks]
mod benchmarks {
	use super::*;

	// This will measure the execution time of `set_foo`.
	#[benchmark]
	fn set_foo_benchmark() {
		// This is the benchmark setup phase.
		// `set_foo` is a constant time function, hence we hard-code some random value here.
		let value = 1000u32.into();
		#[extrinsic_call]
		set_foo(RawOrigin::Root, value, 10u128); // The execution phase is just running `set_foo` extrinsic call

		// This is the optional benchmark verification phase, asserting certain states.
		assert_eq!(Foo::<T>::get(), Some(value))
	}

	// This will measure the execution time of `set_foo_using_authorize`.
	#[benchmark]
	fn set_foo_using_authorize() {
		// This is the benchmark setup phase.

		// `set_foo_using_authorize` is only authorized when value is 42 so we will use it.
		let value = 42u32;
		// We dispatch with authorized origin, it is the origin resulting from authorization.
		let origin = RawOrigin::Authorized;

		#[extrinsic_call]
		_(origin, value); // The execution phase is just running `set_foo_using_authorize` extrinsic call

		// This is the optional benchmark verification phase, asserting certain states.
		assert_eq!(Foo::<T>::get(), Some(42))
	}

	// This will measure the weight for the closure in `[pallet::authorize(...)]`.
	#[benchmark]
	fn authorize_set_foo_using_authorize() {
		// This is the benchmark setup phase.

		let call = Call::<T>::set_foo_using_authorize { new_foo: 42 };
		let source = TransactionSource::External;
		Foo::<T>::kill();

		// We use a block with specific code to benchmark the closure.
		#[block]
		{
			use frame_support::traits::Authorize;
			call.authorize(source)
				.expect("Call give some authorization")
				.expect("Authorization is successful");
		}
	}

	// This line generates test cases for benchmarking, and could be run by:
	//   `cargo test -p pallet-example-kitchensink --all-features`, you will see one line per case:
	//   `test benchmarking::bench_set_foo_benchmark ... ok`
	//   `test benchmarking::bench_set_foo_using_authorize_benchmark ... ok` in the result.
	//   `test benchmarking::bench_authorize_set_foo_using_authorize_benchmark ... ok` in the
	// result.
	//
	// The line generates three steps per benchmark, with repeat=1 and the three steps are
	//   [low, mid, high] of the range.
	impl_benchmark_test_suite!(Kitchensink, crate::tests::new_test_ext(), crate::tests::Test);
}

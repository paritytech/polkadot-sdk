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

use frame_benchmarking::v2::*;
use frame_support_test::Config;
use frame_support::parameter_types;

#[benchmarks]
mod benches {
	use super::*;

	const MY_CONST: u32 = 100;

	const fn my_fn() -> u32 {
		200
	}

	parameter_types! {
		const MyConst: u32 = MY_CONST;
	}

	#[benchmark(skip_meta, extra)]
	fn bench(a: Linear<{MY_CONST * 2}, {my_fn() + MyConst::get()}>) {
		let a = 2 + 2;
		#[block]
		{}
		assert_eq!(a, 4);
	}
}

fn main() {}

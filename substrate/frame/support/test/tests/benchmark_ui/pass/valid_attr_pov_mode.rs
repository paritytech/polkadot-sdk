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

#[benchmarks]
mod benches {
	use super::*;

	#[benchmark(skip_meta, extra, pov_mode = Measured)]
	fn bench1() {
		#[block]
		{}
	}

	#[benchmark(pov_mode = Measured, extra, skip_meta)]
	fn bench2() {
		#[block]
		{}
	}

	#[benchmark(extra, pov_mode = Measured {
		Pallet: Measured,
		Pallet::Storage: MaxEncodedLen,
	}, skip_meta)]
	fn bench3() {
		#[block]
		{}
	}

	#[benchmark(skip_meta, extra, pov_mode = Measured {
		Pallet::Storage: MaxEncodedLen,
		Pallet::StorageSubKey: Measured,
	})]
	fn bench4() {
		#[block]
		{}
	}

	#[benchmark(pov_mode = MaxEncodedLen {
		Pallet::Storage: Measured,
		Pallet::StorageSubKey: Measured
	}, extra, skip_meta)]
	fn bench5() {
		#[block]
		{}
	}

	#[benchmark(pov_mode = MaxEncodedLen {
		Pallet::Storage: Measured,
		Pallet::Storage::Nested: Ignored
	}, extra, skip_meta)]
	fn bench6() {
		#[block]
		{}
	}
}

fn main() {}

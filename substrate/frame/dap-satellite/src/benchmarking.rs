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

//! Benchmarks for pallet-dap-satellite.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::Unbalanced;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark for [`SendToDap::send_native`].
	///
	/// This measures the full cost of a satellite-to-DAP transfer.
	#[benchmark]
	fn send_native() {
		let satellite = Pallet::<T>::satellite_account();
		let ed = T::Currency::minimum_balance();
		let amount = T::MinTransferAmount::get();

		// Fund with ED (to keep account alive) plus the amount to be sent.
		T::Currency::write_balance(&satellite, ed + amount)
			.expect("benchmark setup should succeed");

		#[block]
		{
			let _ = T::SendToDap::send_native(satellite, amount);
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(true), crate::mock::Test);
}

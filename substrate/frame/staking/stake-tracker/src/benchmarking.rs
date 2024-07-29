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

//! # Stake Tracker Pallet benchmarking.

use super::*;
use crate::Pallet as StakeTracker;

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

const SEED: u32 = 0;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn settle() {
		let caller = whitelisted_caller();
		let target: T::AccountId = account("target", 0, SEED);

		StakeTracker::<T>::setup_target(&target);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), target.clone());
	}

	impl_benchmark_test_suite!(
		StakeTracker,
		crate::mock::ExtBuilder::default().set_update_threshold(Some(50))
		crate::mock::Test,
		exec_name = build_and_execute
	);
}

mod utils {
	use super::*;

	fn bond_target<T: Config>() -> T::AccountId {
		let target = account("target", 0, SEED);
		target
	}
}

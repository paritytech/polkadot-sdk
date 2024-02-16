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

//! StakeTracker pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::{mock::add_dangling_target_with_nominators, *};
pub use frame_benchmarking::v1::{
	account, benchmarks, impl_benchmark_test_suite, whitelist_account, whitelisted_caller,
};
use frame_system::RawOrigin;

const SEED: u32 = 0;

// returns the target and voter account IDs.
fn add_dangling_target<T: Config>() -> (T::AccountId, T::AccountId) {
	let target = account("target", 0, SEED);
	let voter = account("voter", 0, SEED);

	add_dangling_target_with_nominators(target, vec![voter]);

	(target, voter)
}

benchmarks! {
	drop_dangling_nomination {
		let caller  = account("caller", 0, SEED);
		whitelist_account!(caller);

		let (target, voter) = add_dangling_target::<T>();

	}: _(RawOrigin::Signed(caller), voter, target)
	verify {
		// voter is not nominating validator anymore
		// target is not in the target list
	}

	impl_benchmark_test_suite!(
		StakeTracker,
		crate::mock::ExtBuilder::default(),
		crate::mock::Test,
		exec_name = build_and_execute
	);
}

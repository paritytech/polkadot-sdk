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
use crate::{LastSettledApprovals, Pallet as StakeTracker};

use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;

const SEED: u32 = 0;
// sensible high and low nomination quota to extrapolate the costs of settling approvals for
// different `Staking::MaxNominations`.
const LOW_NOMINATIONS_QUOTA: u32 = 6;
const HIGH_NOMINATIONS_QUOTA: u32 = 24;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn settle_approvals(n: Linear<LOW_NOMINATIONS_QUOTA, HIGH_NOMINATIONS_QUOTA>) {
		let caller = whitelisted_caller();
		// 1. nominator nominates n targets.
		// 2. on_stake_update(nominator, NominatorReward)
		// 3. verify last seen != active stake
		// 4. settle
		// 5. verify last seen == active stake

		let nominator: T::AccountId = account("nominator", 0, SEED);

		//assert_ok!(StakeTracker::<T>::setup_unsettled_approvals(&nominator, n));
		assert!(LastSettledApprovals::<T>::get(&nominator).is_some());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), nominator.clone());

		assert!(LastSettledApprovals::<T>::get(&nominator).is_none());
	}

	impl_benchmark_test_suite!(
		StakeTracker,
		crate::mock::ExtBuilder::default(),
		crate::mock::Test,
		exec_name = build_and_execute
	);
}

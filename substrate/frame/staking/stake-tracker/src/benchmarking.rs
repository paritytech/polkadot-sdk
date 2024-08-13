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
use crate::{Config, LastSettledApprovals, Pallet as StakeTracker};

use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_std::vec::Vec;

const SEED: u32 = 0;
// sensible high and low nomination quota to extrapolate the costs of settling approvals for
// different `Staking::MaxNominations`.
const LOW_NOMINATIONS_QUOTA: u32 = 6;
const HIGH_NOMINATIONS_QUOTA: u32 = 16;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn settle_approvals(
		n: Linear<LOW_NOMINATIONS_QUOTA, HIGH_NOMINATIONS_QUOTA>,
	) -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();

		let nominations = utils::add_targets::<T>(n).map_err(|_| "error generating targets.")?;
		let nominator =
			utils::add_voter::<T>(nominations.clone()).map_err(|_| "error creating voter.")?;

		assert_ok!(StakeTracker::<T>::setup_unsettled_approvals(&nominator));
		assert!(LastSettledApprovals::<T>::get(&nominator).is_some());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), nominator.clone(), nominations.len() as u32);

		assert!(LastSettledApprovals::<T>::get(&nominator).is_none());

		Ok(())
	}

	impl_benchmark_test_suite!(
		StakeTracker,
		crate::mock::ExtBuilder::default(),
		crate::mock::Test,
		exec_name = build_and_execute
	);
}

mod utils {
	use super::*;
	use frame_election_provider_support::ElectionDataProvider;

	pub(crate) fn create_funded_staker<T: Config>(domain: &'static str, id: u32) -> T::AccountId {
		let account = frame_benchmarking::account::<T::AccountId>(domain, id, SEED);
		account
	}

	/// Adds new targets and returns a vec with their account IDs.
	pub(crate) fn add_targets<T: Config>(n: u32) -> Result<Vec<T::AccountId>, ()> {
		let mut targets = vec![];
		for a in 0..n {
			let target = create_funded_staker::<T>("target", a);
			<T::BenchmarkingElectionDataProvider as ElectionDataProvider>::add_target(
				target.clone(),
			);
			targets.push(target);
		}

		Ok(targets)
	}

	/// Adds new voter with nominations and returns its account ID.
	pub(crate) fn add_voter<T: Config>(nominations: Vec<T::AccountId>) -> Result<T::AccountId, ()> {
		let voter = create_funded_staker::<T>("voter", 1);
		<T::BenchmarkingElectionDataProvider as ElectionDataProvider>::add_voter(
			voter.clone(),
			10_000,
			nominations.try_into().map_err(|_| ())?,
		);

		Ok(voter)
	}
}

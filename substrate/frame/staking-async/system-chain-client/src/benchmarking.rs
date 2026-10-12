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

//! Benchmarking setup for pallet-staking-async-system-chain-client.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as StakingSystemChainClient;
use alloc::vec::Vec;
use codec::Decode;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::traits::{EnsureOrigin, Get};
use sp_runtime::traits::TrailingZeroInput;

fn any_source<T: Config>() -> T::Source {
	T::Source::decode(&mut TrailingZeroInput::zeroes())
		.expect("a source decodes from an infinite input of zeroes; qed")
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_points_per_block() -> Result<(), BenchmarkError> {
		let origin =
			T::PointsOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let source = any_source::<T>();
		let points = T::MaxPointsPerBlock::get();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, source, points);

		assert_eq!(PointsPerBlock::<T>::get(source), points);
		Ok(())
	}

	/// The part of `report_authored_blocks` this pallet does, measured with a report from a
	/// source without points so that [`Config::RewardPoints`] is not measured here.
	#[benchmark]
	fn report_authored_blocks() {
		let source = any_source::<T>();
		PointsPerBlock::<T>::remove(source);
		let blocks = (0..1_000)
			.map(|i| (account::<T::AccountId>("validator", i, 0), 1))
			.collect::<Vec<_>>();
		let entries = blocks.len() as u32;

		#[block]
		{
			Pallet::<T>::credit_authored_blocks(source, blocks);
		}

		frame_system::Pallet::<T>::assert_last_event(
			<T as frame_system::Config>::RuntimeEvent::from(Event::<T>::BlocksIgnored {
				source,
				entries,
			}),
		);
	}

	impl_benchmark_test_suite!(
		StakingSystemChainClient,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}

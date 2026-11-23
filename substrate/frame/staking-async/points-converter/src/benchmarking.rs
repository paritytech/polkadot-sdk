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

//! Benchmarking setup for pallet-staking-async-points-converter.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as StakingPointsConverter;
use alloc::vec::Vec;
use codec::Decode;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::traits::{EnsureOrigin, Get};
use sp_runtime::traits::TrailingZeroInput;
use sp_staking::{RewardPoints, ServiceKind};

fn any_source<T: Config>() -> T::Source {
	T::Source::decode(&mut TrailingZeroInput::zeroes())
		.expect("a source decodes from an infinite input of zeroes; qed")
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_points_per_service() -> Result<(), BenchmarkError> {
		let origin =
			T::PointsOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let source = any_source::<T>();
		let points = T::MaxPointsPerService::get();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, source, ServiceKind::Collation, points);

		assert_eq!(PointsPerService::<T>::get(source, ServiceKind::Collation), points);
		Ok(())
	}

	#[benchmark]
	fn set_units_per_era_limit() -> Result<(), BenchmarkError> {
		let origin =
			T::PointsOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let source = any_source::<T>();
		let limit = T::MaxUnitsPerEraLimit::get();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, source, ServiceKind::Collation, limit);

		assert_eq!(UnitsPerEraLimit::<T>::get(source, ServiceKind::Collation), limit);
		Ok(())
	}

	/// The converter's own cost of a report of `n` entries. The source earns points and an era is
	/// active, so the report reaches the limit check. The limit is zero, so every entry is cut and
	/// nothing reaches [`Config::RewardPoints`]. [`Pallet::report_authored_blocks_weight`] adds
	/// the `RewardPoints` weight of the credited entries, so measuring it here charges it twice.
	#[benchmark]
	fn report_authored_blocks(n: Linear<1, 1_000>) -> Result<(), BenchmarkError> {
		let origin =
			T::ReportOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let source = T::ReportOrigin::ensure_origin(origin.clone()).map_err(|_| {
			BenchmarkError::Stop("`ReportOrigin` rejects its own successful origin")
		})?;
		PointsPerService::<T>::insert(source, ServiceKind::Collation, 1);
		T::RewardPoints::ensure_active_era();
		let era = T::RewardPoints::active_era()
			.ok_or(BenchmarkError::Stop("no active era after `ensure_active_era`"))?;
		UnitsPerEraLimit::<T>::insert(source, ServiceKind::Collation, 0);
		let blocks = (0..n)
			.map(|i| (account::<T::AccountId>("validator", i, 0), 1))
			.collect::<Vec<_>>();
		let total_blocks = blocks.len() as u32;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, blocks);

		frame_system::Pallet::<T>::assert_last_event(
			<T as frame_system::Config>::RuntimeEvent::from(Event::<T>::BlocksOverCap {
				source,
				era,
				blocks: total_blocks,
			}),
		);
		assert_eq!(CreditedUnits::<T>::get(source, ServiceKind::Collation), (era, 0));
		Ok(())
	}

	impl_benchmark_test_suite!(
		StakingPointsConverter,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}

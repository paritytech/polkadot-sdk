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

//! Default weights for `pallet_staking_async_points_converter`, from the Asset Hub Westend
//! runtime benchmark.
//!
//! Runtimes should use their own benchmarked weights.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_staking_async_points_converter`.
pub trait WeightInfo {
	fn set_points_per_service() -> Weight;
	fn report_authored_blocks() -> Weight;
}

/// Default weights for `pallet_staking_async_points_converter`, see the module doc for their
/// source.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `StakingPointsConverter::PointsPerService` (r:0 w:1)
	/// Proof: `StakingPointsConverter::PointsPerService` (`max_values`: None, `max_size`: Some(25),
	/// added: 2500, mode: `MaxEncodedLen`)
	fn set_points_per_service() -> Weight {
		Weight::from_parts(3_948_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `StakingPointsConverter::PointsPerService` (r:1 w:0)
	/// Proof: `StakingPointsConverter::PointsPerService` (`max_values`: None, `max_size`: Some(25),
	/// added: 2500, mode: `MaxEncodedLen`)
	fn report_authored_blocks() -> Weight {
		Weight::from_parts(6_923_000, 3_490).saturating_add(T::DbWeight::get().reads(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `StakingPointsConverter::PointsPerService` (r:0 w:1)
	/// Proof: `StakingPointsConverter::PointsPerService` (`max_values`: None, `max_size`: Some(25),
	/// added: 2500, mode: `MaxEncodedLen`)
	fn set_points_per_service() -> Weight {
		Weight::from_parts(3_948_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `StakingPointsConverter::PointsPerService` (r:1 w:0)
	/// Proof: `StakingPointsConverter::PointsPerService` (`max_values`: None, `max_size`: Some(25),
	/// added: 2500, mode: `MaxEncodedLen`)
	fn report_authored_blocks() -> Weight {
		Weight::from_parts(6_923_000, 3_490).saturating_add(RocksDbWeight::get().reads(1_u64))
	}
}

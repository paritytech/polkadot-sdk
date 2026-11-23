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
	fn report_authored_blocks(n: u32) -> Weight;
	fn set_units_per_era_limit() -> Weight;
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
	/// Storage: `Staking::ActiveEra` (r:1 w:0)
	/// Proof: `Staking::ActiveEra` (`max_values`: Some(1), `max_size`: Some(13), added: 508, mode:
	/// `MaxEncodedLen`)
	/// Storage: `StakingPointsConverter::CreditedUnits` (r:1 w:1)
	/// Proof: `StakingPointsConverter::CreditedUnits` (`max_values`: None, `max_size`: Some(29),
	/// added: 2504, mode: `MaxEncodedLen`)
	/// Storage: `StakingPointsConverter::UnitsPerEraLimit` (r:1 w:0)
	/// Proof: `StakingPointsConverter::UnitsPerEraLimit` (`max_values`: None, `max_size`:
	/// Some(25), added: 2500, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn report_authored_blocks(n: u32) -> Weight {
		Weight::from_parts(15_000_000, 3_494)
			.saturating_add(Weight::from_parts(100_000, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `StakingPointsConverter::UnitsPerEraLimit` (r:0 w:1)
	/// Proof: `StakingPointsConverter::UnitsPerEraLimit` (`max_values`: None, `max_size`:
	/// Some(25), added: 2500, mode: `MaxEncodedLen`)
	fn set_units_per_era_limit() -> Weight {
		Weight::from_parts(3_948_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
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
	/// Storage: `Staking::ActiveEra` (r:1 w:0)
	/// Proof: `Staking::ActiveEra` (`max_values`: Some(1), `max_size`: Some(13), added: 508, mode:
	/// `MaxEncodedLen`)
	/// Storage: `StakingPointsConverter::CreditedUnits` (r:1 w:1)
	/// Proof: `StakingPointsConverter::CreditedUnits` (`max_values`: None, `max_size`: Some(29),
	/// added: 2504, mode: `MaxEncodedLen`)
	/// Storage: `StakingPointsConverter::UnitsPerEraLimit` (r:1 w:0)
	/// Proof: `StakingPointsConverter::UnitsPerEraLimit` (`max_values`: None, `max_size`:
	/// Some(25), added: 2500, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn report_authored_blocks(n: u32) -> Weight {
		Weight::from_parts(15_000_000, 3_494)
			.saturating_add(Weight::from_parts(100_000, 0).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `StakingPointsConverter::UnitsPerEraLimit` (r:0 w:1)
	/// Proof: `StakingPointsConverter::UnitsPerEraLimit` (`max_values`: None, `max_size`:
	/// Some(25), added: 2500, mode: `MaxEncodedLen`)
	fn set_units_per_era_limit() -> Weight {
		Weight::from_parts(3_948_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}

// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{constants::RocksDbWeight, Weight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_vaults`.
pub trait WeightInfo {
	fn create_vault() -> Weight;
	fn deposit_collateral() -> Weight;
	fn withdraw_collateral() -> Weight;
	fn mint() -> Weight;
	fn repay() -> Weight;
	fn liquidate_vault() -> Weight;
	fn close_vault() -> Weight;
	fn set_minimum_collateralization_ratio() -> Weight;
	fn set_initial_collateralization_ratio() -> Weight;
	fn set_stability_fee() -> Weight;
	fn set_liquidation_penalty() -> Weight;
	fn heal() -> Weight;
	fn set_max_liquidation_amount() -> Weight;
	fn poke() -> Weight;
	fn set_max_issuance() -> Weight;
	fn set_max_position_amount() -> Weight;
	fn on_idle_one_vault() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn create_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn deposit_collateral() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn withdraw_collateral() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn mint() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn repay() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn liquidate_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn close_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_minimum_collateralization_ratio() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_initial_collateralization_ratio() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_stability_fee() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_liquidation_penalty() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn heal() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_max_liquidation_amount() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn poke() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_max_issuance() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn set_max_position_amount() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn on_idle_one_vault() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(4_u64, 2_u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn create_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn deposit_collateral() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn withdraw_collateral() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn mint() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn repay() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn liquidate_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn close_vault() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_minimum_collateralization_ratio() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_initial_collateralization_ratio() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_stability_fee() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_liquidation_penalty() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn heal() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_max_liquidation_amount() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn poke() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(1_u64, 1_u64))
	}
	fn set_max_issuance() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn set_max_position_amount() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn on_idle_one_vault() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads_writes(4_u64, 2_u64))
	}
}

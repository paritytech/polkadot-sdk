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

//! Weights for `pallet_hrmp_para`.
//!
//! NOTE: not benchmarked yet. These are conservative estimates from the storage each function
//! touches, to be replaced by generated weights.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_hrmp_para`.
pub trait WeightInfo {
	/// `receive` of `SetDeposits` with both sides set, and its answer; also `force_set_deposit`.
	fn receive_set_deposits() -> Weight;
	/// `receive` of `Release` with both sides set.
	fn receive_release() -> Weight;
	/// `migrate_deposits` of `n` deposits.
	fn migrate_deposits(n: u32) -> Weight;
}

/// Weights for `pallet_hrmp_para`, using the runtime's database weights.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn receive_set_deposits() -> Weight {
		Weight::from_parts(80_000_000, 10_000)
			.saturating_add(T::DbWeight::get().reads(8))
			.saturating_add(T::DbWeight::get().writes(8))
	}
	fn receive_release() -> Weight {
		Weight::from_parts(60_000_000, 8_000)
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(6))
	}
	fn migrate_deposits(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(Weight::from_parts(70_000_000, 8_000).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4_u64.saturating_mul(n.into())))
			.saturating_add(T::DbWeight::get().writes(4_u64.saturating_mul(n.into())))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn receive_set_deposits() -> Weight {
		Weight::from_parts(80_000_000, 10_000)
			.saturating_add(RocksDbWeight::get().reads(8))
			.saturating_add(RocksDbWeight::get().writes(8))
	}
	fn receive_release() -> Weight {
		Weight::from_parts(60_000_000, 8_000)
			.saturating_add(RocksDbWeight::get().reads(6))
			.saturating_add(RocksDbWeight::get().writes(6))
	}
	fn migrate_deposits(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(Weight::from_parts(70_000_000, 8_000).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(4_u64.saturating_mul(n.into())))
			.saturating_add(RocksDbWeight::get().writes(4_u64.saturating_mul(n.into())))
	}
}

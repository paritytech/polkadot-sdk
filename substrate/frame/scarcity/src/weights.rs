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

//! Placeholder weights for `pallet_scarcity`.
//!
//! Replace these constants with runtime benchmark results before production use.

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_scarcity`.
pub trait WeightInfo {
	fn create_collection() -> Weight;
	fn define_item() -> Weight;
	fn mint() -> Weight;
	fn transfer() -> Weight;
}

/// Placeholder weights using the Substrate node and recommended hardware.
#[cfg_attr(
	not(feature = "std"),
	deprecated(
		note = "SubstrateWeight is a placeholder and must be replaced with runtime benchmarked weights."
	)
)]
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn create_collection() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	fn define_item() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	fn mint() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}

	fn transfer() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

impl WeightInfo for () {
	fn create_collection() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn define_item() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn mint() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(4_u64))
	}

	fn transfer() -> Weight {
		Weight::from_parts(10_000, 0)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
}

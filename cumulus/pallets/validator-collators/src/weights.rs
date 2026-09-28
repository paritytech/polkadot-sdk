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

//! Placeholder weights for `pallet_validator_collators` until benchmarked in CI.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_validator_collators`.
pub trait WeightInfo {
	fn set_validators(n: u32) -> Weight;
	fn set_max_collators() -> Weight;
	fn announce(n: u32) -> Weight;
	fn send_announcements(n: u32) -> Weight;
}

/// Weights for `pallet_validator_collators` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	fn set_max_collators() -> Weight {
		Weight::from_parts(5_000_000, 0).saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn announce(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}
	fn send_announcements(n: u32) -> Weight {
		Weight::from_parts(50_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn set_validators(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	fn set_max_collators() -> Weight {
		Weight::from_parts(5_000_000, 0).saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn announce(n: u32) -> Weight {
		Weight::from_parts(10_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(4_u64))
	}
	fn send_announcements(n: u32) -> Weight {
		Weight::from_parts(50_000_000, 1_500)
			.saturating_add(Weight::from_parts(50_000, 32).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}

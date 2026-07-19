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

//! Placeholder weights for `pallet_footprint`.
//!
//! These values are intentionally provisional. CI benchmarking will replace them with measured
//! weights for the target runtime and hardware.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]
#![allow(dead_code)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_footprint`.
pub trait WeightInfo {
	fn set_purchased() -> Weight;
	fn claim_base() -> Weight;
	fn revalidate_base() -> Weight;
}

/// Placeholder weights for `pallet_footprint` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Footprint::Allowances` (r:1 w:1)
	/// Storage: `Footprint::Usage` (r:1 w:0)
	/// Storage: currency account and holds (r:2 w:2)
	fn set_purchased() -> Weight {
		Weight::from_parts(25_000_000, 3_593)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	/// Storage: `Footprint::Allowances` (r:2 w:2)
	/// Storage: `Footprint::Claims` (r:1 w:1)
	/// Storage: `Footprint::Usage` (r:1 w:0)
	fn claim_base() -> Weight {
		Weight::from_parts(30_000_000, 3_593)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	/// Storage: `Footprint::Allowances` (r:1 w:1)
	/// Storage: `Footprint::Claims` (r:1 w:1)
	fn revalidate_base() -> Weight {
		Weight::from_parts(20_000_000, 3_593)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

impl WeightInfo for () {
	fn set_purchased() -> Weight {
		Weight::from_parts(25_000_000, 3_593)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn claim_base() -> Weight {
		Weight::from_parts(30_000_000, 3_593)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn revalidate_base() -> Weight {
		Weight::from_parts(20_000_000, 3_593)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
}

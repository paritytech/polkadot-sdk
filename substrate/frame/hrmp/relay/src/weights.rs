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

//! Weights for `pallet_hrmp_relay`.
//!
//! **Placeholders**, derived from storage access alone. Regenerate with the benchmark CLI.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_hrmp_relay`.
pub trait WeightInfo {
	fn receive() -> Weight;
	fn relay_request() -> Weight;
	fn flush_releases(n: u32) -> Weight;
}

/// Weights for `pallet_hrmp_relay` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn receive() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(8))
			.saturating_add(T::DbWeight::get().writes(8))
	}
	fn relay_request() -> Weight {
		Weight::from_parts(40_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(12))
			.saturating_add(T::DbWeight::get().writes(12))
	}
	fn flush_releases(n: u32) -> Weight {
		Weight::from_parts(5_000_000, 0)
			.saturating_add(Weight::from_parts(10_000_000, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
			.saturating_add(T::DbWeight::get().writes(2).saturating_mul(n.into()))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn receive() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(8))
			.saturating_add(RocksDbWeight::get().writes(8))
	}
	fn relay_request() -> Weight {
		Weight::from_parts(40_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(12))
			.saturating_add(RocksDbWeight::get().writes(12))
	}
	fn flush_releases(n: u32) -> Weight {
		Weight::from_parts(5_000_000, 0)
			.saturating_add(Weight::from_parts(10_000_000, 0).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
			.saturating_add(RocksDbWeight::get().writes(2).saturating_mul(n.into()))
	}
}

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

//! Weights for `pallet_bootnodes`.
//!
//! These are hand-written placeholders. Regenerate them with the benchmark CLI
//! (`/cmd bench --pallet pallet_bootnodes`) before using the pallet in production.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_bootnodes`.
pub trait WeightInfo {
	fn add_bootnode() -> Weight;
	fn remove_bootnode() -> Weight;
}

/// Weights for `pallet_bootnodes` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Bootnodes::LastSeq` (r:1 w:1)
	/// Proof: `Bootnodes::LastSeq` (`max_values`: None, `max_size`: Some(72), added: 2547, mode: `MaxEncodedLen`)
	/// Storage: `Bootnodes::Bootnodes` (r:1 w:1)
	/// Proof: `Bootnodes::Bootnodes` (`max_values`: Some(1), `max_size`: Some(945), added: 1440, mode: `MaxEncodedLen`)
	fn add_bootnode() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `3537`
		// Minimum execution time: 70_000_000 picoseconds.
		Weight::from_parts(70_000_000, 3537)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: `Bootnodes::Bootnodes` (r:1 w:1)
	/// Proof: `Bootnodes::Bootnodes` (`max_values`: Some(1), `max_size`: Some(945), added: 1440, mode: `MaxEncodedLen`)
	fn remove_bootnode() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `1440`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(10_000_000, 1440)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn add_bootnode() -> Weight {
		Weight::from_parts(70_000_000, 3537)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	fn remove_bootnode() -> Weight {
		Weight::from_parts(10_000_000, 1440)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}

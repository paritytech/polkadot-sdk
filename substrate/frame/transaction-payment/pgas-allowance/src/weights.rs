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

//! Weights for `pallet_pgas_allowance`.
//! see /cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights/pallet_pgas_allowance.rs

#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_pgas_allowance`.
pub trait WeightInfo {
	/// Full PGAS path: validate, withdraw into a credit and resolve refund.
	fn charge_pgas() -> Weight;
	/// PGAS path is skipped (unsigned, filter miss, or insufficient PGAS balance).
	fn charge_pgas_skip() -> Weight;
}

/// Weights for `pallet_pgas_allowance` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Assets::Asset` (r:1 w:1)
	/// Proof: `Assets::Asset` (`max_values`: None, `max_size`: Some(210), added: 2685, mode:
	/// `MaxEncodedLen`) Storage: `Assets::Account` (r:1 w:1)
	/// Proof: `Assets::Account` (`max_values`: None, `max_size`: Some(134), added: 2609, mode:
	/// `MaxEncodedLen`) Storage: `AssetsFreezer::FrozenBalances` (r:1 w:0)
	/// Proof: `AssetsFreezer::FrozenBalances` (`max_values`: None, `max_size`: Some(84), added:
	/// 2559, mode: `MaxEncodedLen`)
	fn charge_pgas() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `459`
		//  Estimated: `3675`
		// Minimum execution time: 58_073_000 picoseconds.
		Weight::from_parts(61_039_000, 0)
			.saturating_add(Weight::from_parts(0, 3675))
			.saturating_add(T::DbWeight::get().reads(3))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	/// Storage: `Assets::Asset` (r:1 w:0)
	/// Proof: `Assets::Asset` (`max_values`: None, `max_size`: Some(210), added: 2685, mode:
	/// `MaxEncodedLen`) Storage: `Assets::Account` (r:1 w:0)
	/// Proof: `Assets::Account` (`max_values`: None, `max_size`: Some(134), added: 2609, mode:
	/// `MaxEncodedLen`)
	fn charge_pgas_skip() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `347`
		//  Estimated: `3675`
		// Minimum execution time: 10_930_000 picoseconds.
		Weight::from_parts(11_844_000, 0)
			.saturating_add(Weight::from_parts(0, 3675))
			.saturating_add(T::DbWeight::get().reads(2))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `Assets::Asset` (r:1 w:1)
	/// Proof: `Assets::Asset` (`max_values`: None, `max_size`: Some(210), added: 2685, mode:
	/// `MaxEncodedLen`) Storage: `Assets::Account` (r:1 w:1)
	/// Proof: `Assets::Account` (`max_values`: None, `max_size`: Some(134), added: 2609, mode:
	/// `MaxEncodedLen`) Storage: `AssetsFreezer::FrozenBalances` (r:1 w:0)
	/// Proof: `AssetsFreezer::FrozenBalances` (`max_values`: None, `max_size`: Some(84), added:
	/// 2559, mode: `MaxEncodedLen`)
	fn charge_pgas() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `459`
		//  Estimated: `3675`
		// Minimum execution time: 58_073_000 picoseconds.
		Weight::from_parts(61_039_000, 0)
			.saturating_add(Weight::from_parts(0, 3675))
			.saturating_add(T::DbWeight::get().reads(3))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	/// Storage: `Assets::Asset` (r:1 w:0)
	/// Proof: `Assets::Asset` (`max_values`: None, `max_size`: Some(210), added: 2685, mode:
	/// `MaxEncodedLen`) Storage: `Assets::Account` (r:1 w:0)
	/// Proof: `Assets::Account` (`max_values`: None, `max_size`: Some(134), added: 2609, mode:
	/// `MaxEncodedLen`)
	fn charge_pgas_skip() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `347`
		//  Estimated: `3675`
		// Minimum execution time: 10_930_000 picoseconds.
		Weight::from_parts(11_844_000, 0)
			.saturating_add(Weight::from_parts(0, 3675))
			.saturating_add(T::DbWeight::get().reads(2))
	}
}

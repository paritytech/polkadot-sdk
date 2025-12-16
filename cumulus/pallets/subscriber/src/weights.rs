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

//! Generated weights for `cumulus_pallet_subscriber`
//!
//! THESE WEIGHTS WERE GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2025-12-16, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `asset-hub-westend`, CPU: `<UNKNOWN>`
//! WASM-EXECUTION: `Compiled`, CHAIN: `asset-hub-westend-dev`, DB CACHE: `1024`

// Executed Command:
// ./target/release/polkadot-parachain
// benchmark
// pallet
// --pallet
// cumulus-pallet-subscriber
// --chain
// asset-hub-westend-dev
// --output
// cumulus/pallets/subscriber/src/weights.rs
// --template
// substrate/.maintain/frame-weight-template.hbs
// --extrinsic
//

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]
#![allow(dead_code)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `cumulus_pallet_subscriber`.
pub trait WeightInfo {
	fn collect_publisher_roots(n: u32) -> Weight;
	fn process_published_data(n: u32, k: u32, s: u32) -> Weight;
	fn clear_stored_roots() -> Weight;

	/// Weight for processing relay proof excluding handler execution.
	/// Benchmarked with no-op handler. Handler weights are added at runtime.
	///
	/// Parameters:
	/// - `num_publishers`: Number of publishers being processed
	/// - `num_keys`: Total number of keys across all publishers
	/// - `total_bytes`: Total bytes of data being decoded
	fn process_proof_excluding_handler(num_publishers: u32, num_keys: u32, total_bytes: u32) -> Weight {
		Self::collect_publisher_roots(num_publishers)
			.saturating_add(Self::process_published_data(num_publishers, num_keys, total_bytes))
	}
}

/// Weights for `cumulus_pallet_subscriber` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// The range of component `n` is `[1, 100]`.
	fn collect_publisher_roots(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 1_000_000 picoseconds.
		Weight::from_parts(1_000_000, 0)
			.saturating_add(Weight::from_parts(0, 0))
			// Standard Error: 2_289
			.saturating_add(Weight::from_parts(1_853_718, 0).saturating_mul(n.into()))
	}
	/// Storage: `Subscriber::PreviousPublishedDataRoots` (r:1 w:1)
	/// Proof: `Subscriber::PreviousPublishedDataRoots` (`max_values`: Some(1), `max_size`: Some(3702), added: 4197, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 100]`.
	/// The range of component `k` is `[1, 10]`.
	/// The range of component `s` is `[1, 2048]`.
	fn process_published_data(n: u32, k: u32, _s: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `142`
		//  Estimated: `5187`
		// Minimum execution time: 51_000_000 picoseconds.
		Weight::from_parts(51_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5187))
			// Standard Error: 448_042
			.saturating_add(Weight::from_parts(33_087_314, 0).saturating_mul(n.into()))
			// Standard Error: 4_535_424
			.saturating_add(Weight::from_parts(311_706_924, 0).saturating_mul(k.into()))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: `Subscriber::PreviousPublishedDataRoots` (r:1 w:1)
	/// Proof: `Subscriber::PreviousPublishedDataRoots` (`max_values`: Some(1), `max_size`: Some(3702), added: 4197, mode: `MaxEncodedLen`)
	fn clear_stored_roots() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `206`
		//  Estimated: `5187`
		// Minimum execution time: 8_000_000 picoseconds.
		Weight::from_parts(9_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5187))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn collect_publisher_roots(n: u32) -> Weight {
		Weight::from_parts(1_000_000, 0)
			.saturating_add(Weight::from_parts(0, 0))
			.saturating_add(Weight::from_parts(1_853_718, 0).saturating_mul(n.into()))
	}

	fn process_published_data(n: u32, k: u32, _s: u32) -> Weight {
		Weight::from_parts(51_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5187))
			.saturating_add(Weight::from_parts(33_087_314, 0).saturating_mul(n.into()))
			.saturating_add(Weight::from_parts(311_706_924, 0).saturating_mul(k.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn clear_stored_roots() -> Weight {
		Weight::from_parts(9_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5187))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}

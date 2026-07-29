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

//! Conservative placeholder weights for `pallet_scarcity`.
//!
//! These formulas use the storage and proof accounting observed in the benchmark-enabled dev
//! runtime, with deliberately padded local execution times. The bench-bot must replace them with
//! reference-hardware values through `/cmd bench --runtime dev --pallet pallet_scarcity`.

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

/// Weight functions needed for `pallet_scarcity`.
pub trait WeightInfo {
	fn create_collection() -> Weight;
	fn define_item(metadata_entries: u32) -> Weight;
	fn mint() -> Weight;
	fn transfer() -> Weight;
	fn burn() -> Weight;
	fn set_collection_metadata() -> Weight;
	fn set_item_metadata() -> Weight;
	fn as_scarcity_pipeline() -> Weight;
}

/// Conservative placeholder weights for `pallet_scarcity`.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Scarcity::NextCollectionId` (r:1 w:1)
	/// Storage: `Parameters::Parameters` (r:2 w:0)
	/// Storage: `Balances::Holds` (r:1 w:1)
	/// Storage: `Scarcity::Collections` (r:0 w:1)
	fn create_collection() -> Weight {
		Weight::from_parts(110_000_000, 28_584)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:1)
	/// Storage: `Parameters::Parameters` (r:2 w:0)
	/// Storage: `Balances::Holds` (r:1 w:1)
	/// Storage: `Scarcity::ItemDefs` (r:0 w:1)
	/// Storage per metadata entry: `ItemMetadata` (r:1 w:1).
	/// Every entry is benchmarked at the configured maximum key and value lengths.
	fn define_item(metadata_entries: u32) -> Weight {
		Weight::from_parts(120_000_000, 28_584)
			.saturating_add(
				Weight::from_parts(100_000_000, 2_822)
					.saturating_mul(metadata_entries.into()),
			)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(
				T::DbWeight::get().reads(metadata_entries.into()),
			)
			.saturating_add(T::DbWeight::get().writes(3_u64))
			.saturating_add(
				T::DbWeight::get().writes(metadata_entries.into()),
			)
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::ItemDefs` (r:1 w:1)
	/// Storage: `Scarcity::NftsByOwner` (r:1 w:1)
	/// Storage: `Scarcity::NextInstanceId` (r:1 w:1)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Storage: `Parameters::Parameters` (r:2 w:0)
	/// Storage: `Balances::Holds` (r:1 w:1)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	/// Storage: `Scarcity::InstanceDeposits` (r:0 w:1)
	fn mint() -> Weight {
		Weight::from_parts(160_000_000, 28_584)
			.saturating_add(T::DbWeight::get().reads(8_u64))
			.saturating_add(T::DbWeight::get().writes(6_u64))
	}

	/// Storage: `Scarcity::NftsByOwner` (r:1 w:1)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	fn transfer() -> Weight {
		Weight::from_parts(40_000_000, 3_553)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	/// Storage: `Scarcity::InstanceDeposits` (r:1 w:1)
	/// Storage: `Balances::Holds` (r:1 w:1)
	fn burn() -> Weight {
		Weight::from_parts(100_000_000, 4_018)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::CollectionMetadata` (r:1 w:1)
	/// Storage: `Parameters::Parameters` (r:2 w:0)
	/// Storage: `Balances::Holds` (r:1 w:1)
	/// Benchmarked at the maximum key/value lengths while creating a ticket.
	fn set_collection_metadata() -> Weight {
		Weight::from_parts(120_000_000, 28_584)
			.saturating_add(T::DbWeight::get().reads(5_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::ItemDefs` (r:1 w:0)
	/// Storage: `Scarcity::ItemMetadata` (r:1 w:1)
	/// Storage: `Parameters::Parameters` (r:2 w:0)
	/// Storage: `Balances::Holds` (r:1 w:1)
	/// Benchmarked at the maximum key/value lengths while creating a ticket.
	fn set_item_metadata() -> Weight {
		Weight::from_parts(130_000_000, 28_584)
			.saturating_add(T::DbWeight::get().reads(6_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Storage: `Scarcity::Locked` (r:1 w:1)
	/// Storage: `Scarcity::NftsByOwner` (r:2 w:1)
	fn as_scarcity_pipeline() -> Weight {
		Weight::from_parts(60_000_000, 6_116)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

impl WeightInfo for () {
	fn create_collection() -> Weight {
		Weight::from_parts(110_000_000, 28_584)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn define_item(metadata_entries: u32) -> Weight {
		Weight::from_parts(120_000_000, 28_584)
			.saturating_add(
				Weight::from_parts(100_000_000, 2_822)
					.saturating_mul(metadata_entries.into()),
			)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(
				RocksDbWeight::get()
					.reads(metadata_entries.into()),
			)
			.saturating_add(RocksDbWeight::get().writes(3_u64))
			.saturating_add(
				RocksDbWeight::get()
					.writes(metadata_entries.into()),
			)
	}

	fn mint() -> Weight {
		Weight::from_parts(160_000_000, 28_584)
			.saturating_add(RocksDbWeight::get().reads(8_u64))
			.saturating_add(RocksDbWeight::get().writes(6_u64))
	}

	fn transfer() -> Weight {
		Weight::from_parts(40_000_000, 3_553)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn burn() -> Weight {
		Weight::from_parts(100_000_000, 4_018)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn set_collection_metadata() -> Weight {
		Weight::from_parts(120_000_000, 28_584)
			.saturating_add(RocksDbWeight::get().reads(5_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn set_item_metadata() -> Weight {
		Weight::from_parts(130_000_000, 28_584)
			.saturating_add(RocksDbWeight::get().reads(6_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn as_scarcity_pipeline() -> Weight {
		Weight::from_parts(60_000_000, 6_116)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
}

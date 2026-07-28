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

//! Estimated weights for `pallet_scarcity`.
//!
//! WARNING: These values are honest engineering estimates for review integration only. The
//! bench-bot must replace them with measured target-runtime values after `/cmd bench`.

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
	fn define_item(metadata_entries: u32, value_len: u32) -> Weight;
	fn mint() -> Weight;
	fn transfer() -> Weight;
	fn burn() -> Weight;
	fn set_collection_metadata() -> Weight;
	fn set_item_metadata() -> Weight;
	fn as_scarcity_pipeline() -> Weight;
}

/// Estimated weights for `pallet_scarcity` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Scarcity::NextCollectionId` (r:1 w:1)
	/// Storage: `Scarcity::Collections` (r:0 w:1)
	fn create_collection() -> Weight {
		Weight::from_parts(12_000_000, 3_593)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:1)
	/// Storage: `Scarcity::ItemDefs` (r:1 w:1)
	/// Storage per metadata entry: `Collections` (r:1), `ItemDefs` (r:1),
	/// `ItemMetadata` (r:1 w:1), `ItemMetadataCount` (r:1 w:1).
	/// Components: metadata entry count and maximum value length.
	fn define_item(metadata_entries: u32, value_len: u32) -> Weight {
		Weight::from_parts(18_000_000, 4_096)
			.saturating_add(
				Weight::from_parts(120_000, 0).saturating_mul(metadata_entries.into()),
			)
			.saturating_add(Weight::from_parts(2_000, 0).saturating_mul(value_len.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(
				T::DbWeight::get().reads(4_u64.saturating_mul(metadata_entries.into())),
			)
			.saturating_add(T::DbWeight::get().writes(2_u64))
			.saturating_add(
				T::DbWeight::get().writes(2_u64.saturating_mul(metadata_entries.into())),
			)
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::ItemDefs` (r:1 w:1)
	/// Storage: `Scarcity::NftsByOwner` (r:1 w:1)
	/// Storage: `Scarcity::NextInstanceId` (r:1 w:1)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	/// Storage: `Scarcity::InstanceDeposits` (r:0 w:1)
	fn mint() -> Weight {
		Weight::from_parts(38_000_000, 6_148)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(5_u64))
	}

	/// Storage: `Scarcity::NftsByOwner` (r:1 w:1)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	fn transfer() -> Weight {
		Weight::from_parts(18_000_000, 3_593)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::Instances` (r:0 w:1)
	/// Storage: `Scarcity::InstanceDeposits` (r:1 w:1)
	/// Includes a conservative read/write allowance for dropping the consideration ticket.
	fn burn() -> Weight {
		Weight::from_parts(20_000_000, 6_148)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::CollectionMetadata` (r:1 w:1)
	/// Benchmarked at the maximum key/value lengths while updating an existing ticket.
	fn set_collection_metadata() -> Weight {
		Weight::from_parts(20_000_000, 6_148)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	/// Storage: `Scarcity::Collections` (r:1 w:0)
	/// Storage: `Scarcity::ItemDefs` (r:1 w:0)
	/// Storage: `Scarcity::ItemMetadata` (r:1 w:1)
	/// Benchmarked at the maximum key/value lengths while updating an existing ticket.
	fn set_item_metadata() -> Weight {
		Weight::from_parts(24_000_000, 8_192)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	/// Storage: `Scarcity::Locked` (r:1 w:0)
	/// Storage: `Scarcity::NftsByOwner` (r:3 w:1)
	fn as_scarcity_pipeline() -> Weight {
		Weight::from_parts(32_000_000, 8_192)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

impl WeightInfo for () {
	fn create_collection() -> Weight {
		Weight::from_parts(12_000_000, 3_593)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn define_item(metadata_entries: u32, value_len: u32) -> Weight {
		Weight::from_parts(18_000_000, 4_096)
			.saturating_add(
				Weight::from_parts(120_000, 0).saturating_mul(metadata_entries.into()),
			)
			.saturating_add(Weight::from_parts(2_000, 0).saturating_mul(value_len.into()))
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(
				RocksDbWeight::get()
					.reads(4_u64.saturating_mul(metadata_entries.into())),
			)
			.saturating_add(RocksDbWeight::get().writes(2_u64))
			.saturating_add(
				RocksDbWeight::get()
					.writes(2_u64.saturating_mul(metadata_entries.into())),
			)
	}

	fn mint() -> Weight {
		Weight::from_parts(38_000_000, 6_148)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(5_u64))
	}

	fn transfer() -> Weight {
		Weight::from_parts(18_000_000, 3_593)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn burn() -> Weight {
		Weight::from_parts(20_000_000, 6_148)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn set_collection_metadata() -> Weight {
		Weight::from_parts(20_000_000, 6_148)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn set_item_metadata() -> Weight {
		Weight::from_parts(24_000_000, 8_192)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn as_scarcity_pipeline() -> Weight {
		Weight::from_parts(32_000_000, 8_192)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}

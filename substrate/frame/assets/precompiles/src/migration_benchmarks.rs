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

//! Benchmarks for the foreign asset precompile migration.
//!
//! These benchmarks measure the weight of migration operations:
//! - Migrating a new asset (inserting a new mapping)
//! - Skipping an already-mapped asset
//! - Finishing iteration (no more assets)

#![cfg(feature = "runtime-benchmarks")]

use crate::foreign_assets::pallet::{Config, Pallet};
use frame_benchmarking::v2::*;

#[benchmarks(
	where
		T::ForeignAssetId: From<u32>,
)]
mod benchmarks {
	use super::*;

	/// Benchmark inserting a new asset mapping.
	///
	/// This measures the weight of:
	/// - 1 read: ForeignAssetIdToAssetIndex (check if already mapped)
	/// - 1 read: NextAssetIndex
	/// - 1 write: NextAssetIndex
	/// - 1 write: AssetIndexToForeignAssetId
	/// - 1 write: ForeignAssetIdToAssetIndex
	#[benchmark]
	fn migrate_asset_step_migrate() {
		let asset_id: T::ForeignAssetId = 1u32.into();

		// Ensure no mapping exists
		assert!(Pallet::<T>::asset_index_of(&asset_id).is_none());

		#[block]
		{
			let _ = Pallet::<T>::insert_asset_mapping(&asset_id);
		}

		// Verify the mapping was created
		assert!(Pallet::<T>::asset_index_of(&asset_id).is_some());
	}

	/// Benchmark checking an already-mapped asset (skip case).
	///
	/// This measures the weight of:
	/// - 1 read: ForeignAssetIdToAssetIndex
	#[benchmark]
	fn migrate_asset_step_skip() {
		let asset_id: T::ForeignAssetId = 2u32.into();

		// Pre-create the mapping
		let _ = Pallet::<T>::insert_asset_mapping(&asset_id);
		assert!(Pallet::<T>::asset_index_of(&asset_id).is_some());

		#[block]
		{
			// Simulate the check that happens when an asset is already mapped
			let _ = Pallet::<T>::asset_index_of(&asset_id);
		}
	}

	/// Benchmark the case when there are no more assets to iterate.
	///
	/// This measures the weight of a simple storage read returning None.
	#[benchmark]
	fn migrate_asset_step_finished() {
		let nonexistent_id: T::ForeignAssetId = 999u32.into();

		#[block]
		{
			// Simulate checking for an asset that doesn't exist
			let _ = Pallet::<T>::asset_index_of(&nonexistent_id);
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}

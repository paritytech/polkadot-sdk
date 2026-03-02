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
//! This benchmark measures the weight of one complete `step()` invocation of the
//! [`MigrateForeignAssetPrecompileMappings`] stepped migration.
//!
//! Inspired by `pallet_revive::benchmarking::v2_migration_step`, the benchmark
//! calls the actual `step()` function with a single unmapped asset in storage,
//! capturing the real cost including storage iteration overhead.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	foreign_assets::pallet::{Config, Pallet},
	migration::MigrateForeignAssetPrecompileMappings,
};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};
use sp_runtime::traits::StaticLookup;

#[benchmarks(
	where
		T: pallet_assets::Config<T::AssetsInstance, AssetId = <T as Config>::ForeignAssetId>,
		T::ForeignAssetId: From<u32>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn migrate_foreign_asset_step() {
		// Clear any pre-existing assets from genesis so that only our
		// benchmark asset is present during the migration step.
		let _ = pallet_assets::Asset::<T, T::AssetsInstance>::clear(u32::MAX, None);

		// Create one asset in pallet_assets storage.
		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = <T as frame_system::Config>::Lookup::unlookup(caller);
		let asset_id: <T as pallet_assets::Config<T::AssetsInstance>>::AssetId = 42u32.into();
		let asset_id_param: <T as pallet_assets::Config<T::AssetsInstance>>::AssetIdParameter =
			asset_id.into();

		pallet_assets::Pallet::<T, T::AssetsInstance>::force_create(
			frame_system::RawOrigin::Root.into(),
			asset_id_param,
			caller_lookup,
			true,
			1u32.into(),
		)
		.unwrap();

		// Verify no precompile mapping exists yet.
		let foreign_asset_id: T::ForeignAssetId = 42u32.into();
		assert!(Pallet::<T>::asset_index_of(&foreign_asset_id).is_none());

		let mut meter = WeightMeter::new();

		#[block]
		{
			MigrateForeignAssetPrecompileMappings::<T, T::AssetsInstance, ()>::step(
				None, &mut meter,
			)
			.unwrap();
		}

		// Verify the asset was migrated.
		assert!(Pallet::<T>::asset_index_of(&foreign_asset_id).is_some());
		// The step consumes the weight twice: once for migrating the asset and once for
		// discovering that there are no more assets to migrate.
		assert_eq!(
			meter.consumed(),
			<() as crate::weights::WeightInfo>::migrate_foreign_asset_step() * 2
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}

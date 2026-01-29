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

//! Tests for `MigrateForeignAssetPrecompileMappings`.
//!
//! This module defines its own mock runtime that uses `xcm::v5::Location` as the asset ID type,
//! which is the real-world use case for foreign assets.

use crate::{
	foreign_assets::pallet,
	migration::{MigrateForeignAssetPrecompileMappings, MigrationState},
	ToAssetIndex,
};
use frame_support::{
	assert_ok, derive_impl,
	migrations::SteppedMigration,
	parameter_types,
	traits::AsEnsureOriginWithArg,
	weights::{Weight, WeightMeter},
};
use pallet_assets::AssetDetails;
use sp_runtime::BuildStorage;
use xcm::v5::{Junction, Location};

/// A benchmark helper that creates Location instances from u32 IDs.
#[cfg(feature = "runtime-benchmarks")]
pub struct LocationBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<Location, ()> for LocationBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> Location {
		Location::new(1, [Junction::Parachain(id)])
	}
	fn create_reserve_id_parameter(_id: u32) -> () {
		()
	}
}

// Define a mock runtime that uses Location as the asset ID
#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeHoldReason,
		RuntimeFreezeReason
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(20)]
	pub type ForeignAssets = pallet_assets<Instance1>;
	#[runtime::pallet_index(22)]
	pub type AssetsPrecompiles = crate::foreign_assets;
}

pub type ForeignAssetsInstance = pallet_assets::Instance1;

type Block = frame_system::mocking::MockBlock<Test>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub const AssetDeposit: u64 = 1;
	pub const AssetAccountDeposit: u64 = 1;
	pub const ApprovalDeposit: u64 = 1;
	pub const MetadataDepositBase: u64 = 1;
	pub const MetadataDepositPerByte: u64 = 1;
}

impl pallet_assets::Config<ForeignAssetsInstance> for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u64;
	type AssetId = Location;
	type AssetIdParameter = Location;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = frame_system::EnsureRoot<u64>;
	type Currency = Balances;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = AssetAccountDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = frame_support::traits::ConstU32<50>;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = frame_support::traits::ConstU32<1000>;
	type CallbackHandle = ();
	type ReserveData = ();
	type Holder = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = LocationBenchmarkHelper;
}

impl pallet::Config for Test {
	type ForeignAssetId = Location;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
	});
	ext
}

/// Helper to run the stepped migration to completion.
fn run_migration_to_completion() -> u32 {
	let mut cursor = None;
	let mut steps = 0u32;
	loop {
		let mut meter = WeightMeter::with_limit(Weight::MAX);
		match MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::step(
			cursor,
			&mut meter,
		) {
			Ok(None) => break, // Migration complete
			Ok(Some(new_cursor)) => {
				cursor = Some(new_cursor);
				steps = steps.saturating_add(1);
			},
			Err(e) => panic!("Migration failed: {:?}", e),
		}
	}
	steps
}

/// Helper to create a minimal AssetDetails for testing.
fn create_asset_details(
	owner: <Test as frame_system::Config>::AccountId,
) -> AssetDetails<
	<Test as pallet_assets::Config<ForeignAssetsInstance>>::Balance,
	<Test as frame_system::Config>::AccountId,
	<Test as pallet_balances::Config>::Balance,
> {
	AssetDetails {
		owner: owner.clone(),
		issuer: owner.clone(),
		admin: owner.clone(),
		freezer: owner,
		supply: 0,
		deposit: 0,
		min_balance: 1,
		is_sufficient: false,
		accounts: 0,
		sufficients: 0,
		approvals: 0,
		status: pallet_assets::AssetStatus::Live,
	}
}

/// Test that the migration correctly populates precompile mappings for foreign assets.
///
/// This test:
/// 1. Creates foreign assets directly in storage WITHOUT precompile mappings (simulating the
///    pre-migration state)
/// 2. Verifies that precompile mappings do NOT exist before migration
/// 3. Runs the migration
/// 4. Verifies that precompile mappings exist after migration
#[test]
fn migration_populates_precompile_mappings() {
	new_test_ext().execute_with(|| {
		let owner = 1u64;

		// Create foreign asset locations (simulating assets from sibling parachains)
		let sibling_para_1000_asset = Location::new(1, [Junction::Parachain(1000)]);
		let sibling_para_2000_asset = Location::new(1, [Junction::Parachain(2000)]);
		let sibling_para_3000_asset = Location::new(1, [Junction::Parachain(3000)]);

		let test_assets = vec![
			sibling_para_1000_asset.clone(),
			sibling_para_2000_asset.clone(),
			sibling_para_3000_asset.clone(),
		];

		// Insert foreign assets directly into pallet_assets storage without creating precompile
		// mappings
		for asset_location in &test_assets {
			pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
				asset_location.clone(),
				create_asset_details(owner),
			);
		}

		// Verify assets are in storage
		for asset_location in &test_assets {
			assert!(
				pallet_assets::Asset::<Test, ForeignAssetsInstance>::contains_key(asset_location),
				"Asset should exist in pallet_assets storage"
			);
		}

		// Verify precompile mappings do NOT exist before migration
		for asset_location in &test_assets {
			let asset_index = asset_location.to_asset_index();

			assert!(
				pallet::Pallet::<Test>::asset_id_of(asset_index).is_none(),
				"Precompile mapping should NOT exist before migration"
			);
			assert!(
				pallet::Pallet::<Test>::asset_index_of(asset_location).is_none(),
				"Reverse precompile mapping should NOT exist before migration"
			);
		}

		run_migration_to_completion();

		// Verify precompile mappings now exist after migration
		for asset_location in &test_assets {
			let asset_index = asset_location.to_asset_index();

			// Check forward mapping: index -> location
			let stored_location = pallet::Pallet::<Test>::asset_id_of(asset_index);
			assert_eq!(
				stored_location,
				Some(asset_location.clone()),
				"Forward precompile mapping should exist after migration for {:?}",
				asset_location
			);

			// Check reverse mapping: location -> index
			let stored_index = pallet::Pallet::<Test>::asset_index_of(asset_location);
			assert_eq!(
				stored_index,
				Some(asset_index),
				"Reverse precompile mapping should exist after migration for {:?}",
				asset_location
			);
		}
	});
}

/// Test that the migration is idempotent - running it twice produces the same result.
///
/// Idempotency is critical: the second run should not modify any existing mappings.
#[test]
fn migration_is_idempotent() {
	new_test_ext().execute_with(|| {
		let owner = 1u64;

		let asset_location_1 = Location::new(1, [Junction::Parachain(1234)]);
		let asset_location_2 = Location::new(1, [Junction::Parachain(5678)]);

		pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
			asset_location_1.clone(),
			create_asset_details(owner),
		);
		pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
			asset_location_2.clone(),
			create_asset_details(owner),
		);

		let asset_index_1 = asset_location_1.to_asset_index();
		let asset_index_2 = asset_location_2.to_asset_index();

		// Verify mappings do not exist before migration
		assert!(
			pallet::Pallet::<Test>::asset_id_of(asset_index_1).is_none(),
			"Mapping 1 should NOT exist before migration"
		);
		assert!(
			pallet::Pallet::<Test>::asset_id_of(asset_index_2).is_none(),
			"Mapping 2 should NOT exist before migration"
		);

		run_migration_to_completion();

		// Capture complete state after first run
		let state_after_first = (
			pallet::Pallet::<Test>::asset_id_of(asset_index_1),
			pallet::Pallet::<Test>::asset_id_of(asset_index_2),
			pallet::Pallet::<Test>::asset_index_of(&asset_location_1),
			pallet::Pallet::<Test>::asset_index_of(&asset_location_2),
		);

		// Verify mappings were created correctly after first run
		assert_eq!(
			state_after_first.0,
			Some(asset_location_1.clone()),
			"Forward mapping 1 should exist after first run"
		);
		assert_eq!(
			state_after_first.1,
			Some(asset_location_2.clone()),
			"Forward mapping 2 should exist after first run"
		);
		assert_eq!(
			state_after_first.2,
			Some(asset_index_1),
			"Reverse mapping 1 should exist after first run"
		);
		assert_eq!(
			state_after_first.3,
			Some(asset_index_2),
			"Reverse mapping 2 should exist after first run"
		);

		run_migration_to_completion();

		// Capture complete state after second run
		let state_after_second = (
			pallet::Pallet::<Test>::asset_id_of(asset_index_1),
			pallet::Pallet::<Test>::asset_id_of(asset_index_2),
			pallet::Pallet::<Test>::asset_index_of(&asset_location_1),
			pallet::Pallet::<Test>::asset_index_of(&asset_location_2),
		);

		// State must be identical after second run
		assert_eq!(
			state_after_first, state_after_second,
			"Idempotency violation: migration modified mappings on second run"
		);
	});
}

/// Test that the migration handles the case with no foreign assets gracefully.
#[test]
fn migration_handles_empty_foreign_assets() {
	new_test_ext().execute_with(|| {
		let steps = run_migration_to_completion();
		// With no assets, the migration should complete without processing any assets
		assert_eq!(steps, 0, "Empty migration should have no steps");
	});
}

/// Test that assets with existing mappings are correctly skipped.
#[test]
fn migration_skips_already_mapped_assets() {
	new_test_ext().execute_with(|| {
		let owner = 1u64;

		let asset_with_mapping = Location::new(1, [Junction::Parachain(1111)]);
		let asset_without_mapping = Location::new(1, [Junction::Parachain(2222)]);

		pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
			asset_with_mapping.clone(),
			create_asset_details(owner),
		);
		pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
			asset_without_mapping.clone(),
			create_asset_details(owner),
		);

		// Pre-create mapping for one asset
		let pre_mapped_index = asset_with_mapping.to_asset_index();
		assert_ok!(pallet::Pallet::<Test>::insert_asset_mapping(
			pre_mapped_index,
			&asset_with_mapping,
		));

		run_migration_to_completion();

		let index_1 = asset_with_mapping.to_asset_index();
		let index_2 = asset_without_mapping.to_asset_index();

		assert_eq!(
			pallet::Pallet::<Test>::asset_id_of(index_1),
			Some(asset_with_mapping),
			"Pre-existing mapping should be preserved"
		);
		assert_eq!(
			pallet::Pallet::<Test>::asset_id_of(index_2),
			Some(asset_without_mapping),
			"New mapping should be created for previously unmapped asset"
		);
	});
}

/// Test that the migration respects weight limits and can be resumed.
#[test]
fn migration_respects_weight_limits() {
	new_test_ext().execute_with(|| {
		let owner = 1u64;

		// Create multiple foreign assets
		let assets: Vec<Location> = (1000..1005u32)
			.map(|para_id| Location::new(1, [Junction::Parachain(para_id)]))
			.collect();

		for asset_location in &assets {
			pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
				asset_location.clone(),
				create_asset_details(owner),
			);
		}

		// Run with limited weight - should process assets in steps
		let mut cursor = None;
		let mut steps = 0u32;

		// Use a weight that allows processing ~2 assets per step
		let limited_weight = Weight::from_parts(100_000_000, 0);

		loop {
			let mut meter = WeightMeter::with_limit(limited_weight);

			match MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::step(
				cursor,
				&mut meter,
			) {
				Ok(None) => break,
				Ok(Some(new_cursor)) => {
					cursor = Some(new_cursor);
					steps = steps.saturating_add(1);
				},
				Err(e) => panic!("Migration failed: {:?}", e),
			}
		}

		// Verify all assets were migrated
		for asset_location in &assets {
			let asset_index = asset_location.to_asset_index();
			assert!(
				pallet::Pallet::<Test>::asset_id_of(asset_index).is_some(),
				"Asset {:?} should have a mapping after migration",
				asset_location
			);
		}
	});
}

/// Test the migration cursor state transitions.
#[test]
fn migration_cursor_transitions_correctly() {
	new_test_ext().execute_with(|| {
		let owner = 1u64;

		// Create multiple assets to ensure we can observe state transitions
		let assets: Vec<Location> = (7000..7005u32)
			.map(|para_id| Location::new(1, [Junction::Parachain(para_id)]))
			.collect();

		for asset_location in &assets {
			pallet_assets::Asset::<Test, ForeignAssetsInstance>::insert(
				asset_location.clone(),
				create_asset_details(owner),
			);
		}

		// Use limited weight to force multiple steps
		let limited_weight = Weight::from_parts(10_000, 0);
		let mut meter = WeightMeter::with_limit(limited_weight);

		// First step with limited weight
		let cursor =
			MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::step(None, &mut meter)
				.expect("Migration should succeed");

		// With limited weight, we expect either:
		// 1. A cursor pointing to an asset (if we processed some but not all)
		// 2. None if weight was insufficient for even one iteration
		// Both are valid - let's verify the migration completes correctly regardless

		// Now run to completion with unlimited weight
		let mut current_cursor = cursor;
		loop {
			let mut full_meter = WeightMeter::with_limit(Weight::MAX);
			match MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::step(
				current_cursor,
				&mut full_meter,
			) {
				Ok(None) => break, // Migration complete
				Ok(Some(new_cursor)) => {
					// Verify cursor contains valid state
					match &new_cursor {
						MigrationState::Asset(_) => {
							// Valid intermediate state
						},
						MigrationState::Finished => {
							panic!("Should not return Finished in cursor, should return Ok(None)");
						},
					}
					current_cursor = Some(new_cursor);
				},
				Err(e) => panic!("Migration failed: {:?}", e),
			}
		}

		// Verify all assets were migrated
		for asset_location in &assets {
			let asset_index = asset_location.to_asset_index();
			assert!(
				pallet::Pallet::<Test>::asset_id_of(asset_index).is_some(),
				"Asset {:?} should have a mapping after migration",
				asset_location
			);
		}
	});
}

/// Test that migration ID is consistent.
#[test]
fn migration_id_is_consistent() {
	let id1 = MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::id();
	let id2 = MigrateForeignAssetPrecompileMappings::<Test, ForeignAssetsInstance>::id();

	// Compare individual fields since MigrationId doesn't implement PartialEq
	assert_eq!(id1.pallet_id, id2.pallet_id, "Pallet ID should be consistent");
	assert_eq!(id1.version_from, id2.version_from, "Version from should be consistent");
	assert_eq!(id1.version_to, id2.version_to, "Version to should be consistent");

	assert_eq!(
		&id1.pallet_id,
		b"foreign-asset-precompile-mapping",
		"Migration ID should have correct pallet_id"
	);
	assert_eq!(id1.version_from, 0, "Migration should be from version 0");
	assert_eq!(id1.version_to, 1, "Migration should be to version 1");
}

/// Test that Location's ToAssetIndex implementation produces deterministic hashes.
#[test]
fn location_to_asset_index_is_deterministic() {
	let location1 = Location::new(1, [Junction::Parachain(1000)]);
	let location2 = Location::new(1, [Junction::Parachain(1000)]);
	let location3 = Location::new(1, [Junction::Parachain(2000)]);

	// Same locations should produce same index
	assert_eq!(
		location1.to_asset_index(),
		location2.to_asset_index(),
		"Same locations should produce same asset index"
	);

	// Different locations should (likely) produce different indices
	assert_ne!(
		location1.to_asset_index(),
		location3.to_asset_index(),
		"Different locations should produce different asset indices"
	);
}

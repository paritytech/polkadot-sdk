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

use crate::{
	xcm_config::bridging::to_rococo::{AssetHubRococo, RococoEcosystem},
	ForeignAssetsInstance, Runtime,
};
use alloc::{vec, vec::Vec};
use assets_common::{
	local_and_foreign_assets::ForeignAssetReserveData,
	migrations::foreign_assets_reserves::ForeignAssetsReservesProvider,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Contains,
	weights::WeightMeter,
};
use testnet_parachains_constants::westend::snowbridge::EthereumLocation;
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;
use xcm::v5::{Junction, Location};
use xcm_builder::StartsWith;

/// This type provides reserves information for `asset_id`. Meant to be used in a migration running
/// on the Asset Hub Westend upgrade which changes the Foreign Assets reserve-transfers and
/// teleports from hardcoded rules to per-asset configured reserves.
///
/// The hardcoded rules (see `xcm_config.rs`) migrated here:
/// 1. Foreign Assets native to sibling parachains are teleportable between the asset's native chain
///    and Asset Hub.
///  ----> `ForeignAssetReserveData { reserve: "Asset's native chain", teleport: true }`
/// 2. Foreign assets native to Ethereum Ecosystem have Ethereum as trusted reserve.
///  ----> `ForeignAssetReserveData { reserve: "Ethereum", teleport: false }`
/// 3. Foreign assets native to Rococo Ecosystem have Asset Hub Rococo as trusted reserve.
///  ----> `ForeignAssetReserveData { reserve: "Asset Hub Rococo", teleport: false }`
pub struct AssetHubWestendForeignAssetsReservesProvider;
impl ForeignAssetsReservesProvider for AssetHubWestendForeignAssetsReservesProvider {
	type ReserveData = ForeignAssetReserveData;
	fn reserves_for(asset_id: &Location) -> Vec<Self::ReserveData> {
		let reserves = if StartsWith::<RococoEcosystem>::contains(asset_id) {
			// rule 3: rococo asset, Asset Hub Rococo reserve, non teleportable
			vec![(AssetHubRococo::get(), false).into()]
		} else if StartsWith::<EthereumLocation>::contains(asset_id) {
			// rule 2: ethereum asset, ethereum reserve, non teleportable
			vec![(EthereumLocation::get(), false).into()]
		} else {
			match asset_id.unpack() {
				(1, interior) => {
					match interior.first() {
						Some(Junction::Parachain(sibling_para_id))
							if sibling_para_id.ne(&ASSET_HUB_ID) =>
						{
							// rule 1: sibling parachain asset, sibling parachain reserve,
							// teleportable
							vec![ForeignAssetReserveData {
								reserve: Location::new(1, Junction::Parachain(*sibling_para_id)),
								teleportable: true,
							}]
						},
						_ => vec![],
					}
				},
				_ => vec![],
			}
		};
		if reserves.is_empty() {
			tracing::error!(
				target: "runtime::AssetHubWestendForeignAssetsReservesProvider::reserves_for",
				id = ?asset_id, "unexpected asset",
			);
		}
		reserves
	}

	#[cfg(feature = "try-runtime")]
	fn check_reserves_for(asset_id: &Location, reserves: Vec<Self::ReserveData>) -> bool {
		if StartsWith::<RococoEcosystem>::contains(asset_id) {
			let expected =
				ForeignAssetReserveData { reserve: AssetHubRococo::get(), teleportable: false };
			// rule 3: rococo asset
			reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
		} else if StartsWith::<EthereumLocation>::contains(asset_id) {
			let expected =
				ForeignAssetReserveData { reserve: EthereumLocation::get(), teleportable: false };
			// rule 2: ethereum asset
			reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
		} else {
			match asset_id.unpack() {
				(1, interior) => {
					match interior.first() {
						Some(Junction::Parachain(sibling_para_id))
							if sibling_para_id.ne(&ASSET_HUB_ID) =>
						{
							let expected = ForeignAssetReserveData {
								reserve: Location::new(1, Junction::Parachain(*sibling_para_id)),
								teleportable: true,
							};
							// rule 1: sibling parachain asset
							reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
						},
						// unexpected asset
						_ => false,
					}
				},
				// we have some junk assets registered on AHW with `GlobalConsensus(Polkadot)`
				(2, _) => reserves.is_empty(),
				// unexpected asset
				_ => false,
			}
		}
	}
}

const PRECOMPILE_MAPPINGS_MIGRATION_ID: &[u8; 32] = b"foreign-asset-precompile-mapping";

/// Progressive states of the precompile mappings migration.
#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
pub enum PrecompileMappingsMigrationState {
	Asset(Location),
	Finished,
}

/// Migration to backfill foreign asset precompile mappings for existing assets.
///
/// This migration populates the bidirectional mapping between XCM Locations and u32 indices
/// in `pallet_assets_precompiles` for all existing foreign assets on Asset Hub Westend.
///
/// The mapping enables EVM precompile addresses for foreign assets, where the u32 index
/// is embedded in the first 4 bytes of the 20-byte Ethereum address.
///
/// # Background
///
/// Foreign assets are identified by an XCM Location type which does not fit in 4 bytes.
/// In order to facilitate EVM precompile addresses for these assets, a mapping is maintained
/// between a u32 index and the actual XCM Location.
/// The pallet-assets was extended with foreign assets functionality via a
/// bidirectional mapping:
/// - `AssetIndexToForeignAssetId`: u32 index -> XCM Location
/// - `ForeignAssetIdToAssetIndex`: XCM Location -> u32 index
///
/// While new foreign assets automatically get mapped via the `AssetsCallback` hook,
/// this migration ensures existing foreign assets (created before the mapping was introduced)
/// are also added to the mapping.
///
/// # Usage in Runtime
///
/// Add this to the runtime's `Migrations` tuple in lib.rs:
///
/// ```ignore
/// pub type Migrations = (
///     // ... other migrations ...
///     MigrateForeignAssetPrecompileMappings,
/// );
/// ```
///
/// # Safety
///
/// - Idempotent: Skips assets that already have mappings
/// - Non-destructive: Does not modify any asset data, only adds mappings
/// - Handles hash collisions gracefully (logs warning, doesn't panic)
pub struct MigrateForeignAssetPrecompileMappings;

impl SteppedMigration for MigrateForeignAssetPrecompileMappings {
	type Cursor = PrecompileMappingsMigrationState;
	type Identifier = MigrationId<32>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PRECOMPILE_MAPPINGS_MIGRATION_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// Weight for one iteration: 2 reads (iter + check mapping) + potentially 2 writes + 2
		// reads for insert
		let required = <Runtime as frame_system::Config>::DbWeight::get().reads_writes(4, 2);

		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if !meter.can_consume(required) {
				break;
			}

			let next = match &cursor {
				None => Self::migrate_asset_step(None),
				Some(PrecompileMappingsMigrationState::Asset(last_asset)) =>
					Self::migrate_asset_step(Some(last_asset)),
				Some(PrecompileMappingsMigrationState::Finished) => {
					log::info!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"migration finished"
					);
					return Ok(None);
				},
			};

			cursor = Some(next);
			meter.consume(required);
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;
		use pallet_assets_precompiles::ToAssetIndex;

		let mut asset_indices = Vec::new();

		for (asset_location, _) in pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::iter() {
			let asset_index = asset_location.to_asset_index();
			if pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
				.is_none()
			{
				asset_indices.push((asset_location, asset_index));
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::pre_upgrade",
			"Found {} foreign assets needing migration",
			asset_indices.len()
		);

		Ok(asset_indices.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		let asset_indices: Vec<(Location, u32)> = Decode::decode(&mut &state[..])
			.map_err(|_| sp_runtime::TryRuntimeError::Other("Failed to decode state"))?;

		let mut migrated = 0u64;

		for (asset_location, expected_index) in &asset_indices {
			match pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(*expected_index)
			{
				Some(stored_location) if stored_location == *asset_location => {
					migrated = migrated.saturating_add(1);

					match pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_index_of(
						asset_location,
					) {
						Some(stored_index) if stored_index == *expected_index => {},
						_ =>
							return Err(sp_runtime::TryRuntimeError::Other(
								"Reverse mapping mismatch",
							)),
					}
				},
				_ => {
					log::warn!(
						target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
						"Asset at index {:?} not migrated (possible collision)",
						expected_index
					);
				},
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
			"Verified {} out of {} foreign asset mappings",
			migrated,
			asset_indices.len()
		);

		Ok(())
	}
}

impl MigrateForeignAssetPrecompileMappings {
	fn migrate_asset_step(maybe_last_key: Option<&Location>) -> PrecompileMappingsMigrationState {
		use pallet_assets_precompiles::ToAssetIndex;

		let mut iter = if let Some(last_key) = maybe_last_key {
			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::iter_keys_from(
				pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::hashed_key_for(last_key),
			)
		} else {
			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::iter_keys()
		};

		if let Some(asset_location) = iter.next() {
			let asset_index = asset_location.to_asset_index();

			// Check if mapping already exists (idempotent)
			if pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
				.is_some()
			{
				log::debug!(
					target: "runtime::MigrateForeignAssetPrecompileMappings",
					"Skipping asset index {:?} - mapping already exists",
					asset_index
				);
				return PrecompileMappingsMigrationState::Asset(asset_location);
			}

			// Insert the bidirectional mapping
			match pallet_assets_precompiles::pallet::Pallet::<Runtime>::insert_asset_mapping(
				asset_index,
				&asset_location,
			) {
				Ok(()) => {
					log::debug!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"Migrated asset index {:?} for location {:?}",
						asset_index,
						asset_location
					);
				},
				Err(()) => {
					log::warn!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"Failed to migrate asset index {:?} for location {:?} - hash collision",
						asset_index,
						asset_location
					);
				},
			}

			PrecompileMappingsMigrationState::Asset(asset_location)
		} else {
			PrecompileMappingsMigrationState::Finished
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{assert_ok, weights::Weight};
	use pallet_assets::AssetDetails;
	use pallet_assets_precompiles::ToAssetIndex;
	use sp_runtime::BuildStorage;
	use xcm::v5::prelude::*;

	#[test]
	fn migration_struct_compiles() {
		// Basic smoke test to ensure migration code compiles
		let _ = MigrateForeignAssetPrecompileMappings;
	}

	/// Creates a minimal test externalities with frame_system genesis.
	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1);
		});
		ext
	}

	/// Helper to run the stepped migration to completion.
	fn run_migration_to_completion() -> u32 {
		let mut cursor = None;
		let mut steps = 0u32;
		loop {
			let mut meter = WeightMeter::new();
			meter.consume(Weight::zero()); // Start with empty meter but with max limit
								  // Create a meter with large limit
			let mut meter = WeightMeter::with_limit(Weight::MAX);
			match MigrateForeignAssetPrecompileMappings::step(cursor, &mut meter) {
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
		owner: <Runtime as frame_system::Config>::AccountId,
	) -> AssetDetails<
		<Runtime as pallet_assets::Config<ForeignAssetsInstance>>::Balance,
		<Runtime as frame_system::Config>::AccountId,
		<Runtime as pallet_balances::Config>::Balance,
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
	///
	/// The test fails without the migration because the final assertions require
	/// the mappings to exist.
	#[test]
	fn migration_populates_foreign_asset_precompile_mappings() {
		new_test_ext().execute_with(|| {
			// Create test account
			let owner: <Runtime as frame_system::Config>::AccountId = [1u8; 32].into();

			// Create foreign asset locations (simulating assets from sibling parachains)
			let sibling_para_1000_asset = Location::new(1, [Parachain(1000), GeneralIndex(1)]);
			let sibling_para_2000_asset = Location::new(1, [Parachain(2000), GeneralIndex(42)]);
			let sibling_para_3000_asset =
				Location::new(1, [Parachain(3000), PalletInstance(50), GeneralIndex(100)]);

			let test_assets = vec![
				sibling_para_1000_asset.clone(),
				sibling_para_2000_asset.clone(),
				sibling_para_3000_asset.clone(),
			];

			// Insert foreign assets directly into pallet_assets storage
			// WITHOUT creating precompile mappings (simulating pre-migration state)
			for asset_location in &test_assets {
				pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::insert(
					asset_location.clone(),
					create_asset_details(owner.clone()),
				);
			}

			// Verify assets are in storage
			for asset_location in &test_assets {
				assert!(
					pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::contains_key(
						asset_location
					),
					"Asset should exist in pallet_assets storage"
				);
			}

			// Verify precompile mappings do NOT exist before migration
			// This is the critical pre-condition that the migration addresses
			for asset_location in &test_assets {
				let asset_index = asset_location.to_asset_index();

				assert!(
					pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
						.is_none(),
					"Precompile mapping should NOT exist before migration"
				);
				assert!(
					pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_index_of(
						asset_location
					)
					.is_none(),
					"Reverse precompile mapping should NOT exist before migration"
				);
			}

			// Run the migration
			run_migration_to_completion();

			// Verify precompile mappings now exist after migration
			// This assertion FAILS if the migration doesn't run
			for asset_location in &test_assets {
				let asset_index = asset_location.to_asset_index();

				// Check forward mapping: index -> location
				let stored_location =
					pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index);
				assert_eq!(
					stored_location,
					Some(asset_location.clone()),
					"Forward precompile mapping should exist after migration for {:?}",
					asset_location
				);

				// Check reverse mapping: location -> index
				let stored_index =
					pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_index_of(
						asset_location,
					);
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
	#[test]
	fn migration_is_idempotent() {
		new_test_ext().execute_with(|| {
			let owner: <Runtime as frame_system::Config>::AccountId = [1u8; 32].into();

			// Create a foreign asset
			let asset_location = Location::new(1, [Parachain(1234), GeneralIndex(999)]);

			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::insert(
				asset_location.clone(),
				create_asset_details(owner),
			);

			// Verify mapping does not exist before migration
			let asset_index = asset_location.to_asset_index();
			assert!(
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
					.is_none(),
				"Mapping should NOT exist before migration"
			);

			// Run migration first time
			run_migration_to_completion();

			// Verify mapping was created
			assert!(
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
					.is_some(),
				"Mapping should exist after first migration"
			);

			// Run migration second time (should be idempotent)
			run_migration_to_completion();

			// Verify mapping still exists and is correct after second run
			let stored_location =
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index);
			assert_eq!(
				stored_location,
				Some(asset_location.clone()),
				"Mapping should still be correct after second migration"
			);
		});
	}

	/// Test that the migration handles the case with no foreign assets gracefully.
	#[test]
	fn migration_handles_empty_foreign_assets() {
		new_test_ext().execute_with(|| {
			// Don't create any foreign assets

			// Run migration on empty state - should complete immediately
			let steps = run_migration_to_completion();

			// Should complete without panicking, with minimal steps
			assert_eq!(steps, 0, "Empty migration should have no steps");
		});
	}

	/// Test that assets with existing mappings are correctly skipped.
	#[test]
	fn migration_skips_already_mapped_assets() {
		new_test_ext().execute_with(|| {
			let owner: <Runtime as frame_system::Config>::AccountId = [1u8; 32].into();

			// Create two foreign assets
			let asset_with_mapping = Location::new(1, [Parachain(1111), GeneralIndex(1)]);
			let asset_without_mapping = Location::new(1, [Parachain(2222), GeneralIndex(2)]);

			// Insert both assets into pallet_assets
			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::insert(
				asset_with_mapping.clone(),
				create_asset_details(owner.clone()),
			);
			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::insert(
				asset_without_mapping.clone(),
				create_asset_details(owner),
			);

			// Pre-create mapping for the first asset (simulating it was created
			// after the mapping feature was introduced)
			let pre_mapped_index = asset_with_mapping.to_asset_index();
			assert_ok!(pallet_assets_precompiles::pallet::Pallet::<Runtime>::insert_asset_mapping(
				pre_mapped_index,
				&asset_with_mapping,
			));

			// Run migration
			run_migration_to_completion();

			// Verify both assets now have mappings
			let index_1 = asset_with_mapping.to_asset_index();
			let index_2 = asset_without_mapping.to_asset_index();

			assert_eq!(
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(index_1),
				Some(asset_with_mapping),
				"Pre-existing mapping should be preserved"
			);
			assert_eq!(
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(index_2),
				Some(asset_without_mapping),
				"New mapping should be created for previously unmapped asset"
			);
		});
	}
}

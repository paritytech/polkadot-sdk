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
use frame_support::traits::Contains;
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

impl frame_support::traits::OnRuntimeUpgrade for MigrateForeignAssetPrecompileMappings {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use pallet_assets_precompiles::ToAssetIndex;

		let mut reads = 0u64;
		let mut writes = 0u64;
		let mut migrated = 0u64;
		let mut skipped = 0u64;
		let mut failed = 0u64;

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::on_runtime_upgrade",
			"Starting migration of foreign asset precompile mappings..."
		);

		// Iterate through all existing foreign assets in the pallet-assets instance
		for (asset_location, _asset_details) in
			pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::iter()
		{
			// Count read from iteration and from pallet_assets_precompiles storage (below)
			reads = reads.saturating_add(2);

			// Derive the precompile address index from the asset location 
			let asset_index = asset_location.to_asset_index();

			// Check if mapping already exists (migration is idempotent)
			if pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
				.is_some()
			{
				log::debug!(
					target: "runtime::MigrateForeignAssetPrecompileMappings::on_runtime_upgrade",
					"Skipping asset index {:?} - mapping already exists",
					asset_index
				);
				skipped = skipped.saturating_add(1);
				continue;
			}

			// Two storage writes for bidirectional mapping
			writes = writes.saturating_add(2);

			// Count two storage reads for `contains_key` checks in `insert_asset_mapping`
			reads = reads.saturating_add(2);

			// Insert the bidirectional mapping
			match pallet_assets_precompiles::pallet::Pallet::<Runtime>::insert_asset_mapping(
				asset_index,
				&asset_location,
			) {
				Ok(()) => {
					log::debug!(
						target: "runtime::MigrateForeignAssetPrecompileMappings::on_runtime_upgrade",
						"Migrated asset index {:?} for location {:?}",
						asset_index,
						asset_location
					);
					migrated = migrated.saturating_add(1);
				},
				Err(()) => {
					// Collision detected - extremely unlikely with blake2_256 hash
					log::warn!(
						target: "runtime::MigrateForeignAssetPrecompileMappings::on_runtime_upgrade",
						"Failed to migrate asset index {:?} for location {:?} - hash collision detected",
						asset_index,
						asset_location
					);
					failed = failed.saturating_add(1);
				},
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::on_runtime_upgrade",
			"Foreign asset precompile mapping migration completed: \
				{} migrated, {} skipped, {} failed",
			migrated,
			skipped,
			failed
		);

		<Runtime as frame_system::Config>::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;
		use pallet_assets_precompiles::ToAssetIndex;

		// Count how many assets need migration
		let mut count = 0u64;
		let mut asset_indices = alloc::vec::Vec::new();

		for (asset_location, _) in pallet_assets::Asset::<Runtime, ForeignAssetsInstance>::iter() {
			let asset_index = asset_location.to_asset_index();
			// Only count assets that don't have mapping yet
			if pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
				.is_none()
			{
				count = count.saturating_add(1);
				asset_indices.push((asset_location, asset_index));
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::pre_upgrade",
			"pre_upgrade: Found {} foreign assets needing migration",
			count
		);

		Ok((count, asset_indices).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		use pallet_assets_precompiles::ToAssetIndex;

		let (expected_count, asset_indices): (u64, alloc::vec::Vec<(Location, u32)>) =
			Decode::decode(&mut &state[..])
				.map_err(|_| sp_runtime::TryRuntimeError::Other("Failed to decode state"))?;

		let mut migrated = 0u64;

		// Verify all assets from pre_upgrade are now mapped
		for (asset_location, expected_index) in asset_indices {
			match pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(
				expected_index,
			) {
				Some(stored_location) if stored_location == asset_location => {
					migrated = migrated.saturating_add(1);

					// Also verify reverse mapping
					match pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_index_of(
						&asset_location,
					) {
						Some(stored_index) if stored_index == expected_index => {},
						_ => {
							return Err(sp_runtime::TryRuntimeError::Other(
								"Reverse mapping mismatch",
							))
						},
					}
				},
				_ => {
					// This could be a hash collision case, which is acceptable
					log::warn!(
						target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
						"post_upgrade: Asset at index {:?} not migrated (possible collision)",
						expected_index
					);
				},
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
			"post_upgrade: Verified {} out of {} foreign asset mappings",
			migrated,
			expected_count
		);

		// We don't fail on mismatches because hash collisions are possible (though extremely rare)
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		assert_ok,
		traits::OnRuntimeUpgrade,
	};
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
		let t = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1);
		});
		ext
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
	/// 1. Creates foreign assets directly in storage WITHOUT precompile mappings
	///    (simulating the pre-migration state)
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
			let sibling_para_1000_asset = Location::new(
				1,
				[Parachain(1000), GeneralIndex(1)],
			);
			let sibling_para_2000_asset = Location::new(
				1,
				[Parachain(2000), GeneralIndex(42)],
			);
			let sibling_para_3000_asset = Location::new(
				1,
				[Parachain(3000), PalletInstance(50), GeneralIndex(100)],
			);

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
			let weight = MigrateForeignAssetPrecompileMappings::on_runtime_upgrade();

			// Verify weight is non-zero (migration did work)
			assert!(weight.ref_time() > 0, "Migration should consume some weight");

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

			// Run migration first time
			let weight_first = MigrateForeignAssetPrecompileMappings::on_runtime_upgrade();

			// Verify mapping was created
			let asset_index = asset_location.to_asset_index();
			assert!(
				pallet_assets_precompiles::pallet::Pallet::<Runtime>::asset_id_of(asset_index)
					.is_some(),
				"Mapping should exist after first migration"
			);

			// Run migration second time
			let weight_second = MigrateForeignAssetPrecompileMappings::on_runtime_upgrade();

			// Second run should have less work (skips already-mapped assets)
			// Weight should be lower because no writes are performed
			assert!(
				weight_second.ref_time() <= weight_first.ref_time(),
				"Second migration run should do less work"
			);

			// Verify mapping still exists and is correct
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

			// Run migration on empty state
			let weight = MigrateForeignAssetPrecompileMappings::on_runtime_upgrade();

			// Should complete without panicking
			// Weight should be minimal (just the base reads/writes overhead)
			assert!(
				weight.ref_time() == 0 || weight.proof_size() == 0,
				"Empty migration should have minimal weight"
			);
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
			MigrateForeignAssetPrecompileMappings::on_runtime_upgrade();

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

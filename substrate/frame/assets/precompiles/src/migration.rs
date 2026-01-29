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

//! Migrations for `pallet-assets-precompiles`.

use crate::{foreign_assets::pallet, weights::WeightInfo};
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	weights::{Weight, WeightMeter},
};

const PRECOMPILE_MAPPINGS_MIGRATION_ID: &[u8; 32] = b"foreign-asset-precompile-mapping";

/// Progressive states of the precompile mappings migration.
#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
pub enum MigrationState<A> {
	Asset(A),
	Finished,
}

/// Migration to backfill foreign asset precompile mappings for existing assets.
///
/// This migration populates the bidirectional mapping between foreign asset IDs (e.g., XCM
/// Locations) and sequential u32 indices in `pallet_assets_precompiles` for all existing foreign
/// assets.
///
/// The mapping enables EVM precompile addresses for foreign assets, where the u32 index
/// is embedded in the first 4 bytes of the 20-byte Ethereum address.
///
/// # Background
///
/// Foreign assets are identified by types (like XCM Location) that do not fit in 4 bytes.
/// In order to facilitate EVM precompile addresses for these assets, a mapping is maintained
/// between a sequential u32 index and the actual foreign asset ID.
/// The pallet maintains a bidirectional mapping:
/// - `AssetIndexToForeignAssetId`: u32 index -> Foreign Asset ID
/// - `ForeignAssetIdToAssetIndex`: Foreign Asset ID -> u32 index
///
/// While new foreign assets automatically get mapped via the `AssetsCallback` hook,
/// this migration ensures existing foreign assets (created before the mapping was introduced)
/// are also added to the mapping with sequential indices.
///
/// # Type Parameters
///
/// - `T`: The runtime configuration implementing both `pallet_assets::Config<I>` and
///   `pallet::Config`
/// - `I`: The pallet_assets instance identifier (e.g., `ForeignAssetsInstance`)
/// - `W`: The weight info implementation for the migration benchmarks
///
/// # Usage in Runtime
///
/// Add this to the runtime's `Migrations` tuple in lib.rs:
///
/// ```ignore
/// pub type Migrations = (
///     // ... other migrations ...
///     pallet_assets_precompiles::MigrateForeignAssetPrecompileMappings<Runtime, ForeignAssetsInstance, ()>,
/// );
/// ```
///
/// # Safety
///
/// - Idempotent: Skips assets that already have mappings
/// - Non-destructive: Does not modify any asset data, only adds mappings
/// - Sequential indices: Each migrated asset gets the next available index
pub struct MigrateForeignAssetPrecompileMappings<T, I = (), W = ()>(PhantomData<(T, I, W)>);

impl<T, I, W> SteppedMigration for MigrateForeignAssetPrecompileMappings<T, I, W>
where
	T: pallet_assets::Config<I>
		+ pallet::Config<ForeignAssetId = <T as pallet_assets::Config<I>>::AssetId>,
	I: 'static,
	W: WeightInfo,
{
	type Cursor = MigrationState<<T as pallet_assets::Config<I>>::AssetId>;
	type Identifier = MigrationId<32>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PRECOMPILE_MAPPINGS_MIGRATION_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// Use the maximum weight (migrate case) as the required weight per iteration,
		// since we don't know ahead of time whether we'll migrate or skip.
		let required = W::migrate_asset_step_migrate();

		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if !meter.can_consume(required) {
				break;
			}

			let (next, weight) = match &cursor {
				None => Self::migrate_asset_step(None),
				Some(MigrationState::Asset(last_asset)) =>
					Self::migrate_asset_step(Some(last_asset)),
				Some(MigrationState::Finished) => {
					log::info!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"migration finished"
					);
					return Ok(None);
				},
			};

			cursor = Some(next);
			meter.consume(weight);
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		let mut unmapped_assets = alloc::vec::Vec::new();

		for (asset_id, _) in pallet_assets::Asset::<T, I>::iter() {
			if pallet::Pallet::<T>::asset_index_of(&asset_id).is_none() {
				unmapped_assets.push(asset_id);
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::pre_upgrade",
			"Found {} foreign assets needing migration",
			unmapped_assets.len()
		);

		Ok(unmapped_assets.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		let unmapped_assets: alloc::vec::Vec<<T as pallet_assets::Config<I>>::AssetId> =
			Decode::decode(&mut &state[..])
				.map_err(|_| sp_runtime::TryRuntimeError::Other("Failed to decode state"))?;

		let mut migrated = 0u64;

		for asset_id in &unmapped_assets {
			// Check that a mapping now exists for this asset
			match pallet::Pallet::<T>::asset_index_of(asset_id) {
				Some(index) => {
					// Verify reverse mapping is consistent
					match pallet::Pallet::<T>::asset_id_of(index) {
						Some(stored_id) if stored_id == *asset_id => {
							migrated = migrated.saturating_add(1);
						},
						_ =>
							return Err(sp_runtime::TryRuntimeError::Other(
								"Reverse mapping mismatch",
							)),
					}
				},
				None => {
					log::error!(
						target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
						"Asset {:?} not migrated",
						asset_id
					);
					return Err(sp_runtime::TryRuntimeError::Other("Asset not migrated"));
				},
			}
		}

		log::info!(
			target: "runtime::MigrateForeignAssetPrecompileMappings::post_upgrade",
			"Verified {} foreign asset mappings",
			migrated
		);

		Ok(())
	}
}

impl<T, I, W> MigrateForeignAssetPrecompileMappings<T, I, W>
where
	T: pallet_assets::Config<I>
		+ pallet::Config<ForeignAssetId = <T as pallet_assets::Config<I>>::AssetId>,
	I: 'static,
	W: WeightInfo,
{
	/// Performs a single step of the migration.
	/// Returns the new migration state and the weight consumed.
	fn migrate_asset_step(
		maybe_last_key: Option<&<T as pallet_assets::Config<I>>::AssetId>,
	) -> (MigrationState<<T as pallet_assets::Config<I>>::AssetId>, Weight) {
		let mut iter = if let Some(last_key) = maybe_last_key {
			pallet_assets::Asset::<T, I>::iter_keys_from(
				pallet_assets::Asset::<T, I>::hashed_key_for(last_key),
			)
		} else {
			pallet_assets::Asset::<T, I>::iter_keys()
		};

		if let Some(asset_id) = iter.next() {
			// Check if mapping already exists (idempotent)
			if pallet::Pallet::<T>::asset_index_of(&asset_id).is_some() {
				log::debug!(
					target: "runtime::MigrateForeignAssetPrecompileMappings",
					"Skipping asset {:?} - mapping already exists",
					asset_id
				);
				return (MigrationState::Asset(asset_id), W::migrate_asset_step_skip());
			}

			// Insert the bidirectional mapping with a new sequential index
			match pallet::Pallet::<T>::insert_asset_mapping(&asset_id) {
				Ok(asset_index) => {
					log::debug!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"Migrated asset {:?} to index {:?}",
						asset_id,
						asset_index
					);
				},
				Err(()) => {
					log::warn!(
						target: "runtime::MigrateForeignAssetPrecompileMappings",
						"Failed to migrate asset {:?}",
						asset_id
					);
				},
			}

			(MigrationState::Asset(asset_id), W::migrate_asset_step_migrate())
		} else {
			(MigrationState::Finished, W::migrate_asset_step_finished())
		}
	}
}

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

use crate::foreign_assets::{pallet, ToAssetIndex};
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Get,
	weights::WeightMeter,
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
/// This migration populates the bidirectional mapping between XCM Locations and u32 indices
/// in `pallet_assets_precompiles` for all existing foreign assets.
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
/// # Type Parameters
///
/// - `T`: The runtime configuration implementing both `pallet_assets::Config<I>` and
///   `pallet::Config`
/// - `I`: The pallet_assets instance identifier (e.g., `ForeignAssetsInstance`)
///
/// # Usage in Runtime
///
/// Add this to the runtime's `Migrations` tuple in lib.rs:
///
/// ```ignore
/// pub type Migrations = (
///     // ... other migrations ...
///     pallet_assets_precompiles::MigrateForeignAssetPrecompileMappings<Runtime, ForeignAssetsInstance>,
/// );
/// ```
///
/// # Safety
///
/// - Idempotent: Skips assets that already have mappings
/// - Non-destructive: Does not modify any asset data, only adds mappings
/// - Handles hash collisions gracefully (logs warning, doesn't panic)
pub struct MigrateForeignAssetPrecompileMappings<T, I = ()>(PhantomData<(T, I)>);

impl<T, I> SteppedMigration for MigrateForeignAssetPrecompileMappings<T, I>
where
	T: pallet_assets::Config<I>
		+ pallet::Config<ForeignAssetId = <T as pallet_assets::Config<I>>::AssetId>,
	<T as pallet_assets::Config<I>>::AssetId: ToAssetIndex,
	I: 'static,
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
		// Weight for one iteration: 2 reads (iter + check mapping) + potentially 2 writes + 2
		// reads for insert
		let required = <T as frame_system::Config>::DbWeight::get().reads_writes(4, 2);

		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if !meter.can_consume(required) {
				break;
			}

			let next = match &cursor {
				None => Self::migrate_asset_step(None),
				Some(MigrationState::Asset(last_asset)) => Self::migrate_asset_step(Some(last_asset)),
				Some(MigrationState::Finished) => {
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
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		let mut asset_indices = alloc::vec::Vec::new();

		for (asset_location, _) in pallet_assets::Asset::<T, I>::iter() {
			let asset_index = asset_location.to_asset_index();
			if pallet::Pallet::<T>::asset_id_of(asset_index).is_none() {
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
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		let asset_indices: alloc::vec::Vec<(<T as pallet_assets::Config<I>>::AssetId, u32)> =
			Decode::decode(&mut &state[..])
				.map_err(|_| sp_runtime::TryRuntimeError::Other("Failed to decode state"))?;

		let mut migrated = 0u64;

		for (asset_location, expected_index) in &asset_indices {
			match pallet::Pallet::<T>::asset_id_of(*expected_index) {
				Some(stored_location) if stored_location == *asset_location => {
					migrated = migrated.saturating_add(1);

					match pallet::Pallet::<T>::asset_index_of(asset_location) {
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

impl<T, I> MigrateForeignAssetPrecompileMappings<T, I>
where
	T: pallet_assets::Config<I>
		+ pallet::Config<ForeignAssetId = <T as pallet_assets::Config<I>>::AssetId>,
	<T as pallet_assets::Config<I>>::AssetId: ToAssetIndex,
	I: 'static,
{
	fn migrate_asset_step(
		maybe_last_key: Option<&<T as pallet_assets::Config<I>>::AssetId>,
	) -> MigrationState<<T as pallet_assets::Config<I>>::AssetId> {
		let mut iter = if let Some(last_key) = maybe_last_key {
			pallet_assets::Asset::<T, I>::iter_keys_from(
				pallet_assets::Asset::<T, I>::hashed_key_for(last_key),
			)
		} else {
			pallet_assets::Asset::<T, I>::iter_keys()
		};

		if let Some(asset_location) = iter.next() {
			let asset_index = asset_location.to_asset_index();

			// Check if mapping already exists (idempotent)
			if pallet::Pallet::<T>::asset_id_of(asset_index).is_some() {
				log::debug!(
					target: "runtime::MigrateForeignAssetPrecompileMappings",
					"Skipping asset index {:?} - mapping already exists",
					asset_index
				);
				return MigrationState::Asset(asset_location);
			}

			// Insert the bidirectional mapping
			match pallet::Pallet::<T>::insert_asset_mapping(asset_index, &asset_location) {
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

			MigrationState::Asset(asset_location)
		} else {
			MigrationState::Finished
		}
	}
}

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

/// `pallet-assets` has been enhanced with asset reserves information so that AH foreign assets
/// can be registered as either teleportable or reserve-based.
/// Originally, all foreign assets were exclusively teleportable, whereas now, on creation they
/// are reserve-based by default and can be made teleportable by the asset `Owner`.
///
/// This migration adds `Here` (the local chain) as a trusted reserve for the existing foreign
/// assets so as to preserve their existing teleportable status. For new assets, that status
/// needs to be explicitly opted-in to by the asset's `Owner`.
///
/// See <https://github.com/paritytech/polkadot-sdk/pull/9948> for more info.
pub mod foreign_assets_v2 {
	use crate::*;
	use alloc::vec;
	use codec::{Decode, Encode, MaxEncodedLen};
	use core::marker::PhantomData;
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		traits::{ContainsPair, GetStorageVersion, StorageVersion},
		weights::WeightMeter,
	};
	use pallet_assets::WeightInfo;

	const PALLET_MIGRATIONS_ID: &[u8; 21] = b"pallet-foreign-assets";

	#[cfg(feature = "try-runtime")]
	use frame_support::traits::Len;
	#[cfg(feature = "try-runtime")]
	#[derive(Encode, Decode)]
	struct TryRuntimeState<T: pallet_assets::Config<I>, I: 'static> {
		assets: Vec<T::AssetId>,
	}

	/// Progressive states of a migration. The migration starts with the first variant and ends with
	/// the last.
	#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
	pub enum MigrationState<A> {
		Asset(A),
		Finished,
	}

	/// The resulting state of the step and the actual weight consumed.
	type StepResultOf<T, I> = MigrationState<<T as pallet_assets::Config<I>>::AssetId>;

	pub struct ForeignAssetsLazyMigrationV1ToV2<T, I, AssetFilter>(
		PhantomData<(T, I, AssetFilter)>,
	);
	impl<T, I, AssetFilter> SteppedMigration for ForeignAssetsLazyMigrationV1ToV2<T, I, AssetFilter>
	where
		T: pallet_assets::Config<I, AssetId = xcm::v5::Location, ReserveId = xcm::v5::Location>,
		I: 'static,
		AssetFilter: ContainsPair<xcm::v5::Location, xcm::v5::Location>,
	{
		type Cursor = StepResultOf<T, I>;
		type Identifier = MigrationId<21>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			if pallet_assets::Pallet::<T, I>::on_chain_storage_version() !=
				Self::id().version_from as u16
			{
				return Ok(None);
			}

			// Check that we have enough weight for at least the next step. If we don't, then the
			// migration cannot be complete.
			let required =
				<T as pallet_assets::Config<I>>::WeightInfo::migration_v2_foreign_asset_set_reserve_weight();
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			loop {
				if !meter.can_consume(required) {
					break;
				}

				let next = match &cursor {
					// At first, start migrating assets.
					None => Self::asset_step(None),
					// Migrate any remaining assets.
					Some(MigrationState::Asset(maybe_last_asset)) =>
						Self::asset_step(Some(maybe_last_asset)),
					// After the last asset, migration is finished.
					Some(MigrationState::Finished) => {
						StorageVersion::new(Self::id().version_to as u16)
							.put::<pallet_assets::Pallet<T, I>>();
						return Ok(None)
					},
				};

				cursor = Some(next);
				meter.consume(required);
			}

			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let assets = pallet_assets::Asset::<T, I>::iter_keys().collect();
			let state = TryRuntimeState::<T, I> { assets };
			Ok(state.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let prev_state = TryRuntimeState::<T, I>::decode(&mut &state[..])
				.expect("Failed to decode the previous storage state");
			let local_chain = xcm::v5::Location::here();
			for id in prev_state.assets {
				if AssetFilter::contains(&id, &id) {
					let reserves = pallet_assets::ReserveLocations::<T, I>::get(id);
					assert_eq!(reserves.len(), 1);
					assert_eq!(reserves[0], local_chain);
				}
			}
			Ok(())
		}
	}

	impl<T, I, AssetFilter> ForeignAssetsLazyMigrationV1ToV2<T, I, AssetFilter>
	where
		T: pallet_assets::Config<I, AssetId = xcm::v5::Location, ReserveId = xcm::v5::Location>,
		I: 'static,
		AssetFilter: ContainsPair<xcm::v5::Location, xcm::v5::Location>,
	{
		// Make `Here` a reserve location for one entry of `Asset`.
		fn asset_step(maybe_last_key: Option<&T::AssetId>) -> StepResultOf<T, I> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				pallet_assets::Asset::<T, I>::iter_keys_from(
					pallet_assets::Asset::<T, I>::hashed_key_for(last_key),
				)
			} else {
				pallet_assets::Asset::<T, I>::iter_keys()
			};
			if let Some(asset_id) = iter.next() {
				if AssetFilter::contains(&asset_id, &asset_id) {
					let _ = pallet_assets::Pallet::<T, I>::unchecked_update_reserves(
						asset_id.clone(),
						vec![xcm::v5::Location::here()],
					);
				}
				MigrationState::Asset(asset_id)
			} else {
				MigrationState::Finished
			}
		}
	}
}

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
/// Originally, all foreign assets were exclusively teleportable, whereas now, after creation the
/// asset `Owner` also needs to set the asset's trusted reserves.
///
/// This migration adds the origin chain of each existing foreign asset as a trusted reserve for it.
/// It also adds `Here` (the local chain) as a trusted reserve for the existing Sibling Parachain
/// foreign assets so as to preserve their existing teleportable status. For new assets, that status
/// needs to be explicitly opted-in to by the asset's `Owner`.
///
/// See <https://github.com/paritytech/polkadot-sdk/pull/9948> for more info.
pub mod foreign_assets_reserves {
	use crate::*;
	use codec::{Decode, Encode, MaxEncodedLen};
	use core::{fmt::Debug, marker::PhantomData};
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		weights::WeightMeter,
	};
	use pallet_assets::WeightInfo;

	const MIGRATIONS_ID: &[u8; 23] = b"foreign-assets-reserves";

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

	/// Trait for plugging in the type that chooses the correct reserves per asset_id for this
	/// migration.
	pub trait ForeignAssetsReservesProvider {
		type ReserveData: Debug;
		fn reserves_for(asset_id: &Location) -> Vec<Self::ReserveData>;
		#[cfg(feature = "try-runtime")]
		fn check_reserves_for(asset_id: &Location, reserves: Vec<Self::ReserveData>) -> bool;
	}

	/// The resulting state of the step and the actual weight consumed.
	type StepResultOf<T, I> = MigrationState<<T as pallet_assets::Config<I>>::AssetId>;

	/// This migration adds the native origin chain as a trusted reserve for already existing
	/// Foreign Assets registered on Asset Hub. Also, for Sibling Parachain foreign assets which
	/// have been teleportable since registration, this migration is used to add the local chain
	/// (`Here` - Asset Hub) as a reserve for existing foreign assets, so as not to change any
	/// existing behaviors.
	///
	/// Newly registered foreign assets will not have any reserves set by default so they will not
	/// be transferable cross-chain unless the asset `owner` also configures trusted reserves for
	/// them, through a dedicated `pallet_assets::set_reserves()` call done post-asset-creation.
	///
	/// `ReservesProvider` implementation shall provide the migration with the actual reserve
	/// locations for each asset_id.
	pub struct ForeignAssetsReservesMigration<T, I, ReservesProvider>(
		PhantomData<(T, I, ReservesProvider)>,
	);
	impl<T, I, ReservesProvider> SteppedMigration
		for ForeignAssetsReservesMigration<T, I, ReservesProvider>
	where
		ReservesProvider: ForeignAssetsReservesProvider,
		T: pallet_assets::Config<
			I,
			AssetId = Location,
			ReserveData = ReservesProvider::ReserveData,
		>,
		I: 'static,
	{
		type Cursor = StepResultOf<T, I>;
		type Identifier = MigrationId<23>;

		fn id() -> Self::Identifier {
			// this migration doesn't change pallet storage version, from and to are both `1`
			MigrationId { pallet_id: *MIGRATIONS_ID, version_from: 1, version_to: 1 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			// Check that we have enough weight for at least the next step. If we don't, then the
			// migration cannot be complete.
			let required =
				<T as pallet_assets::Config<I>>::WeightInfo::migration_v2_foreign_asset_set_reserve_weight();
			tracing::debug!(target: "runtime::ForeignAssetsReservesMigration", ?meter, ?required);
			if !meter.can_consume(required) {
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
						tracing::info!(target: "runtime::ForeignAssetsReservesMigration", "migration finished");
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
			tracing::info!(target: "runtime::ForeignAssetsReservesMigration::pre_upgrade", ?assets);
			let state = TryRuntimeState::<T, I> { assets };
			Ok(state.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let prev_state = TryRuntimeState::<T, I>::decode(&mut &state[..])
				.expect("Failed to decode the previous storage state");
			// let local_chain = Location::here();
			for id in prev_state.assets {
				let reserves = pallet_assets::ReserveLocations::<T, I>::get(id.clone());
				tracing::info!(target: "runtime::ForeignAssetsReservesMigration::post_upgrade", ?id, ?reserves, "verify asset");
				assert!(ReservesProvider::check_reserves_for(&id, reserves.into()));
			}
			Ok(())
		}
	}

	impl<T, I, ReservesProvider> ForeignAssetsReservesMigration<T, I, ReservesProvider>
	where
		ReservesProvider: ForeignAssetsReservesProvider,
		T: pallet_assets::Config<
			I,
			AssetId = Location,
			ReserveData = ReservesProvider::ReserveData,
		>,
		I: 'static,
	{
		fn asset_step(maybe_last_key: Option<&T::AssetId>) -> StepResultOf<T, I> {
			tracing::debug!(target: "runtime::ForeignAssetsReservesMigration::asset_step", ?maybe_last_key);
			let mut iter = if let Some(last_key) = maybe_last_key {
				pallet_assets::Asset::<T, I>::iter_keys_from(
					pallet_assets::Asset::<T, I>::hashed_key_for(last_key),
				)
			} else {
				pallet_assets::Asset::<T, I>::iter_keys()
			};
			if let Some(asset_id) = iter.next() {
				let reserves = ReservesProvider::reserves_for(&asset_id);
				tracing::info!(
					target: "runtime::ForeignAssetsReservesMigration::asset_step",
					?asset_id, ?reserves, "updating reserves for"
				);
				if let Err(e) = pallet_assets::Pallet::<T, I>::unchecked_update_reserves(
					asset_id.clone(),
					reserves,
				) {
					tracing::error!(
						target: "runtime::ForeignAssetsReservesMigration::asset_step",
						?e, ?asset_id, "failed migrating reserves for asset"
					);
				}
				MigrationState::Asset(asset_id)
			} else {
				MigrationState::Finished
			}
		}
	}
}

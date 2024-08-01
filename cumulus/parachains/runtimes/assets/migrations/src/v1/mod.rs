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

//! V1 migration.
//!
//! This migration is meant to upgrade the XCM version of asset locations from V3 to V4.
//! It's only needed if the `AssetId` for this pallet is `VersionedLocation`

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};
use pallet_assets::{Asset, Config};
use xcm::{v3, v4};

pub const PALLET_MIGRATIONS_ID: &[u8; 13] = b"pallet-assets";

/// Storage aliases for on-chain storage types before running the migration.
pub mod old {
	use super::{v3, Config};
	use frame_support::{storage_alias, Blake2_128Concat};
	use pallet_assets::{AssetDetails, DepositBalanceOf, Pallet};

	pub type AssetDetailsOf<T, I> = AssetDetails<
		<T as Config<I>>::Balance,
		<T as frame_system::Config>::AccountId,
		DepositBalanceOf<T, I>,
	>;

	/// The storage item we are migrating from.
	#[storage_alias]
	pub(super) type Asset<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, v3::Location, AssetDetailsOf<T, I>>;
}

pub struct Migration<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> SteppedMigration for Migration<T, I>
where
	<T as Config<I>>::AssetId: From<v4::Location>,
{
	type Cursor = v3::Location;
	type Identifier = MigrationId<13>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		_meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the value corresponding
			// to the last key processed in the previous step of the migration.
			old::Asset::<T, I>::iter_from(old::Asset::<T, I>::hashed_key_for(last_key))
		} else {
			// If there is no cursor, start iterating from the beginning.
			old::Asset::<T, I>::iter()
		};

		if let Some((key, value)) = iter.next() {
			// Most likely all locations will be able to be converted, but if they can't
			// we log them to try again later.
			let maybe_new_key: Result<v4::Location, _> = key.try_into();
			if let Ok(new_key) = maybe_new_key {
				old::Asset::<T, I>::remove(&key);
				let new_key: <T as Config<I>>::AssetId = new_key.into();
				Asset::<T, I>::insert(&new_key, value);
				log::trace!(target: "migration", "Successfully migrated key: {:?}", new_key);
			} else {
				log::warn!(target: "migration", "{:?} couldn't be converted to V4", key);
			}
			// Return the key as the new cursor to continue the migration.
			Ok(Some(key))
		} else {
			// Signal the migration is complete.
			Ok(None)
		}
	}
}

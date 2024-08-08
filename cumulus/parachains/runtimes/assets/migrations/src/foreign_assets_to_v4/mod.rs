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

mod weights;
pub use weights::WeightInfo;

use codec::{Encode, Decode, MaxEncodedLen};
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
	use frame_support::{pallet_prelude::*, traits::Currency, storage_alias};
	use pallet_assets::{AssetDetails, AssetAccount, Approval, AssetMetadata, DepositBalanceOf, Pallet};

	pub type AssetDetailsOf<T, I> = AssetDetails<
		<T as Config<I>>::Balance,
		<T as frame_system::Config>::AccountId,
		DepositBalanceOf<T, I>,
	>;

	pub type AssetAccountOf<T, I> = AssetAccount<
		<T as Config<I>>::Balance,
		DepositBalanceOf<T, I>,
		<T as Config<I>>::Extra,
		<T as frame_system::Config>::AccountId,
	>;

	#[storage_alias]
	pub(super) type Asset<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, v3::Location, AssetDetailsOf<T, I>>;

	#[storage_alias]
	pub(super) type Account<T: Config<I>, I: 'static> =
		StorageDoubleMap<Pallet<T, I>, Blake2_128Concat, v3::Location, Blake2_128Concat, <T as frame_system::Config>::AccountId, AssetAccountOf<T, I>>;

	#[storage_alias]
	pub(super) type Approvals<T: Config<I>, I: 'static> =
		StorageNMap<
			Pallet<T, I>,
			(
				NMapKey<Blake2_128Concat, v3::Location>,
				NMapKey<Blake2_128Concat, <T as frame_system::Config>::AccountId>,
				NMapKey<Blake2_128Concat, <T as frame_system::Config>::AccountId>,
			),
			Approval<<T as Config<I>>::Balance, DepositBalanceOf<T, I>>,
		>;

	#[storage_alias]
	pub(super) type Metadata<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Blake2_128Concat,
		v3::Location,
		AssetMetadata<DepositBalanceOf<T, I>, BoundedVec<u8, <T as Config<I>>::StringLimit>>,
		ValueQuery,
	>;
}

/// Custom cursor for the migration.
///
/// Variant indicates what storage item is being migrated.
/// They all use a `v3::Location` as an internal cursor.
#[derive(Encode, Decode, MaxEncodedLen)]
pub enum CustomCursor {
	Asset(Option<v3::Location>),
	Account(Option<v3::Location>),
	Approvals(Option<v3::Location>),
	Metadata(Option<v3::Location>),
}

pub struct Migration<T, I, W>(PhantomData<(T, I, W)>);
impl<T: Config<I>, I: 'static, W: weights::WeightInfo> SteppedMigration for Migration<T, I, W>
where
	<T as Config<I>>::AssetId: From<v4::Location>,
{
	type Cursor = CustomCursor;
	type Identifier = MigrationId<13>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = W::conversion_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		let mut steps = 0;

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			cursor = match cursor {
				// We start with `convert_asset`.
				None => Self::convert_asset(None),
				Some(CustomCursor::Asset(inner)) => Self::convert_asset(inner),
				Some(CustomCursor::Account(inner)) => Self::convert_account(inner),
				Some(CustomCursor::Approvals(inner)) => Self::convert_approvals(inner),
				Some(CustomCursor::Metadata(inner)) => Self::convert_metadata(inner),
			};
			steps += 1;

			if cursor.is_none() {
				break;
			}
		}

		log::trace!(target: "migrations", "Number of steps: {:?}", steps);

		Ok(cursor)
	}
}

impl<T: Config<I>, I: 'static, W> Migration<T, I, W>
where
	<T as Config<I>>::AssetId: From<v4::Location>,
{
	pub fn convert_asset(cursor: Option<v3::Location>) -> Option<CustomCursor> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the value corresponding
			// to the last key processed in the previous step of the migration.
			old::Asset::<T, I>::iter_from(old::Asset::<T, I>::hashed_key_for(last_key))
		} else {
			// If there is no cursor, start iterating from the beginning.
			old::Asset::<T, I>::iter()
		};

		if let Some((key, value)) = iter.next() {
			log::trace!(target: "migrations", "Migrating an asset");
			// Most likely all locations will be able to be converted, but if they can't
			// we log them to try again later.
			let maybe_new_key: Result<v4::Location, _> = key.try_into();
			if let Ok(new_key) = maybe_new_key {
				let new_key: <T as Config<I>>::AssetId = new_key.into();
				assert_eq!(key.encode(), new_key.encode());
				log::trace!(target: "migration", "New key is equal to old one: {:?}", new_key);
			} else {
				log::warn!(target: "migration", "{:?} couldn't be converted to V4", key);
			}
			// Return the key as the new cursor to continue the migration.
			Some(CustomCursor::Asset(Some(key)))
		} else {
			// Signal we need to migrate the next storage item.
			Some(CustomCursor::Account(None))
		}
	}

	pub fn convert_account(cursor: Option<v3::Location>) -> Option<CustomCursor> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the value corresponding
			// to the last key processed in the previous step of the migration.
			old::Account::<T, I>::iter_from(old::Asset::<T, I>::hashed_key_for(last_key))
		} else {
			// If there is no cursor, start iterating from the beginning.
			old::Account::<T, I>::iter()
		};

		if let Some((key, _, value)) = iter.next() {
			log::trace!(target: "migrations", "Migrating an account");
			// Most likely all locations will be able to be converted, but if they can't
			// we log them to try again later.
			let maybe_new_key: Result<v4::Location, _> = key.try_into();
			if let Ok(new_key) = maybe_new_key {
				let new_key: <T as Config<I>>::AssetId = new_key.into();
				assert_eq!(key.encode(), new_key.encode());
				log::trace!(target: "migration", "New key is equal to old one: {:?}", new_key);
			} else {
				log::warn!(target: "migration", "{:?} couldn't be converted to V4", key);
			}
			// Return the key as the new cursor to continue the migration.
			Some(CustomCursor::Account(Some(key)))
		} else {
			// Signal we need to migrate the next storage item.
			Some(CustomCursor::Approvals(None))
		}
	}

	pub fn convert_approvals(cursor: Option<v3::Location>) -> Option<CustomCursor> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the value corresponding
			// to the last key processed in the previous step of the migration.
			old::Approvals::<T, I>::iter_from(old::Asset::<T, I>::hashed_key_for(last_key))
		} else {
			// If there is no cursor, start iterating from the beginning.
			old::Approvals::<T, I>::iter()
		};

		if let Some(((key, _, _), value)) = iter.next() {
			log::trace!(target: "migrations", "Migrating an approval");
			// Most likely all locations will be able to be converted, but if they can't
			// we log them to try again later.
			let maybe_new_key: Result<v4::Location, _> = key.try_into();
			if let Ok(new_key) = maybe_new_key {
				let new_key: <T as Config<I>>::AssetId = new_key.into();
				assert_eq!(key.encode(), new_key.encode());
				log::trace!(target: "migration", "New key is equal to old one: {:?}", new_key);
			} else {
				log::warn!(target: "migration", "{:?} couldn't be converted to V4", key);
			}
			// Return the key as the new cursor to continue the migration.
			Some(CustomCursor::Approvals(Some(key)))
		} else {
			// Signal we need to migrate the next storage item.
			Some(CustomCursor::Metadata(None))
		}
	}

	pub fn convert_metadata(cursor: Option<v3::Location>) -> Option<CustomCursor> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the value corresponding
			// to the last key processed in the previous step of the migration.
			old::Metadata::<T, I>::iter_from(old::Asset::<T, I>::hashed_key_for(last_key))
		} else {
			// If there is no cursor, start iterating from the beginning.
			old::Metadata::<T, I>::iter()
		};

		if let Some((key, value)) = iter.next() {
			log::trace!(target: "migrations", "Migrating metadata");
			// Most likely all locations will be able to be converted, but if they can't
			// we log them to try again later.
			let maybe_new_key: Result<v4::Location, _> = key.try_into();
			if let Ok(new_key) = maybe_new_key {
				let new_key: <T as Config<I>>::AssetId = new_key.into();
				assert_eq!(key.encode(), new_key.encode());
				log::trace!(target: "migration", "New key is equal to old one: {:?}", new_key);
			} else {
				log::warn!(target: "migration", "{:?} couldn't be converted to V4", key);
			}
			// Return the key as the new cursor to continue the migration.
			Some(CustomCursor::Metadata(Some(key)))
		} else {
			// Signal we need to finish the migration.
			None
		}
	}
}

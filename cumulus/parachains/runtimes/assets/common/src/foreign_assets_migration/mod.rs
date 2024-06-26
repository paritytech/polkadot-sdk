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

//! This migration is meant to upgrade the XCM version of asset locations from V3 to V4.
//! It's only needed if the `AssetId` for this pallet is `VersionedLocation`

use pallet_assets::{Asset, Config};
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
	Hashable,
};

#[cfg(test)]
mod tests;

// TODO: Move this further up.
mod identifier {
	use codec::{Decode, Encode, MaxEncodedLen};

	#[derive(MaxEncodedLen, Encode, Decode)]
	pub struct MigrationIdentifier {
		pub pallet_identifier: [u8; 16],
		pub version_from: u8,
		pub version_to: u8,
	}

	pub const PALLET_MIGRATIONS_ID: &[u8; 13] = b"pallet-assets";
}

use identifier::*;

/// Storage aliases for on-chain storage types before running the migration.
mod old {
	use super::Config;
	use pallet_assets::{
		Pallet,
		AssetDetails, DepositBalanceOf,
	};
	use frame_support::{storage_alias, Blake2_128Concat};

	/// The storage item we are migrating from.
	#[storage_alias]
	pub(super) type Asset<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Blake2_128Concat,
		xcm::v3::Location,
		AssetDetails<
			<T as Config<I>>::Balance,
			<T as frame_system::Config>::AccountId,
			DepositBalanceOf<T, I>,
		>,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);
impl<T: Config<I, AssetId = xcm::v3::Location>, I: 'static> SteppedMigration
	for Migration<T, I>
{
	type Cursor = T::AssetId;
	type Identifier = MigrationIdentifier;

	fn id() -> Self::Identifier {
		MigrationIdentifier {
			pallet_identifier: (*PALLET_MIGRATIONS_ID).twox_128(),
			version_from: 0,
			version_to: 1,
		}
	}

	// TODO: For now I'm letting it run forever, check.
	fn max_steps() -> Option<u32> {
		None
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

		if let Some((key, _value)) = iter.next() {
			// If there is a next item in the iterator, migrate it.
			Asset::<T, I>::remove(key);
			// TODO: Insert the item with a V4 key.
			// Return the processed key as the new cursor to continue the migration.
			Ok(Some(key))
		} else {
			// Signal the migration is complete.
			Ok(None)
		}
	}
}

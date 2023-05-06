// Copyright 2022 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A module that is responsible for migration of storage.

use crate::{Config, Configuration, Overweight, Pallet, DEFAULT_POV_SIZE};
use frame_support::{
	pallet_prelude::*,
	traits::StorageVersion,
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Migrates the pallet storage to the most recent version, checking and setting the
/// `StorageVersion`.
pub fn migrate_to_latest<T: Config>() -> Weight {
	let mut weight = T::DbWeight::get().reads(1);

	if StorageVersion::get::<Pallet<T>>() == 0 {
		weight.saturating_accrue(migrate_to_v1::<T>());
		StorageVersion::new(1).put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));
	}

	if StorageVersion::get::<Pallet<T>>() == 1 {
		weight.saturating_accrue(migrate_to_v2::<T>());
		StorageVersion::new(2).put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));
	}

	weight
}

mod v0 {
	use super::*;
	use codec::{Decode, Encode};

	#[derive(Decode, Encode, Debug)]
	pub struct ConfigData {
		pub max_individual: u64,
	}

	impl Default for ConfigData {
		fn default() -> Self {
			ConfigData { max_individual: 10u64 * WEIGHT_REF_TIME_PER_MILLIS }
		}
	}
}

/// Migrates `QueueConfigData` from v1 (using only reference time weights) to v2 (with
/// 2D weights).
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
pub fn migrate_to_v1<T: Config>() -> Weight {
	let translate = |pre: v0::ConfigData| -> super::ConfigData {
		super::ConfigData {
			max_individual: Weight::from_parts(pre.max_individual, DEFAULT_POV_SIZE),
		}
	};

	if Configuration::<T>::translate(|pre| pre.map(translate)).is_err() {
		log::error!(
			target: "dmp_queue",
			"unexpected error when performing translation of the QueueConfig type during storage upgrade to v2"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

/// Migrates `Overweight` so that it initializes the storage map's counter.
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
pub fn migrate_to_v2<T: Config>() -> Weight {
	let overweight_messages = Overweight::<T>::initialize_counter() as u64;

	T::DbWeight::get().reads_writes(overweight_messages, 1)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{new_test_ext, Test};

	#[test]
	fn test_migration_to_v1() {
		let v0 = v0::ConfigData { max_individual: 30_000_000_000 };

		new_test_ext().execute_with(|| {
			frame_support::storage::unhashed::put_raw(
				&crate::Configuration::<Test>::hashed_key(),
				&v0.encode(),
			);

			migrate_to_v1::<Test>();

			let v1 = crate::Configuration::<Test>::get();

			assert_eq!(v0.max_individual, v1.max_individual.ref_time());
			assert_eq!(v1.max_individual.proof_size(), DEFAULT_POV_SIZE);
		});
	}
}

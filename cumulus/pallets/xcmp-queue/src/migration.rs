// Copyright 2021 Parity Technologies (UK) Ltd.
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

use crate::{Config, Pallet, Store};
use frame_support::{pallet_prelude::*, traits::StorageVersion, weights::Weight};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Migrates the pallet storage to the most recent version, checking and setting the
/// `StorageVersion`.
pub fn migrate_to_latest<T: Config>() -> Weight {
	let mut weight = 0;

	if StorageVersion::get::<Pallet<T>>() == 0 {
		weight += migrate_to_v1::<T>();
		StorageVersion::new(1).put::<Pallet<T>>();
	}

	weight
}

mod v0 {
	use super::*;
	use codec::{Decode, Encode};

	#[derive(Encode, Decode, Debug)]
	pub struct QueueConfigData {
		pub suspend_threshold: u32,
		pub drop_threshold: u32,
		pub resume_threshold: u32,
		pub threshold_weight: Weight,
		pub weight_restrict_decay: Weight,
	}

	impl Default for QueueConfigData {
		fn default() -> Self {
			QueueConfigData {
				suspend_threshold: 2,
				drop_threshold: 5,
				resume_threshold: 1,
				threshold_weight: 100_000,
				weight_restrict_decay: 2,
			}
		}
	}
}

/// Migrates `QueueConfigData` from v0 (without the `xcmp_max_individual_weight` field) to v1 (with
/// max individual weight).
/// Uses the `Default` implementation of `QueueConfigData` to choose a value for
/// `xcmp_max_individual_weight`.
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
pub fn migrate_to_v1<T: Config>() -> Weight {
	let translate = |pre: v0::QueueConfigData| -> super::QueueConfigData {
		super::QueueConfigData {
			suspend_threshold: pre.suspend_threshold,
			drop_threshold: pre.drop_threshold,
			resume_threshold: pre.resume_threshold,
			threshold_weight: pre.threshold_weight,
			weight_restrict_decay: pre.weight_restrict_decay,
			xcmp_max_individual_weight: super::QueueConfigData::default()
				.xcmp_max_individual_weight,
		}
	};

	if let Err(_) = <Pallet<T> as Store>::QueueConfig::translate(|pre| pre.map(translate)) {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the QueueConfig type during storage upgrade to v1"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	fn test_migration_to_v1() {
		let v0 = v0::QueueConfigData {
			suspend_threshold: 5,
			drop_threshold: 12,
			resume_threshold: 3,
			threshold_weight: 333_333,
			weight_restrict_decay: 1,
		};

		new_test_ext().execute_with(|| {
			// Put the v0 version in the state
			frame_support::storage::unhashed::put_raw(
				&crate::QueueConfig::<Test>::hashed_key(),
				&v0.encode(),
			);

			migrate_to_v1::<Test>();

			let v1 = crate::QueueConfig::<Test>::get();

			assert_eq!(v0.suspend_threshold, v1.suspend_threshold);
			assert_eq!(v0.drop_threshold, v1.drop_threshold);
			assert_eq!(v0.resume_threshold, v1.resume_threshold);
			assert_eq!(v0.threshold_weight, v1.threshold_weight);
			assert_eq!(v0.weight_restrict_decay, v1.weight_restrict_decay);
			assert_eq!(v1.xcmp_max_individual_weight, 20_000_000_000);
		});
	}
}

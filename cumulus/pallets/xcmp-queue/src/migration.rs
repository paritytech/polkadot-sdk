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

use crate::{Config, Pallet, Store, DEFAULT_POV_SIZE};
use frame_support::{
	pallet_prelude::*,
	traits::StorageVersion,
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

/// Migrates the pallet storage to the most recent version, checking and setting the
/// `StorageVersion`.
pub fn migrate_to_latest<T: Config>() -> Weight {
	let mut weight = T::DbWeight::get().reads(1);

	if StorageVersion::get::<Pallet<T>>() == 1 {
		weight.saturating_accrue(migrate_to_v2::<T>());
		StorageVersion::new(2).put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));
	}

	if StorageVersion::get::<Pallet<T>>() == 2 {
		weight.saturating_accrue(migrate_to_v3::<T>());
		StorageVersion::new(3).put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));
	}

	weight
}

mod v1 {
	use super::*;
	use codec::{Decode, Encode};

	#[derive(Encode, Decode, Debug)]
	pub struct QueueConfigData {
		pub suspend_threshold: u32,
		pub drop_threshold: u32,
		pub resume_threshold: u32,
		pub threshold_weight: u64,
		pub weight_restrict_decay: u64,
		pub xcmp_max_individual_weight: u64,
	}

	impl Default for QueueConfigData {
		fn default() -> Self {
			QueueConfigData {
				suspend_threshold: 2,
				drop_threshold: 5,
				resume_threshold: 1,
				threshold_weight: 100_000,
				weight_restrict_decay: 2,
				xcmp_max_individual_weight: 20u64 * WEIGHT_REF_TIME_PER_MILLIS,
			}
		}
	}
}

/// Migrates `QueueConfigData` from v1 (using only reference time weights) to v2 (with
/// 2D weights).
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
pub fn migrate_to_v2<T: Config>() -> Weight {
	let translate = |pre: v1::QueueConfigData| -> super::QueueConfigData {
		super::QueueConfigData {
			suspend_threshold: pre.suspend_threshold,
			drop_threshold: pre.drop_threshold,
			resume_threshold: pre.resume_threshold,
			threshold_weight: Weight::from_ref_time(pre.threshold_weight),
			weight_restrict_decay: Weight::from_ref_time(pre.weight_restrict_decay),
			xcmp_max_individual_weight: Weight::from_parts(
				pre.xcmp_max_individual_weight,
				DEFAULT_POV_SIZE,
			),
		}
	};

	if let Err(_) = <Pallet<T> as Store>::QueueConfig::translate(|pre| pre.map(translate)) {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the QueueConfig type during storage upgrade to v2"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

pub fn migrate_to_v3<T: Config>() -> Weight {
	let overweight_messages = <Pallet<T> as Store>::Overweight::initialize_counter() as u64;

	T::DbWeight::get().reads_writes(overweight_messages, 1)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	fn test_migration_to_v2() {
		let v1 = v1::QueueConfigData {
			suspend_threshold: 5,
			drop_threshold: 12,
			resume_threshold: 3,
			threshold_weight: 333_333,
			weight_restrict_decay: 1,
			xcmp_max_individual_weight: 10_000_000_000,
		};

		new_test_ext().execute_with(|| {
			frame_support::storage::unhashed::put_raw(
				&crate::QueueConfig::<Test>::hashed_key(),
				&v1.encode(),
			);

			migrate_to_v2::<Test>();

			let v2 = crate::QueueConfig::<Test>::get();

			assert_eq!(v1.suspend_threshold, v2.suspend_threshold);
			assert_eq!(v1.drop_threshold, v2.drop_threshold);
			assert_eq!(v1.resume_threshold, v2.resume_threshold);
			assert_eq!(v1.threshold_weight, v2.threshold_weight.ref_time());
			assert_eq!(v1.weight_restrict_decay, v2.weight_restrict_decay.ref_time());
			assert_eq!(v1.xcmp_max_individual_weight, v2.xcmp_max_individual_weight.ref_time());
		});
	}
}

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{
	Config, OutboundState, OutboundXcmpMessages, OutboundXcmpStatus, Overweight, Pallet, ParaId,
	QueueConfig, DEFAULT_POV_SIZE,
};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

/// Migrates the pallet storage to the most recent version.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut weight = T::DbWeight::get().reads(1);
		let storage_version = StorageVersion::get::<Pallet<T>>();

		if storage_version == 1 {
			weight.saturating_accrue(migrate_to_v2::<T>());
			StorageVersion::new(2).put::<Pallet<T>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		}

		if storage_version == 2 {
			weight.saturating_accrue(migrate_to_v3::<T>());
			StorageVersion::new(3).put::<Pallet<T>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		}

		if storage_version == 3 {
			weight.saturating_accrue(migrate_to_v4::<T>());
			StorageVersion::new(4).put::<Pallet<T>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		}

		weight
	}
}

mod v3 {
	use super::*;
	use codec::{Decode, Encode};
	use frame_support::storage_alias;

	#[derive(Encode, Decode, Debug)]
	pub struct OutboundChannelDetails {
		pub recipient: ParaId,
		pub state: OutboundState,
		pub signals_exist: bool,
		pub first_index: u16,
		pub last_index: u16,
	}

	#[storage_alias]
	pub type OutboundXcmpMessages<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ParaId,
		Twox64Concat,
		u16,
		Vec<u8>,
		ValueQuery,
	>;
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
			threshold_weight: Weight::from_parts(pre.threshold_weight, 0),
			weight_restrict_decay: Weight::from_parts(pre.weight_restrict_decay, 0),
			xcmp_max_individual_weight: Weight::from_parts(
				pre.xcmp_max_individual_weight,
				DEFAULT_POV_SIZE,
			),
		}
	};

	if QueueConfig::<T>::translate(|pre| pre.map(translate)).is_err() {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the QueueConfig type during storage upgrade to v2"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

pub fn migrate_to_v3<T: Config>() -> Weight {
	let overweight_messages = Overweight::<T>::initialize_counter() as u64;

	T::DbWeight::get().reads_writes(overweight_messages, 1)
}

pub fn migrate_to_v4<T: Config>() -> Weight {
	let translate = |pre: Vec<v3::OutboundChannelDetails>| -> Vec<super::OutboundChannelDetails> {
		pre.into_iter()
			.map(|details| super::OutboundChannelDetails {
				recipient: details.recipient,
				state: details.state,
				signals_exist: details.signals_exist,
				first_index: details.first_index as u64,
				last_index: details.last_index as u64,
			})
			.collect()
	};

	if OutboundXcmpStatus::<T>::translate(|pre| pre.map(translate)).is_err() {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the OutboundXcmpStatus type during storage upgrade to v4"
		);
	}

	let mut weight = T::DbWeight::get().reads_writes(1, 1);

	for (paraid, idx, msg) in v3::OutboundXcmpMessages::<T>::drain() {
		OutboundXcmpMessages::<T>::insert(paraid, idx as u64, msg);
		weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 2));
	}

	weight
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

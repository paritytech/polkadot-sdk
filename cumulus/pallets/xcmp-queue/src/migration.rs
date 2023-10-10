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
	pallet::QueueConfig, Config, OverweightIndex, Pallet, ParaId, QueueConfigData, DEFAULT_POV_SIZE,
};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

pub const LOG: &str = "xcmp-queue-migration";

/// Migrates the pallet storage to the most recent version.
pub struct MigrationToV3<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrationToV3<T> {
	fn on_runtime_upgrade() -> Weight {
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

		if StorageVersion::get::<Pallet<T>>() == 3 {
			weight.saturating_accrue(migrate_to_v4::<T>());
			StorageVersion::new(4).put::<Pallet<T>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		}

		weight
	}
}

mod v1 {
	use super::*;
	use codec::{Decode, Encode};

	#[frame_support::storage_alias]
	pub(crate) type QueueConfig<T: Config> = StorageValue<Pallet<T>, QueueConfigData, ValueQuery>;

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

mod v2 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type QueueConfig<T: Config> = StorageValue<Pallet<T>, QueueConfigData, ValueQuery>;

	#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct QueueConfigData {
		pub suspend_threshold: u32,
		pub drop_threshold: u32,
		pub resume_threshold: u32,
		#[deprecated(note = "Will be removed")]
		pub threshold_weight: Weight,
		#[deprecated(note = "Will be removed")]
		pub weight_restrict_decay: Weight,
		#[deprecated(note = "Will be removed")]
		pub xcmp_max_individual_weight: Weight,
	}

	impl Default for QueueConfigData {
		fn default() -> Self {
			#![allow(deprecated)]
			Self {
				drop_threshold: 48,    // 64KiB * 48 = 3MiB
				suspend_threshold: 32, // 64KiB * 32 = 2MiB
				resume_threshold: 8,   // 64KiB * 8 = 512KiB
				// unused:
				threshold_weight: Weight::from_parts(100_000, 0),
				weight_restrict_decay: Weight::from_parts(2, 0),
				xcmp_max_individual_weight: Weight::from_parts(
					20u64 * WEIGHT_REF_TIME_PER_MILLIS,
					DEFAULT_POV_SIZE,
				),
			}
		}
	}
}

/// Migrates `QueueConfigData` from v1 (using only reference time weights) to v2 (with
/// 2D weights).
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
#[allow(deprecated)]
pub fn migrate_to_v2<T: Config>() -> Weight {
	let translate = |pre: v1::QueueConfigData| -> v2::QueueConfigData {
		v2::QueueConfigData {
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

	if v2::QueueConfig::<T>::translate(|pre| pre.map(translate)).is_err() {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the QueueConfig type \
			during storage upgrade to v2"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

pub mod v3 {
	use super::*;
	use crate::*;

	/// Status of the inbound XCMP channels.
	#[frame_support::storage_alias]
	pub(crate) type InboundXcmpStatus<T: Config> =
		StorageValue<Pallet<T>, Vec<InboundChannelDetails>, OptionQuery>;

	/// Inbound aggregate XCMP messages. It can only be one per ParaId/block.
	#[frame_support::storage_alias]
	pub(crate) type InboundXcmpMessages<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ParaId,
		Twox64Concat,
		RelayBlockNumber,
		Vec<u8>,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub(crate) type QueueConfig<T: Config> =
		StorageValue<Pallet<T>, v2::QueueConfigData, ValueQuery>;

	#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, TypeInfo)]
	pub struct InboundChannelDetails {
		/// The `ParaId` of the parachain that this channel is connected with.
		pub sender: ParaId,
		/// The state of the channel.
		pub state: InboundState,
		/// The ordered metadata of each inbound message.
		///
		/// Contains info about the relay block number that the message was sent at, and the format
		/// of the incoming message.
		pub message_metadata: Vec<(RelayBlockNumber, XcmpMessageFormat)>,
	}

	#[derive(
		Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug, TypeInfo,
	)]
	pub enum InboundState {
		Ok,
		Suspended,
	}
}

pub fn migrate_to_v3<T: Config>() -> Weight {
	#[frame_support::storage_alias]
	type Overweight<T: Config> =
		CountedStorageMap<Pallet<T>, Twox64Concat, OverweightIndex, ParaId>;
	let overweight_messages = Overweight::<T>::initialize_counter() as u64;

	T::DbWeight::get().reads_writes(overweight_messages, 1)
}

/// Migrates `QueueConfigData` from v2 to v4, removing deprecated fields and bumping page
/// thresholds to at least the default values.
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
#[allow(deprecated)]
pub fn migrate_to_v4<T: Config>() -> Weight {
	let translate = |pre: v2::QueueConfigData| -> QueueConfigData {
		use std::cmp::max;

		let default = QueueConfigData::default();
		let post = QueueConfigData {
			suspend_threshold: max(pre.suspend_threshold, default.suspend_threshold),
			drop_threshold: max(pre.drop_threshold, default.drop_threshold),
			resume_threshold: max(pre.resume_threshold, default.resume_threshold),
		};
		post
	};

	if QueueConfig::<T>::translate(|pre| pre.map(translate)).is_err() {
		log::error!(
			target: super::LOG_TARGET,
			"unexpected error when performing translation of the QueueConfig type \
			during storage upgrade to v4"
		);
	}

	T::DbWeight::get().reads_writes(1, 1)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	#[allow(deprecated)]
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

			let v2 = v2::QueueConfig::<Test>::get();

			assert_eq!(v1.suspend_threshold, v2.suspend_threshold);
			assert_eq!(v1.drop_threshold, v2.drop_threshold);
			assert_eq!(v1.resume_threshold, v2.resume_threshold);
			assert_eq!(v1.threshold_weight, v2.threshold_weight.ref_time());
			assert_eq!(v1.weight_restrict_decay, v2.weight_restrict_decay.ref_time());
			assert_eq!(v1.xcmp_max_individual_weight, v2.xcmp_max_individual_weight.ref_time());
		});
	}

	#[test]
	#[allow(deprecated)]
	fn test_migration_to_v4() {
		new_test_ext().execute_with(|| {
			let v2 = v2::QueueConfigData {
				drop_threshold: 5,
				suspend_threshold: 2,
				resume_threshold: 1,
				..Default::default()
			};

			frame_support::storage::unhashed::put_raw(
				&crate::QueueConfig::<Test>::hashed_key(),
				&v2.encode(),
			);

			migrate_to_v4::<Test>();

			let v4 = QueueConfig::<Test>::get();

			assert_eq!(
				v4,
				QueueConfigData { suspend_threshold: 32, drop_threshold: 48, resume_threshold: 8 }
			);
		});

		new_test_ext().execute_with(|| {
			let v2 = v2::QueueConfigData {
				drop_threshold: 100,
				suspend_threshold: 50,
				resume_threshold: 40,
				..Default::default()
			};

			frame_support::storage::unhashed::put_raw(
				&crate::QueueConfig::<Test>::hashed_key(),
				&v2.encode(),
			);

			migrate_to_v4::<Test>();

			let v4 = QueueConfig::<Test>::get();

			assert_eq!(
				v4,
				QueueConfigData {
					suspend_threshold: 50,
					drop_threshold: 100,
					resume_threshold: 40
				}
			);
		});
	}
}

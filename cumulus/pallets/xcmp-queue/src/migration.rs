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

use crate::{Config, OverweightIndex, Pallet, ParaId, QueueConfig, DEFAULT_POV_SIZE};
use cumulus_primitives_core::XcmpMessageFormat;
use frame_support::{
	pallet_prelude::*,
	traits::{EnqueueMessage, OnRuntimeUpgrade, StorageVersion},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

pub const LOG: &str = "runtime::xcmp-queue-migration";

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

		weight
	}
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

/// Migrates `QueueConfigData` from v1 (using only reference time weights) to v2 (with
/// 2D weights).
///
/// NOTE: Only use this function if you know what you're doing. Default to using
/// `migrate_to_latest`.
#[allow(deprecated)]
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
	#[frame_support::storage_alias]
	type Overweight<T: Config> =
		CountedStorageMap<Pallet<T>, Twox64Concat, OverweightIndex, ParaId>;
	let overweight_messages = Overweight::<T>::initialize_counter() as u64;

	T::DbWeight::get().reads_writes(overweight_messages, 1)
}

pub fn lazy_migrate_inbound_queue<T: Config>() {
	let Some(mut states) = v3::InboundXcmpStatus::<T>::get() else {
		log::debug!(target: LOG, "Lazy migration finished: item gone");
		return
	};
	let Some(ref mut next) = states.first_mut() else {
		log::debug!(target: LOG, "Lazy migration finished: item empty");
		v3::InboundXcmpStatus::<T>::kill();
		return
	};
	log::debug!(
		"Migrating inbound HRMP channel with sibling {:?}, msgs left {}.",
		next.sender,
		next.message_metadata.len()
	);
	// We take the last element since the MQ is a FIFO and we want to keep the order.
	let Some((block_number, format)) = next.message_metadata.pop() else {
		states.remove(0);
		v3::InboundXcmpStatus::<T>::put(states);
		return
	};
	if format != XcmpMessageFormat::ConcatenatedVersionedXcm {
		log::warn!(target: LOG,
			"Dropping message with format {:?} (not ConcatenatedVersionedXcm)",
			format
		);
		v3::InboundXcmpMessages::<T>::remove(&next.sender, &block_number);
		v3::InboundXcmpStatus::<T>::put(states);
		return
	}

	let Some(msg) = v3::InboundXcmpMessages::<T>::take(&next.sender, &block_number) else {
		defensive!("Storage corrupted: HRMP message missing:", (next.sender, block_number));
		v3::InboundXcmpStatus::<T>::put(states);
		return
	};

	let Ok(msg): Result<BoundedVec<_, _>, _> = msg.try_into() else {
		log::error!(target: LOG, "Message dropped: too big");
		v3::InboundXcmpStatus::<T>::put(states);
		return
	};

	// Finally! We have a proper message.
	T::XcmpQueue::enqueue_message(msg.as_bounded_slice(), next.sender);
	log::debug!(target: LOG, "Migrated HRMP message to MQ: {:?}", (next.sender, block_number));
	v3::InboundXcmpStatus::<T>::put(states);
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

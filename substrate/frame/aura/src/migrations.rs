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

//! Migrations for the AURA pallet.

use frame_support::{pallet_prelude::*, traits::Get, weights::Weight};
use sp_consensus_aura::Slot;
use sp_runtime::traits::SaturatedConversion;

struct __LastTimestamp<T>(core::marker::PhantomData<T>);
impl<T: RemoveLastTimestamp> frame_support::traits::StorageInstance for __LastTimestamp<T> {
	fn pallet_prefix() -> &'static str {
		T::PalletPrefix::get()
	}
	const STORAGE_PREFIX: &'static str = "LastTimestamp";
}

type LastTimestamp<T> = StorageValue<__LastTimestamp<T>, (), ValueQuery>;

pub trait RemoveLastTimestamp: super::Config {
	type PalletPrefix: Get<&'static str>;
}

/// Remove the `LastTimestamp` storage value.
///
/// This storage value was removed and replaced by `CurrentSlot`. As we only remove this storage
/// value, it is safe to call this method multiple times.
///
/// This migration requires a type `T` that implements [`RemoveLastTimestamp`].
pub fn remove_last_timestamp<T: RemoveLastTimestamp>() -> Weight {
	LastTimestamp::<T>::kill();
	T::DbWeight::get().writes(1)
}

/// Migrate `CurrentSlot` to account for a slot duration change.
///
/// This migration recalculates the current slot based on the current timestamp and the new
/// slot duration. This is necessary when upgrading to a runtime with a different slot duration,
/// as the slot number calculated from the timestamp using the new duration may be less than
/// the previously stored slot, which would cause a panic in `on_initialize`.
///
/// The migration:
/// 1. Gets the current timestamp from `pallet_timestamp`
/// 2. Gets the new slot duration from the runtime configuration
/// 3. Calculates the new slot: `timestamp / slot_duration`
/// 4. Updates `CurrentSlot` with the recalculated value
///
/// This migration is safe to call multiple times (idempotent).
pub struct MigrateCurrentSlot<T>(core::marker::PhantomData<T>);

impl<T: super::Config> frame_support::traits::OnRuntimeUpgrade for MigrateCurrentSlot<T> {
	fn on_runtime_upgrade() -> Weight {
		use super::pallet::CurrentSlot;
		use pallet_timestamp::Pallet as Timestamp;

		let current_timestamp = Timestamp::<T>::get();
		let new_slot_duration = T::SlotDuration::get();

		if new_slot_duration.is_zero() {
			log::error!(
				target: "runtime::aura::migration",
				"Slot duration is zero, cannot migrate CurrentSlot"
			);
			return T::DbWeight::get().reads(2)
		}

		let new_slot = current_timestamp / new_slot_duration;
		let new_slot = Slot::from(new_slot.saturated_into::<u64>());

		let old_slot = CurrentSlot::<T>::get();
		CurrentSlot::<T>::put(new_slot);

		log::info!(
			target: "runtime::aura::migration",
			"Migrated CurrentSlot from {} to {} (timestamp: {:?}, slot_duration: {:?})",
			u64::from(old_slot),
			u64::from(new_slot),
			current_timestamp,
			new_slot_duration
		);

		T::DbWeight::get().reads_writes(2, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use super::pallet::CurrentSlot;
		use pallet_timestamp::Pallet as Timestamp;

		let current_timestamp = Timestamp::<T>::get();
		let new_slot_duration = T::SlotDuration::get();
		let old_slot = CurrentSlot::<T>::get();

		if new_slot_duration.is_zero() {
			return Err("Slot duration is zero".into())
		}

		let new_slot = current_timestamp / new_slot_duration;
		let new_slot = Slot::from(new_slot.saturated_into::<u64>());

		log::info!(
			target: "runtime::aura::migration",
			"Pre-upgrade: CurrentSlot will be migrated from {} to {}",
			u64::from(old_slot),
			u64::from(new_slot)
		);

		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use super::pallet::CurrentSlot;
		use pallet_timestamp::Pallet as Timestamp;

		let current_timestamp = Timestamp::<T>::get();
		let new_slot_duration = T::SlotDuration::get();
		let current_slot = CurrentSlot::<T>::get();

		let expected_slot = current_timestamp / new_slot_duration;
		let expected_slot = Slot::from(expected_slot.saturated_into::<u64>());

		frame_support::ensure!(
			current_slot == expected_slot,
			"CurrentSlot migration failed: expected {}, got {}",
			u64::from(expected_slot),
			u64::from(current_slot)
		);

		log::info!(
			target: "runtime::aura::migration",
			"Post-upgrade: CurrentSlot is correctly set to {}",
			u64::from(current_slot)
		);

		Ok(())
	}
}

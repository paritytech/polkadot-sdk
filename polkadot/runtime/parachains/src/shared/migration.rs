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

use super::*;
use frame_support::{
	pallet_prelude::ValueQuery, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use polkadot_primitives::vstaging::RelayParentInfo;

#[cfg(feature = "try-runtime")]
const LOG_TARGET: &str = "runtime::shared";

pub mod v1 {
	use super::*;
	use alloc::collections::vec_deque::VecDeque;
	use codec::{Decode, Encode};
	use frame_support::storage_alias;

	/// The old `AllowedRelayParents` storage at version 1 (a StorageValue).
	/// This occupied the storage key `twox128("ParasShared") ++ twox128("AllowedRelayParents")`.
	/// In v2, this storage name is reused as a StorageDoubleMap, so the old value must be removed.
	#[storage_alias]
	pub(crate) type AllowedRelayParents<T: Config> = StorageValue<
		Pallet<T>,
		AllowedRelayParentsTracker<<T as frame_system::Config>::Hash, BlockNumberFor<T>>,
		ValueQuery,
	>;

	/// The v1 relay parent info stored in the old tracker's buffer.
	#[derive(Encode, Decode, Default, TypeInfo, Debug)]
	pub struct RelayParentInfo<Hash> {
		pub relay_parent: Hash,
		pub state_root: Hash,
		pub claim_queue: BTreeMap<Id, BTreeMap<u8, BTreeSet<CoreIndex>>>,
	}

	/// The v1 allowed relay parents tracker (StorageValue format).
	#[derive(Encode, Decode, Default, TypeInfo)]
	pub struct AllowedRelayParentsTracker<Hash, BlockNumber> {
		pub buffer: VecDeque<RelayParentInfo<Hash>>,
		pub latest_number: BlockNumber,
	}

	impl<Hash: PartialEq, BlockNumber: AtLeast32BitUnsigned + Copy>
		AllowedRelayParentsTracker<Hash, BlockNumber>
	{
		pub(crate) fn hypothetical_earliest_block_number(
			&self,
			now: BlockNumber,
			max_ancestry_len: u32,
		) -> BlockNumber {
			let allowed_ancestry_len = max_ancestry_len.min(self.buffer.len() as u32);

			now - allowed_ancestry_len.into()
		}

		pub(crate) fn get_number(&self, relay_parent: Hash) -> Option<BlockNumber> {
			let pos = self.buffer.iter().position(|info| info.relay_parent == relay_parent)?;
			let age = (self.buffer.len() - 1) - pos;
			let number = self.latest_number - BlockNumber::from(age as u32);

			Some(number)
		}
	}
}

mod v2 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	use frame_support::{
		ensure,
		traits::{GetStorageVersion, StorageVersion},
	};

	pub struct VersionUncheckedMigrateToV2<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV2<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			log::trace!(target: LOG_TARGET, "Running pre_upgrade() for shared MigrateToV2");

			let old_tracker = v1::AllowedRelayParents::<T>::get();
			let buf_len = old_tracker.buffer.len() as u32;
			log::trace!(
				target: LOG_TARGET,
				"Old AllowedRelayParents tracker has {} entries",
				buf_len
			);

			Ok(buf_len.to_ne_bytes().to_vec())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			// Remove the old AllowedRelayParents StorageValue (v1 format).
			let old_tracker = v1::AllowedRelayParents::<T>::take();
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

			let latest_number = old_tracker.latest_number;
			let buf_len = old_tracker.buffer.len();

			// Convert v1 AllowedRelayParentsTracker to new AllowedSchedulingParentsTracker.
			// The scheduling tracker keeps: scheduling_parent (was relay_parent), claim_queue.
			// It drops: state_root (now stored in the relay parent DoubleMap).
			let mut new_buffer: VecDeque<SchedulingParentInfo<T::Hash>> =
				VecDeque::with_capacity(buf_len);

			// Populate the new AllowedRelayParents DoubleMap from the old tracker entries.
			// All existing entries are from the current session (the old tracker was cleared
			// on session changes). Block numbers are derived from buffer position and
			// latest_number.
			let current_session = CurrentSessionIndex::<T>::get();
			weight = weight.saturating_add(T::DbWeight::get().reads(1));

			for (idx, info) in old_tracker.buffer.into_iter().enumerate() {
				// Compute block number from position in the buffer.
				let age = (buf_len - 1) - idx;
				let block_number = latest_number - BlockNumberFor::<T>::from(age as u32);

				// Insert into the new AllowedRelayParents DoubleMap.
				AllowedRelayParents::<T>::insert(
					current_session,
					info.relay_parent,
					RelayParentInfo { number: block_number, state_root: info.state_root },
				);

				// Build the scheduling parents buffer entry.
				new_buffer.push_back(SchedulingParentInfo {
					scheduling_parent: info.relay_parent,
					claim_queue: info.claim_queue,
				});
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(buf_len as u64));

			AllowedSchedulingParents::<T>::set(AllowedSchedulingParentsTracker {
				buffer: new_buffer,
				latest_number,
			});
			weight = weight.saturating_add(T::DbWeight::get().writes(1));

			OldestRelayParentSession::<T>::set(current_session);
			weight = weight.saturating_add(T::DbWeight::get().writes(1));

			// Initialize MinimumRelayParentNumber for the current session.
			// The oldest entry in the buffer has the smallest block number.
			if buf_len > 0 {
				let min_block_number =
					latest_number - BlockNumberFor::<T>::from((buf_len - 1) as u32);
				MinimumRelayParentNumber::<T>::insert(current_session, min_block_number);
			} else {
				MinimumRelayParentNumber::<T>::insert(current_session, latest_number);
			}

			weight = weight.saturating_add(T::DbWeight::get().writes(1));

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::trace!(target: LOG_TARGET, "Running post_upgrade() for shared MigrateToV2");

			ensure!(
				Pallet::<T>::on_chain_storage_version() >= StorageVersion::new(2),
				"Storage version should be >= 2 after the migration"
			);

			let old_buf_len = u32::from_ne_bytes(
				state
					.try_into()
					.expect("u32::from_ne_bytes(to_ne_bytes(u32)) always works; qed"),
			);

			// The scheduling parents tracker should have the same number of entries.
			let new_tracker = AllowedSchedulingParents::<T>::get();
			ensure!(
				old_buf_len as usize == new_tracker.buffer.len(),
				"AllowedSchedulingParents buffer length should match the old tracker"
			);

			// The old AllowedRelayParents StorageValue should be gone (taken).
			ensure!(
				v1::AllowedRelayParents::<T>::get().buffer.is_empty(),
				"Old AllowedRelayParents StorageValue should be empty after migration"
			);

			// OldestRelayParentSession should be set.
			let oldest = OldestRelayParentSession::<T>::get();
			let current = CurrentSessionIndex::<T>::get();
			ensure!(
				oldest == current,
				"OldestRelayParentSession should equal current session after migration"
			);

			// The AllowedRelayParents DoubleMap should have the same number of entries.
			let double_map_count = AllowedRelayParents::<T>::iter_prefix(current).count();
			ensure!(
				old_buf_len as usize == double_map_count,
				"AllowedRelayParents DoubleMap should have the same number of entries as the old tracker"
			);

			// MinimumRelayParentNumber should be set if the buffer was non-empty.
			if old_buf_len > 0 {
				ensure!(
					MinimumRelayParentNumber::<T>::contains_key(current),
					"MinimumRelayParentNumber should be set for current session after migration"
				);
			}

			Ok(())
		}
	}
}

/// Migrate shared module storage from v1 to v2.
pub type MigrateToV2<T> = frame_support::migrations::VersionedMigration<
	1,
	2,
	v2::VersionUncheckedMigrateToV2<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::{v1, v2::VersionUncheckedMigrateToV2, *};
	use crate::mock::{new_test_ext, MockGenesisConfig, Test};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use polkadot_primitives::Hash;

	#[test]
	fn migrate_v1_to_v2() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Set up v1 state: populate the old AllowedRelayParents StorageValue.
			let old_tracker = v1::AllowedRelayParentsTracker {
				latest_number: 200u32,
				buffer: (0..10u32)
					.map(|idx| v1::RelayParentInfo {
						relay_parent: Hash::from_low_u64_ne(idx as u64),
						state_root: Hash::from_low_u64_ne(100 + idx as u64),
						claim_queue: [(Id::from(idx), BTreeMap::new())].into_iter().collect(),
					})
					.collect(),
			};
			v1::AllowedRelayParents::<Test>::put(old_tracker);

			// Set session index.
			let session_index = 5;
			CurrentSessionIndex::<Test>::set(session_index);

			// Run migration.
			<VersionUncheckedMigrateToV2<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			// Verify AllowedSchedulingParents was populated correctly.
			let new_tracker = AllowedSchedulingParents::<Test>::get();
			assert_eq!(new_tracker.buffer.len(), 10);
			assert_eq!(new_tracker.latest_number, 200u32);

			for idx in 0..10u32 {
				let expected_hash = Hash::from_low_u64_ne(idx as u64);
				assert_eq!(new_tracker.buffer[idx as usize].scheduling_parent, expected_hash);
				let expected_cq = [(Id::from(idx), BTreeMap::new())].into_iter().collect();
				assert_eq!(new_tracker.buffer[idx as usize].claim_queue, expected_cq);
			}

			// Verify old AllowedRelayParents StorageValue was removed.
			assert!(v1::AllowedRelayParents::<Test>::get().buffer.is_empty());

			// Verify OldestRelayParentSession was initialized.
			assert_eq!(OldestRelayParentSession::<Test>::get(), 5);

			// Verify AllowedRelayParents DoubleMap was populated with all entries
			// under the current session (5).
			assert_eq!(AllowedRelayParents::<Test>::iter_prefix(session_index).count(), 10);
			assert_eq!(AllowedRelayParents::<Test>::iter().count(), 10);

			for idx in 0..10u32 {
				let relay_parent = Hash::from_low_u64_ne(idx as u64);
				let expected_state_root = Hash::from_low_u64_ne(100 + idx as u64);

				let info = AllowedRelayParents::<Test>::get(session_index, relay_parent)
					.expect("relay parent should be in DoubleMap");
				assert_eq!(info.state_root, expected_state_root);
				assert_eq!(info.number, 200 - 10 + idx + 1);
			}

			// Verify the MinimumRelayParentNumber was set correctly.
			assert_eq!(MinimumRelayParentNumber::<Test>::get(session_index).unwrap(), 191);
		});
	}

	#[test]
	fn migrate_v1_to_v2_empty_tracker() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// v1 state with empty tracker.
			v1::AllowedRelayParents::<Test>::put(v1::AllowedRelayParentsTracker::<Hash, u32> {
				buffer: Default::default(),
				latest_number: 300,
			});

			CurrentSessionIndex::<Test>::set(1);

			<VersionUncheckedMigrateToV2<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			let new_tracker = AllowedSchedulingParents::<Test>::get();
			assert!(new_tracker.buffer.is_empty());
			assert_eq!(OldestRelayParentSession::<Test>::get(), 1);

			assert_eq!(AllowedRelayParents::<Test>::iter().count(), 0);

			// Verify the MinimumRelayParentNumber was set correctly.
			assert_eq!(MinimumRelayParentNumber::<Test>::get(1).unwrap(), 300);
		});
	}
}

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
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::ValueQuery, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

#[cfg(feature = "try-runtime")]
const LOG_TARGET: &str = "runtime::shared";

pub mod v0 {
	use super::*;
	use alloc::collections::vec_deque::VecDeque;

	use frame_support::storage_alias;

	/// All allowed relay-parents storage at version 0.
	#[storage_alias]
	pub(crate) type AllowedRelayParents<T: Config> = StorageValue<
		Pallet<T>,
		super::v0::AllowedRelayParentsTracker<<T as frame_system::Config>::Hash, BlockNumberFor<T>>,
		ValueQuery,
	>;

	#[derive(Encode, Decode, Default, TypeInfo)]
	pub struct AllowedRelayParentsTracker<Hash, BlockNumber> {
		// The past relay parents, paired with state roots, that are viable to build upon.
		//
		// They are in ascending chronologic order, so the newest relay parents are at
		// the back of the deque.
		//
		// (relay_parent, state_root)
		pub buffer: VecDeque<(Hash, Hash)>,

		// The number of the most recent relay-parent, if any.
		// If the buffer is empty, this value has no meaning and may
		// be nonsensical.
		pub latest_number: BlockNumber,
	}

	// Required to workaround #64.
	impl<Hash: PartialEq + Copy, BlockNumber: AtLeast32BitUnsigned + Copy>
		AllowedRelayParentsTracker<Hash, BlockNumber>
	{
		/// Returns block number of the earliest block the buffer would contain if
		/// `now` is pushed into it.
		pub(crate) fn hypothetical_earliest_block_number(
			&self,
			now: BlockNumber,
			max_ancestry_len: u32,
		) -> BlockNumber {
			let allowed_ancestry_len = max_ancestry_len.min(self.buffer.len() as u32);

			now - allowed_ancestry_len.into()
		}
	}

	impl<Hash, BlockNumber> From<AllowedRelayParentsTracker<Hash, BlockNumber>>
		for super::AllowedRelayParentsTracker<Hash, BlockNumber>
	{
		fn from(value: AllowedRelayParentsTracker<Hash, BlockNumber>) -> Self {
			Self {
				latest_number: value.latest_number,
				buffer: value
					.buffer
					.into_iter()
					.map(|(relay_parent, state_root)| super::RelayParentInfo {
						relay_parent,
						state_root,
						claim_queue: Default::default(),
					})
					.collect(),
			}
		}
	}
}

mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	use frame_support::{
		ensure,
		traits::{GetStorageVersion, StorageVersion},
	};

	pub struct VersionUncheckedMigrateToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			log::trace!(target: LOG_TARGET, "Running pre_upgrade() for shared MigrateToV1");
			let bytes = u32::to_ne_bytes(v0::AllowedRelayParents::<T>::get().buffer.len() as u32);

			Ok(bytes.to_vec())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			// Read old storage.
			let old_rp_tracker = v0::AllowedRelayParents::<T>::take();

			super::AllowedRelayParents::<T>::set(old_rp_tracker.into());

			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::trace!(target: LOG_TARGET, "Running post_upgrade() for shared MigrateToV1");
			ensure!(
				Pallet::<T>::on_chain_storage_version() >= StorageVersion::new(1),
				"Storage version should be >= 1 after the migration"
			);

			let relay_parent_count = u32::from_ne_bytes(
				state
					.try_into()
					.expect("u32::from_ne_bytes(to_ne_bytes(u32)) always works; qed"),
			);

			let rp_tracker = AllowedRelayParents::<T>::get();

			ensure!(
				relay_parent_count as usize == rp_tracker.buffer.len(),
				"Number of allowed relay parents should be the same as the one before the upgrade."
			);

			Ok(())
		}
	}
}

/// Migrate shared module storage to v1.
pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::VersionUncheckedMigrateToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::{v1::VersionUncheckedMigrateToV1, *};
	use crate::mock::{new_test_ext, MockGenesisConfig, Test};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use polkadot_primitives::Hash;

	#[test]
	fn migrate_to_v1() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let rp_tracker = v0::AllowedRelayParentsTracker {
				latest_number: 9,
				buffer: (0..10u64)
					.into_iter()
					.map(|idx| (Hash::from_low_u64_ne(idx), Hash::from_low_u64_ne(2 * idx)))
					.collect::<VecDeque<_>>(),
			};

			v0::AllowedRelayParents::<Test>::put(rp_tracker);

			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			let rp_tracker = AllowedRelayParents::<Test>::get();

			assert_eq!(rp_tracker.buffer.len(), 10);

			for idx in 0..10u64 {
				let relay_parent = Hash::from_low_u64_ne(idx);
				let state_root = Hash::from_low_u64_ne(2 * idx);
				let (info, block_num) = rp_tracker.acquire_info(relay_parent, None).unwrap();

				assert!(info.claim_queue.is_empty());
				assert_eq!(info.relay_parent, relay_parent);
				assert_eq!(info.state_root, state_root);
				assert_eq!(block_num as u64, idx);
			}
		});
	}
}

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

use super::*;
use core::marker::PhantomData;
use frame_support::{
	storage_alias,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;

pub mod v1 {
	use super::*;

	pub struct MigrateToV1Impl<T>(PhantomData<T>);

	#[storage_alias]
	type ChildBountyDescriptions<T: Config + pallet_bounties::Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		BountyIndex,
		BoundedVec<u8, <T as pallet_bounties::Config>::MaximumReasonLength>,
	>;

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateToV1Impl<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			// increment reads/writes after the action
			let mut reads = 0u64;
			let mut writes = 0u64;

			let mut old_bounty_ids = Vec::new();
			// first iteration collect all existing ids not to mutate map as we iterate it
			for (parent_bounty_id, old_child_bounty_id) in ChildBounties::<T>::iter_keys() {
				reads += 1;
				old_bounty_ids.push((parent_bounty_id, old_child_bounty_id));
			}

			log::info!(
				target: LOG_TARGET,
				"Migrating {} child bounties",
				old_bounty_ids.len(),
			);

			for (parent_bounty_id, old_child_bounty_id) in old_bounty_ids {
				// assign new child bounty id
				let new_child_bounty_id = ParentTotalChildBounties::<T>::get(parent_bounty_id);
				reads += 1;
				ParentTotalChildBounties::<T>::insert(
					parent_bounty_id,
					new_child_bounty_id.saturating_add(1),
				);
				writes += 1;

				log::info!(
					target: LOG_TARGET,
					"Remapped parent bounty {} child bounty id {}->{}",
					parent_bounty_id,
					old_child_bounty_id,
					new_child_bounty_id,
				);

				let bounty_description = ChildBountyDescriptions::<T>::take(old_child_bounty_id);
				writes += 1;
				let child_bounty = ChildBounties::<T>::take(parent_bounty_id, old_child_bounty_id);
				writes += 1;

				// should always be some
				if let Some(taken) = child_bounty {
					ChildBounties::<T>::insert(parent_bounty_id, new_child_bounty_id, taken);
					writes += 1;
				}
				if let Some(bounty_description) = bounty_description {
					super::super::ChildBountyDescriptions::<T>::insert(
						parent_bounty_id,
						new_child_bounty_id,
						bounty_description,
					);
					writes += 1;
				}
			}

			log::info!(
				target: LOG_TARGET,
				"Migration done, reads: {}, writes: {}",
				reads, writes,
			);

			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let old_child_bounty_count = ChildBounties::<T>::iter_keys().count() as u32;
			let old_child_bounty_descriptions =
				v1::ChildBountyDescriptions::<T>::iter_keys().count() as u32;
			Ok((old_child_bounty_count, old_child_bounty_descriptions).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			type StateType = (u32, u32);
			let (old_child_bounty_count, old_child_bounty_descriptions) =
				StateType::decode(&mut &state[..]).expect("Can't decode previous state");
			let new_child_bounty_count = ChildBounties::<T>::iter_keys().count() as u32;
			let new_child_bounty_descriptions =
				super::super::ChildBountyDescriptions::<T>::iter_keys().count() as u32;

			ensure!(
				old_child_bounty_count == new_child_bounty_count,
				"child bounty count doesn't match"
			);
			ensure!(
				old_child_bounty_descriptions == new_child_bounty_descriptions,
				"child bounty descriptions count doesn't match"
			);

			log::info!(
				"old child bounty descriptions: {}",
				v1::ChildBountyDescriptions::<T>::iter_keys().count()
			);

			Ok(())
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateToV1Impl<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

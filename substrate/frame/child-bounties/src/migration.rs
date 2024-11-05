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

use alloc::collections::BTreeSet;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;

pub mod v1 {
	use super::*;

	/// Creates a new ids for the child balances based on the child bounty count per parent bounty
	/// instead of the total child bounty count. Translates the existing child bounties to the new
	/// ids. Creates the `V0ToV1ChildBountyIds` map from `old_child_id` to new (`parent_id`,
	/// `new_child_id`).
	///
	/// `TransferWeight` returns `Weight` of `T::Currency::transfer` and `T::Currency::free_balance`
	/// operation which is performed during this migration.
	pub struct MigrateToV1Impl<T, TransferWeight>(PhantomData<(T, TransferWeight)>);

	#[storage_alias]
	type ChildBountyDescriptions<T: Config + pallet_bounties::Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		BountyIndex,
		BoundedVec<u8, <T as pallet_bounties::Config>::MaximumReasonLength>,
	>;

	impl<T: Config, TransferWeight: Get<Weight>> UncheckedOnRuntimeUpgrade
		for MigrateToV1Impl<T, TransferWeight>
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			// increment reads/writes after the action
			let mut reads = 0u64;
			let mut writes = 0u64;
			let mut transfer_weights: Weight = Weight::zero();

			// keep ids order roughly the same with the old order
			let mut old_bounty_ids = BTreeSet::new();
			// first iteration collect all existing ids not to mutate map as we iterate it
			for (parent_bounty_id, old_child_bounty_id) in ChildBounties::<T>::iter_keys() {
				reads += 1;
				old_bounty_ids.insert((parent_bounty_id, old_child_bounty_id));
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

				V0ToV1ChildBountyIds::<T>::insert(
					old_child_bounty_id,
					(parent_bounty_id, new_child_bounty_id),
				);
				writes += 1;

				let old_child_bounty_account =
					Self::old_child_bounty_account_id(old_child_bounty_id);
				let new_child_bounty_account =
					Pallet::<T>::child_bounty_account_id(parent_bounty_id, new_child_bounty_id);
				let old_balance = T::Currency::free_balance(&old_child_bounty_account);
				log::info!(
					"Transferring {:?} funds from old child bounty account {:?} to new child bounty account {:?}",
					old_balance, old_child_bounty_account, new_child_bounty_account
				);
				if let Err(err) = T::Currency::transfer(
					&old_child_bounty_account,
					&new_child_bounty_account,
					old_balance,
					AllowDeath,
				) {
					log::error!(
						target: LOG_TARGET,
						"Error transferring funds: {:?}",
						err
					);
				}
				transfer_weights += TransferWeight::get();

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
				} else {
					log::error!(
						"child bounty with old id {} not found, should be impossible",
						old_child_bounty_id
					);
				}
				if let Some(bounty_description) = bounty_description {
					super::super::ChildBountyDescriptionsV1::<T>::insert(
						parent_bounty_id,
						new_child_bounty_id,
						bounty_description,
					);
					writes += 1;
				} else {
					log::error!(
						"child bounty description with old id {} not found, should be impossible",
						old_child_bounty_id
					);
				}
			}

			log::info!(
				target: LOG_TARGET,
				"Migration done, reads: {}, writes: {}, transfer weights: {}",
				reads, writes, transfer_weights
			);

			T::DbWeight::get().reads_writes(reads, writes) + transfer_weights
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let old_child_bounty_count = ChildBounties::<T>::iter_keys().count() as u32;
			let old_child_bounty_descriptions =
				v1::ChildBountyDescriptions::<T>::iter_keys().count() as u32;
			let old_child_bounty_ids = ChildBounties::<T>::iter_keys().collect::<Vec<_>>();
			Ok((old_child_bounty_count, old_child_bounty_descriptions, old_child_bounty_ids)
				.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			type StateType = (u32, u32, Vec<(u32, u32)>);
			let (old_child_bounty_count, old_child_bounty_descriptions, old_child_bounty_ids) =
				StateType::decode(&mut &state[..]).expect("Can't decode previous state");
			let new_child_bounty_count = ChildBounties::<T>::iter_keys().count() as u32;
			let new_child_bounty_descriptions =
				super::super::ChildBountyDescriptionsV1::<T>::iter_keys().count() as u32;

			ensure!(
				old_child_bounty_count == new_child_bounty_count,
				"child bounty count doesn't match"
			);
			ensure!(
				old_child_bounty_descriptions == new_child_bounty_descriptions,
				"child bounty descriptions count doesn't match"
			);

			let old_child_bounty_descriptions_storage =
				v1::ChildBountyDescriptions::<T>::iter_keys().count();
			log::info!("old child bounty descriptions: {}", old_child_bounty_descriptions_storage);
			ensure!(
				old_child_bounty_descriptions_storage == 0,
				"Old bounty descriptions should have been drained."
			);

			for (_, old_child_bounty_id) in old_child_bounty_ids {
				let old_account_id = Self::old_child_bounty_account_id(old_child_bounty_id);
				let balance = T::Currency::total_balance(&old_account_id);
				if !balance.is_zero() {
					log::error!(
						"Old child bounty id {} still has balance {:?}",
						old_child_bounty_id,
						balance
					);
				}
			}

			Ok(())
		}
	}

	impl<T: Config, TransferWeight: Get<Weight>> MigrateToV1Impl<T, TransferWeight> {
		fn old_child_bounty_account_id(id: BountyIndex) -> T::AccountId {
			// This function is taken from the parent (bounties) pallet, but the
			// prefix is changed to have different AccountId when the index of
			// parent and child is same.
			T::PalletId::get().into_sub_account_truncating(("cb", id))
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T, TransferWeight> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateToV1Impl<T, TransferWeight>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

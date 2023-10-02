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

use crate::{types::BalanceOf, Config, Pallet, *};
use frame_support::{
	storage::unhashed,
	traits::{Defensive, Get, GetStorageVersion, OnRuntimeUpgrade},
	weights::Weight,
	Twox64Concat,
};
use sp_staking::EraIndex;
use sp_std::prelude::*;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

// The old queue, removed in <https://github.com/paritytech/polkadot-sdk/pull/1772>, used in v1 and v2.
#[frame_support::storage_alias]
type Queue<T: Config> = CountedStorageMap<
	Pallet<T>,
	Twox64Concat,
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
>;

pub mod v1 {
	use super::*;

	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log!(
				info,
				"Running migration with current storage version {:?} / onchain {:?}",
				current,
				onchain
			);

			if current == 1 && onchain == 0 {
				// update the version nonetheless.
				current.put::<Pallet<T>>();

				// if a head exists, then we put them back into the queue.
				if Head::<T>::exists() {
					if let Some((stash, _, deposit)) =
						unhashed::take::<(T::AccountId, Vec<EraIndex>, BalanceOf<T>)>(
							&Head::<T>::hashed_key(),
						)
						.defensive()
					{
						Queue::<T>::insert(stash, deposit);
					} else {
						// not much we can do here -- head is already deleted.
					}
					T::DbWeight::get().reads_writes(2, 3)
				} else {
					T::DbWeight::get().reads(2)
				}
			} else {
				log!(info, "Migration did not execute. This probably should be removed");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 0,
				"The onchain storage version must be zero for the migration to execute."
			);
			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 1,
				"The onchain version must be updated after the migration."
			);
			Ok(())
		}
	}
}

/// Migration that moves this pallet into v2, whereby:
///
/// 1. Named holds are used instead of `ReservableCurrency`.
/// 2. Structure of both storage items [`UnstakeQueue`] and [`Head`] are changed to no longer
///    contain any deposit amounts.
pub mod v2 {
	use frame_support::migrations::VersionedMigration;
	use sp_runtime::BoundedVec;

	use crate::types::{MaxChecking, UnstakeRequest};

	use super::*;

	/// Unchecked migration type.
	pub struct MigrateQueue<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateQueue<T> {
		fn on_runtime_upgrade() -> Weight {
			// move all items of old queue to the new one.
			let old_count = Queue::<T>::count();
			Queue::<T>::iter_keys().drain().for_each(|stash| {
				UnstakeQueue::<T>::insert(stash, ());
			});
			// This will in fact trick the counter to be wiped from storage.
			Queue::<T>::initialize_counter();
			T::DbWeight::get().reads_writes(old_count.into(), (2 * old_count).into())
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(UnstakeQueue::<T>::iter().count() == 0, "The unstake queue must be empty.");
			Ok(Queue::<T>::count().encode().to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(data: Vec<u8>) -> Result<(), TryRuntimeError> {
			let pre_count = <u32 as Decode>::decode(&mut &*data)
				.map_err(|e| "failed to decode pre_upgrade data".into());
			ensure!(pre_count == UnstakeQueue::<T>::count(), "wrong number of items migrated");
			Ok(())
		}
	}

	#[derive(codec::Decode, codec::Encode)]
	struct OldUnstakeRequest<T: Config> {
		/// This list of stashes are being processed in this request.
		pub stashes: BoundedVec<(T::AccountId, BalanceOf<T>), T::BatchSize>,
		/// The list of eras for which they have been checked.
		pub checked: BoundedVec<EraIndex, MaxChecking<T>>,
	}

	pub struct MigrateHead<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateHead<T> {
		fn on_runtime_upgrade() -> Weight {
			let outcome = Head::<T>::translate::<OldUnstakeRequest<T>, _>(|maybe_previous| {
				maybe_previous.map(|previous| UnstakeRequest {
					checked: previous.checked,
					stashes: previous
						.stashes
						.into_iter()
						.map(|(s, _)| s)
						.collect::<Vec<_>>()
						.try_into()
						// bound has not changed, should not fail.
						.defensive_unwrap_or_default(),
				})
			});

			if outcome.is_err() {
				log!(error, "Failed to migrate head storage item.");
			}

			T::DbWeight::get().reads_writes(1, 2)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			use codec::Encode;
			Ok(Head::<T>::get().map(|head| head.stashes.len()).encode())
		}
		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			// should be able to decode the head, if it exists.
			if let Some(stashes_len) = <Option<usize> as Decode>::decode(&mut &*state)
				.expect("failed to decode pre_upgrade data")
			{
				ensure!(
					stashes_len == Head::<T>::get().map(|head| head.stashes.len()).unwrap_or(0),
					"wrong number of items migrated"
				);
			}

			Ok(())
		}
	}

	/// Checked migration type to be added to a runtime.
	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		(MigrateQueue<T>, MigrateHead<T>),
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	#[cfg(test)]
	mod tests {
		use super::*;
		use crate::{
			migrations::Queue,
			mock::{Deposit, Runtime},
			types::UnstakeRequest,
			Head,
		};
		use codec::Encode;
		use frame_support::traits::OnRuntimeUpgrade;
		use sp_core::bounded_vec;
		use sp_io::TestExternalities;

		#[test]
		fn it_works() {
			TestExternalities::new_empty().execute_with(|| {
				// a couple of people in the queue in the old format.
				Queue::<Runtime>::insert(1, Deposit::get());
				Queue::<Runtime>::insert(2, Deposit::get());
				Queue::<Runtime>::insert(3, Deposit::get());

				// a head being migrated.
				let old_request = OldUnstakeRequest::<Runtime> {
					stashes: bounded_vec![(4, Deposit::get())],
					checked: bounded_vec![42],
				};
				sp_io::storage::set(&Head::<Runtime>::hashed_key(), &old_request.encode());

				// we can't run the versioned one as it will not work here.
				<(MigrateQueue<Runtime>, MigrateHead<Runtime>) as OnRuntimeUpgrade>::on_runtime_upgrade();

				// new state.
				assert_eq!(
					UnstakeQueue::<Runtime>::iter().collect::<Vec<_>>(),
					vec![(1, ()), (3, ()), (2, ())]
				);
				assert_eq!(
					Head::<Runtime>::get().unwrap(),
					UnstakeRequest { checked: bounded_vec![42], stashes: bounded_vec![(4)] }
				)
			});
		}
	}
}

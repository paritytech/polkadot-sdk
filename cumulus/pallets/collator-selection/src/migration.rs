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

//! A module that is responsible for migration of storage for Collator Selection.

use super::*;
use frame_support::traits::OnRuntimeUpgrade;
use log;

/// Migrate to v2. Should have been part of https://github.com/paritytech/polkadot-sdk/pull/1340
pub mod v2 {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		storage_alias,
		traits::{Currency, ReservableCurrency},
	};
	use sp_runtime::traits::{Saturating, Zero};

	#[storage_alias]
	pub type Candidates<T: Config> = StorageValue<
		Pallet<T>,
		BoundedVec<CandidateInfo<<T as frame_system::Config>::AccountId, <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance>, <T as Config>::MaxCandidates>,
		ValueQuery,
	>;

	/// Migrate to V2.
	pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let on_chain_version = Pallet::<T>::on_chain_storage_version();
			if on_chain_version == 1 {
				let mut weight = Weight::zero();
				let mut count: u32 = 0;
				// candidates who exist under the old `Candidates` key
				let candidates = Candidates::<T>::take();

				// New candidates who have registered since the upgrade. Under normal circumstances,
				// this should not exist because the migration should be applied when the upgrade
				// happens. But in Polkadot/Kusama we messed this up, and people registered under
				// `CandidateList` while their funds were locked in `Candidates`.
				let new_candidate_list = CandidateList::<T>::get();
				if new_candidate_list.len().is_zero() {
					// The new list is empty, so this is essentially being applied correctly. We
					// just put the candidates into the new storage item.
					CandidateList::<T>::put(&candidates);
					// 1 write for the new list
					weight.saturating_accrue(T::DbWeight::get().reads_writes(0, 1));
				} else {
					// Oops, the runtime upgraded without the migration. There are new candidates in
					// `CandidateList`. So, let's just refund the old ones and assume they have
					// already started participating in the new system.
					for candidate in candidates {
						let _err = T::Currency::unreserve(&candidate.who, candidate.deposit);
						count.saturating_inc();
					}
					// TODO: set each accrue to weight of `unreserve`
					weight.saturating_accrue(T::DbWeight::get().reads_writes(0, count as u64));
				}

				StorageVersion::new(2).put::<Pallet<T>>();
				log::info!(
					target: LOG_TARGET,
					"Unreserved locked bond of {} candidates, upgraded storage to version 2",
					count,
				);
				// 1 read/write for storage version
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				weight
			} else {
				log::info!(
					target: LOG_TARGET,
					"Migration did not execute. This probably should be removed"
				);
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			let number_of_candidates = Candidates::<T>::get().to_vec().len();
			Ok((number_of_candidates as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_number_of_candidates: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			let new_number_of_candidates = Candidates::<T>::get().to_vec().len();
			assert_eq!(
				new_number_of_candidates, 0 as usize,
				"after migration, the candidates map should be empty"
			);

			let on_chain_version = Pallet::<T>::on_chain_storage_version();
			frame_support::ensure!(on_chain_version >= 1, "must_upgrade");

			Ok(())
		}
	}
}

/// Version 1 Migration
/// This migration ensures that any existing `Invulnerables` storage lists are sorted.
pub mod v1 {
	use super::*;
	use frame_support::pallet_prelude::*;
	#[cfg(feature = "try-runtime")]
	use sp_std::prelude::*;

	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let on_chain_version = Pallet::<T>::on_chain_storage_version();
			if on_chain_version == 0 {
				let invulnerables_len = Invulnerables::<T>::get().to_vec().len();
				Invulnerables::<T>::mutate(|invulnerables| {
					invulnerables.sort();
				});

				StorageVersion::new(1).put::<Pallet<T>>();
				log::info!(
					target: LOG_TARGET,
					"Sorted {} Invulnerables, upgraded storage to version 1",
					invulnerables_len,
				);
				// Similar complexity to `set_invulnerables` (put storage value)
				// Plus 1 read for length, 1 read for `on_chain_version`, 1 write to put version
				T::WeightInfo::set_invulnerables(invulnerables_len as u32)
					.saturating_add(T::DbWeight::get().reads_writes(2, 1))
			} else {
				log::info!(
					target: LOG_TARGET,
					"Migration did not execute. This probably should be removed"
				);
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			let number_of_invulnerables = Invulnerables::<T>::get().to_vec().len();
			Ok((number_of_invulnerables as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(number_of_invulnerables: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			let stored_invulnerables = Invulnerables::<T>::get().to_vec();
			let mut sorted_invulnerables = stored_invulnerables.clone();
			sorted_invulnerables.sort();
			assert_eq!(
				stored_invulnerables, sorted_invulnerables,
				"after migration, the stored invulnerables should be sorted"
			);

			let number_of_invulnerables: u32 = Decode::decode(
				&mut number_of_invulnerables.as_slice(),
			)
			.expect("the state parameter should be something that was generated by pre_upgrade");
			let stored_invulnerables_len = stored_invulnerables.len() as u32;
			assert_eq!(
				number_of_invulnerables, stored_invulnerables_len,
				"after migration, there should be the same number of invulnerables"
			);

			let on_chain_version = Pallet::<T>::on_chain_storage_version();
			frame_support::ensure!(on_chain_version >= 1, "must_upgrade");

			Ok(())
		}
	}
}

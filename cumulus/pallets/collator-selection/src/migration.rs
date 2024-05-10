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
use frame_support::traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade};
use log;

/// Migrate to v2. Should have been part of <https://github.com/paritytech/polkadot-sdk/pull/1340>.
pub mod v2 {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		storage_alias,
		traits::{Currency, ReservableCurrency},
	};
	use sp_runtime::traits::{Saturating, Zero};
	#[cfg(feature = "try-runtime")]
	use sp_std::vec::Vec;

	/// [`UncheckedMigrationToV2`] wrapped in a
	/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), ensuring the
	/// migration is only performed when on-chain version is 1.
	pub type MigrationToV2<T> = frame_support::migrations::VersionedMigration<
		1,
		2,
		UncheckedMigrationToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	#[storage_alias]
	pub type Candidates<T: Config> = StorageValue<
		Pallet<T>,
		BoundedVec<CandidateInfo<<T as frame_system::Config>::AccountId, <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance>, <T as Config>::MaxCandidates>,
		ValueQuery,
	>;

	/// Migrate to V2.
	pub struct UncheckedMigrationToV2<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config + pallet_balances::Config> UncheckedOnRuntimeUpgrade for UncheckedMigrationToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			let mut count: u64 = 0;
			// candidates who exist under the old `Candidates` key
			let candidates = Candidates::<T>::take();

			// New candidates who have registered since the upgrade. Under normal circumstances,
			// this should not exist because the migration should be applied when the upgrade
			// happens. But in Polkadot/Kusama we messed this up, and people registered under
			// `CandidateList` while their funds were locked in `Candidates`.
			let new_candidate_list = CandidateList::<T>::get();
			if new_candidate_list.len().is_zero() {
				// The new list is empty, so this is essentially being applied correctly. We just
				// put the candidates into the new storage item.
				CandidateList::<T>::put(&candidates);
				// 1 write for the new list
				weight.saturating_accrue(T::DbWeight::get().reads_writes(0, 1));
			} else {
				// Oops, the runtime upgraded without the migration. There are new candidates in
				// `CandidateList`. So, let's just refund the old ones and assume they have already
				// started participating in the new system.
				for candidate in candidates {
					let err = T::Currency::unreserve(&candidate.who, candidate.deposit);
					if err > Zero::zero() {
						log::error!(
							target: LOG_TARGET,
							"{:?} balance was unable to be unreserved from {:?}",
							err, &candidate.who,
						);
					}
					count.saturating_inc();
				}
				weight.saturating_accrue(
					<<T as pallet_balances::Config>::WeightInfo as pallet_balances::WeightInfo>::force_unreserve().saturating_mul(count.into()),
				);
			}

			log::info!(
				target: LOG_TARGET,
				"Unreserved locked bond of {} candidates, upgraded storage to version 2",
				count,
			);

			weight.saturating_accrue(T::DbWeight::get().reads_writes(3, 2));
			weight
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

#[cfg(all(feature = "try-runtime", test))]
mod tests {
	use super::*;
	use crate::{
		migration::v2::Candidates,
		mock::{new_test_ext, Balances, Test},
	};
	use frame_support::{
		traits::{Currency, ReservableCurrency, StorageVersion},
		BoundedVec,
	};
	use sp_runtime::traits::ConstU32;

	#[test]
	fn migrate_to_v2_with_new_candidates() {
		new_test_ext().execute_with(|| {
			let storage_version = StorageVersion::new(1);
			storage_version.put::<Pallet<Test>>();

			let one = 1u64;
			let two = 2u64;
			let three = 3u64;
			let deposit = 10u64;

			// Set balance to 100
			Balances::make_free_balance_be(&one, 100u64);
			Balances::make_free_balance_be(&two, 100u64);
			Balances::make_free_balance_be(&three, 100u64);

			// Reservations: 10 for the "old" candidacy and 10 for the "new"
			Balances::reserve(&one, 10u64).unwrap(); // old
			Balances::reserve(&two, 20u64).unwrap(); // old + new
			Balances::reserve(&three, 10u64).unwrap(); // new

			// Candidate info
			let candidate_one = CandidateInfo { who: one, deposit };
			let candidate_two = CandidateInfo { who: two, deposit };
			let candidate_three = CandidateInfo { who: three, deposit };

			// Storage lists
			let bounded_candidates =
				BoundedVec::<CandidateInfo<u64, u64>, ConstU32<20>>::try_from(vec![
					candidate_one.clone(),
					candidate_two.clone(),
				])
				.expect("it works");
			let bounded_candidate_list =
				BoundedVec::<CandidateInfo<u64, u64>, ConstU32<20>>::try_from(vec![
					candidate_two.clone(),
					candidate_three.clone(),
				])
				.expect("it works");

			// Set storage
			Candidates::<Test>::put(bounded_candidates);
			CandidateList::<Test>::put(bounded_candidate_list.clone());

			// Sanity check
			assert_eq!(Balances::free_balance(one), 90);
			assert_eq!(Balances::free_balance(two), 80);
			assert_eq!(Balances::free_balance(three), 90);

			// Run migration
			v2::MigrationToV2::<Test>::on_runtime_upgrade();

			let new_storage_version = StorageVersion::get::<Pallet<Test>>();
			assert_eq!(new_storage_version, 2);

			// 10 should have been unreserved from the old candidacy
			assert_eq!(Balances::free_balance(one), 100);
			assert_eq!(Balances::free_balance(two), 90);
			assert_eq!(Balances::free_balance(three), 90);
			// The storage item should be gone
			assert!(Candidates::<Test>::get().is_empty());
			// The new storage item should be preserved
			assert_eq!(CandidateList::<Test>::get(), bounded_candidate_list);
		});
	}

	#[test]
	fn migrate_to_v2_without_new_candidates() {
		new_test_ext().execute_with(|| {
			let storage_version = StorageVersion::new(1);
			storage_version.put::<Pallet<Test>>();

			let one = 1u64;
			let two = 2u64;
			let deposit = 10u64;

			// Set balance to 100
			Balances::make_free_balance_be(&one, 100u64);
			Balances::make_free_balance_be(&two, 100u64);

			// Reservations
			Balances::reserve(&one, 10u64).unwrap(); // old
			Balances::reserve(&two, 10u64).unwrap(); // old

			// Candidate info
			let candidate_one = CandidateInfo { who: one, deposit };
			let candidate_two = CandidateInfo { who: two, deposit };

			// Storage lists
			let bounded_candidates =
				BoundedVec::<CandidateInfo<u64, u64>, ConstU32<20>>::try_from(vec![
					candidate_one.clone(),
					candidate_two.clone(),
				])
				.expect("it works");

			// Set storage
			Candidates::<Test>::put(bounded_candidates.clone());

			// Sanity check
			assert_eq!(Balances::free_balance(one), 90);
			assert_eq!(Balances::free_balance(two), 90);

			// Run migration
			v2::MigrationToV2::<Test>::on_runtime_upgrade();

			let new_storage_version = StorageVersion::get::<Pallet<Test>>();
			assert_eq!(new_storage_version, 2);

			// Nothing changes deposit-wise
			assert_eq!(Balances::free_balance(one), 90);
			assert_eq!(Balances::free_balance(two), 90);
			// The storage item should be gone
			assert!(Candidates::<Test>::get().is_empty());
			// The new storage item should have the info now
			assert_eq!(CandidateList::<Test>::get(), bounded_candidates);
		});
	}
}

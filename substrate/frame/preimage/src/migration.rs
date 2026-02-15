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

//! Storage migrations for the preimage pallet.

use super::*;
use alloc::collections::btree_map::BTreeMap;
use frame_support::{
	storage_alias,
	traits::{ConstU32, Currency, OnRuntimeUpgrade, ReservableCurrency},
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The deprecated `OldRequestStatus` type from storage version 1.
///
/// This type was used in the `StatusFor` storage map before being replaced
/// by `RequestStatus` which uses `Consideration` tickets instead of raw balance deposits.
#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug)]
pub enum OldRequestStatus<AccountId, Balance> {
	/// The associated preimage has not yet been requested by the system. The given deposit (if
	/// some) is being held until either it becomes requested or the user retracts the preimage.
	Unrequested { deposit: (AccountId, Balance), len: u32 },
	/// There are a non-zero number of outstanding requests for this hash by this chain. If there
	/// is a preimage registered, then `len` is `Some` and it may be removed iff this counter
	/// becomes zero.
	Requested { deposit: Option<(AccountId, Balance)>, count: u32, len: Option<u32> },
}

/// Storage prefix for the deprecated `StatusFor` storage map.
///
/// Provides access to the same storage location that was previously declared as
/// `#[pallet::storage] pub type StatusFor<T>` in the pallet, allowing migrations to
/// read/write entries after the storage item has been removed from the pallet's Config.
pub(crate) struct StatusForPrefix<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> frame_support::traits::StorageInstance for StatusForPrefix<T> {
	fn pallet_prefix() -> &'static str {
		<Pallet<T> as frame_support::traits::PalletInfoAccess>::name()
	}
	const STORAGE_PREFIX: &'static str = "StatusFor";
}

/// Type alias for the deprecated `StatusFor` storage map.
///
/// Generic over `OldCurrency` to resolve the `Balance` type that was used in storage encoding.
/// This allows migrations to access the storage after `type Currency` has been removed from
/// the pallet's `Config` trait.
pub(crate) type OldStatusFor<T, OldCurrency> = frame_support::storage::types::StorageMap<
	StatusForPrefix<T>,
	frame_support::Identity,
	<T as frame_system::Config>::Hash,
	OldRequestStatus<
		<T as frame_system::Config>::AccountId,
		<OldCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance,
	>,
>;

/// The original data layout of the preimage pallet without a specific version number.
mod v0 {
	use super::*;

	#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug)]
	pub enum OldRequestStatus<AccountId, Balance> {
		Unrequested(Option<(AccountId, Balance)>),
		Requested(u32),
	}

	#[storage_alias]
	pub type PreimageFor<T: Config> = StorageMap<
		Pallet<T>,
		Identity,
		<T as frame_system::Config>::Hash,
		BoundedVec<u8, ConstU32<MAX_SIZE>>,
	>;

	/// Type alias for the v0 `StatusFor` storage map, using the v0 value format.
	pub type StatusFor<T, OldCurrency> = frame_support::storage::types::StorageMap<
		super::StatusForPrefix<T>,
		frame_support::Identity,
		<T as frame_system::Config>::Hash,
		OldRequestStatus<
			<T as frame_system::Config>::AccountId,
			<OldCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance,
		>,
	>;

	/// Returns the number of images or `None` if the storage is corrupted.
	#[cfg(feature = "try-runtime")]
	pub fn image_count<T: Config, OldCurrency>() -> Option<u32>
	where
		OldCurrency: Currency<T::AccountId>,
	{
		let images = PreimageFor::<T>::iter_values().count() as u32;
		let status = StatusFor::<T, OldCurrency>::iter_values().count() as u32;

		if images == status {
			Some(images)
		} else {
			None
		}
	}
}

pub mod v1 {
	use super::*;

	/// The log target.
	const TARGET: &str = "runtime::preimage::migration::v1";

	/// Migration for moving preimage from V0 to V1 storage.
	///
	/// Note: This needs to be run with the same hashing algorithm as before
	/// since it is not re-hashing the preimages.
	///
	/// `OldCurrency` is the currency type that was previously used for deposits.
	/// It is needed to decode the v0 `StatusFor` entries which contain `Balance` values.
	pub struct Migration<T, OldCurrency>(core::marker::PhantomData<(T, OldCurrency)>);

	impl<T: Config, OldCurrency> OnRuntimeUpgrade for Migration<T, OldCurrency>
	where
		OldCurrency: ReservableCurrency<T::AccountId>,
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(StorageVersion::get::<Pallet<T>>() == 0, "can only upgrade from version 0");

			let images = v0::image_count::<T, OldCurrency>().expect("v0 storage corrupted");
			log::info!(target: TARGET, "Migrating {} images", &images);
			Ok((images as u32).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);
			if StorageVersion::get::<Pallet<T>>() != 0 {
				log::warn!(
					target: TARGET,
					"skipping MovePreimagesIntoBuckets: executed on wrong storage version.\
				Expected version 0"
				);
				return weight;
			}

			let status = v0::StatusFor::<T, OldCurrency>::drain().collect::<Vec<_>>();
			weight.saturating_accrue(T::DbWeight::get().reads(status.len() as u64));

			let preimages = v0::PreimageFor::<T>::drain().collect::<BTreeMap<_, _>>();
			weight.saturating_accrue(T::DbWeight::get().reads(preimages.len() as u64));

			for (hash, status) in status.into_iter() {
				let preimage = if let Some(preimage) = preimages.get(&hash) {
					preimage
				} else {
					log::error!(target: TARGET, "preimage not found for hash {:?}", &hash);
					continue;
				};
				let len = preimage.len() as u32;
				if len > MAX_SIZE {
					log::error!(
						target: TARGET,
						"preimage too large for hash {:?}, len: {}",
						&hash,
						len
					);
					continue;
				}

				let status = match status {
					v0::OldRequestStatus::Unrequested(deposit) => match deposit {
						Some(deposit) => super::OldRequestStatus::Unrequested { deposit, len },
						// `None` depositor becomes system-requested.
						None => super::OldRequestStatus::Requested {
							deposit: None,
							count: 1,
							len: Some(len),
						},
					},
					v0::OldRequestStatus::Requested(0) => {
						log::error!(
							target: TARGET,
							"preimage has counter of zero: {:?}",
							hash
						);
						continue;
					},
					v0::OldRequestStatus::Requested(count) => {
						super::OldRequestStatus::Requested { deposit: None, count, len: Some(len) }
					},
				};
				log::trace!(target: TARGET, "Moving preimage {:?} with len {}", hash, len);

				super::OldStatusFor::<T, OldCurrency>::insert(hash, status);
				crate::PreimageFor::<T>::insert(&(hash, len), preimage);

				weight.saturating_accrue(T::DbWeight::get().writes(2));
			}
			StorageVersion::new(1).put::<Pallet<T>>();

			weight.saturating_add(T::DbWeight::get().writes(1))
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> DispatchResult {
			let old_images: u32 =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");
			let new_images = image_count::<T, OldCurrency>().expect("V1 storage corrupted");

			if new_images != old_images {
				log::error!(
					target: TARGET,
					"migrated {} images, expected {}",
					new_images,
					old_images
				);
			}
			ensure!(StorageVersion::get::<Pallet<T>>() == 1, "must upgrade");
			Ok(())
		}
	}

	/// Returns the number of images or `None` if the storage is corrupted.
	#[cfg(feature = "try-runtime")]
	pub fn image_count<T: Config, OldCurrency>() -> Option<u32>
	where
		OldCurrency: Currency<T::AccountId>,
	{
		// Use iter_values() to ensure that the values are decodable.
		let images = crate::PreimageFor::<T>::iter_values().count() as u32;
		let status = super::OldStatusFor::<T, OldCurrency>::iter_values().count() as u32;

		if images == status {
			Some(images)
		} else {
			None
		}
	}
}

pub mod v2 {
	use super::*;
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		weights::WeightMeter,
	};

	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	/// Pallet migrations ID.
	const PALLET_MIGRATIONS_ID: &[u8; 15] = b"pallet-preimage";

	/// Migration to convert preimage deposits from `Currency::reserve` to `Consideration` holds.
	///
	/// This migration iterates through all entries in the deprecated `StatusFor` storage and
	/// converts them to `RequestStatusFor` entries:
	/// - Unreserves old `Currency`-based deposits
	/// - Creates new `Consideration`-based tickets (which use fungible holds internally)
	/// - Writes the new `RequestStatusFor` entry
	/// - Removes the old `StatusFor` entry
	///
	/// `OldCurrency` is the currency type that was previously configured as `type Currency` in the
	/// pallet's `Config`. It is used to call `unreserve` on old deposits.
	pub struct LazyMigrationV1ToV2<T, OldCurrency>(core::marker::PhantomData<(T, OldCurrency)>);

	impl<T, OldCurrency> SteppedMigration for LazyMigrationV1ToV2<T, OldCurrency>
	where
		T: Config,
		OldCurrency: ReservableCurrency<T::AccountId> + 'static,
	{
		type Cursor = BoundedVec<u8, ConstU32<256>>;
		type Identifier = MigrationId<15>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			let required = T::WeightInfo::v2_migration_step();

			// Check minimum weight requirement.
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			// Get iterator, resuming from cursor if present.
			let mut iter = if let Some(ref last_key) = cursor {
				super::OldStatusFor::<T, OldCurrency>::iter_from(last_key.to_vec())
			} else {
				super::OldStatusFor::<T, OldCurrency>::iter()
			};

			// Process as many entries as weight allows.
			loop {
				if meter.try_consume(required).is_err() {
					break;
				}

				if let Some((hash, old_status)) = iter.next() {
					let new_status = match old_status {
						super::OldRequestStatus::Unrequested {
							deposit: (ref who, amount),
							len,
						} => {
							// Unreserve old deposit.
							let remaining = OldCurrency::unreserve(who, amount);
							if !remaining.is_zero() {
								log::warn!(
									"Migration: Could not fully unreserve deposit for {:?}. \
									 Remaining: {:?}",
									who,
									remaining
								);
							}

							// Create new consideration ticket.
							match T::Consideration::new(who, Footprint::from_parts(1, len as usize))
							{
								Ok(ticket) => RequestStatus::Unrequested {
									ticket: (who.clone(), ticket),
									len,
								},
								Err(err) => {
									log::error!(
										"Migration: Failed to create consideration for {:?}: \
										 {:?}. Skipping entry.",
										who,
										err
									);
									super::OldStatusFor::<T, OldCurrency>::remove(&hash);
									let raw_key =
										super::OldStatusFor::<T, OldCurrency>::hashed_key_for(
											&hash,
										);
									cursor = Some(BoundedVec::try_from(raw_key).unwrap_or_else(
										|mut v| {
											v.truncate(256);
											BoundedVec::try_from(v)
												.expect("truncated to bound; qed")
										},
									));
									continue;
								},
							}
						},
						super::OldRequestStatus::Requested {
							deposit: ref maybe_deposit,
							count,
							len: maybe_len,
						} => {
							let maybe_ticket = if let Some((ref who, deposit)) = maybe_deposit {
								// Unreserve old deposit.
								let remaining = OldCurrency::unreserve(who, *deposit);
								if !remaining.is_zero() {
									log::warn!(
										"Migration: Could not fully unreserve deposit for {:?}. \
										 Remaining: {:?}",
										who,
										remaining
									);
								}

								// Only create a ticket if we know the preimage length.
								if let Some(len) = maybe_len {
									match T::Consideration::new(
										who,
										Footprint::from_parts(1, len as usize),
									) {
										Ok(ticket) => Some((who.clone(), ticket)),
										Err(err) => {
											log::error!(
												"Migration: Failed to create consideration for \
												 {:?}: {:?}",
												who,
												err
											);
											None
										},
									}
								} else {
									None
								}
							} else {
								None
							};
							RequestStatus::Requested { maybe_ticket, count, maybe_len }
						},
					};

					RequestStatusFor::<T>::insert(&hash, new_status);
					super::OldStatusFor::<T, OldCurrency>::remove(&hash);

					let raw_key = super::OldStatusFor::<T, OldCurrency>::hashed_key_for(&hash);
					cursor = Some(BoundedVec::try_from(raw_key).unwrap_or_else(|mut v| {
						v.truncate(256);
						BoundedVec::try_from(v).expect("truncated to bound; qed")
					}));
				} else {
					// No more entries — migration complete.
					StorageVersion::new(2).put::<Pallet<T>>();
					log::info!("Preimage migration v1 to v2 complete");
					return Ok(None);
				}
			}

			// Ran out of weight, return cursor to resume later.
			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let count = super::OldStatusFor::<T, OldCurrency>::iter().count() as u64;
			log::info!("Pre-upgrade: Found {} StatusFor entries to migrate", count);
			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let old_count: u64 =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides valid state; qed");
			let remaining = super::OldStatusFor::<T, OldCurrency>::iter().count() as u64;
			ensure!(remaining == 0, "Migration incomplete: StatusFor entries remaining");
			log::info!("Post-upgrade: Successfully migrated {} StatusFor entries", old_count);
			Ok(())
		}
	}
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
	use super::*;
	use crate::mock::{Balances, Test as T, *};

	use sp_runtime::bounded_vec;

	#[test]
	fn v0_to_v1_migration_works() {
		new_test_ext().execute_with(|| {
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 0);
			// Insert some preimages into the v0 storage:

			// Case 1: Unrequested without deposit
			let (p, h) = preimage::<T>(128);
			v0::PreimageFor::<T>::insert(h, p);
			v0::StatusFor::<T, Balances>::insert(h, v0::OldRequestStatus::Unrequested(None));
			// Case 2: Unrequested with deposit
			let (p, h) = preimage::<T>(1024);
			v0::PreimageFor::<T>::insert(h, p);
			v0::StatusFor::<T, Balances>::insert(
				h,
				v0::OldRequestStatus::Unrequested(Some((1, 1))),
			);
			// Case 3: Requested by 0 (invalid)
			let (p, h) = preimage::<T>(8192);
			v0::PreimageFor::<T>::insert(h, p);
			v0::StatusFor::<T, Balances>::insert(h, v0::OldRequestStatus::Requested(0));
			// Case 4: Requested by 10
			let (p, h) = preimage::<T>(65536);
			v0::PreimageFor::<T>::insert(h, p);
			v0::StatusFor::<T, Balances>::insert(h, v0::OldRequestStatus::Requested(10));

			assert_eq!(v0::image_count::<T, Balances>(), Some(4));
			assert_eq!(v1::image_count::<T, Balances>(), None, "V1 storage should be corrupted");

			let state = v1::Migration::<T, Balances>::pre_upgrade().unwrap();
			let _w = v1::Migration::<T, Balances>::on_runtime_upgrade();
			v1::Migration::<T, Balances>::post_upgrade(state).unwrap();

			// V0 and V1 share the same prefix, so `iter_values` still counts the same.
			assert_eq!(v0::image_count::<T, Balances>(), Some(3));
			assert_eq!(v1::image_count::<T, Balances>(), Some(3)); // One gets skipped therefore 3.
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 1);

			// Case 1: Unrequested without deposit becomes system-requested
			let (p, h) = preimage::<T>(128);
			assert_eq!(crate::PreimageFor::<T>::get(&(h, 128)), Some(p));
			assert_eq!(
				OldStatusFor::<T, Balances>::get(h),
				Some(OldRequestStatus::Requested { deposit: None, count: 1, len: Some(128) })
			);
			// Case 2: Unrequested with deposit becomes unrequested
			let (p, h) = preimage::<T>(1024);
			assert_eq!(crate::PreimageFor::<T>::get(&(h, 1024)), Some(p));
			assert_eq!(
				OldStatusFor::<T, Balances>::get(h),
				Some(OldRequestStatus::Unrequested { deposit: (1, 1), len: 1024 })
			);
			// Case 3: Requested by 0 should be skipped
			let (_, h) = preimage::<T>(8192);
			assert_eq!(crate::PreimageFor::<T>::get(&(h, 8192)), None);
			assert_eq!(OldStatusFor::<T, Balances>::get(h), None);
			// Case 4: Requested by 10 becomes requested by 10
			let (p, h) = preimage::<T>(65536);
			assert_eq!(crate::PreimageFor::<T>::get(&(h, 65536)), Some(p));
			assert_eq!(
				OldStatusFor::<T, Balances>::get(h),
				Some(OldRequestStatus::Requested { deposit: None, count: 10, len: Some(65536) })
			);
		});
	}

	#[test]
	fn v1_to_v2_migration_works() {
		new_test_ext().execute_with(|| {
			use frame_support::{
				migrations::SteppedMigration, traits::fungible::InspectHold, weights::WeightMeter,
			};

			// Set storage version to 1 (post v0→v1 migration).
			StorageVersion::new(1).put::<Pallet<T>>();

			let alice: u64 = 2;

			// Insert an Unrequested entry with deposit.
			let preimage1 = 1u32.to_le_bytes();
			let hash1 = <T as frame_system::Config>::Hashing::hash(&preimage1[..]);
			// Reserve funds to simulate pre-migration state.
			<Balances as ReservableCurrency<u64>>::reserve(&alice, 10).unwrap();
			OldStatusFor::<T, Balances>::insert(
				&hash1,
				OldRequestStatus::Unrequested { deposit: (alice, 10), len: 4 },
			);

			// Insert a Requested entry with deposit and known length.
			let preimage2 = 2u32.to_le_bytes();
			let hash2 = <T as frame_system::Config>::Hashing::hash(&preimage2[..]);
			<Balances as ReservableCurrency<u64>>::reserve(&alice, 5).unwrap();
			OldStatusFor::<T, Balances>::insert(
				&hash2,
				OldRequestStatus::Requested { deposit: Some((alice, 5)), count: 3, len: Some(8) },
			);

			// Insert a Requested entry without deposit.
			let preimage3 = 3u32.to_le_bytes();
			let hash3 = <T as frame_system::Config>::Hashing::hash(&preimage3[..]);
			OldStatusFor::<T, Balances>::insert(
				&hash3,
				OldRequestStatus::Requested { deposit: None, count: 1, len: Some(16) },
			);

			// Verify initial state.
			assert_eq!(OldStatusFor::<T, Balances>::iter().count(), 3);
			assert_eq!(RequestStatusFor::<T>::iter().count(), 0);

			// Run migration.
			let mut meter = WeightMeter::new();
			let result = v2::LazyMigrationV1ToV2::<T, Balances>::step(None, &mut meter).unwrap();
			// Should complete in one step with enough weight.
			assert!(result.is_none(), "Migration should complete in one step");

			// Verify old storage is empty.
			assert_eq!(OldStatusFor::<T, Balances>::iter().count(), 0);
			// Verify new storage has entries.
			assert_eq!(RequestStatusFor::<T>::iter().count(), 3);

			// Verify Unrequested entry was migrated.
			let status1 = RequestStatusFor::<T>::get(hash1).unwrap();
			match status1 {
				RequestStatus::Unrequested { ticket: (who, _), len } => {
					assert_eq!(who, alice);
					assert_eq!(len, 4);
				},
				_ => panic!("Expected Unrequested status"),
			}

			// Verify Requested entry with deposit was migrated.
			let status2 = RequestStatusFor::<T>::get(hash2).unwrap();
			match status2 {
				RequestStatus::Requested { maybe_ticket, count, maybe_len } => {
					assert!(maybe_ticket.is_some());
					let (who, _) = maybe_ticket.unwrap();
					assert_eq!(who, alice);
					assert_eq!(count, 3);
					assert_eq!(maybe_len, Some(8));
				},
				_ => panic!("Expected Requested status"),
			}

			// Verify Requested entry without deposit was migrated.
			let status3 = RequestStatusFor::<T>::get(hash3).unwrap();
			match status3 {
				RequestStatus::Requested { maybe_ticket, count, maybe_len } => {
					assert!(maybe_ticket.is_none());
					assert_eq!(count, 1);
					assert_eq!(maybe_len, Some(16));
				},
				_ => panic!("Expected Requested status"),
			}

			// Verify new holds were created.
			// Note: In pallet-balances, holds are tracked in the same `reserved` field,
			// so `reserved_balance` includes both legacy reserves and holds.
			let hold_amount =
				<Balances as InspectHold<u64>>::balance_on_hold(&PreimageHoldReason::get(), &alice);
			assert!(hold_amount > 0, "Should have holds after migration");
			// Old reserves (15) should have been released and replaced by holds (16).
			// The reserved balance equals the hold amount since no legacy reserves remain.
			assert_eq!(
				<Balances as ReservableCurrency<u64>>::reserved_balance(&alice),
				hold_amount,
			);
			// Verify storage version was bumped.
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 2);
		});
	}

	#[test]
	fn v1_to_v2_migration_empty_storage() {
		new_test_ext().execute_with(|| {
			use frame_support::{migrations::SteppedMigration, weights::WeightMeter};

			StorageVersion::new(1).put::<Pallet<T>>();

			// No StatusFor entries.
			assert_eq!(OldStatusFor::<T, Balances>::iter().count(), 0);

			let mut meter = WeightMeter::new();
			let result = v2::LazyMigrationV1ToV2::<T, Balances>::step(None, &mut meter).unwrap();
			assert!(result.is_none(), "Migration should complete immediately with empty storage");
			// Verify storage version was bumped.
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 2);
		});
	}

	/// Returns a preimage with a given size and its hash.
	fn preimage<T: Config>(
		len: usize,
	) -> (BoundedVec<u8, ConstU32<MAX_SIZE>>, <T as frame_system::Config>::Hash) {
		let p = bounded_vec![1; len];
		let h = <T as frame_system::Config>::Hashing::hash_of(&p);
		(p, h)
	}
}

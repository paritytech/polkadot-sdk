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

// Migrations for Multisig Pallet

use crate::*;
use frame::prelude::*;

pub mod v2 {
	use super::*;
	use frame::{
		deps::frame_support::{
			migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
			weights::WeightMeter,
		},
		traits::ReservableCurrency,
	};

	#[cfg(feature = "try-runtime")]
	extern crate alloc;
	#[cfg(feature = "try-runtime")]
	use alloc::{collections::btree_map::BTreeMap, vec::Vec};
	#[cfg(feature = "try-runtime")]
	use frame::deps::sp_runtime::TryRuntimeError;

	/// Pallet migrations ID.
	const PALLET_MIGRATIONS_ID: &[u8; 15] = b"pallet-multisig";

	/// Migration to convert deposits from Currency::reserve to Fungible::hold.
	///
	/// This migration iterates through all entries in `Multisigs` storage and converts
	/// the depositor's reserved balance to a hold with `HoldReason::MultisigOperation`.
	pub struct LazyMigrationV1ToV2<T, OldCurrency>(core::marker::PhantomData<(T, OldCurrency)>);

	impl<T, OldCurrency> SteppedMigration for LazyMigrationV1ToV2<T, OldCurrency>
	where
		T: Config,
		OldCurrency: ReservableCurrency<T::AccountId, Balance = BalanceOf<T>>,
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
			let required = T::WeightInfo::v2_migration_step(T::MaxSignatories::get());

			// Check minimum weight requirement
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			// Get iterator, resuming from cursor if present
			let mut iter = if let Some(ref last_key) = cursor {
				Multisigs::<T>::iter_from(last_key.to_vec())
			} else {
				Multisigs::<T>::iter()
			};

			// Loop to process as many entries as weight allows
			loop {
				if meter.try_consume(required).is_err() {
					break;
				}

				// Process next entry
				if let Some((multisig_account, call_hash, multisig_data)) = iter.next() {
					// Only process if there's a deposit to migrate
					if !multisig_data.deposit.is_zero() {
						let depositor = &multisig_data.depositor;
						let deposit = multisig_data.deposit;

						// Step 1: Unreserve the old reserved balance
						let remaining = OldCurrency::unreserve(depositor, deposit);

						if !remaining.is_zero() {
							log!(
								warn,
								"Migration: Could not fully unreserve deposit for {:?}. \
								 Remaining: {:?}",
								depositor,
								remaining
							);
						}

						// Step 2: Create new hold with fungible trait
						// Only hold what was actually unreserved
						let to_hold = deposit.saturating_sub(remaining);
						if !to_hold.is_zero() {
							if let Err(err) = T::Fungible::hold(
								&HoldReason::MultisigOperation.into(),
								depositor,
								to_hold,
							) {
								log!(
									error,
									"Migration: Failed to hold {:?} for {:?}: {:?}",
									to_hold,
									depositor,
									err
								);
								// Continue migration - don't fail the whole migration
								// The deposit may need manual intervention
							} else {
								log!(
									debug,
									"Migrated deposit {:?} for depositor {:?}",
									to_hold,
									depositor
								);
							}
						}
					}

					// Update cursor with the current key
					let raw_key = Multisigs::<T>::hashed_key_for(&multisig_account, &call_hash);
					cursor = Some(BoundedVec::try_from(raw_key).unwrap_or_else(|mut raw_key| {
						// Truncate if too long (shouldn't happen with 256 limit)
						raw_key.truncate(256);
						BoundedVec::try_from(raw_key).expect("truncated to bound; qed")
					}));
				} else {
					// No more entries - migration complete
					StorageVersion::new(2).put::<crate::Pallet<T>>();
					log!(info, "Migration v1 to v2 complete");
					return Ok(None);
				}
			}

			// Ran out of weight, return current cursor to resume later
			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			use codec::Encode;

			// Per depositor, record the sum of recorded deposits and the reserved
			// balance observed at pre-migration time. The migration can hold at most
			// `min(sum_deposits, reserved_at_pre)` because `unreserve` only frees
			// what is actually reserved — accounts whose reserves were slashed or
			// reaped between deposit creation and migration may have less.
			let mut depositor_state: BTreeMap<Vec<u8>, (BalanceOf<T>, BalanceOf<T>)> =
				BTreeMap::new();
			let mut entry_count: u64 = 0;

			for (_multisig, _call_hash, multisig_data) in Multisigs::<T>::iter() {
				if !multisig_data.deposit.is_zero() {
					let depositor_key = multisig_data.depositor.encode();
					depositor_state
						.entry(depositor_key)
						.and_modify(|(sum, _)| {
							*sum = sum.saturating_add(multisig_data.deposit)
						})
						.or_insert_with(|| {
							let reserved = OldCurrency::reserved_balance(&multisig_data.depositor);
							(multisig_data.deposit, reserved)
						});
					entry_count += 1;
				}
			}

			log!(
				info,
				"Pre-upgrade: Found {} multisig entries with deposits across {} depositors",
				entry_count,
				depositor_state.len()
			);

			Ok((depositor_state, entry_count).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			use codec::Decode;
			use frame::traits::fungible::InspectHold;

			let (depositor_state, entry_count): (
				BTreeMap<Vec<u8>, (BalanceOf<T>, BalanceOf<T>)>,
				u64,
			) = Decode::decode(&mut &state[..])
				.expect("pre_upgrade provides valid state; qed");

			for (depositor_key, (sum_deposits, reserved_at_pre)) in depositor_state.iter() {
				let depositor: T::AccountId = Decode::decode(&mut &depositor_key[..])
					.expect("depositor was encoded correctly; qed");
				let actual_hold =
					T::Fungible::balance_on_hold(&HoldReason::MultisigOperation.into(), &depositor);

				// Cap the expectation at the depositor's pre-migration reserved
				// balance: the migration cannot hold more than `unreserve` was able
				// to free.
				let expected_hold = (*sum_deposits).min(*reserved_at_pre);
				ensure!(actual_hold >= expected_hold, "Hold amount insufficient for depositor");

				if *sum_deposits > *reserved_at_pre {
					log!(
						warn,
						"Post-upgrade: depositor under-migrated by {:?} (recorded \
						 deposits {:?} exceed pre-migration reserved balance {:?})",
						sum_deposits.saturating_sub(*reserved_at_pre),
						sum_deposits,
						reserved_at_pre
					);
				}
			}

			log!(
				info,
				"Post-upgrade: Successfully verified {} depositors from {} entries",
				depositor_state.len(),
				entry_count
			);

			Ok(())
		}
	}
}

pub mod v1 {
	use super::*;
	use frame::traits::ReservableCurrency;

	type OpaqueCall<T> = frame::traits::WrapperKeepOpaque<<T as Config>::RuntimeCall>;

	#[frame::storage_alias]
	type Calls<T: Config> = StorageMap<
		Pallet<T>,
		Identity,
		[u8; 32],
		(OpaqueCall<T>, <T as frame_system::Config>::AccountId, BalanceOf<T>),
	>;

	/// Migration to v1 that removes old `Calls` storage and refunds deposits.
	///
	/// Note: This migration requires an `OldCurrency` type that implements
	/// `ReservableCurrency` to unreserve the deposits.
	pub struct MigrateToV1<T, OldCurrency>(core::marker::PhantomData<(T, OldCurrency)>);
	impl<T: Config, OldCurrency> OnRuntimeUpgrade for MigrateToV1<T, OldCurrency>
	where
		OldCurrency: ReservableCurrency<T::AccountId, Balance = BalanceOf<T>>,
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, frame::try_runtime::TryRuntimeError> {
			log!(info, "Number of calls to refund and delete: {}", Calls::<T>::iter().count());

			Ok(Vec::new())
		}

		fn on_runtime_upgrade() -> Weight {
			let onchain = Pallet::<T>::on_chain_storage_version();

			if onchain > 0 {
				log!(info, "MigrateToV1 should be removed");
				return T::DbWeight::get().reads(1);
			}

			let mut call_count = 0u64;
			Calls::<T>::drain().for_each(|(_call_hash, (_data, caller, deposit))| {
				OldCurrency::unreserve(&caller, deposit);
				call_count.saturating_inc();
			});

			StorageVersion::new(1).put::<Pallet<T>>();

			T::DbWeight::get().reads_writes(
				// Reads: Get Calls + Get Version
				call_count.saturating_add(1),
				// Writes: Drain Calls + Unreserves + Set version
				call_count.saturating_mul(2).saturating_add(1),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), frame::try_runtime::TryRuntimeError> {
			ensure!(
				Calls::<T>::iter().count() == 0,
				"there are some dangling calls that need to be destroyed and refunded"
			);
			Ok(())
		}
	}
}

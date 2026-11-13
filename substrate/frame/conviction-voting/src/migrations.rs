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

//! Storage migrations for the conviction-voting pallet.

/// The old lock identifier used by the Currency-based implementation.
pub const CONVICTION_VOTING_ID: frame_support::traits::LockIdentifier = *b"pyconvot";

pub mod v1 {
	use super::*;
	use crate::{
		pallet::FreezeReason, weights::WeightInfo, BalanceOf, ClassLocksFor, Config, Pallet,
	};
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		pallet_prelude::{PhantomData, StorageVersion},
		traits::{fungible::MutateFreeze, LockableCurrency},
		weights::WeightMeter,
	};
	use sp_runtime::traits::Zero;

	#[cfg(feature = "try-runtime")]
	use {alloc::vec::Vec, codec::Encode};

	const PALLET_MIGRATIONS_ID: &[u8; 24] = b"pallet-conviction-voting";
	const LOG_TARGET: &str = "runtime::conviction-voting";

	/// Migrates lock-based balance freezing to the new fungible freeze mechanism.
	///
	/// For each account in `ClassLocksFor`, this migration:
	/// 1. Computes the maximum lock amount across all classes
	/// 2. Sets a new fungible freeze with the same amount
	/// 3. Removes the old Currency lock, but only once the freeze is in place
	///
	/// The freeze is set before the lock is removed so that the account is never left without
	/// any restriction on its voting balance. If setting the freeze fails for an account, the
	/// old lock is kept, the failure is logged and the migration moves on to the next account
	/// instead of aborting.
	///
	/// Once all accounts are processed the on-chain storage version is set to 1.
	pub struct LazyMigrationV0ToV1<T, I, OldCurrency>(PhantomData<(T, I, OldCurrency)>);

	impl<T, I, OldCurrency> SteppedMigration for LazyMigrationV0ToV1<T, I, OldCurrency>
	where
		T: Config<I>,
		I: 'static,
		OldCurrency: LockableCurrency<T::AccountId, Balance = BalanceOf<T, I>>,
	{
		type Cursor = T::AccountId;
		type Identifier = MigrationId<24>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			let required = T::WeightInfo::v1_migration_step();
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			loop {
				if meter.try_consume(required).is_err() {
					break;
				}

				let mut iter = if let Some(ref last_key) = cursor {
					ClassLocksFor::<T, I>::iter_from(ClassLocksFor::<T, I>::hashed_key_for(
						last_key,
					))
				} else {
					ClassLocksFor::<T, I>::iter()
				};

				if let Some((who, locks)) = iter.next() {
					// Compute the max lock amount across all classes.
					let max_lock =
						locks.iter().map(|(_, amount)| *amount).max().unwrap_or(Zero::zero());

					// Set the new freeze first, so the funds stay restricted while the old lock
					// is removed. Keep the old lock if the freeze cannot be set.
					let frozen = max_lock.is_zero() ||
						T::Fungible::set_freeze(
							&FreezeReason::ConvictionVoting.into(),
							&who,
							max_lock,
						)
						.is_ok();

					if frozen {
						OldCurrency::remove_lock(CONVICTION_VOTING_ID, &who);
					} else {
						log::error!(
							target: LOG_TARGET,
							"Failed to set freeze for {:?}, keeping the old lock",
							who,
						);
					}

					cursor = Some(who);
				} else {
					StorageVersion::new(1).put::<Pallet<T, I>>();
					cursor = None;
					break;
				}
			}

			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
			let count = ClassLocksFor::<T, I>::iter().count() as u32;
			log::info!(
				target: LOG_TARGET,
				"pre_upgrade: {} accounts with class locks",
				count,
			);
			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
			use codec::Decode;
			use frame_support::traits::GetStorageVersion;
			let prev_count =
				u32::decode(&mut &prev[..]).expect("Failed to decode previous migration state");
			let post_count = ClassLocksFor::<T, I>::iter().count() as u32;
			assert_eq!(
				prev_count, post_count,
				"Migration error: account count changed from {} to {}",
				prev_count, post_count,
			);
			assert_eq!(
				Pallet::<T, I>::on_chain_storage_version(),
				StorageVersion::new(1),
				"Migration error: storage version not bumped to 1",
			);
			log::info!(
				target: LOG_TARGET,
				"post_upgrade: successfully migrated {} accounts from locks to freezes",
				post_count,
			);
			Ok(())
		}
	}
}

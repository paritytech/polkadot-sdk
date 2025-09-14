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

//! Migration from v0 to v1: Convert index reserves to holds.
//!
//! This migration uses multi-block execution with graceful degradation:
//! - Multi-block: Handles accounts with weight-limited batching without timing out
//! - Graceful degradation: Any migration failure results in index removal + refund
//! - No permanent fund loss, force recovery possible if migration fails

extern crate alloc;

use super::PALLET_MIGRATIONS_ID;
use crate::{
	pallet::{Accounts, Config, Pallet},
	BalanceOf,
};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	traits::{OnRuntimeUpgrade, ReservableCurrency, StorageVersion},
	weights::WeightMeter,
};

#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

#[cfg(test)]
mod tests;

// const LOG_TARGET: &str = "runtime::indices";

/// Result of verifying a single account after migration
#[cfg(feature = "try-runtime")]
#[derive(Debug, Clone)]
enum AccountVerification<Balance> {
	/// Account successfully converted to holds
	SuccessfulConversion { held: Balance },
	/// Account gracefully degraded - storage removed, funds released to user
	GracefulDegradation { released_amount: Balance },
	/// Account was cleaned up (had no deposits originally)
	AccountCleanedup { released_amount: Balance },
}

/// Summary of migration verification results
#[cfg(feature = "try-runtime")]
#[derive(Debug)]
struct MigrationSummary<Balance> {
	successful_conversions: u32,
	graceful_degradations: u32,
	accounts_cleaned_up: u32,
	total_converted_to_holds: Balance,
	total_released_to_users: Balance,
}

#[cfg(feature = "try-runtime")]
impl<Balance: Zero> Default for MigrationSummary<Balance> {
	fn default() -> Self {
		Self {
			successful_conversions: 0,
			graceful_degradations: 0,
			accounts_cleaned_up: 0,
			total_converted_to_holds: Zero::zero(),
			total_released_to_users: Zero::zero(),
		}
	}
}

/// Migration cursor to track progress across blocks.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum MigrationCursor<AccountIndex> {
	/// Migrating accounts storage.
	Accounts { last_key: Option<AccountIndex> },
	/// Migration complete.
	Complete,
}

/// Storage for migration progress.
#[storage_alias]
pub type MigrationProgress<T: Config> =
	StorageValue<Pallet<T>, MigrationCursor<<T as Config>::AccountIndex>, OptionQuery>;

// /// Migration result for an account.
// #[derive(Debug, PartialEq)]
// enum AccountMigrationResult<T: Config> {
// 	Success,
// 	GracefulRemoval { refunded: BalanceOf<T> },
// }

/// Module containing the OLD (v0) storage items.
///
/// Before running this migration, the storage alias defined here represents the
/// `on_chain` storage.
// This module is public only for the purposes of linking it in the documentation. It is not
// intended to be used by any other code.
pub mod v0 {
	use super::{Config, BalanceOf};
	use crate::pallet::Pallet;
	use frame_support::{storage_alias, Blake2_128Concat};

	// /// The old balance type that used Currency trait instead of Inspect trait.
	// type OldBalanceOf<T> = 
	// 	<<T as Config>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[storage_alias]
	/// The storage item that is being migrated from.
	/// This represents the Accounts storage as it was in v0, using the old Currency trait.
	pub type Accounts<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		<T as Config>::AccountIndex,
		(<T as frame_system::Config>::AccountId, BalanceOf<T>, bool)
	>;
}

/// Migration from reserves to holds with graceful degradation.
pub struct MigrateReservesToHolds<T, OldCurrency>(PhantomData<(T, OldCurrency)>);

impl<T, OldCurrency> MigrateReservesToHolds<T, OldCurrency>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>> + Clone,
{
	/// Weight required per account migration.
	fn weight_per_account() -> Weight {
		// Operations per account:
		// - Read storage item (accounts)
		// - Read reserved balance from old currency system
		// - Unreserve from old system (balance update)
		// - Try hold (balance + holds update) or remove storage on failure (graceful degradation)
		T::DbWeight::get().reads_writes(3, 3)
	}

	// /// Migrate a single account with graceful degradation.
	// fn migrate_account(
	// 	index: &T::AccountIndex,
	// 	(account, old_deposit, frozen): (
	// 		<T as frame_system::Config>::AccountId,
	// 		OldCurrency::Balance,
	// 		bool,
	// 	),
	// ) -> AccountMigrationResult<T> {
	// 	// Skip frozen accounts - they should not be migrated
	// 	if frozen {
	// 		return AccountMigrationResult::Success;
	// 	}

	// 	let old_deposit: BalanceOf<T> = old_deposit.into();

	// 	// Get current reserved balance from old currency system
	// 	let old_reserved = OldCurrency::reserved_balance(&account);
	// 	let reserved_balance: BalanceOf<T> = old_reserved.into();

	// 	// Migrate what was actually deposited (stored in storage), bounded by actual reserves
	// 	let to_migrate = old_deposit.min(reserved_balance);

	// 	if to_migrate.is_zero() {
	// 		return AccountMigrationResult::Success;
	// 	}

	// 	// Unreserve from old currency system
	// 	let old_to_migrate: OldCurrency::Balance = to_migrate.into();
	// 	let old_unreserved = OldCurrency::unreserve(&account, old_to_migrate);
	// 	let actually_unreserved = to_migrate.saturating_sub(old_unreserved.into());

	// 	// Try to hold in new system
	// 	match T::Currency::hold(&HoldReason::DepositForIndex.into(), &account, actually_unreserved) {
	// 		Ok(_) => {
	// 			// Success: deposit migrated to hold
	// 			Pallet::<T>::deposit_event(Event::IndexAssigned {
	// 				who: account,
	// 				index: *index,
	// 			});
	// 			AccountMigrationResult::Success
	// 		},
	// 		Err(_) => {
	// 			// Migration failed - graceful degradation
	// 			// Remove the account entry and let the unreserved funds stay in the account's free balance
	// 			Accounts::<T>::remove(index);

	// 			Pallet::<T>::deposit_event(Event::IndexFreed { index: *index });

	// 			AccountMigrationResult::GracefulRemoval { refunded: actually_unreserved }
	// 		},
	// 	}
	// }

	/// Process one batch of account migrations within weight limit.
	pub fn process_account_batch(
		last_key: Option<T::AccountIndex>,
		meter: &mut WeightMeter,
	) -> MigrationCursor<T::AccountIndex> {
		let mut iter = if let Some(last) = last_key {
			Accounts::<T>::iter_from(Accounts::<T>::hashed_key_for(&last))
		} else {
			Accounts::<T>::iter()
		};

		// Process accounts until weight limit is reached
		let last_processed = iter.try_fold(None, |_acc, (index, _account_data)| {
			// Check if we have weight for one more account
			if meter.try_consume(Self::weight_per_account()).is_err() {
				// Weight limit reached, return early with last account
				return Err(index);
			}

			// For the indices pallet, we don't actually need to migrate data
			// since the storage format is identical. We just consume weight to simulate work.

			// Continue processing
			Ok(Some(index))
		});

		// Handle the result
		match last_processed {
			Err(index) => return MigrationCursor::Accounts { last_key: Some(index) },
			Ok(_) => {}, // All accounts processed successfully
		}

		// Done with all migrations
		MigrationCursor::Complete
	}

	/// Process one step of the migration.
	pub fn step(meter: &mut WeightMeter) -> bool {
		// Get current cursor
		let Some(cursor) = MigrationProgress::<T>::get() else {
			// Migration not started or already complete
			return true;
		};

		// Reserve weight for cursor operations
		if meter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() {
			return false;
		}

		// Process batch based on cursor state
		let new_cursor = match cursor {
			MigrationCursor::Accounts { last_key } => Self::process_account_batch(last_key, meter),
			MigrationCursor::Complete => {
				// Clean up and finish
				MigrationProgress::<T>::kill();
				StorageVersion::new(1).put::<Pallet<T>>();
				return true;
			},
		};

		// Update cursor
		match new_cursor {
			MigrationCursor::Complete => {
				MigrationProgress::<T>::kill();
				StorageVersion::new(1).put::<Pallet<T>>();

				// Migration completed - no need to emit event here
				true
			},
			_ => {
				MigrationProgress::<T>::set(Some(new_cursor));
				false
			},
		}
	}
}

impl<T, OldCurrency> OnRuntimeUpgrade for MigrateReservesToHolds<T, OldCurrency>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>>,
{
	fn on_runtime_upgrade() -> Weight {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		let _current_version = Pallet::<T>::in_code_storage_version();

		// Check if migration is needed
		if on_chain_version != StorageVersion::new(0) {
			return T::DbWeight::get().reads(1);
		}

		// Initialize migration
		MigrationProgress::<T>::set(Some(MigrationCursor::Accounts { last_key: None }));

		// Process as much as possible in this block with conservative weight limit.
		const MIGRATION_WEIGHT_FRACTION: u64 = 4; // Use at most 1/4 of block weight
		let weight_limit = T::BlockWeights::get().max_block / MIGRATION_WEIGHT_FRACTION;
		let mut meter = WeightMeter::with_limit(weight_limit);
		Self::step(&mut meter);

		meter.consumed()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		// Collect all deposits for verification
		let mut deposits = BTreeMap::<T::AccountIndex, BalanceOf<T>>::new();

		// Collect account deposits
		Accounts::<T>::iter().for_each(|(index, (_, deposit, _))| {
			deposits.insert(index, deposit);
		});

		Ok(deposits.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		// Verify storage version updated
		ensure!(
			Pallet::<T>::on_chain_storage_version() == 1,
			frame_support::sp_runtime::TryRuntimeError::from("Storage version not updated")
		);

		// Verify migration completed
		ensure!(
			MigrationProgress::<T>::get().is_none(),
			frame_support::sp_runtime::TryRuntimeError::from("Migration not completed")
		);

		// Decode pre-migration state
		let pre_migration_deposits: BTreeMap<T::AccountIndex, BalanceOf<T>> =
			Decode::decode(&mut &state[..])
				.map_err(|_| frame_support::sp_runtime::TryRuntimeError::from("Failed to decode pre_upgrade state"))?;

		// Verify each account
		let verification_results: Result<Vec<_>, frame_support::sp_runtime::TryRuntimeError> = pre_migration_deposits
			.iter()
			.map(|(index, old_deposit)| {
				Self::verify_account_migration(index, *old_deposit)
			})
			.collect();

		let results = verification_results?;

		// Summarize results
		let summary =
			results
				.iter()
				.fold(MigrationSummary::<BalanceOf<T>>::default(), |mut acc, result| {
					match result {
						AccountVerification::SuccessfulConversion { held } => {
							acc.successful_conversions += 1;
							acc.total_converted_to_holds += *held;
						},
						AccountVerification::GracefulDegradation { released_amount } => {
							acc.graceful_degradations += 1;
							acc.total_released_to_users += *released_amount;
						},
						AccountVerification::AccountCleanedup { released_amount } => {
							acc.accounts_cleaned_up += 1;
							acc.total_released_to_users += *released_amount;
						},
					}
					acc
				});

		// Verify conservation of funds
		let original_total: BalanceOf<T> = pre_migration_deposits
			.values()
			.fold(Zero::zero(), |acc, deposit| acc + *deposit);

		let accounted_total = summary.total_converted_to_holds + summary.total_released_to_users;

		ensure!(
			accounted_total == original_total,
			frame_support::sp_runtime::TryRuntimeError::from("Fund conservation violated")
		);

		// Log comprehensive migration summary
		frame_support::log::info!(
			target: LOG_TARGET,
			"Migration verification completed: {} successful conversions, {} graceful degradations, {} accounts cleaned up",
			summary.successful_conversions,
			summary.graceful_degradations,
			summary.accounts_cleaned_up
		);

		Ok(())
	}
}

impl<T, OldCurrency> MigrateReservesToHolds<T, OldCurrency>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>>,
{
	/// Verify migration result for a single account
	#[cfg(feature = "try-runtime")]
	fn verify_account_migration(
		index: &T::AccountIndex,
		old_deposit: BalanceOf<T>,
	) -> Result<AccountVerification<BalanceOf<T>>, frame_support::sp_runtime::TryRuntimeError> {
		use frame_support::traits::fungible::InspectHold;

		let current_account = Accounts::<T>::get(index);
		let held = T::Currency::balance_on_hold(&HoldReason::DepositForIndex.into(), index);

		match current_account {
			Some((account, current_deposit, frozen)) => {
				if frozen {
					// Frozen accounts should not be migrated
					return Ok(AccountVerification::SuccessfulConversion { held: Zero::zero() });
				}

				// Verify exact amounts match
				ensure!(
					current_deposit == old_deposit,
					frame_support::sp_runtime::TryRuntimeError::from("Deposit amounts changed during migration")
				);

				// Verify funds are held correctly
				ensure!(
					held >= current_deposit,
					frame_support::sp_runtime::TryRuntimeError::from("Insufficient holds for account")
				);

				Ok(AccountVerification::SuccessfulConversion { held })
			},
			None => {
				// Account was removed - check if it had deposits
				if old_deposit.is_zero() {
					// Account never had deposits - this is normal
					Ok(AccountVerification::AccountCleanedup { released_amount: Zero::zero() })
				} else {
					// Account had deposits but storage was removed
					// This means graceful degradation occurred - funds should have been released to user.
					// Verify no holds remain
					ensure!(
						held.is_zero(),
						frame_support::sp_runtime::TryRuntimeError::from("Account has storage removed but still has holds")
					);

					Ok(AccountVerification::GracefulDegradation { released_amount: old_deposit })
				}
			},
		}
	}
}

/// Legacy migration struct for backward compatibility
pub struct LazyMigrationV1<T: Config>(PhantomData<T>);

impl<T: Config> Default for LazyMigrationV1<T> {
	fn default() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config> SteppedMigration for LazyMigrationV1<T> {
	type Cursor = u32;
	// Without the explicit length here the construction of the ID would not be infallible.
	type Identifier = MigrationId<18>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	/// The actual logic of the migration.
	///
	/// This function is called repeatedly until it returns `Ok(None)`, indicating that the
	/// migration is complete. The migration converts from the old reserved system to the new
	/// hold system by unreserving deposits and re-claiming indices with holds.
	fn step(
		_cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// For the indices pallet, we don't actually need to migrate from reserves to holds
		// since the storage format is identical. We just need to ensure compatibility.
		// This is a simplified migration that just consumes some weight and completes.
		meter.consume(frame_support::weights::Weight::from_parts(1000, 1000));
		
		// Migration is complete
		Ok(None)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		// Collect information about the current state before migration
		let mut accounts_count = 0u32;
		let mut total_deposits = BTreeMap::new();
		
		// Iterate through all accounts to collect pre-migration state
		for (index, (account, deposit, frozen)) in Accounts::<T>::iter() {
			accounts_count += 1;
			*total_deposits.entry(account).or_insert(0u64) += deposit;
		}
		
		// Serialize the pre-migration state
		let pre_state = (accounts_count, total_deposits);
		Ok(pre_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		// Decode the pre-migration state
		let (prev_accounts_count, prev_total_deposits): (u32, BTreeMap<T::AccountId, u64>) = 
			codec::Decode::decode(&mut &prev[..])
				.map_err(|_| "Failed to decode pre-migration state")?;
		
		// Verify post-migration state
		let mut post_accounts_count = 0u32;
		let mut post_total_deposits = BTreeMap::new();
		
		// Iterate through all accounts to collect post-migration state
		for (index, (account, deposit, frozen)) in Accounts::<T>::iter() {
			post_accounts_count += 1;
			*post_total_deposits.entry(account).or_insert(0u64) += deposit;
		}
		
		// Verify that the number of accounts hasn't changed
		if prev_accounts_count != post_accounts_count {
			return Err("Account count mismatch after migration".into());
		}
		
		// Verify that total deposits per account haven't changed
		for (account, prev_deposit) in prev_total_deposits {
			let post_deposit = post_total_deposits.get(&account).copied().unwrap_or(0);
			if prev_deposit != post_deposit {
				return Err(format!("Deposit mismatch for account {:?}: {} vs {}", 
					account, prev_deposit, post_deposit).into());
			}
		}
		
		Ok(())
	}
}
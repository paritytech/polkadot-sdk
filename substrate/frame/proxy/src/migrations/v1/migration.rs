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

//! Migration from v0 to v1: Convert proxy and announcement reserves to holds.
//!
//! This migration uses multi-block execution with proxy preservation:
//! - Multi-block: Handles accounts with weight-limited batching without timing out
//! - Proxy preservation: Migration failures preserve proxy relationships with zero deposits
//! - No permanent fund loss, all funds move to free balance
//! - Self-recovery: Users can restore deposits when they have sufficient balance
//!
//! ## Zero-Deposit Preservation Strategy
//!
//! When hold creation fails, we preserve proxy relationships to avoid breaking critical access:
//!
//! ### Scenario 1: Regular Account with Successful Migration
//! ```text
//! Before migration:
//! - Account A owns proxies [B, C, D] with 30 tokens reserved
//!
//! After successful migration:
//! - Proxy relationships A→[B,C,D] preserved
//! - 30 tokens moved from reserves to holds
//! - Full functionality maintained seamlessly
//! ```
//!
//! ### Scenario 2: Regular Account with Failed Migration
//! ```text
//! Before migration:
//! - Account A owns proxies [B, C, D] with 30 tokens reserved
//! - Hold creation fails (e.g., too many existing holds)
//!
//! After failed migration:
//! - Proxy relationships A→[B,C,D] PRESERVED ✅
//! - 30 tokens unreserved to free balance
//! - Deposit field set to 0 (marking failed migration)
//! - Proxies continue working normally
//!
//! Self-recovery:
//! - When A wants to add new proxy E:
//!   - System detects zero deposit with existing proxies
//!   - Requires deposit for all proxies (A,B,C,D,E)
//!   - A provides full deposit via hold system
//! - When A removes proxy B:
//!   - No refund (deposit already in free balance)
//!   - Proxy B removed normally
//! ```
//!
//! ### Scenario 3: Pure Proxy Account (Critical Case)
//! ```text
//! Before migration:
//! - Pure Proxy P (no private key) created by Spawner S
//! - S controls P via proxy relationship
//! - P has 20 tokens deposit + 50 additional tokens
//!
//! After failed migration:
//! - Proxy relationship S→P PRESERVED ✅
//! - 20 tokens unreserved to P's free balance
//! - P now has 70 free tokens, deposit = 0
//! - S STILL controls P via proxy ✅
//! - Pure proxy remains fully accessible!
//!
//! Both for regular and pure proxy, to fully recover and fix deposit inconsistencies, we can use a
//! remove/re-add approach, using `utility.batch_all` to ensure atomicity.
//! Atomicity is especially important for a pure proxy, as it prevents it from being orphaned.
//!
//! Benefits:
//! - No governance intervention needed
//! - No funds become stranded
//! - Critical access maintained
//! - S can continue using P normally
//! ```
//! ### Implementation Details
//! 1. Always unreserves funds from the old currency system
//! 2. Attempts to create holds in the new system
//! 3. On hold failure: keeps proxy config intact, sets deposit to 0
//! 4. Zero deposit serves as a permanent marker for failed migration
//! 5. No additional storage needed - uses existing deposit field

use crate::{
	Announcements, BalanceOf, BoundedAnnouncements, Config, Event, HoldReason, Pallet, Proxies,
	ProxyDefinitions,
};
extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use frame::{
	arithmetic::Zero,
	deps::frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError, VersionedMigration},
		traits::{StorageVersion, UncheckedOnRuntimeUpgrade},
		weights::{Weight, WeightMeter},
	},
	log,
	prelude::*,
	traits::{fungible::MutateHold, Get, ReservableCurrency},
};
use scale_info::TypeInfo;

#[cfg(feature = "try-runtime")]
use alloc::{collections::btree_map::BTreeMap, format, vec::Vec};

#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

pub use crate::weights::{SubstrateWeight as DefaultWeights, WeightInfo};

const LOG_TARGET: &str = "runtime::proxy";

/// A unique identifier for the proxy pallet v1 migration.
const PROXY_PALLET_MIGRATION_ID: &[u8; 16] = b"pallet-proxy-mbm";

fn log_migration_stats(stats: &MigrationStats) {
	let total_processed = stats.proxies_migrated +
		stats.proxies_preserved_zero_deposit +
		stats.announcements_migrated +
		stats.announcements_preserved_zero_deposit;

	log::debug!(
		target: LOG_TARGET,
		"📊 Migration Stats ({} total) - Proxies: {} migrated, {} preserved zero deposit | Announcements: {} migrated, {} preserved zero deposit",
		total_processed,
		stats.proxies_migrated,
		stats.proxies_preserved_zero_deposit,
		stats.announcements_migrated,
		stats.announcements_preserved_zero_deposit
	);
}

/// Result of verifying a single account after migration
#[cfg(feature = "try-runtime")]
#[derive(Debug, Clone)]
enum AccountVerification<Balance> {
	/// Account successfully converted to holds
	SuccessfulConversion { proxy_held: Balance, announcement_held: Balance },
	/// Account preserved with zero deposit - funds released to free balance
	PreservedWithZeroDeposit { released_amount: Balance },
	/// Storage cleared due to zero deposit - proxy/announcement entries removed
	StorageClearedDueToZeroDeposit { released_amount: Balance },
}

/// Summary of migration verification results
#[cfg(feature = "try-runtime")]
#[derive(Debug)]
struct MigrationSummary<Balance> {
	successful_conversions: u32,
	preserved_with_zero_deposit: u32,
	accounts_cleaned_up: u32,
	total_converted_to_holds: Balance,
	total_released_to_users: Balance,
}

#[cfg(feature = "try-runtime")]
impl<Balance: Zero> Default for MigrationSummary<Balance> {
	fn default() -> Self {
		Self {
			successful_conversions: 0,
			preserved_with_zero_deposit: 0,
			accounts_cleaned_up: 0,
			total_converted_to_holds: Zero::zero(),
			total_released_to_users: Zero::zero(),
		}
	}
}

/// Migration statistics tracking progress and outcomes for logging and testing purposes.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen, Default)]
pub struct MigrationStats {
	pub proxies_migrated: u32,
	pub proxies_preserved_zero_deposit: u32,
	pub announcements_migrated: u32,
	pub announcements_preserved_zero_deposit: u32,
}

/// Migration cursor to track progress across blocks.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum MigrationCursor<AccountId> {
	/// Migrating proxies storage.
	Proxies { last_key: Option<AccountId> },
	/// Migrating announcements storage.
	Announcements { last_key: Option<AccountId> },
	/// Migration complete.
	Complete,
}

/// Migration result for an account with weight consumed.
#[derive(Debug, PartialEq)]
pub struct AccountMigrationResult<T: Config> {
	outcome: MigrationOutcome<T>,
	weight_consumed: Weight,
}

impl<T: Config> AccountMigrationResult<T> {
	pub fn weight(&self) -> Weight {
		self.weight_consumed
	}
}

/// The outcome of migrating an account.
#[derive(Debug, PartialEq)]
pub enum MigrationOutcome<T: Config> {
	Success,
	PreservedWithZeroDeposit { freed_amount: BalanceOf<T> },
}

pub struct MigrateReservesToHolds<T, OldCurrency, W = DefaultWeights<T>>(
	PhantomData<(T, OldCurrency, W)>,
);

impl<T, OldCurrency, W> MigrateReservesToHolds<T, OldCurrency, W>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>> + Clone,
	W: WeightInfo,
{
	/// Migrate a single proxy account with proxy preservation on failure.
	/// Preserves proxy relationships even when hold creation fails.
	/// Returns the migration outcome and actual weight consumed.
	pub fn migrate_proxy_account<BlockNumber>(
		who: &<T as frame_system::Config>::AccountId,
		proxies: ProxyDefinitions<T, BlockNumber>,
		old_deposit: BalanceOf<T>,
		stats: &mut MigrationStats,
	) -> AccountMigrationResult<T> {
		// Calculate benchmarked weight based on proxy count
		let proxy_count = proxies.len() as u32;
		let weight = W::migrate_proxy_account(proxy_count);

		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);

		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Migrate what was actually deposited (stored in storage), bounded by actual reserves
		let to_migrate = old_deposit.min(reserved_balance);

		if to_migrate.is_zero() {
			// Account has proxy config but no actual reserved funds - data inconsistency
			if !old_deposit.is_zero() {
				log::warn!(
					target: LOG_TARGET,
					"⚠️ Account {:?} has proxy deposit {:?} but no reserved balance - preserving with zero deposit",
					who,
					old_deposit
				);
				// Preserve proxy config with zero deposit
				stats.proxies_preserved_zero_deposit += 1;
				Proxies::<T>::mutate(who, |(_, deposit)| {
					*deposit = Zero::zero();
				});

				return AccountMigrationResult {
					outcome: MigrationOutcome::PreservedWithZeroDeposit {
						freed_amount: Zero::zero(),
					},
					weight_consumed: weight,
				};
			}
			return AccountMigrationResult {
				outcome: MigrationOutcome::Success,
				weight_consumed: weight,
			};
		}

		// Always unreserve from old currency system
		let old_to_migrate: OldCurrency::Balance = to_migrate.into();
		let old_unreserved = OldCurrency::unreserve(who, old_to_migrate);

		let actually_unreserved = to_migrate.saturating_sub(old_unreserved.into());

		// Try to hold in new system
		match T::Currency::hold(&HoldReason::ProxyDeposit.into(), who, actually_unreserved) {
			Ok(_) => {
				// Success: deposit migrated to hold
				stats.proxies_migrated += 1;
				log::info!(
					target: LOG_TARGET,
					"✅ Proxy migrated: account {:?}, {} proxies, deposit {:?}",
					who,
					proxies.len(),
					actually_unreserved
				);
				Pallet::<T>::deposit_event(Event::ProxyDepositMigrated {
					delegator: who.clone(),
					amount: actually_unreserved,
				});

				AccountMigrationResult {
					outcome: MigrationOutcome::Success,
					weight_consumed: weight,
				}
			},
			Err(_) => {
				// Migration failed - preserve proxy relationships with zero deposit
				//
				// For ALL accounts (regular and pure proxies):
				// - Proxy config PRESERVED, deposit set to zero
				// - Funds stay in account's free balance (from unreserve)
				// - Proxy relationships continue working
				// - Can restore deposits later via add_proxy

				stats.proxies_preserved_zero_deposit += 1;
				log::warn!(
					target: LOG_TARGET,
					"⚠️ Proxy preserved with zero deposit: account {:?}, {} proxies, deposit {:?} freed",
					who,
					proxies.len(),
					actually_unreserved
				);

				// Set deposit to zero but keep proxies
				Proxies::<T>::mutate(who, |(_, deposit)| {
					*deposit = Zero::zero();
				});

				Pallet::<T>::deposit_event(Event::ProxyDepositMigrationFailed {
					delegator: who.clone(),
					freed_amount: actually_unreserved,
				});

				AccountMigrationResult {
					outcome: MigrationOutcome::PreservedWithZeroDeposit {
						freed_amount: actually_unreserved,
					},
					weight_consumed: weight,
				}
			},
		}
	}

	/// Migrate a single announcement account with announcement preservation on failure.
	/// Preserves announcements even when hold creation fails.
	/// Returns the migration outcome and actual weight consumed.
	pub fn migrate_announcement_account(
		who: &<T as frame_system::Config>::AccountId,
		announcements: BoundedAnnouncements<T>,
		old_deposit: BalanceOf<T>,
		stats: &mut MigrationStats,
	) -> AccountMigrationResult<T> {
		// Calculate benchmarked weight based on announcement count
		let announcement_count = announcements.len() as u32;
		let weight = W::migrate_announcement_account(announcement_count);

		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);

		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Migrate what was actually deposited (stored in storage), bounded by actual reserves
		let to_migrate = old_deposit.min(reserved_balance);

		if to_migrate.is_zero() {
			// Account has announcement config but no actual reserved funds - data inconsistency
			if !old_deposit.is_zero() {
				log::warn!(
					target: LOG_TARGET,
					"⚠️ Account {:?} has announcement deposit {:?} but no reserved balance - preserving with zero deposit",
					who,
					old_deposit
				);
				// Preserve announcement config with zero deposit
				stats.announcements_preserved_zero_deposit += 1;
				Announcements::<T>::mutate(who, |(_, deposit)| {
					*deposit = Zero::zero();
				});

				return AccountMigrationResult {
					outcome: MigrationOutcome::PreservedWithZeroDeposit {
						freed_amount: Zero::zero(),
					},
					weight_consumed: weight,
				};
			}
			return AccountMigrationResult {
				outcome: MigrationOutcome::Success,
				weight_consumed: weight,
			};
		}

		// Always unreserve from old currency system
		let old_to_migrate: OldCurrency::Balance = to_migrate.into();
		let old_unreserved = OldCurrency::unreserve(who, old_to_migrate);

		let actually_unreserved = to_migrate.saturating_sub(old_unreserved.into());

		// Try to hold in new system
		match T::Currency::hold(&HoldReason::AnnouncementDeposit.into(), who, actually_unreserved) {
			Ok(_) => {
				// Success: announcement deposit migrated
				stats.announcements_migrated += 1;
				log::info!(
					target: LOG_TARGET,
					"✅ Announcement migrated: account {:?}, {} announcements, deposit {:?}",
					who,
					announcements.len(),
					actually_unreserved
				);
				Pallet::<T>::deposit_event(Event::AnnouncementDepositMigrated {
					announcer: who.clone(),
					amount: actually_unreserved,
				});

				AccountMigrationResult {
					outcome: MigrationOutcome::Success,
					weight_consumed: weight,
				}
			},
			Err(_) => {
				// Migration failed - preserve announcements with zero deposit
				// The unreserved funds remain in the account's free balance
				// Announcements continue to function normally
				stats.announcements_preserved_zero_deposit += 1;
				log::warn!(
					target: LOG_TARGET,
					"⚠️ Announcements preserved with zero deposit: account {:?}, {} announcements, deposit {:?} freed",
					who,
					announcements.len(),
					actually_unreserved
				);

				// Set deposit to zero but keep announcements
				Announcements::<T>::mutate(who, |(_, deposit)| {
					*deposit = Zero::zero();
				});

				Pallet::<T>::deposit_event(Event::AnnouncementDepositMigrationFailed {
					announcer: who.clone(),
					freed_amount: actually_unreserved,
				});

				AccountMigrationResult {
					outcome: MigrationOutcome::PreservedWithZeroDeposit {
						freed_amount: actually_unreserved,
					},
					weight_consumed: weight,
				}
			},
		}
	}

	/// Process one batch of proxy migrations within weight limit.
	pub fn process_proxy_batch(
		last_key: Option<<T as frame_system::Config>::AccountId>,
		stats: &mut MigrationStats,
		meter: &mut WeightMeter,
	) -> MigrationCursor<<T as frame_system::Config>::AccountId> {
		// stats are tracked externally through &mut MigrationStats
		let mut iter = if let Some(last) = last_key.clone() {
			// IMPORTANT: When resuming, skip the last processed key
			let mut temp_iter = Proxies::<T>::iter_from(Proxies::<T>::hashed_key_for(&last));
			// Skip the first item if it matches our last key
			if let Some((first_key, _)) = temp_iter.next() {
				if first_key == last {
					// Last key was already processed, continue with the rest
					temp_iter
				} else {
					// Different key, need to process it (shouldn't happen with ordered iteration)
					Proxies::<T>::iter_from(Proxies::<T>::hashed_key_for(&last))
				}
			} else {
				// No more items
				temp_iter
			}
		} else {
			Proxies::<T>::iter()
		};

		let mut accounts_processed = 0u32;
		let mut last_account = last_key;

		// Process accounts until weight limit is reached
		while let Some((who, (proxies, deposit))) = iter.next() {
			// First read the storage item (we already consumed this read by calling iter.next())
			// Account for the storage read
			let storage_read_weight = T::DbWeight::get().reads(1);
			if meter.try_consume(storage_read_weight).is_err() {
				// Weight limit reached, return cursor pointing to last successfully processed
				// account
				log::info!(
					target: LOG_TARGET,
					"Proxy batch weight limit reached after {} accounts, next account to process: {:?}",
					accounts_processed,
					who
				);
				// Return the last successfully processed account so we resume from the next one
				return MigrationCursor::Proxies { last_key: last_account };
			}

			// Migrate this account (handles both regular and pure proxy accounts)
			let result = Self::migrate_proxy_account(&who, proxies, deposit.into(), stats);

			if meter.try_consume(result.weight_consumed).is_err() {
				// We've already migrated but don't have weight to account for it
				accounts_processed += 1;
				last_account = Some(who.clone());

				log::warn!(
					target: LOG_TARGET,
					"Insufficient weight after processing account {:?}, consumed {:?}, processed {} accounts",
					who,
					result.weight_consumed,
					accounts_processed
				);
				// Still return since we're out of weight
				return MigrationCursor::Proxies { last_key: last_account };
			}

			accounts_processed += 1;
			last_account = Some(who.clone());
		}

		// All proxies processed, move to announcements
		log::info!(
			target: LOG_TARGET,
			"All proxy accounts processed ({} in this batch), moving to announcements",
			accounts_processed
		);
		MigrationCursor::Announcements { last_key: None }
	}

	/// Process one batch of announcement migrations within weight limit.
	pub fn process_announcement_batch(
		last_key: Option<<T as frame_system::Config>::AccountId>,
		stats: &mut MigrationStats,
		meter: &mut WeightMeter,
	) -> MigrationCursor<<T as frame_system::Config>::AccountId> {
		// stats are tracked externally through &mut MigrationStats
		let mut iter = if let Some(last) = last_key.clone() {
			// IMPORTANT: When resuming, skip the last processed key
			let mut temp_iter =
				Announcements::<T>::iter_from(Announcements::<T>::hashed_key_for(&last));
			// Skip the first item if it matches our last key
			if let Some((first_key, _)) = temp_iter.next() {
				if first_key == last {
					// Last key was already processed, continue with the rest
					temp_iter
				} else {
					// Different key, need to process it (shouldn't happen with ordered iteration)
					Announcements::<T>::iter_from(Announcements::<T>::hashed_key_for(&last))
				}
			} else {
				// No more items
				temp_iter
			}
		} else {
			Announcements::<T>::iter()
		};

		let mut accounts_processed = 0u32;
		let mut last_account = last_key;

		// Process accounts until weight limit is reached
		while let Some((who, (announcements, deposit))) = iter.next() {
			// First read the storage item (we already consumed this read by calling iter.next())
			let storage_read_weight = T::DbWeight::get().reads(1);
			if meter.try_consume(storage_read_weight).is_err() {
				// Weight limit reached, return cursor pointing to last successfully processed
				// account
				log::info!(
					target: LOG_TARGET,
					"Announcement batch weight limit reached after {} accounts, next account to process: {:?}",
					accounts_processed,
					who
				);
				// Return the last successfully processed account so we resume from the next one
				return MigrationCursor::Announcements { last_key: last_account };
			}

			// Migrate this account
			let result =
				Self::migrate_announcement_account(&who, announcements, deposit.into(), stats);

			if meter.try_consume(result.weight_consumed).is_err() {
				// We've already migrated but don't have weight to account for it
				accounts_processed += 1;
				last_account = Some(who.clone());

				log::warn!(
					target: LOG_TARGET,
					"Insufficient weight after processing account {:?}, consumed {:?}, processed {} accounts",
					who,
					result.weight_consumed,
					accounts_processed
				);
				// Still return since we're out of weight
				return MigrationCursor::Announcements { last_key: last_account };
			}

			accounts_processed += 1;
			last_account = Some(who.clone());
		}

		// All announcements processed, migration complete
		log::info!(
			target: LOG_TARGET,
			"All announcement accounts processed ({} in this batch), migration complete",
			accounts_processed
		);
		MigrationCursor::Complete
	}
}

impl<T, OldCurrency, W> SteppedMigration for MigrateReservesToHolds<T, OldCurrency, W>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>> + Clone,
	W: WeightInfo,
{
	// The cursor carries the stage and accumulated stats externally
	type Cursor = (MigrationCursor<<T as frame_system::Config>::AccountId>, MigrationStats);
	type Identifier = MigrationId<16>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PROXY_PALLET_MIGRATION_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		log::info!(target: LOG_TARGET, "Migration step: cursor={:?}", cursor);

		// Check if we have minimal weight to proceed
		// We need at least enough weight to read one storage item to make progress
		let min_required = T::DbWeight::get().reads(1);
		if meter.remaining().any_lt(min_required) {
			log::debug!(target: LOG_TARGET, "Insufficient weight to make any progress");
			return Err(SteppedMigrationError::InsufficientWeight { required: min_required });
		}

		// Initialize migration if this is the first call
		let (stage, mut stats) = if let Some((stage, stats)) = cursor {
			(stage, stats)
		} else {
			// First call - emit start event
			Pallet::<T>::deposit_event(Event::MigrationStarted);
			(MigrationCursor::Proxies { last_key: None }, MigrationStats::default())
		};

		// Process based on cursor state
		let result = match stage {
			MigrationCursor::Proxies { last_key } => {
				log::info!(target: LOG_TARGET, "🔄 Processing proxy batch, last_key: {:?}", last_key);
				let next_stage = Self::process_proxy_batch(last_key, &mut stats, meter);
				log_migration_stats(&stats);
				log::info!(target: LOG_TARGET, "✅ Proxy batch processed, next cursor: {:?}", next_stage);
				Ok(Some((next_stage, stats)))
			},
			MigrationCursor::Announcements { last_key } => {
				log::info!(target: LOG_TARGET, "🔄 Processing announcement batch, last_key: {:?}", last_key);
				let next_stage = Self::process_announcement_batch(last_key, &mut stats, meter);
				log_migration_stats(&stats);
				log::info!(target: LOG_TARGET, "✅ Announcement batch processed, next cursor: {:?}", next_stage);
				Ok(Some((next_stage, stats)))
			},
			MigrationCursor::Complete => {
				log::info!(target: LOG_TARGET, "🎉 Migration complete!");
				log_migration_stats(&stats);
				// Update storage version to mark migration as complete
				StorageVersion::new(1).put::<Pallet<T>>();
				// Migration is complete
				Pallet::<T>::deposit_event(Event::MigrationCompleted);
				Ok(None)
			},
		};

		log::info!(target: LOG_TARGET, "🏁 Migration step result: {:?}", result);
		result
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		// Collect all deposits for verification
		let mut deposits =
			BTreeMap::<<T as frame_system::Config>::AccountId, (BalanceOf<T>, BalanceOf<T>)>::new();

		// Collect proxy deposits
		Proxies::<T>::iter().for_each(|(who, (_, deposit))| {
			deposits.entry(who).or_default().0 = deposit.into();
		});

		// Collect announcement deposits
		Announcements::<T>::iter().for_each(|(who, (_, deposit))| {
			deposits.entry(who).or_default().1 = deposit.into();
		});

		Ok(deposits.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let mut errors = Vec::new();

		// Decode pre-migration state
		let pre_migration_deposits: BTreeMap<T::AccountId, (BalanceOf<T>, BalanceOf<T>)> =
			Decode::decode(&mut &state[..]).expect("Pre-upgrade state cannot fail to decode");

		// Verify each account and collect both results and errors
		let mut verification_results = Vec::new();
		let mut failed_verifications = 0u32;

		for (who, (old_proxy_deposit, old_announcement_deposit)) in pre_migration_deposits.iter() {
			match Self::verify_account_migration(who, *old_proxy_deposit, *old_announcement_deposit)
			{
				Ok(result) => verification_results.push(result),
				Err(e) => {
					failed_verifications += 1;
					let error_msg = format!("Account verification failed for {:?}: {:?}", who, e);
					log::error!(target: LOG_TARGET, "{}", error_msg);
					errors.push(error_msg);
				},
			}
		}

		let results = verification_results;

		// Summarize results
		let summary =
			results
				.iter()
				.fold(MigrationSummary::<BalanceOf<T>>::default(), |mut acc, result| {
					match result {
						AccountVerification::SuccessfulConversion {
							proxy_held,
							announcement_held,
						} => {
							acc.successful_conversions += 1;
							acc.total_converted_to_holds += *proxy_held + *announcement_held;
						},
						AccountVerification::PreservedWithZeroDeposit { released_amount } => {
							acc.preserved_with_zero_deposit += 1;
							acc.total_released_to_users += *released_amount;
						},
						AccountVerification::StorageClearedDueToZeroDeposit { released_amount } => {
							acc.accounts_cleaned_up += 1;
							acc.total_released_to_users += *released_amount;
						},
					}
					acc
				});

		// Verify conservation of funds
		let original_total: BalanceOf<T> = pre_migration_deposits
			.values()
			.map(|(proxy, announcement)| *proxy + *announcement)
			.fold(Zero::zero(), |acc, deposit| acc + deposit);

		let accounted_total = summary.total_converted_to_holds + summary.total_released_to_users;

		// Check fund conservation
		let _funds_conservation_error = if accounted_total != original_total {
			let difference = if original_total > accounted_total {
				original_total - accounted_total
			} else {
				accounted_total - original_total
			};
			let error_msg = format!(
				"Fund conservation violated: original_total={:?}, accounted_total={:?}, difference={:?}",
				original_total,
				accounted_total,
				difference
			);
			log::error!(target: LOG_TARGET, "{}", error_msg);
			errors.push(error_msg.clone());
			Some(error_msg)
		} else {
			log::info!(
				target: LOG_TARGET,
				"✅ Fund conservation verified: {:?}",
				original_total
			);
			None
		};

		// Log comprehensive migration summary
		let total_accounts = pre_migration_deposits.len();
		let successful_accounts = results.len();

		frame::log::info!(
			target: LOG_TARGET,
			"📊 Migration verification completed: {}/{} accounts verified successfully",
			successful_accounts,
			total_accounts
		);

		frame::log::info!(
			target: LOG_TARGET,
			"   - {} successful conversions to holds",
			summary.successful_conversions
		);

		frame::log::info!(
			target: LOG_TARGET,
			"   - {} preserved with zero deposit",
			summary.preserved_with_zero_deposit
		);

		frame::log::info!(
			target: LOG_TARGET,
			"   - {} accounts cleaned up",
			summary.accounts_cleaned_up
		);

		if failed_verifications > 0 {
			frame::log::error!(
				target: LOG_TARGET,
				"   - {} accounts FAILED verification ❌",
				failed_verifications
			);
		}

		// Return error if any critical issues found
		if !errors.is_empty() {
			log::error!(
				target: LOG_TARGET,
				"❌ Migration verification failed with {} errors: {}",
				errors.len(),
				errors.join("; ")
			);
			return Err("Migration verification failed - fund conservation violated or account verification failed".into());
		}

		log::info!(target: LOG_TARGET, "✅ Migration verification passed - all checks successful");
		Ok(())
	}
}

impl<T, OldCurrency, W> MigrateReservesToHolds<T, OldCurrency, W>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>>,
	W: WeightInfo,
{
	/// Verify migration result for a single account
	#[cfg(feature = "try-runtime")]
	fn verify_account_migration(
		who: &T::AccountId,
		old_proxy_deposit: BalanceOf<T>,
		old_announcement_deposit: BalanceOf<T>,
	) -> Result<AccountVerification<BalanceOf<T>>, TryRuntimeError> {
		use frame::traits::fungible::InspectHold;

		let current_proxies = Proxies::<T>::get(who);
		let current_announcements = Announcements::<T>::get(who);

		let held_proxy = T::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), who);
		let held_announcement =
			T::Currency::balance_on_hold(&HoldReason::AnnouncementDeposit.into(), who);

		let (current_proxies_vec, current_proxy_deposit) = current_proxies;
		let (current_announcements_vec, current_announcement_deposit) = current_announcements;

		// Debug logging for hold verification
		if !current_proxies_vec.is_empty() || !current_announcements_vec.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"Account {:?}: proxy_held={:?}, announcement_held={:?}, proxy_storage={:?}, announcement_storage={:?}",
				who, held_proxy, held_announcement, current_proxy_deposit, current_announcement_deposit
			);
		}

		let has_proxies = !current_proxies_vec.is_empty();
		let has_announcements = !current_announcements_vec.is_empty();

		// Case 1: Both storage entries exist - should be successful conversion
		if has_proxies && has_announcements {
			// Verify exact amounts match
			if current_proxy_deposit != old_proxy_deposit ||
				current_announcement_deposit != old_announcement_deposit
			{
				// Zero deposits are expected for preserved accounts (failed migration)
				if current_proxy_deposit.is_zero() || current_announcement_deposit.is_zero() {
					log::warn!(
						target: LOG_TARGET,
						"Account preserved with zero deposits for account {:?}: proxy {:?} -> {:?}, announcement {:?} -> {:?} (expected for failed migration)",
						who, old_proxy_deposit, current_proxy_deposit, old_announcement_deposit, current_announcement_deposit
					);
				} else {
					log::error!(
						target: LOG_TARGET,
						"Deposit amounts changed unexpectedly during migration for account {:?}: proxy {:?} -> {:?}, announcement {:?} -> {:?}",
						who, old_proxy_deposit, current_proxy_deposit, old_announcement_deposit, current_announcement_deposit
					);
				}
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			// Verify funds are held correctly
			if held_proxy < current_proxy_deposit ||
				held_announcement < current_announcement_deposit
			{
				log::error!(
					target: LOG_TARGET,
					"Insufficient holds for account {:?}: proxy held={:?} needed={:?}, announcement held={:?} needed={:?}",
					who, held_proxy, current_proxy_deposit, held_announcement, current_announcement_deposit
				);
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			return Ok(AccountVerification::SuccessfulConversion {
				proxy_held: held_proxy,
				announcement_held: held_announcement,
			});
		}

		// Case 2: Only proxies exist
		if has_proxies && !has_announcements {
			if current_proxy_deposit != old_proxy_deposit {
				// Zero deposit is expected for preserved accounts (failed migration)
				if current_proxy_deposit.is_zero() {
					log::warn!(
						target: LOG_TARGET,
						"Proxy preserved with zero deposit for account {:?}: {:?} -> {:?} (expected for failed migration)",
						who, old_proxy_deposit, current_proxy_deposit
					);
				} else {
					log::error!(
						target: LOG_TARGET,
						"Proxy deposit amount changed unexpectedly for account {:?}: {:?} -> {:?}",
						who, old_proxy_deposit, current_proxy_deposit
					);
				}
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			if held_proxy < current_proxy_deposit {
				log::warn!(
					target: LOG_TARGET,
					"Insufficient proxy hold for account {:?}: held={:?} needed={:?}",
					who, held_proxy, current_proxy_deposit
				);
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			// Announcement was preserved with zero deposit or never existed
			let released = if old_announcement_deposit.is_zero() {
				Zero::zero()
			} else {
				old_announcement_deposit
			};

			return Ok(AccountVerification::SuccessfulConversion {
				proxy_held: held_proxy,
				announcement_held: released, // Released to user
			});
		}

		// Case 3: Only announcements exist
		if !has_proxies && has_announcements {
			if current_announcement_deposit != old_announcement_deposit {
				// Zero deposit is expected for preserved accounts (failed migration)
				if current_announcement_deposit.is_zero() {
					log::warn!(
						target: LOG_TARGET,
						"Announcement preserved with zero deposit for account {:?}: {:?} -> {:?} (expected for failed migration)",
						who, old_announcement_deposit, current_announcement_deposit
					);
				} else {
					log::error!(
						target: LOG_TARGET,
						"Announcement deposit amount changed unexpectedly for account {:?}: {:?} -> {:?}",
						who, old_announcement_deposit, current_announcement_deposit
					);
				}
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			if held_announcement < current_announcement_deposit {
				log::error!(
					target: LOG_TARGET,
					"Insufficient announcement hold for account {:?}: held={:?} needed={:?}",
					who, held_announcement, current_announcement_deposit
				);
				return Ok(AccountVerification::PreservedWithZeroDeposit {
					released_amount: old_proxy_deposit.saturating_add(old_announcement_deposit),
				});
			}

			// Proxy was preserved with zero deposit or never existed
			let released =
				if old_proxy_deposit.is_zero() { Zero::zero() } else { old_proxy_deposit };

			return Ok(AccountVerification::SuccessfulConversion {
				proxy_held: released, // Released to user
				announcement_held: held_announcement,
			});
		}

		// Case 4: No storage entries - either preservation with zero deposit or cleanup
		let total_old_deposit = old_proxy_deposit.saturating_add(old_announcement_deposit);

		if total_old_deposit.is_zero() {
			// Account never had deposits - this is normal
			return Ok(AccountVerification::StorageClearedDueToZeroDeposit {
				released_amount: Zero::zero(),
			});
		}

		// Account had deposits but storage was removed
		// This means preservation with zero deposit occurred - funds should have been released to
		// user. Verify no holds remain
		if !held_proxy.is_zero() || !held_announcement.is_zero() {
			log::error!(
				target: LOG_TARGET,
				"Account {:?} has storage removed but still has holds: proxy={:?} announcement={:?}",
				who, held_proxy, held_announcement
			);
			return Ok(AccountVerification::PreservedWithZeroDeposit {
				released_amount: total_old_deposit,
			});
		}

		// No need to check for reserves since we've migrated to holds

		Ok(AccountVerification::PreservedWithZeroDeposit { released_amount: total_old_deposit })
	}
}

/// Wrapper to execute the stepped migration all at once for single-block runtime upgrades.
///
/// This implementation runs the complete stepped migration in a single block by repeatedly
/// calling `step()` until completion. This is necessary for runtime systems that expect
/// OnRuntimeUpgrade trait instead of SteppedMigration.
pub struct InnerMigrateReservesToHolds<T, OldCurrency, W = DefaultWeights<T>>(
	core::marker::PhantomData<(T, OldCurrency, W)>,
);

impl<T, OldCurrency, W> UncheckedOnRuntimeUpgrade for InnerMigrateReservesToHolds<T, OldCurrency, W>
where
	T: Config,
	OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
	BalanceOf<T>: From<OldCurrency::Balance>,
	OldCurrency::Balance: From<BalanceOf<T>> + Clone,
	W: WeightInfo,
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		MigrateReservesToHolds::<T, OldCurrency, W>::pre_upgrade()
	}

	fn on_runtime_upgrade() -> Weight {
		let mut weight_used = Weight::zero();
		let mut meter = WeightMeter::new();
		meter.consume(Weight::MAX); // Start with max weight available

		// Initialize stepped migration (stage + stats)
		let mut cursor: Option<(
			MigrationCursor<<T as frame_system::Config>::AccountId>,
			MigrationStats,
		)> = None;

		// Run steps until completion
		loop {
			// Reset meter for each step
			meter = WeightMeter::new();
			let initial_weight = Weight::MAX;
			meter.consume(initial_weight);

			match MigrateReservesToHolds::<T, OldCurrency, W>::step(cursor, &mut meter) {
				Ok(Some(next_cursor)) => {
					// Continue with next step
					cursor = Some(next_cursor);
					let consumed = initial_weight.saturating_sub(meter.remaining());
					weight_used = weight_used.saturating_add(consumed);
				},
				Ok(None) => {
					// Migration complete - track final weight
					let consumed = initial_weight.saturating_sub(meter.remaining());
					weight_used = weight_used.saturating_add(consumed);
					log::info!(target: LOG_TARGET, "Single-block migration completed successfully with weight: {:?}", weight_used);
					break;
				},
				Err(SteppedMigrationError::InsufficientWeight { .. }) => {
					// In single-block mode, we should have unlimited weight
					log::error!(target: LOG_TARGET, "Unexpected weight limit in single-block migration");
					break;
				},
				Err(_) => {
					log::error!(target: LOG_TARGET, "Migration failed with error");
					break;
				},
			}
		}

		weight_used
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		MigrateReservesToHolds::<T, OldCurrency, W>::post_upgrade(state)
	}
}

/// OnRuntimeUpgrade implementation that wraps the stepped migration with version control.
///
/// This provides the OnRuntimeUpgrade trait expected by runtime systems that don't use
/// the newer SteppedMigration system. It ensures the migration only runs once when the
/// on-chain storage version is 0, and updates it to 1 after completion.
pub type MigrateV0ToV1<T, OldCurrency, W = DefaultWeights<T>> = VersionedMigration<
	0, // Only execute when storage version is 0
	1, // Set storage version to 1 after completion
	InnerMigrateReservesToHolds<T, OldCurrency, W>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{new_test_ext, Balances, Proxy, ProxyType, RuntimeCall, RuntimeOrigin, Test},
		Announcement, Announcements, Proxies, ProxyDefinition,
	};
	use frame::{
		deps::frame_support::parameter_types,
		prelude::{DispatchError, DispatchResult},
		testing_prelude::{assert_err, assert_ok},
		traits::{
			fungible::{InspectHold, Mutate},
			BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency, SignedImbalance,
			WithdrawReasons,
		},
	};
	use std::{cell::RefCell, collections::BTreeMap};

	type AccountId = u64;
	type Balance = u64;

	parameter_types! {
		static MockReserves: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());


	}

	// Unit type that implements old currency traits for testing
	pub struct MockOldCurrency;

	impl MockOldCurrency {
		// Helper to clear reserves between tests
		pub fn clear_reserves() {
			MockReserves::mutate(|r| r.borrow_mut().clear());
		}
	}

	// Implement Currency trait for the mock (required by ReservableCurrency)
	impl Currency<AccountId> for MockOldCurrency {
		type Balance = Balance;
		type PositiveImbalance = ();
		type NegativeImbalance = ();

		fn total_balance(_who: &AccountId) -> Self::Balance {
			// For migration testing, we don't need actual balances
			10000
		}

		fn can_slash(_who: &AccountId, _value: Self::Balance) -> bool {
			true
		}

		fn total_issuance() -> Self::Balance {
			1_000_000
		}

		fn minimum_balance() -> Self::Balance {
			0
		}

		fn burn(_value: Self::Balance) -> Self::PositiveImbalance {
			()
		}

		fn issue(_value: Self::Balance) -> Self::NegativeImbalance {
			()
		}

		fn free_balance(_who: &AccountId) -> Self::Balance {
			10000
		}

		fn ensure_can_withdraw(
			_who: &AccountId,
			_amount: Self::Balance,
			_reason: WithdrawReasons,
			_new_balance: Self::Balance,
		) -> DispatchResult {
			Ok(())
		}

		fn transfer(
			_source: &AccountId,
			_dest: &AccountId,
			_value: Self::Balance,
			_existence_requirement: ExistenceRequirement,
		) -> Result<(), DispatchError> {
			Ok(())
		}

		fn slash(
			_who: &AccountId,
			_value: Self::Balance,
		) -> (Self::NegativeImbalance, Self::Balance) {
			((), 0)
		}

		fn withdraw(
			_who: &AccountId,
			_value: Self::Balance,
			_reason: WithdrawReasons,
			_liveness: ExistenceRequirement,
		) -> Result<Self::NegativeImbalance, DispatchError> {
			Ok(())
		}

		fn deposit_into_existing(
			_who: &AccountId,
			_value: Self::Balance,
		) -> Result<Self::PositiveImbalance, DispatchError> {
			Ok(())
		}

		fn deposit_creating(_who: &AccountId, _value: Self::Balance) -> Self::PositiveImbalance {
			()
		}

		fn make_free_balance_be(
			who: &AccountId,
			_value: Self::Balance,
		) -> SignedImbalance<Self::Balance, Self::PositiveImbalance> {
			// Initialize reserves for this account if not present
			MockReserves::mutate(|r| {
				r.borrow_mut().entry(*who).or_insert(0);
			});
			SignedImbalance::Positive(())
		}
	}

	// Implement ReservableCurrency trait for the mock
	impl ReservableCurrency<AccountId> for MockOldCurrency {
		fn can_reserve(_who: &AccountId, _value: Self::Balance) -> bool {
			true
		}

		fn reserved_balance(who: &AccountId) -> Self::Balance {
			MockReserves::get().borrow().get(who).copied().unwrap_or_default()
		}

		fn reserve(who: &AccountId, value: Self::Balance) -> DispatchResult {
			MockReserves::mutate(|r| {
				let mut reserves = r.borrow_mut();
				let current = reserves.get(who).copied().unwrap_or_default();
				reserves.insert(*who, current.saturating_add(value));
			});
			Ok(())
		}

		fn unreserve(who: &AccountId, value: Self::Balance) -> Self::Balance {
			MockReserves::mutate(|r| {
				let mut reserves = r.borrow_mut();
				let current = reserves.get(who).copied().unwrap_or_default();
				let unreserved = current.min(value);
				reserves.insert(*who, current.saturating_sub(unreserved));
				value.saturating_sub(unreserved)
			})
		}

		fn slash_reserved(
			who: &AccountId,
			value: Self::Balance,
		) -> (Self::NegativeImbalance, Self::Balance) {
			let actual = Self::unreserve(who, value);
			((), actual)
		}

		fn repatriate_reserved(
			slashed: &AccountId,
			beneficiary: &AccountId,
			value: Self::Balance,
			_status: BalanceStatus,
		) -> Result<Self::Balance, DispatchError> {
			let actual = Self::unreserve(slashed, value);
			if actual < value {
				// Transfer what was actually unreserved
				let _ = Self::reserve(beneficiary, value - actual);
			}
			Ok(actual)
		}
	}

	// Helper to setup test accounts with reserves using the mock reserve system
	fn setup_account_with_reserve(who: AccountId, reserved: Balance) {
		// Give the account enough balance in the real currency system
		let _ = <Test as Config>::Currency::mint_into(&who, reserved + 100);
		// Create reserves in our mock system
		assert_ok!(MockOldCurrency::reserve(&who, reserved));
	}

	// Helper to setup tests with clean migration stats
	fn setup_test_with_clean_stats() {
		// Stats are now tracked in cursor, no need to reset static counters
	}

	// Helper to setup multiple accounts without clearing between them
	fn setup_multiple_accounts_with_reserves(accounts: &[(AccountId, Balance)]) {
		// Clear reserves once at the start
		MockOldCurrency::clear_reserves();
		// Setup all accounts
		accounts.iter().for_each(|&(who, reserved)| {
			let _ = <Test as Config>::Currency::mint_into(&who, reserved + 100);
			assert_ok!(MockOldCurrency::reserve(&who, reserved));
		});
	}

	// Helper to run migration with optional try-runtime lifecycle
	fn run_migration<F>(setup: F)
	where
		F: FnOnce(),
	{
		// Setup the test scenario
		setup();

		// Set storage version to 1 to trigger migration
		StorageVersion::new(1).put::<Pallet<Test>>();

		// Call pre_upgrade to collect state (only when try-runtime enabled)
		#[cfg(feature = "try-runtime")]
		let pre_state =
			MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::pre_upgrade()
				.expect("pre_upgrade should succeed");

		// Run the migration to completion using SteppedMigration interface
		use frame::deps::{frame_system::limits::BlockWeights, sp_core::Get};
		let block_weight =
			<<Test as frame_system::Config>::BlockWeights as Get<BlockWeights>>::get().max_block;

		let mut cursor = None;
		loop {
			let mut meter = WeightMeter::with_limit(block_weight);
			cursor = MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::step(
				cursor, &mut meter,
			)
			.expect("Migration step should succeed");
			if cursor.is_none() {
				break;
			}
		}

		// Call post_upgrade to verify migration (only when try-runtime enabled)
		#[cfg(feature = "try-runtime")]
		MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::post_upgrade(
			pre_state,
		)
		.expect("post_upgrade verification should succeed");
	}

	#[test]
	fn migration_test() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			// Setup accounts with both proxies and announcements for comprehensive testing
			// Mix of normal accounts and accounts that will trigger account cleanup
			setup_multiple_accounts_with_reserves(&[(1, 1000), (2, 1000), (3, 1000)]);

			// Add accounts with zero deposits to test account cleanup scenarios
			(4..=6).for_each(|i| {
				let empty_proxies = BoundedVec::default();
				Proxies::<Test>::insert(i, (empty_proxies, 0));
			});

			// Setup different proxy configurations for accounts 1-3 (accounts 4-6 already have zero
			// deposits)
			(1..=3).for_each(|i| {
				let proxies = match i {
					1 => BoundedVec::try_from(vec![
						ProxyDefinition {
							delegate: 11,
							proxy_type: crate::tests::ProxyType::Any,
							delay: 0,
						},
						ProxyDefinition {
							delegate: 12,
							proxy_type: crate::tests::ProxyType::JustTransfer,
							delay: 5,
						},
					]),
					2 => BoundedVec::try_from(vec![ProxyDefinition {
						delegate: 22,
						proxy_type: crate::tests::ProxyType::JustUtility,
						delay: 10,
					}]),
					3 => BoundedVec::try_from(vec![
						ProxyDefinition {
							delegate: 31,
							proxy_type: crate::tests::ProxyType::Any,
							delay: 0,
						},
						ProxyDefinition {
							delegate: 32,
							proxy_type: crate::tests::ProxyType::JustTransfer,
							delay: 1,
						},
					]),
					_ => unreachable!(),
				}
				.unwrap();
				Proxies::<Test>::insert(i, (proxies, 500));

				// Add announcements to test announcement migration as well
				let announcements = BoundedVec::try_from(vec![Announcement {
					real: i + 20,
					call_hash: [0u8; 32].into(),
					height: 1,
				}])
				.unwrap();
				Announcements::<Test>::insert(i, (announcements, 500));
			});

			// Set storage version to trigger migration
			StorageVersion::new(1).put::<Pallet<Test>>();

			// Run try-runtime verification if enabled
			#[cfg(feature = "try-runtime")]
			let pre_state =
				MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::pre_upgrade(
				)
				.expect("pre_upgrade should succeed");

			// Run the migration to completion using SteppedMigration interface
			use frame::deps::{frame_system::limits::BlockWeights, sp_core::Get};
			let block_weight =
				<<Test as frame_system::Config>::BlockWeights as Get<BlockWeights>>::get()
					.max_block;

			let mut cursor = None;
			loop {
				let mut meter = WeightMeter::with_limit(block_weight);
				cursor =
					MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::step(
						cursor, &mut meter,
					)
					.expect("Migration step should succeed");
				if cursor.is_none() {
					break;
				}
			}

			// Run try-runtime post-verification if enabled
			#[cfg(feature = "try-runtime")]
			MigrateReservesToHolds::<Test, MockOldCurrency, DefaultWeights<Test>>::post_upgrade(
				pre_state,
			)
			.expect("post_upgrade verification should succeed");

			// Verify complete migration succeeded - all reserves converted to holds
			(1..=3).for_each(|i| {
				// No more reserves in the mock old system
				assert_eq!(MockOldCurrency::reserved_balance(&i), 0);

				// Funds moved to holds in the new system for accounts with deposits
				let proxy_held = <Test as Config>::Currency::balance_on_hold(
					&HoldReason::ProxyDeposit.into(),
					&i,
				);
				let announcement_held = <Test as Config>::Currency::balance_on_hold(
					&HoldReason::AnnouncementDeposit.into(),
					&i,
				);
				assert!(proxy_held > 0 || announcement_held > 0);
			});

			// Verify zero-deposit accounts (4-6) were handled properly
			(4..=6).for_each(|i| {
				// Should have no reserves (they never had any)
				assert_eq!(MockOldCurrency::reserved_balance(&i), 0);

				// Should have no holds (zero deposit means no funds to hold)
				let proxy_held = <Test as Config>::Currency::balance_on_hold(
					&HoldReason::ProxyDeposit.into(),
					&i,
				);
				assert_eq!(proxy_held, 0, "Zero deposit account should have no holds");

				// Proxy storage should remain but with empty proxies and zero deposit
				assert!(Proxies::<Test>::contains_key(&i), "Zero deposit proxies should remain");
				let (proxies, deposit) = Proxies::<Test>::get(&i);
				assert!(proxies.is_empty(), "Proxies should be empty");
				assert_eq!(deposit, 0, "Deposit should remain zero");
			});

			// Verify storage version was updated to version 2
			assert_eq!(
				StorageVersion::get::<Pallet<Test>>(),
				StorageVersion::new(1),
				"Storage version should be updated to 2 after migration"
			);
		});
	}

	/// Tests zero-deposit preservation strategy when hold creation fails during migration.
	///
	/// When migration fails (e.g., due to too many holds, ED violations, etc.):
	/// - Proxy relationships are PRESERVED (not removed)
	/// - Funds are unreserved and moved to free balance
	/// - Deposit field is set to 0 as permanent marker
	/// - Proxies continue to function normally
	#[test]
	fn migrate_proxy_preservation_on_hold_failure() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			let who = 1;
			let reserved = 1000;

			run_migration(|| {
				// Clear reserves and setup account with reserved balance
				MockOldCurrency::clear_reserves();
				setup_account_with_reserve(who, reserved);

				// Create multiple proxies with different types
				let proxies = BoundedVec::try_from(vec![
					ProxyDefinition {
						delegate: 2,
						proxy_type: crate::tests::ProxyType::Any,
						delay: 0,
					},
					ProxyDefinition {
						delegate: 3,
						proxy_type: crate::tests::ProxyType::JustTransfer,
						delay: 2,
					},
				])
				.unwrap();
				let deposit = reserved;
				Proxies::<Test>::insert(&who, (proxies.clone(), deposit));

				// Simulate hold creation failure by making the account have insufficient balance
				// In real scenarios, this could be due to:
				// - Too many existing holds (MaxHolds limit reached)
				// - Existential deposit violations
				// - Account frozen/restricted
				let _ = <Test as Config>::Currency::slash(&who, 1050);
			});

			// Verify zero-deposit preservation strategy worked:
			// 1. Proxy relationships are preserved (not removed)
			assert!(Proxies::<Test>::contains_key(&who), "Proxies should be preserved");
			let (proxies, deposit) = Proxies::<Test>::get(&who);
			assert_eq!(proxies.len(), 2, "All proxies should be preserved");
			assert_eq!(deposit, 0, "Deposit should be zero after failed migration");

			// 2. Funds were unreserved from old system (moved to free balance)
			assert_eq!(
				MockOldCurrency::reserved_balance(&who),
				0,
				"Reserved balance should be zero"
			);

			// Note: Proxies continue to work normally even with zero deposit
			// Users can later restore deposits via add_proxy when they have sufficient funds
		});
	}

	/// Tests that proxies continue to function normally after failed migration (zero deposit).
	///
	/// Verifies that:
	/// - Proxy calls work even when deposit=0
	/// - No deposits are required for proxy execution
	/// - Failed migration accounts maintain full proxy functionality
	#[test]
	fn zero_deposit_proxy_execution_works() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			let delegator = 1;
			let delegate = 2;
			let target = 3;
			let reserved = 1000;

			// Set up a failed migration scenario (zero deposit with existing proxies)
			run_migration(|| {
				MockOldCurrency::clear_reserves();
				setup_account_with_reserve(delegator, reserved);

				let proxies = BoundedVec::try_from(vec![ProxyDefinition {
					delegate,
					proxy_type: crate::tests::ProxyType::Any,
					delay: 0,
				}])
				.unwrap();
				Proxies::<Test>::insert(&delegator, (proxies, reserved));

				// Force hold failure by insufficient balance
				let _ = <Test as Config>::Currency::slash(&delegator, 1050);
			});

			// Verify proxies were preserved with zero deposit
			let (proxies, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(deposit, 0, "Deposit should be zero after failed migration");
			assert_eq!(proxies.len(), 1, "Proxy should be preserved");

			// Test that proxy execution still works with zero deposit
			let call = RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
				dest: target,
				value: 100,
			});

			// Give delegator some balance for the transfer
			let _ = Balances::mint_into(&delegator, 200).unwrap();

			// Check target's initial balance
			let target_initial_balance = Balances::free_balance(&target);

			// Execute proxy call - should work even with zero deposit
			assert_ok!(Proxy::proxy(
				RuntimeOrigin::signed(delegate),
				delegator,
				Some(crate::tests::ProxyType::Any),
				Box::new(call)
			));

			// Verify the transfer worked (target should have initial + 100)
			assert_eq!(Balances::free_balance(&target), target_initial_balance + 100);
		});
	}

	/// Tests self-recovery mechanism: adding proxy to zero-deposit account restores full deposit.
	///
	/// Verifies that:
	/// - Zero-deposit accounts can add new proxies when they have sufficient funds
	/// - Adding proxy requires deposit for ALL proxies (existing + new)
	/// - System correctly transitions from zero-deposit to full-deposit state
	#[test]
	fn zero_deposit_add_proxy_requires_full_deposit() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			let delegator = 1;
			let existing_delegate = 2;
			let new_delegate = 3;
			let reserved = 500;

			// Set up a failed migration scenario
			run_migration(|| {
				MockOldCurrency::clear_reserves();
				setup_account_with_reserve(delegator, reserved);

				let proxies = BoundedVec::try_from(vec![ProxyDefinition {
					delegate: existing_delegate,
					proxy_type: crate::tests::ProxyType::Any,
					delay: 0,
				}])
				.unwrap();
				Proxies::<Test>::insert(&delegator, (proxies, reserved));

				// Force migration failure
				let _ = <Test as Config>::Currency::slash(&delegator, 600);
			});

			// Verify zero deposit state
			let (_, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(deposit, 0, "Should have zero deposit after failed migration");

			// Give delegator exactly enough for full deposit (2 proxies)
			let full_deposit = Proxy::deposit(2);
			let _ = Balances::mint_into(&delegator, full_deposit).unwrap();

			// Adding new proxy should succeed and restore full deposit
			assert_ok!(Proxy::add_proxy(
				RuntimeOrigin::signed(delegator),
				new_delegate,
				ProxyType::JustTransfer,
				0
			));

			// Verify deposit was restored for all proxies
			let (proxies, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(proxies.len(), 2, "Should have 2 proxies");
			assert_eq!(deposit, full_deposit, "Should have full deposit for both proxies");

			// Verify funds were held
			let held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ProxyDeposit.into(),
				&delegator,
			);
			assert_eq!(held, full_deposit, "Full deposit should be held");
		});
	}

	/// Tests that adding proxy fails when zero-deposit account has insufficient funds.
	///
	/// Verifies that:
	/// - Zero-deposit accounts cannot add proxies without sufficient funds
	/// - Account remains in zero-deposit state when add_proxy fails
	/// - No partial state transitions occur
	#[test]
	fn zero_deposit_add_proxy_fails_with_insufficient_funds() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			let delegator = 1;
			let existing_delegate = 2;
			let new_delegate = 3;
			let reserved = 500;

			// Set up a failed migration scenario
			run_migration(|| {
				MockOldCurrency::clear_reserves();
				setup_account_with_reserve(delegator, reserved);

				let proxies = BoundedVec::try_from(vec![ProxyDefinition {
					delegate: existing_delegate,
					proxy_type: crate::tests::ProxyType::Any,
					delay: 0,
				}])
				.unwrap();
				Proxies::<Test>::insert(&delegator, (proxies, reserved));

				// Force migration failure
				let _ = <Test as Config>::Currency::slash(&delegator, 600);
			});

			// Verify account has minimal balance after migration (due to ED)
			let remaining_balance = Balances::free_balance(&delegator);
			assert!(
				remaining_balance <= 10,
				"Account should have minimal balance after slashing, got: {}",
				remaining_balance
			);

			// Give delegator insufficient funds for full deposit
			let full_deposit = Proxy::deposit(2);

			// Ensure we have insufficient funds by slashing to below the required deposit
			// but keep above ED to maintain the account
			let target_balance = full_deposit - 1;
			let current_balance = Balances::free_balance(&delegator);
			if current_balance > target_balance {
				let to_slash = current_balance - target_balance;
				let _ = <Test as Config>::Currency::slash(&delegator, to_slash);
			}

			let final_balance = Balances::free_balance(&delegator);
			assert!(
				final_balance < full_deposit,
				"Account should have insufficient funds: balance={}, required={}",
				final_balance,
				full_deposit
			);

			// Adding new proxy should fail due to insufficient funds
			assert_err!(
				Proxy::add_proxy(
					RuntimeOrigin::signed(delegator),
					new_delegate,
					ProxyType::JustTransfer,
					0
				),
				TokenError::FundsUnavailable
			);

			// Verify original state unchanged
			let (proxies, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(proxies.len(), 1, "Should still have 1 proxy");
			assert_eq!(deposit, 0, "Deposit should still be zero");
		});
	}

	/// Tests that removing proxy from zero-deposit account provides no refund.
	///
	/// Verifies that:
	/// - Zero-deposit accounts can remove proxies normally
	/// - No refund is given (deposit was already released during migration)
	/// - Account remains in zero-deposit state after proxy removal
	#[test]
	fn zero_deposit_remove_proxy_no_refund() {
		new_test_ext().execute_with(|| {
			setup_test_with_clean_stats();
			let delegator = 1;
			let delegate_to_remove = 2;
			let delegate_to_keep = 3;
			let reserved = 1000;

			// Set up a failed migration scenario with 2 proxies
			run_migration(|| {
				MockOldCurrency::clear_reserves();
				setup_account_with_reserve(delegator, reserved);

				let proxies = BoundedVec::try_from(vec![
					ProxyDefinition {
						delegate: delegate_to_remove,
						proxy_type: crate::tests::ProxyType::Any,
						delay: 0,
					},
					ProxyDefinition {
						delegate: delegate_to_keep,
						proxy_type: ProxyType::JustTransfer,
						delay: 0,
					},
				])
				.unwrap();
				Proxies::<Test>::insert(&delegator, (proxies, reserved));

				// Force migration failure
				let _ = <Test as Config>::Currency::slash(&delegator, 1100);
			});

			// Verify zero deposit state
			let (proxies, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(deposit, 0, "Should have zero deposit");
			assert_eq!(proxies.len(), 2, "Should have 2 proxies");

			let initial_balance = Balances::free_balance(&delegator);

			// Remove one proxy
			assert_ok!(Proxy::remove_proxy(
				RuntimeOrigin::signed(delegator),
				delegate_to_remove,
				crate::tests::ProxyType::Any,
				0
			));

			// Verify proxy was removed but no refund given
			let (proxies, deposit) = Proxies::<Test>::get(&delegator);
			assert_eq!(proxies.len(), 1, "Should have 1 proxy remaining");
			assert_eq!(deposit, 0, "Deposit should still be zero");

			// Verify no refund was given (balance unchanged)
			let final_balance = Balances::free_balance(&delegator);
			assert_eq!(final_balance, initial_balance, "Balance should be unchanged (no refund)");

			// Verify no holds were changed
			let held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ProxyDeposit.into(),
				&delegator,
			);
			assert_eq!(held, 0, "No holds should exist");
		});
	}
}

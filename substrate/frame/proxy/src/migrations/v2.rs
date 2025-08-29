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

//! Migration from v1 to v2: Convert proxy and announcement reserves to holds.
//!
//! This migration uses multi-block execution with graceful degradation:
//! - Multi-block: Handles accounts with weight-limited batching without timing out
//! - Graceful degradation: Any migration failure results in proxy removal + refund
//! - No permanent fund loss, force recovery possible if migration fails (e.g. for pure proxies)

use crate::{
	Announcement, Announcements, BalanceOf, CallHashOf, Config, Event, HoldReason, Pallet, Proxies,
	ProxyDefinition,
};
use frame::{
	prelude::*,
	runtime::prelude::weights::WeightMeter,
	storage_alias,
	traits::{fungible::MutateHold, OnRuntimeUpgrade, ReservableCurrency, StorageVersion},
};

const LOG_TARGET: &str = "runtime::proxy";

#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;
#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

use frame::log;

/// Result of verifying a single account after migration
#[cfg(feature = "try-runtime")]
#[derive(Debug, Clone)]
enum AccountVerification<Balance> {
	/// Account successfully converted to holds
	SuccessfulConversion { proxy_held: Balance, announcement_held: Balance },
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
pub enum MigrationCursor<AccountId> {
	/// Migrating proxies storage.
	Proxies { last_key: Option<AccountId> },
	/// Migrating announcements storage.  
	Announcements { last_key: Option<AccountId> },
	/// Migration complete.
	Complete,
}

/// Storage for migration progress.
#[storage_alias]
pub type MigrationProgress<T: Config> =
	StorageValue<Pallet<T>, MigrationCursor<<T as frame_system::Config>::AccountId>, OptionQuery>;

/// Migration result for an account.
#[derive(Debug, PartialEq)]
enum AccountMigrationResult<T: Config> {
	Success,
	GracefulRemoval { refunded: BalanceOf<T> },
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
		// - Read storage item (proxies or announcements)
		// - Read reserved balance from old currency system
		// - Unreserve from old system (balance update)
		// - Try hold (balance + holds update)  or remove storage on failure (graceful degradation)
		T::DbWeight::get().reads_writes(3, 3)
	}

	/// NOTE: Pure proxy detection is not implemented during migration.
	///
	/// **Why we can't detect pure proxies reliably:**
	/// During migration, we only have access to:
	/// - Account X being migrated (with its proxy deposits)
	/// - Who X delegates to (X's proxy list)
	///
	/// We do NOT have:
	/// - Who delegates to X (requires scanning all proxy relationships)
	/// - Whether X is a pure proxy or regular account (no storage marker)
	/// - Who spawned X as pure proxy (spawner info not stored)

	/// Migrate a single proxy account with graceful degradation.
	/// Handles both regular accounts and pure proxies.
	fn migrate_proxy_account<BlockNumber>(
		who: &<T as frame_system::Config>::AccountId,
		proxies: BoundedVec<
			ProxyDefinition<<T as frame_system::Config>::AccountId, T::ProxyType, BlockNumber>,
			T::MaxProxies,
		>,
		old_deposit: BalanceOf<T>,
	) -> AccountMigrationResult<T> {
		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);
		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Migrate what was actually deposited (stored in storage), bounded by actual reserves
		let to_migrate = old_deposit.min(reserved_balance);

		if to_migrate.is_zero() {
			return AccountMigrationResult::Success;
		}

		// Unreserve from old currency system
		let old_to_migrate: OldCurrency::Balance = to_migrate.into();
		let old_unreserved = OldCurrency::unreserve(who, old_to_migrate);
		let actually_unreserved = to_migrate.saturating_sub(old_unreserved.into());

		// Try to hold in new system
		match T::Currency::hold(&HoldReason::ProxyDeposit.into(), who, actually_unreserved) {
			Ok(_) => {
				// Success: deposit migrated to hold
				Pallet::<T>::deposit_event(Event::ProxyDepositMigrated {
					delegator: who.clone(),
					amount: actually_unreserved,
				});
				AccountMigrationResult::Success
			},
			Err(_) => {
				// Migration failed - graceful degradation for ALL accounts
				//
				// For regular accounts:
				// - Proxy config removed, funds stay in account's free balance
				// - Owner can re-add proxies later using new hold system
				//
				// For pure proxies (keyless accounts):
				// - Proxy config removed, funds stay in pure proxy's free balance
				// - Governance can recover funds using force recovery functions

				Proxies::<T>::remove(who);

				Pallet::<T>::deposit_event(Event::ProxyRemovedDuringMigration {
					delegator: who.clone(),
					proxy_count: proxies.len() as u32,
					refunded: actually_unreserved,
				});

				AccountMigrationResult::GracefulRemoval { refunded: actually_unreserved }
			},
		}
	}

	/// Migrate a single announcement account with graceful degradation.
	fn migrate_announcement_account<BlockNumber>(
		who: &<T as frame_system::Config>::AccountId,
		announcements: BoundedVec<
			Announcement<<T as frame_system::Config>::AccountId, CallHashOf<T>, BlockNumber>,
			T::MaxPending,
		>,
		old_deposit: BalanceOf<T>,
	) -> AccountMigrationResult<T> {
		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);
		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Migrate what was actually deposited (stored in storage), bounded by actual reserves
		let to_migrate = old_deposit.min(reserved_balance);

		if to_migrate.is_zero() {
			return AccountMigrationResult::Success;
		}

		// Unreserve from old currency system
		let old_to_migrate: OldCurrency::Balance = to_migrate.into();
		let old_unreserved = OldCurrency::unreserve(who, old_to_migrate);
		let actually_unreserved = to_migrate.saturating_sub(old_unreserved.into());

		// Try to hold in new system
		match T::Currency::hold(&HoldReason::AnnouncementDeposit.into(), who, actually_unreserved) {
			Ok(_) => {
				// Success: announcement deposit migrated
				Pallet::<T>::deposit_event(Event::AnnouncementDepositMigrated {
					announcer: who.clone(),
					amount: actually_unreserved,
				});
				AccountMigrationResult::Success
			},
			Err(_) => {
				// Graceful degradation: remove announcements
				// The unreserved funds remain in the account's free balance
				// This is safe since announcements are tied to regular accounts, not pure proxies
				Announcements::<T>::remove(who);

				Pallet::<T>::deposit_event(Event::AnnouncementsRemovedDuringMigration {
					announcer: who.clone(),
					announcement_count: announcements.len() as u32,
					refunded: actually_unreserved,
				});

				AccountMigrationResult::GracefulRemoval { refunded: actually_unreserved }
			},
		}
	}

	/// Process one batch of proxy migrations within weight limit.
	pub fn process_proxy_batch(
		last_key: Option<<T as frame_system::Config>::AccountId>,
		meter: &mut WeightMeter,
	) -> MigrationCursor<<T as frame_system::Config>::AccountId> {
		let mut iter = if let Some(last) = last_key {
			Proxies::<T>::iter_from(Proxies::<T>::hashed_key_for(&last))
		} else {
			Proxies::<T>::iter()
		};

		// Process accounts until weight limit is reached
		let last_processed = iter.try_fold(None, |_acc, (who, (proxies, deposit))| {
			// Check if we have weight for one more account
			if meter.try_consume(Self::weight_per_account()).is_err() {
				// Weight limit reached, return early with last account
				return Err(who);
			}

			// Migrate this account (handles pure proxies internally)
			let result = Self::migrate_proxy_account(&who, proxies, deposit.into());
			if let AccountMigrationResult::GracefulRemoval { refunded } = result {
				log::warn!(
					target: LOG_TARGET,
					"Proxy migration failed for account {:?}, refunded {:?}",
					who, refunded
				);
			}

			// Continue processing
			Ok(Some(who))
		});

		// Handle the result
		match last_processed {
			Err(who) => return MigrationCursor::Proxies { last_key: Some(who) },
			Ok(_) => {}, // All accounts processed successfully
		}

		// Done with proxies, move to announcements
		MigrationCursor::Announcements { last_key: None }
	}

	/// Process one batch of announcement migrations within weight limit.
	pub fn process_announcement_batch(
		last_key: Option<<T as frame_system::Config>::AccountId>,
		meter: &mut WeightMeter,
	) -> MigrationCursor<<T as frame_system::Config>::AccountId> {
		let mut iter = if let Some(last) = last_key {
			Announcements::<T>::iter_from(Announcements::<T>::hashed_key_for(&last))
		} else {
			Announcements::<T>::iter()
		};

		// Process accounts until weight limit is reached
		let last_processed = iter.try_fold(None, |_acc, (who, (announcements, deposit))| {
			// Check if we have weight for one more account
			if meter.try_consume(Self::weight_per_account()).is_err() {
				// Weight limit reached, return early with last account
				return Err(who);
			}

			// Migrate this account
			let result = Self::migrate_announcement_account(&who, announcements, deposit.into());
			if let AccountMigrationResult::GracefulRemoval { refunded } = result {
				log::warn!(
					target: LOG_TARGET,
					"Announcement migration failed for account {:?}, refunded {:?}",
					who, refunded
				);
			}

			// Continue processing
			Ok(Some(who))
		});

		// Handle the result
		match last_processed {
			Err(who) => return MigrationCursor::Announcements { last_key: Some(who) },
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
			MigrationCursor::Proxies { last_key } => Self::process_proxy_batch(last_key, meter),
			MigrationCursor::Announcements { last_key } =>
				Self::process_announcement_batch(last_key, meter),
			MigrationCursor::Complete => {
				// Clean up and finish
				MigrationProgress::<T>::kill();
				StorageVersion::new(2).put::<Pallet<T>>();
				return true;
			},
		};

		// Update cursor
		match new_cursor {
			MigrationCursor::Complete => {
				MigrationProgress::<T>::kill();
				StorageVersion::new(2).put::<Pallet<T>>();

				Pallet::<T>::deposit_event(Event::MigrationCompleted);
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
		let current_version = Pallet::<T>::in_code_storage_version();

		if on_chain_version >= current_version {
			return T::DbWeight::get().reads(1);
		}

		// Initialize migration
		MigrationProgress::<T>::set(Some(MigrationCursor::Proxies { last_key: None }));

		Pallet::<T>::deposit_event(Event::MigrationStarted);

		// Process as much as possible in this block with conservative weight limit.
		// We don't add a MaxServiceWeight config to the proxy pallet because this migration
		// from Currency to fungible traits is a one-time operation that will be removed after
		// all chains have upgraded.
		const MIGRATION_WEIGHT_FRACTION: u64 = 4; // Use at most 1/4 of block weight
		let weight_limit = T::BlockWeights::get().max_block / MIGRATION_WEIGHT_FRACTION;
		let mut meter = WeightMeter::with_limit(weight_limit);
		Self::step(&mut meter);

		meter.consumed()
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
		// Verify storage version updated
		ensure!(Pallet::<T>::on_chain_storage_version() == 2, "Storage version not updated");

		// Verify migration completed
		ensure!(MigrationProgress::<T>::get().is_none(), "Migration not completed");

		// Decode pre-migration state
		let pre_migration_deposits: BTreeMap<T::AccountId, (BalanceOf<T>, BalanceOf<T>)> =
			Decode::decode(&mut &state[..]).map_err(|_| "Failed to decode pre_upgrade state")?;

		// Verify each account
		let verification_results: Result<Vec<_>, TryRuntimeError> = pre_migration_deposits
			.iter()
			.map(|(who, (old_proxy_deposit, old_announcement_deposit))| {
				Self::verify_account_migration(who, *old_proxy_deposit, *old_announcement_deposit)
			})
			.collect();

		let results = verification_results?;

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
			.map(|(proxy, announcement)| *proxy + *announcement)
			.fold(Zero::zero(), |acc, deposit| acc + deposit);

		let accounted_total = summary.total_converted_to_holds + summary.total_released_to_users;

		ensure!(accounted_total == original_total, "Fund conservation violated");

		// Verify the total count makes sense
		let total_processed_accounts = pre_migration_deposits.len() as u32;
		let total_verification_results = summary.successful_conversions +
			summary.graceful_degradations +
			summary.accounts_cleaned_up;

		ensure!(
			total_verification_results == total_processed_accounts,
			"Account count mismatch: processed {} accounts but verification shows {} results"
		);

		// Log comprehensive migration summary
		frame::log::info!(
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

		let has_proxies = !current_proxies_vec.is_empty();
		let has_announcements = !current_announcements_vec.is_empty();

		// Case 1: Both storage entries exist - should be successful conversion
		if has_proxies && has_announcements {
			// Verify exact amounts match
			ensure!(
				current_proxy_deposit == old_proxy_deposit &&
					current_announcement_deposit == old_announcement_deposit,
				"Deposit amounts changed during migration"
			);

			// Verify funds are held correctly
			ensure!(
				held_proxy >= current_proxy_deposit &&
					held_announcement >= current_announcement_deposit,
				"Insufficient holds for account"
			);

			return Ok(AccountVerification::SuccessfulConversion {
				proxy_held: held_proxy,
				announcement_held: held_announcement,
			});
		}

		// Case 2: Only proxies exist
		if has_proxies && !has_announcements {
			ensure!(current_proxy_deposit == old_proxy_deposit, "Proxy deposit amount changed");

			ensure!(held_proxy >= current_proxy_deposit, "Insufficient proxy hold");

			// Announcement was gracefully degraded or never existed
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
			ensure!(
				current_announcement_deposit == old_announcement_deposit,
				"Announcement deposit amount changed"
			);

			ensure!(
				held_announcement >= current_announcement_deposit,
				"Insufficient announcement hold"
			);

			// Proxy was gracefully degraded or never existed
			let released =
				if old_proxy_deposit.is_zero() { Zero::zero() } else { old_proxy_deposit };

			return Ok(AccountVerification::SuccessfulConversion {
				proxy_held: released, // Released to user
				announcement_held: held_announcement,
			});
		}

		// Case 4: No storage entries - either graceful degradation or cleanup
		let total_old_deposit = old_proxy_deposit + old_announcement_deposit;

		if total_old_deposit.is_zero() {
			// Account never had deposits - this is normal
			return Ok(AccountVerification::AccountCleanedup { released_amount: Zero::zero() });
		}

		// Account had deposits but storage was removed
		// This means graceful degradation occurred - funds should have been released to user.
		// Verify no holds remain
		ensure!(
			held_proxy.is_zero() && held_announcement.is_zero(),
			"Account has storage removed but still has holds"
		);

		// No need to check for reserves since we've migrated to holds

		Ok(AccountVerification::GracefulDegradation { released_amount: total_old_deposit })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{new_test_ext, Test},
		Announcement, Announcements, Proxies, ProxyDefinition,
	};
	use frame::{
		prelude::{DispatchError, DispatchResult},
		testing_prelude::assert_ok,
		traits::{
			fungible::{InspectHold, Mutate},
			BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency, SignedImbalance,
			WithdrawReasons,
		},
	};

	type AccountId = u64;
	type Balance = u64;

	// Simple mock for old currency system
	// Using thread_local! for test isolation (each test thread gets its own instance and tests can
	// run in parallel)
	std::thread_local! {
		static MOCK_RESERVES: std::cell::RefCell<std::collections::HashMap<u64, u64>> =
			std::cell::RefCell::new(std::collections::HashMap::new());
	}

	// Unit type that implements old currency traits for testing
	pub struct MockOldCurrency;

	impl MockOldCurrency {
		// Helper to clear reserves between tests
		pub fn clear_reserves() {
			MOCK_RESERVES.with(|r| r.borrow_mut().clear());
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
			MOCK_RESERVES.with(|r| {
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
			MOCK_RESERVES.with(|r| *r.borrow().get(who).unwrap_or(&0))
		}

		fn reserve(who: &AccountId, value: Self::Balance) -> DispatchResult {
			MOCK_RESERVES.with(|r| {
				let mut reserves = r.borrow_mut();
				let current = *reserves.get(who).unwrap_or(&0);
				reserves.insert(*who, current + value);
			});
			Ok(())
		}

		fn unreserve(who: &AccountId, value: Self::Balance) -> Self::Balance {
			MOCK_RESERVES.with(|r| {
				let mut reserves = r.borrow_mut();
				let current = *reserves.get(who).unwrap_or(&0);
				if current >= value {
					reserves.insert(*who, current - value);
					0 // All requested amount was unreserved
				} else {
					reserves.insert(*who, 0);
					value - current // Return amount that couldn't be unreserved
				}
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
		let pre_state = MigrateReservesToHolds::<Test, MockOldCurrency>::pre_upgrade()
			.expect("pre_upgrade should succeed");

		// Run the actual migration using the OnRuntimeUpgrade interface
		// This properly sets storage version and handles everything
		let _weight = MigrateReservesToHolds::<Test, MockOldCurrency>::on_runtime_upgrade();

		// Continue stepping until migration completes (multi-block)
		use frame::deps::{frame_system::limits::BlockWeights, sp_core::Get};
		let block_weight =
			<<Test as frame_system::Config>::BlockWeights as Get<BlockWeights>>::get().max_block;

		std::iter::from_fn(|| {
			let mut meter = WeightMeter::with_limit(block_weight);
			let completed = MigrateReservesToHolds::<Test, MockOldCurrency>::step(&mut meter);
			if completed {
				None
			} else {
				Some(())
			}
		})
		.for_each(|_| {});

		// Call post_upgrade to verify migration (only when try-runtime enabled)
		#[cfg(feature = "try-runtime")]
		MigrateReservesToHolds::<Test, MockOldCurrency>::post_upgrade(pre_state)
			.expect("post_upgrade verification should succeed");
	}

	#[test]
	fn migration_test() {
		new_test_ext().execute_with(|| {
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
			let pre_state = MigrateReservesToHolds::<Test, MockOldCurrency>::pre_upgrade()
				.expect("pre_upgrade should succeed");

			// Run the actual migration
			let _weight = MigrateReservesToHolds::<Test, MockOldCurrency>::on_runtime_upgrade();

			// Complete the multi-block migration
			use frame::deps::{frame_system::limits::BlockWeights, sp_core::Get};
			let block_weight =
				<<Test as frame_system::Config>::BlockWeights as Get<BlockWeights>>::get()
					.max_block;
			std::iter::from_fn(|| {
				let mut meter = WeightMeter::with_limit(block_weight);
				let completed = MigrateReservesToHolds::<Test, MockOldCurrency>::step(&mut meter);
				if completed {
					None
				} else {
					Some(())
				}
			})
			.for_each(|_| {});

			// Run try-runtime post-verification if enabled
			#[cfg(feature = "try-runtime")]
			MigrateReservesToHolds::<Test, MockOldCurrency>::post_upgrade(pre_state)
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

			// Migration should have cleared progress automatically
			assert_eq!(
				MigrationProgress::<Test>::get(),
				None,
				"Migration should have cleared progress automatically"
			);
		});
	}

	#[test]
	fn migrate_proxy_graceful_degradation_on_hold_failure() {
		new_test_ext().execute_with(|| {
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

				// Simulate a scenario where hold would fail.
				// (In real scenario, this could be due to ED violation, too many holds, etc.)
				// For test purposes, we'll simulate by making the account have insufficient balance
				let _ = <Test as Config>::Currency::slash(&who, 1050);
			});

			// Verify migration results - should result in graceful removal
			// Proxies should be removed due to graceful degradation
			assert!(!Proxies::<Test>::contains_key(&who));
		});
	}
}

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
//! - Multi-block: Handles thousands of accounts with weight-limited batching without timing out
//! - Graceful degradation: Any migration failure results in proxy removal + refund
//!  (no permanent fund loss, manual recovery possible for pure proxies)

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

	/// Calculate expected proxy deposit based on current configuration.
	fn calculate_proxy_deposit(proxy_count: usize) -> BalanceOf<T> {
		let base = T::ProxyDepositBase::get();
		let factor = T::ProxyDepositFactor::get();
		base.saturating_add(factor.saturating_mul((proxy_count as u32).into()))
	}

	/// Calculate expected announcement deposit based on current configuration.
	fn calculate_announcement_deposit(announcement_count: usize) -> BalanceOf<T> {
		let base = T::AnnouncementDepositBase::get();
		let factor = T::AnnouncementDepositFactor::get();
		base.saturating_add(factor.saturating_mul((announcement_count as u32).into()))
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
	///
	/// **Manual intervention options for pure proxies:**
	/// If a pure proxy Y (controlled by spawner X) fails migration:
	/// 1. X can call `proxy(Y, transfer, X, amount)` to recover Y's funds to X
	/// 2. X can call `proxy(Y, kill_pure, disambiguation_index)` to destroy Y
	/// 3. X can create a new pure proxy Z using `create_pure()` with new system
	///
	/// This manual process ensures no funds are permanently lost while avoiding
	/// complex and unreliable pure proxy detection during migration.

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
		// Calculate what deposit should be
		let expected_deposit = Self::calculate_proxy_deposit(proxies.len());

		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);
		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Use the minimum of old deposit, expected deposit, and actual reserved
		let to_migrate = old_deposit.min(expected_deposit).min(reserved_balance);

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
				// - Spawner can recover funds manually using:
				//   1. `proxy(pure_proxy, transfer, spawner, amount)`
				//   2. `proxy(pure_proxy, kill_pure, index)` to destroy
				//   3. `create_pure()` to create new one with hold system

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
		// Calculate what deposit should be
		let expected_deposit = Self::calculate_announcement_deposit(announcements.len());

		// Get current reserved balance from old currency system
		let old_reserved = OldCurrency::reserved_balance(who);
		let reserved_balance: BalanceOf<T> = old_reserved.into();

		// Use the minimum of old deposit, expected deposit, and actual reserved
		let to_migrate = old_deposit.min(expected_deposit).min(reserved_balance);

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
		let iter = if let Some(last) = last_key {
			Proxies::<T>::iter_from(Proxies::<T>::hashed_key_for(&last))
		} else {
			Proxies::<T>::iter()
		};

		for (who, (proxies, deposit)) in iter {
			// Check if we have weight for one more account
			if meter.try_consume(Self::weight_per_account()).is_err() {
				return MigrationCursor::Proxies { last_key: Some(who) };
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
		}

		// Done with proxies, move to announcements
		MigrationCursor::Announcements { last_key: None }
	}

	/// Process one batch of announcement migrations within weight limit.
	pub fn process_announcement_batch(
		last_key: Option<<T as frame_system::Config>::AccountId>,
		meter: &mut WeightMeter,
	) -> MigrationCursor<<T as frame_system::Config>::AccountId> {
		let iter = if let Some(last) = last_key {
			Announcements::<T>::iter_from(Announcements::<T>::hashed_key_for(&last))
		} else {
			Announcements::<T>::iter()
		};

		for (who, (announcements, deposit)) in iter {
			// Check if we have weight for one more account
			if meter.try_consume(Self::weight_per_account()).is_err() {
				return MigrationCursor::Announcements { last_key: Some(who) };
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

		// Process as much as possible in this block
		let mut meter = WeightMeter::with_limit(T::BlockWeights::get().max_block);
		Self::step(&mut meter);

		meter.consumed()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		// Collect all deposits for verification
		let mut deposits =
			BTreeMap::<<T as frame_system::Config>::AccountId, (BalanceOf<T>, BalanceOf<T>)>::new();

		for (who, (_, deposit)) in Proxies::<T>::iter() {
			deposits.entry(who).or_default().0 = deposit.into();
		}

		for (who, (_, deposit)) in Announcements::<T>::iter() {
			deposits.entry(who).or_default().1 = deposit.into();
		}

		Ok(deposits.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		// Verify storage version updated
		ensure!(Pallet::<T>::on_chain_storage_version() == 2, "Storage version not updated");

		// Verify migration completed
		ensure!(MigrationProgress::<T>::get().is_none(), "Migration not completed");

		Ok(())
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
		testing_prelude::assert_ok,
		traits::{fungible::InspectHold, Currency, ReservableCurrency},
	};

	type AccountId = u64;
	type Balance = u64;
	type ProxyPallet = crate::Pallet<Test>;

	// We need to import Balances as OldCurrency for testing since it still implements
	// ReservableCurrency
	use pallet_balances::Pallet as Balances;

	// Helper to setup test accounts with reserves using the old reserve system
	fn setup_account_with_reserve(who: AccountId, reserved: Balance) {
		// Give the account enough balance
		let _ = Balances::<Test>::make_free_balance_be(&who, reserved + 100);
		// Use the ReservableCurrency trait to create actual reserves
		assert_ok!(<Balances<Test> as ReservableCurrency<AccountId>>::reserve(&who, reserved));
	}

	#[test]
	fn migrate_proxy_succeeds_when_hold_works() {
		new_test_ext().execute_with(|| {
			let who = 1;
			let reserved = 1000;

			// Setup account with reserved balance
			setup_account_with_reserve(who, reserved);

			// Verify reserves are actually there
			assert_eq!(Balances::<Test>::reserved_balance(&who), reserved, "Reserves not created");

			// Create some proxies (this would normally reserve funds)
			let proxies = BoundedVec::try_from(vec![ProxyDefinition {
				delegate: 2,
				proxy_type: crate::tests::ProxyType::Any,
				delay: 0,
			}])
			.unwrap();
			let deposit = reserved;
			Proxies::<Test>::insert(&who, (proxies.clone(), deposit));

			// Run migration - using Balances as OldCurrency since it implements ReservableCurrency
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;

			// Migrate this specific account
			let result = Migration::migrate_proxy_account::<u64>(&who, proxies, deposit);

			// Should succeed
			assert_eq!(result, AccountMigrationResult::Success, "Migration failed: {:?}", result);

			// Check that reserves were converted to holds
			let remaining_reserved = Balances::<Test>::reserved_balance(&who);
			assert_eq!(
				remaining_reserved, 0,
				"Reserved not unreserved, remaining: {}",
				remaining_reserved
			);
			let held =
				<Test as Config>::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), &who);
			assert_eq!(held, reserved);
		});
	}

	#[test]
	fn migrate_proxy_graceful_degradation_on_hold_failure() {
		new_test_ext().execute_with(|| {
			let who = 1;
			let reserved = 1000;

			// Setup account with reserved balance
			setup_account_with_reserve(who, reserved);

			// Create proxies
			let proxies = BoundedVec::try_from(vec![ProxyDefinition {
				delegate: 2,
				proxy_type: crate::tests::ProxyType::Any,
				delay: 0,
			}])
			.unwrap();
			let deposit = reserved;
			Proxies::<Test>::insert(&who, (proxies.clone(), deposit));

			// Simulate a scenario where hold would fail.
			// (In real scenario, this could be due to ED violation, too many holds, etc.)
			// For test purposes, we'll simulate by making the account have insufficient balance
			let _ = <Test as Config>::Currency::slash(&who, 950);

			// Run migration - using Balances as OldCurrency since it implements ReservableCurrency
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;
			let result = Migration::migrate_proxy_account::<u64>(&who, proxies, deposit);

			// Should result in graceful removal
			match result {
				AccountMigrationResult::GracefulRemoval { refunded } => {
					assert!(refunded > 0);
					// Proxies should be removed
					assert!(!Proxies::<Test>::contains_key(&who));
				},
				_ => panic!("Expected graceful removal"),
			}
		});
	}

	#[test]
	fn migrate_announcement_succeeds_when_hold_works() {
		new_test_ext().execute_with(|| {
			let who = 1;
			let reserved = 500;

			// Setup account with reserved balance
			setup_account_with_reserve(who, reserved);

			// Create announcements
			let announcements = BoundedVec::try_from(vec![Announcement {
				real: 2,
				call_hash: [0u8; 32].into(),
				height: 1,
			}])
			.unwrap();
			let deposit = reserved;
			Announcements::<Test>::insert(&who, (announcements.clone(), deposit));

			// Run migration - using Balances as OldCurrency since it implements ReservableCurrency
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;
			let result =
				Migration::migrate_announcement_account::<u64>(&who, announcements, deposit);

			// Should succeed
			assert_eq!(result, AccountMigrationResult::Success);

			// Check that reserves were converted to holds
			assert_eq!(<Test as Config>::Currency::reserved_balance(&who), 0);
			let held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::AnnouncementDeposit.into(),
				&who,
			);
			assert_eq!(held, reserved);
		});
	}

	#[test]
	fn migration_handles_zero_deposit() {
		new_test_ext().execute_with(|| {
			let who = 1;

			// Account with no reserved balance
			let _ = <Test as Config>::Currency::make_free_balance_be(&who, 1000);

			// Proxies with zero deposit
			let proxies = BoundedVec::default();
			let deposit = 0;

			// Run migration - using Balances as OldCurrency since it implements ReservableCurrency
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;
			let result = Migration::migrate_proxy_account::<u64>(&who, proxies, deposit);

			// Should succeed with no changes
			assert_eq!(result, AccountMigrationResult::Success);
			assert_eq!(<Test as Config>::Currency::reserved_balance(&who), 0);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(&HoldReason::ProxyDeposit.into(), &who),
				0
			);
		});
	}

	#[test]
	fn migration_cursor_tracks_progress() {
		new_test_ext().execute_with(|| {
			// Setup multiple accounts with proxies
			for i in 1..=5 {
				setup_account_with_reserve(i, i as Balance * 100);
				let proxies = BoundedVec::try_from(vec![ProxyDefinition {
					delegate: i + 10,
					proxy_type: crate::tests::ProxyType::Any,
					delay: 0,
				}])
				.unwrap();
				Proxies::<Test>::insert(i, (proxies, i as Balance * 100));
			}

			// Initialize migration
			MigrationProgress::<Test>::set(Some(MigrationCursor::Proxies { last_key: None }));

			// Process with limited weight (should only process some accounts)
			let mut meter = WeightMeter::with_limit(Weight::from_parts(100, 0));
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;
			let completed = Migration::step(&mut meter);

			// Should not complete in one step due to weight limit
			assert!(!completed);

			// Cursor should have advanced
			let cursor = MigrationProgress::<Test>::get();
			assert!(cursor.is_some());
			assert!(matches!(cursor, Some(MigrationCursor::Proxies { last_key: Some(_) })));
		});
	}

	#[test]
	fn full_migration_lifecycle() {
		new_test_ext().execute_with(|| {
			// Setup accounts with both proxies and announcements
			for i in 1..=3 {
				setup_account_with_reserve(i, 1000);

				// Add proxies
				let proxies = BoundedVec::try_from(vec![ProxyDefinition {
					delegate: i + 10,
					proxy_type: crate::tests::ProxyType::Any,
					delay: 0,
				}])
				.unwrap();
				Proxies::<Test>::insert(i, (proxies, 500));

				// Add announcements
				let announcements = BoundedVec::try_from(vec![Announcement {
					real: i + 20,
					call_hash: [0u8; 32].into(),
					height: 1,
				}])
				.unwrap();
				Announcements::<Test>::insert(i, (announcements, 500));
			}

			// Run full migration
			type Migration = MigrateReservesToHolds<Test, Balances<Test>>;
			let weight = Migration::on_runtime_upgrade();
			assert!(weight != Weight::zero());

			// Verify storage version updated
			assert_eq!(ProxyPallet::on_chain_storage_version(), 2);

			// Verify all accounts migrated
			for i in 1..=3 {
				// No more reserves
				assert_eq!(<Test as Config>::Currency::reserved_balance(&i), 0);

				// Funds moved to holds
				let proxy_held = <Test as Config>::Currency::balance_on_hold(
					&HoldReason::ProxyDeposit.into(),
					&i,
				);
				let announcement_held = <Test as Config>::Currency::balance_on_hold(
					&HoldReason::AnnouncementDeposit.into(),
					&i,
				);
				assert!(proxy_held > 0 || announcement_held > 0);
			}
		});
	}
}

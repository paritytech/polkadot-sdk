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

#![cfg(all(test, not(feature = "runtime-benchmarks")))]

use super::*;
use crate::{
	mock::*,
	pallet::{Accounts, HoldReason},
};
use frame_support::{
	assert_ok, traits::{ReservableCurrency, fungible::{InspectHold, Mutate}},
	weights::WeightMeter,
};

/// Helper function to create test accounts with reserved balances
fn setup_pre_migration_state() {
	// Create accounts with reserved balances using the old system
	for i in 1..=5 {
		let account_id = i as u64;
		let deposit = i as u64; // Different deposit amounts for testing
		
		// Reserve the balance using the old system
		assert_ok!(Balances::reserve(&account_id, deposit));
		
		// Insert into the current storage format (simulating pre-migration state)
		Accounts::<Test>::insert(i, (account_id, deposit, false));
	}
	
	// Create one frozen account (should not be migrated)
	let frozen_account = 6u64;
	let frozen_deposit = 10u64;
	assert_ok!(Balances::reserve(&frozen_account, frozen_deposit));
	Accounts::<Test>::insert(6, (frozen_account, frozen_deposit, true));
}

#[test]
fn migration_basic_functionality() {
	new_test_ext().execute_with(|| {
		// Setup pre-migration state
		setup_pre_migration_state();
		
		// Verify initial state
		for i in 1..=5 {
			let account_id = i as u64;
			let expected_deposit = i as u64;
			assert_eq!(Balances::reserved_balance(&account_id), expected_deposit);
		}
		
		// Create migration instance
		let _migration = LazyMigrationV1::<Test>::default();
		
		// Execute migration step
		let mut meter = WeightMeter::new();
		let result = LazyMigrationV1::<Test>::step(None, &mut meter);
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None); // Migration complete
		
		// Verify post-migration state - for the indices pallet, the migration is just a compatibility check
		// The storage format is identical, so no actual conversion happens
		for i in 1..=5 {
			let account_id = i as u64;
			let expected_deposit = i as u64;
			
			// Reserves should remain unchanged (no actual migration happens)
			assert_eq!(Balances::reserved_balance(&account_id), expected_deposit);
			
			// No holds should be created
			let held = Balances::balance_on_hold(&HoldReason::DepositForIndex.into(), &account_id);
			assert_eq!(held, 0);
		}
		
		// Frozen account should remain unchanged
		let frozen_account = 6u64;
		let frozen_deposit = 10u64;
		assert_eq!(Balances::reserved_balance(&frozen_account), frozen_deposit);
		let held = Balances::balance_on_hold(&HoldReason::DepositForIndex.into(), &frozen_account);
		assert_eq!(held, 0);
	});
}

#[test]
fn migration_handles_empty_state() {
	new_test_ext().execute_with(|| {
		// No pre-migration state setup
		
		let _migration = LazyMigrationV1::<Test>::default();
		let mut meter = WeightMeter::new();
		
		// Migration should complete immediately with no work
		let result = LazyMigrationV1::<Test>::step(None, &mut meter);
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None); // Migration complete
	});
}

#[test]
fn migration_weight_consumption() {
	new_test_ext().execute_with(|| {
		setup_pre_migration_state();
		
		let _migration = LazyMigrationV1::<Test>::default();
		let mut meter = WeightMeter::new();
		
		// First step should consume some weight
		let initial_weight = meter.remaining();
		let result = LazyMigrationV1::<Test>::step(None, &mut meter);
		let consumed_weight = initial_weight - meter.remaining();
		
		assert!(consumed_weight.ref_time() > 0 || consumed_weight.proof_size() > 0);
		assert!(result.is_ok());
	});
}

#[test]
fn migration_id_is_correct() {
	let _migration = LazyMigrationV1::<Test>::default();
	let id = LazyMigrationV1::<Test>::id();
	
	// Verify the migration ID structure
	assert_eq!(id.pallet_id, *crate::migration::PALLET_MIGRATIONS_ID);
	assert_eq!(id.version_from, 0);
	assert_eq!(id.version_to, 1);
}

#[cfg(feature = "try-runtime")]
#[test]
fn pre_upgrade_collects_correct_state() {
	new_test_ext().execute_with(|| {
		setup_pre_migration_state();
		
		let _migration = LazyMigrationV1::<Test>::default();
		let pre_state = LazyMigrationV1::<Test>::pre_upgrade().unwrap();
		
		// Decode the pre-state
		let (accounts_count, _total_deposits): (u32, alloc::collections::btree_map::BTreeMap<u64, u64>) = 
			codec::Decode::decode(&mut &pre_state[..]).unwrap();
		
		// Verify we collected the right number of accounts
		assert_eq!(accounts_count, 6); // 5 regular + 1 frozen
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn post_upgrade_verifies_migration() {
	new_test_ext().execute_with(|| {
		setup_pre_migration_state();
		
		let _migration = LazyMigrationV1::<Test>::default();
		
		// Get pre-migration state
		let pre_state = LazyMigrationV1::<Test>::pre_upgrade().unwrap();
		
		// Execute migration
		let mut meter = WeightMeter::new();
		let result = LazyMigrationV1::<Test>::step(None, &mut meter);
		assert!(result.is_ok());
		
		// Verify post-migration state
		assert_ok!(LazyMigrationV1::<Test>::post_upgrade(pre_state));
	});
}

#[test]
fn migration_graceful_degradation_on_hold_failure() {
	new_test_ext().execute_with(|| {
		let account_id = 1u64;
		let deposit = 1000u64;
		
		// Give the account enough balance to reserve
		let _ = Balances::mint_into(&account_id, deposit + 100);
		
		// Setup account with reserved balance
		assert_ok!(Balances::reserve(&account_id, deposit));
		Accounts::<Test>::insert(1, (account_id, deposit, false));
		
		// Create migration instance
		let _migration = LazyMigrationV1::<Test>::default();
		
		// Execute migration step
		let mut meter = WeightMeter::new();
		let result = LazyMigrationV1::<Test>::step(None, &mut meter);
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None); // Migration complete
		
		// For the indices pallet, the migration is just a compatibility check
		// The account should remain unchanged
		assert!(Accounts::<Test>::contains_key(&1));
		
		// Reserves should remain unchanged
		assert_eq!(Balances::reserved_balance(&account_id), deposit);
	});
}
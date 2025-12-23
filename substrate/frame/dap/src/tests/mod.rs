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

//! Tests for the DAP pallet.

use crate::{migrations, mock::*};
use frame_support::traits::{
	fungible::Balanced, tokens::FundingSink, GetStorageVersion, OnRuntimeUpgrade, OnUnbalanced,
	StorageVersion,
};
use sp_runtime::BuildStorage;

type DapPallet = crate::Pallet<Test>;

#[test]
fn genesis_creates_buffer_account() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		// Buffer account should exist after genesis (created via inc_providers)
		assert!(System::account_exists(&buffer));
	});
}

// ===== fill tests =====

#[test]
fn fill_accumulates_from_multiple_sources() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::tokens::Preservation;

		let buffer = DapPallet::buffer_account();

		// Given: accounts have balances, buffer has 0
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(2), 200);
		assert_eq!(Balances::free_balance(3), 300);
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: fill buffer from multiple accounts
		DapPallet::fill(&1, 20, Preservation::Preserve);
		DapPallet::fill(&2, 50, Preservation::Preserve);
		DapPallet::fill(&3, 100, Preservation::Preserve);

		// Then: buffer has accumulated all fills (20 + 50 + 100 = 170)
		assert_eq!(Balances::free_balance(buffer), 170);
		assert_eq!(Balances::free_balance(1), 80);
		assert_eq!(Balances::free_balance(2), 150);
		assert_eq!(Balances::free_balance(3), 200);

		// When: fill with zero amount (no-op)
		DapPallet::fill(&1, 0, Preservation::Preserve);

		// Then: balances unchanged
		assert_eq!(Balances::free_balance(buffer), 170);
		assert_eq!(Balances::free_balance(1), 80);
	});
}

#[test]
fn fill_with_insufficient_balance_transfers_available() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::tokens::Preservation;

		let buffer = DapPallet::buffer_account();

		// Given: account 1 has 100, buffer has 0, ED is 1
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: try to fill 150 (more than balance) with Preserve
		DapPallet::fill(&1, 150, Preservation::Preserve);

		// Then: best-effort transfers 99 (leaving ED of 1)
		assert_eq!(Balances::free_balance(1), 1);
		assert_eq!(Balances::free_balance(buffer), 99);
	});
}

#[test]
fn fill_with_expendable_allows_full_drain() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::tokens::Preservation;

		let buffer = DapPallet::buffer_account();

		// Given: account 1 has 100
		assert_eq!(Balances::free_balance(1), 100);

		// When: fill full balance with Expendable (allows going to 0)
		DapPallet::fill(&1, 100, Preservation::Expendable);

		// Then: account 1 is empty, buffer has 100
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(buffer), 100);
	});
}

#[test]
fn fill_with_preserve_respects_existential_deposit() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::tokens::Preservation;

		let buffer = DapPallet::buffer_account();

		// Given: account 1 has 100, ED is 1 (from TestDefaultConfig)
		assert_eq!(Balances::free_balance(1), 100);
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: try to fill 100 with Preserve (would go below ED)
		DapPallet::fill(&1, 100, Preservation::Preserve);

		// Then: best-effort transfers 99 (leaving ED of 1)
		assert_eq!(Balances::free_balance(1), 1);
		assert_eq!(Balances::free_balance(buffer), 99);
	});
}

// ===== OnUnbalanced (slash) tests =====

#[test]
fn slash_to_dap_accumulates_multiple_slashes_to_buffer() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();

		// Given: buffer has 0
		assert_eq!(Balances::free_balance(buffer), 0);

		// When: multiple slashes occur via OnUnbalanced (simulating a staking slash)
		let credit1 = <Balances as Balanced<u64>>::issue(30);
		DapPallet::on_unbalanced(credit1);

		let credit2 = <Balances as Balanced<u64>>::issue(20);
		DapPallet::on_unbalanced(credit2);

		let credit3 = <Balances as Balanced<u64>>::issue(50);
		DapPallet::on_unbalanced(credit3);

		// Then: buffer has accumulated all slashes (30 + 20 + 50 = 100)
		assert_eq!(Balances::free_balance(buffer), 100);

		// When: slash with zero amount (no-op)
		let credit = <Balances as Balanced<u64>>::issue(0);
		DapPallet::on_unbalanced(credit);

		// Then: buffer unchanged
		assert_eq!(Balances::free_balance(buffer), 100);
	});
}

// ===== Migration tests =====

#[test]
fn check_migration_v0_1() {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances: vec![(1, 100)], ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	sp_io::TestExternalities::from(t).execute_with(|| {
		let buffer = DapPallet::buffer_account();

		// Given: on-chain storage version is 0, buffer account doesn't exist
		assert_eq!(DapPallet::on_chain_storage_version(), StorageVersion::new(0));
		assert!(!System::account_exists(&buffer));

		// When: run the versioned migration
		let _ = migrations::v1::InitBufferAccount::<Test>::on_runtime_upgrade();

		// Then: version updated to 1, buffer account created
		assert_eq!(DapPallet::on_chain_storage_version(), StorageVersion::new(1));
		assert!(System::account_exists(&buffer));
	});
}

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

//! Genesis tests for the DAP pallet.

use crate::{mock::*, BudgetAllocation, BudgetAllocationMap};
#[cfg(feature = "try-runtime")]
use frame_support::traits::Hooks;
use frame_support::traits::{GetStorageVersion, StorageVersion};
use sp_runtime::{BuildStorage, Perbill};
use sp_staking::budget::{BudgetKey, BudgetRecipient};

type DapPallet = crate::Pallet<Test>;

fn allocation(entries: &[(BudgetKey, u32)]) -> BudgetAllocationMap {
	let mut map = BudgetAllocationMap::new();
	for (key, percent) in entries {
		map.try_insert(key.clone(), Perbill::from_percent(*percent)).unwrap();
	}
	map
}

/// Builds storage from the mock runtime, seeding DAP's `budget_allocation` and running
/// `OnGenesis`, which is what writes the on-chain storage version on a fresh chain.
fn build_genesis(budget_allocation: Option<BudgetAllocationMap>) -> sp_io::TestExternalities {
	RuntimeGenesisConfig {
		dap: DapConfig { budget_allocation, ..Default::default() },
		..Default::default()
	}
	.build_storage()
	.unwrap()
	.into()
}

#[test]
fn genesis_creates_buffer_account() {
	build_and_execute(true, || {
		set_default_budget_allocation();

		let buffer = DapPallet::buffer_account();
		// Buffer account should exist after genesis (created via inc_providers)
		assert!(System::account_exists(&buffer));
	});
}

#[test]
fn genesis_config_seeds_budget_allocation() {
	let budget =
		allocation(&[(DapPallet::budget_key(), 15), (TestStakerRecipient::budget_key(), 85)]);

	build_genesis(Some(budget.clone())).execute_with(|| {
		assert_eq!(BudgetAllocation::<Test>::get(), budget);
		// A chain built from genesis is marked as post-seeding by `OnGenesis`.
		assert_eq!(DapPallet::on_chain_storage_version(), StorageVersion::new(2));
		assert!(DapPallet::do_try_state().is_ok());
	});
}

#[test]
fn genesis_config_default_leaves_budget_empty() {
	build_genesis(None).execute_with(|| {
		assert!(BudgetAllocation::<Test>::get().is_empty());
		// Seeding is opt-in: an unseeded chain must still fail the invariant check rather
		// than be silently accepted.
		assert!(DapPallet::do_try_state().is_err());
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_state_runs_at_min_seeded_version() {
	build_genesis(None).execute_with(|| {
		// Fresh genesis puts the on-chain version at 2, so the check runs.
		assert!(<DapPallet as Hooks<u64>>::try_state(0).is_err());
	});

	build_genesis(Some(allocation(&[(DapPallet::budget_key(), 100)]))).execute_with(|| {
		assert!(<DapPallet as Hooks<u64>>::try_state(0).is_ok());
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_state_skipped_below_min_seeded_version() {
	build_genesis(None).execute_with(|| {
		StorageVersion::new(1).put::<DapPallet>();
		assert!(<DapPallet as Hooks<u64>>::try_state(0).is_ok());
	});
}

#[test]
#[should_panic(expected = "does not sum to 100%")]
fn genesis_rejects_allocation_not_summing_to_100() {
	build_genesis(Some(allocation(&[(DapPallet::budget_key(), 60)])));
}

#[test]
#[should_panic(expected = "is not a registered BudgetRecipient")]
fn genesis_rejects_unknown_budget_key() {
	build_genesis(Some(allocation(&[(BudgetKey::truncate_from(b"not_registered".to_vec()), 100)])));
}

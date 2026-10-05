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

mod budget;
mod drain;
mod drip;
mod genesis;
mod on_unbalanced;

use crate::{
	mock::{account_id, AccountId, NativeAndAssets, Test},
	AssetAllocationMap, AssetKindOf, BalanceOf, BudgetAllocationMap, SingleAssetAllocation,
};
use frame_support::{
	assert_ok,
	traits::{
		fungible::NativeOrWithId,
		fungibles::{Create, Inspect},
		tokens::{Fortitude, Preservation},
	},
};
use sp_runtime::{BoundedBTreeMap, Perbill};
use sp_staking::budget::BudgetKey;

fn key(name: &[u8]) -> BudgetKey {
	BudgetKey::truncate_from(name.to_vec())
}

fn budget_map(entries: &[(&[u8], u32)]) -> BudgetAllocationMap {
	let mut map = BoundedBTreeMap::new();
	for (name, pct) in entries {
		map.try_insert(key(name), Perbill::from_percent(*pct)).unwrap();
	}
	map
}

fn asset_allocations(
	entries: &[(AssetKindOf<Test>, &[(&[u8], BalanceOf<Test>)])],
) -> AssetAllocationMap<AssetKindOf<Test>, BalanceOf<Test>> {
	let mut map = BoundedBTreeMap::new();

	for (asset, budget) in entries {
		let mut asset_map = BoundedBTreeMap::new();
		for (name, amount) in *budget {
			asset_map
				.try_insert(key(name), SingleAssetAllocation { amount_per_ms: *amount })
				.expect("Too much assets per budget key");
		}

		map.try_insert(asset.clone(), asset_map).expect("Too much budget recipients");
	}

	map
}

fn balance_of_asset(asset_id: u32, account: &AccountId) -> u64 {
	NativeAndAssets::reducible_balance(
		NativeOrWithId::WithId(asset_id),
		account,
		Preservation::Expendable,
		Fortitude::Polite,
	)
}

fn create_asset(asset_id: u32) {
	assert_ok!(NativeAndAssets::create(NativeOrWithId::WithId(asset_id), account_id(0), false, 1));
}

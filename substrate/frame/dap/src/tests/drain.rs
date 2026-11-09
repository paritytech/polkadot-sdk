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

//! Drain tests for the DAP pallet.

use super::{asset_allocations, balance_of_asset, budget_map, create_asset};
use crate::{
	mock::{
		build_and_execute, AccountId, Balances, Dap, NativeAndAssets, RuntimeOrigin, System, Test,
	},
	Event,
};
use frame_support::{
	assert_ok,
	pallet_prelude::Weight,
	traits::{
		fungible::{Inspect, Mutate as FungibleMutate, NativeOrWithId},
		fungibles::Mutate,
		tokens::{Fortitude, Preservation},
		Hooks,
	},
};

fn native_balance(account: &AccountId) -> u64 {
	Balances::reducible_balance(&account, Preservation::Preserve, Fortitude::Polite)
}

#[test]
fn native_asset_is_drained() {
	build_and_execute(true, || {
		System::set_block_number(1);

		let budget_allocations = budget_map(&[(b"buffer", 100)]);
		assert_ok!(Dap::set_allocations(RuntimeOrigin::root(), Some(budget_allocations), None));

		let staging = Dap::staging_account();
		let buffer = Dap::buffer_account();

		assert_ok!(Balances::mint_into(&staging, 1_000));

		assert_eq!(native_balance(&buffer), 0);
		assert_eq!(native_balance(&staging), 1_000);

		Dap::on_idle(0, Weight::MAX);

		assert_eq!(native_balance(&buffer), 1_000);
		assert_eq!(native_balance(&staging), 0);

		System::assert_has_event(Event::<Test>::StagingDrained { amount: 1_000 }.into());
	});
}

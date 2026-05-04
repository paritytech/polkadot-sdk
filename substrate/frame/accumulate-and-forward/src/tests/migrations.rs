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

//! Unit tests for `DrainLegacyTreasuryToAccumulationAccount`.

use crate::{migrations::DrainLegacyTreasuryToAccumulationAccount, mock::*, Pallet};
use frame_support::{
	traits::{
		fungible::{Inspect, Mutate},
		OnRuntimeUpgrade,
	},
	PalletId,
};
use sp_runtime::traits::AccountIdConversion;

type Migration = DrainLegacyTreasuryToAccumulationAccount<Test>;

fn legacy_account() -> u64 {
	PalletId(*b"py/trsry").into_account_truncating()
}

fn accumulation_account() -> u64 {
	Pallet::<Test>::accumulation_account()
}

#[test]
fn zero_source_is_a_no_op() {
	new_test_ext(true).execute_with(|| {
		// Legacy treasury has no balance at all.
		assert_eq!(Balances::free_balance(legacy_account()), 0);
		let accum_before = Balances::free_balance(accumulation_account());
		let issuance_before = <Balances as Inspect<_>>::total_issuance();

		Migration::on_runtime_upgrade();

		assert_eq!(Balances::free_balance(accumulation_account()), accum_before);
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), issuance_before);
	});
}

#[test]
fn only_ed_in_source_is_also_a_no_op() {
	new_test_ext(true).execute_with(|| {
		let ed = <Balances as Inspect<_>>::minimum_balance();
		// Fund with exactly ED — reducible_balance(Preserve) returns 0.
		Balances::mint_into(&legacy_account(), ed).unwrap();
		let accum_before = Balances::free_balance(accumulation_account());
		let issuance_before = <Balances as Inspect<_>>::total_issuance();

		Migration::on_runtime_upgrade();

		assert_eq!(Balances::free_balance(accumulation_account()), accum_before);
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), issuance_before);
	});
}

#[test]
fn drains_reducible_balance_to_accumulation_account() {
	new_test_ext(true).execute_with(|| {
		let ed = <Balances as Inspect<_>>::minimum_balance();
		// reducible = 90 (total 100, ED 10 kept by Preserve)
		Balances::mint_into(&legacy_account(), ed + 90).unwrap();
		let accum_before = Balances::free_balance(accumulation_account());
		let issuance_before = <Balances as Inspect<_>>::total_issuance();

		Migration::on_runtime_upgrade();

		assert_eq!(Balances::free_balance(legacy_account()), ed);
		assert_eq!(Balances::free_balance(accumulation_account()), accum_before + 90);
		assert_eq!(<Balances as Inspect<_>>::total_issuance(), issuance_before);
	});
}

#[test]
fn idempotent_second_run_is_a_no_op() {
	new_test_ext(true).execute_with(|| {
		let ed = <Balances as Inspect<_>>::minimum_balance();
		Balances::mint_into(&legacy_account(), ed + 50).unwrap();

		Migration::on_runtime_upgrade();
		let accum_after_first = Balances::free_balance(accumulation_account());

		// Second run: legacy reducible is now 0 → early return.
		Migration::on_runtime_upgrade();
		assert_eq!(Balances::free_balance(accumulation_account()), accum_after_first);
	});
}

#[test]
fn total_issuance_invariant_holds() {
	new_test_ext(true).execute_with(|| {
		let ed = <Balances as Inspect<_>>::minimum_balance();
		Balances::mint_into(&legacy_account(), ed + 200).unwrap();
		let issuance_before = <Balances as Inspect<_>>::total_issuance();

		Migration::on_runtime_upgrade();

		assert_eq!(<Balances as Inspect<_>>::total_issuance(), issuance_before);
	});
}

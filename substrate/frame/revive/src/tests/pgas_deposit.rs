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

//! Tests for the PGAS-backed storage deposit orchestrators.
//!
//! The `Test` runtime binds `PgasBackend = ()`, so every charge/refund path deterministically
//! exercises the DOT fallback. These tests pin down the bookkeeping around
//! [`DotByContractUser`], `historic_deposit` and the termination prefix-clear.

use crate::{
	AccountInfo, AccountInfoOf, AccountType, Config, ContractInfo, DepositAsset,
	DotByContractUser, ExecConfig, HoldReason, Pallet,
	test_utils::*,
	tests::{ExtBuilder, Test, test_utils::get_balance_on_hold},
};
use codec::Encode;
use frame_support::{assert_ok, traits::fungible::Mutate};
use pretty_assertions::assert_eq;
use sp_core::H160;

/// Insert a contract record for `address` with the given `historic_deposit`.
///
/// The other `ContractInfo` fields are left at their defaults. `trie_id` is derived from the
/// address so each call produces a distinct value.
fn insert_contract_with_historic(address: H160, historic_deposit: u128) {
	let trie_id = ("test_trie", address).using_encoded(sp_io::hashing::blake2_256);
	let contract = ContractInfo::<Test> {
		trie_id: trie_id.to_vec().try_into().unwrap(),
		code_hash: sp_core::H256::zero(),
		storage_bytes: 0,
		storage_items: 0,
		storage_byte_deposit: 0,
		storage_item_deposit: 0,
		storage_base_deposit: 0,
		historic_deposit,
		immutable_data_len: 0,
	};
	AccountInfo::<Test>::insert_contract(&address, contract);
}

fn historic_deposit_of(address: &H160) -> u128 {
	match AccountInfoOf::<Test>::get(address).unwrap().account_type {
		AccountType::Contract(c) => c.historic_deposit,
		AccountType::EOA => panic!("address is not a contract"),
	}
}

fn charge_storage(origin: &sp_runtime::AccountId32, contract: &sp_runtime::AccountId32, amount: u128) {
	assert_ok!(Pallet::<Test>::charge_deposit(
		Some(HoldReason::StorageDepositReserve),
		origin,
		contract,
		amount,
		&ExecConfig::<Test>::new_substrate_tx(),
	));
}

fn refund_storage(origin: &sp_runtime::AccountId32, contract: &sp_runtime::AccountId32, amount: u128) {
	assert_ok!(Pallet::<Test>::refund_deposit(
		HoldReason::StorageDepositReserve,
		contract,
		origin,
		amount,
		DepositAsset::DotConvertible,
		Some(&ExecConfig::<Test>::new_substrate_tx()),
	));
}

#[test]
fn charge_storage_deposit_takes_dot_and_records_entitlement() {
	ExtBuilder::default().build().execute_with(|| {
		let origin = ALICE;
		let contract_account = BOB;
		let contract_address = BOB_ADDR;
		let amount: u128 = 1_000;

		<Test as Config>::Currency::set_balance(&origin, 10_000);
		<Test as Config>::Currency::set_balance(&contract_account, 1);

		charge_storage(&origin, &contract_account, amount);

		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract_account),
			amount,
		);
		assert_eq!(DotByContractUser::<Test>::get(&contract_address, &origin), amount,);
	});
}

#[test]
fn refund_storage_deposit_consumes_historic_deposit_first() {
	ExtBuilder::default().build().execute_with(|| {
		let origin = ALICE;
		let contract_account = BOB;
		let contract_address = BOB_ADDR;
		let charge: u128 = 1_000;
		let historic: u128 = 400;

		<Test as Config>::Currency::set_balance(&origin, 10_000);
		<Test as Config>::Currency::set_balance(&contract_account, 1);

		insert_contract_with_historic(contract_address, historic);

		charge_storage(&origin, &contract_account, charge);

		// Refund `historic` worth: historic_deposit must absorb it all, entitlement untouched.
		refund_storage(&origin, &contract_account, historic);

		assert_eq!(historic_deposit_of(&contract_address), 0);
		assert_eq!(DotByContractUser::<Test>::get(&contract_address, &origin), charge,);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract_account),
			charge - historic,
		);
	});
}

#[test]
fn refund_storage_deposit_spills_to_dot_convertible_after_historic() {
	ExtBuilder::default().build().execute_with(|| {
		let origin = ALICE;
		let contract_account = BOB;
		let contract_address = BOB_ADDR;
		let charge: u128 = 1_000;
		let historic: u128 = 300;
		let refund: u128 = 500;

		<Test as Config>::Currency::set_balance(&origin, 10_000);
		<Test as Config>::Currency::set_balance(&contract_account, 1);

		insert_contract_with_historic(contract_address, historic);

		charge_storage(&origin, &contract_account, charge);

		refund_storage(&origin, &contract_account, refund);

		// historic drained first, remainder taken from the per-user entitlement.
		assert_eq!(historic_deposit_of(&contract_address), 0);
		let spill = refund - historic;
		assert_eq!(
			DotByContractUser::<Test>::get(&contract_address, &origin),
			charge - spill,
		);
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract_account),
			charge - refund,
		);
	});
}

#[test]
fn refund_storage_deposit_removes_entry_when_exhausted() {
	ExtBuilder::default().build().execute_with(|| {
		let origin = ALICE;
		let contract_account = BOB;
		let contract_address = BOB_ADDR;
		let charge: u128 = 750;

		<Test as Config>::Currency::set_balance(&origin, 10_000);
		<Test as Config>::Currency::set_balance(&contract_account, 1);

		charge_storage(&origin, &contract_account, charge);

		refund_storage(&origin, &contract_account, charge);

		assert!(!DotByContractUser::<Test>::contains_key(&contract_address, &origin,));
		assert_eq!(
			get_balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract_account),
			0,
		);
	});
}

#[test]
fn charge_code_upload_deposit_returns_dot_convertible_on_dot_path() {
	ExtBuilder::default().build().execute_with(|| {
		let owner = ALICE;
		let amount: u128 = 2_000;
		let pallet = Pallet::<Test>::account_id();

		<Test as Config>::Currency::set_balance(&owner, 10_000);

		let before = get_balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &pallet);
		let asset = Pallet::<Test>::charge_deposit(
			Some(HoldReason::CodeUploadDepositReserve),
			&owner,
			&pallet,
			amount,
			&ExecConfig::<Test>::new_substrate_tx(),
		)
		.unwrap();

		assert_eq!(asset, DepositAsset::DotConvertible);
		assert_eq!(
			get_balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &pallet),
			before + amount,
		);

		assert_ok!(Pallet::<Test>::refund_deposit(
			HoldReason::CodeUploadDepositReserve,
			&pallet,
			&owner,
			amount,
			DepositAsset::DotConvertible,
			None,
		));

		assert_eq!(
			get_balance_on_hold(&HoldReason::CodeUploadDepositReserve.into(), &pallet),
			before,
		);
	});
}

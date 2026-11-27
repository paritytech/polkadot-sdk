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

//! The pallet refuses to change state while a multi-block migration is running.

use crate::{
	AccountInfoOf, Code, CodeInfoOf, Config, Error, ExecConfig,
	storage::AccountType,
	test_utils::{ALICE, builder::Contract, deposit_limit},
	tests::{Contracts, ExtBuilder, MigrationOngoing, RuntimeOrigin, Test, builder},
};
use frame_support::{assert_err_ignore_postinfo, assert_noop, assert_ok, traits::fungible::Mutate};
use pallet_revive_fixtures::compile_module;

fn assert_ongoing<R: core::fmt::Debug>(result: Result<R, sp_runtime::DispatchError>) {
	assert_eq!(result.unwrap_err(), Error::<Test>::MigrationsOngoing.into());
}

#[test]
fn state_changes_are_refused_while_migrations_run() {
	let (binary, code_hash) = compile_module("dummy").unwrap();
	let (other_binary, other_code_hash) = compile_module("crypto_hash_keccak_256").unwrap();
	assert_ne!(code_hash, other_code_hash);

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(binary.clone())).build_and_unwrap_contract();
		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			other_binary.clone(),
			deposit_limit::<Test>(),
		));
		let contract_code_hash = || match AccountInfoOf::<Test>::get(addr).unwrap().account_type {
			AccountType::Contract(info) => info.code_hash,
			_ => panic!("not a contract"),
		};
		assert_eq!(contract_code_hash(), code_hash);

		MigrationOngoing::set(true);

		// Calls and instantiations, from a dispatchable and from the bare entry points.
		assert_ongoing(builder::bare_call(addr).build().result);
		assert_ongoing(
			builder::bare_instantiate(Code::Upload(binary.clone()))
				.salt(Some([1; 32]))
				.build()
				.result,
		);
		assert_err_ignore_postinfo!(builder::call(addr).build(), Error::<Test>::MigrationsOngoing);
		assert_err_ignore_postinfo!(
			builder::instantiate_with_code(binary.clone()).salt(Some([2; 32])).build(),
			Error::<Test>::MigrationsOngoing
		);
		assert_err_ignore_postinfo!(
			builder::instantiate(code_hash).salt(Some([3; 32])).build(),
			Error::<Test>::MigrationsOngoing
		);

		// Ethereum transactions.
		assert_err_ignore_postinfo!(
			builder::eth_call(addr).build(),
			Error::<Test>::MigrationsOngoing
		);
		assert_err_ignore_postinfo!(
			builder::eth_instantiate_with_code(binary.clone()).build(),
			Error::<Test>::MigrationsOngoing
		);

		// Code management.
		assert_ongoing(Contracts::bare_upload_code(
			RuntimeOrigin::signed(ALICE),
			binary.clone(),
			deposit_limit::<Test>(),
		));
		assert_noop!(
			Contracts::upload_code(
				RuntimeOrigin::signed(ALICE),
				binary.clone(),
				deposit_limit::<Test>(),
			),
			Error::<Test>::MigrationsOngoing
		);
		assert_err_ignore_postinfo!(
			Contracts::remove_code(RuntimeOrigin::signed(ALICE), other_code_hash),
			Error::<Test>::MigrationsOngoing
		);
		assert_noop!(
			Contracts::set_code(RuntimeOrigin::root(), addr, other_code_hash),
			Error::<Test>::MigrationsOngoing
		);

		// Nothing above changed anything.
		assert!(CodeInfoOf::<Test>::contains_key(other_code_hash));
		assert_eq!(contract_code_hash(), code_hash);

		// A dry run commits nothing, so it is still served.
		let dry_run = builder::bare_call(addr)
			.exec_config(ExecConfig::new_substrate_tx().with_dry_run(None))
			.build();
		assert_ok!(dry_run.result);

		// Everything works again once the migration is done.
		MigrationOngoing::set(false);
		assert_ok!(builder::bare_call(addr).build().result);
		assert_ok!(Contracts::remove_code(RuntimeOrigin::signed(ALICE), other_code_hash));
		assert!(!CodeInfoOf::<Test>::contains_key(other_code_hash));
	});
}

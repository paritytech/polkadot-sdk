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

use crate::{
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config, StorageDeposit,
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Deposit, FixtureType};
use test_case::test_case;

#[test_case(FixtureType::Solc    ; "solc")]
#[test_case(FixtureType::Resolc  ; "resolc")]
fn max_consumed_deposit_integration(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Deposit", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr).data(Deposit::dCall {}.abi_encode()).build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

#[ignore = "TODO: Does not work yet, see https://github.com/paritytech/contract-issues/issues/213"]
#[test_case(FixtureType::Solc    ; "solc")]
#[test_case(FixtureType::Resolc  ; "resolc")]
fn max_consumed_deposit_integration_refunds_subframes(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Deposit", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr).data(Deposit::cCall {}.abi_encode()).build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));

		builder::bare_call(caller_addr).data(Deposit::clearCall {}.abi_encode()).build();

		let result = builder::bare_call(caller_addr).data(Deposit::eCall {}.abi_encode()).build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

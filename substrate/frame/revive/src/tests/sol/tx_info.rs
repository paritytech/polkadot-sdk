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

//! The pallet-revive shared VM integration test suite.

use crate::{
	test_utils::{builder::Contract, ALICE, ALICE_ADDR},
	tests::{builder, ExtBuilder, Test},
	vm::evm::U256Converter,
	Code, Config, Pallet,
};

use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, TransactionInfo};
use pretty_assertions::assert_eq;
use sp_core::H160;
use test_case::test_case;

/// Tests that the gasprice opcode works as expected.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn gasprice_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("TransactionInfo", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				TransactionInfo::TransactionInfoCalls::gasprice(TransactionInfo::gaspriceCall {})
					.abi_encode(),
			)
			.build_and_unwrap_result();
		let decoded = TransactionInfo::gaspriceCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(<Pallet<Test>>::evm_gas_price().into_revm_u256(), decoded);
	});
}

/// Tests that the origin opcode works as expected.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn origin_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("TransactionInfo", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				TransactionInfo::TransactionInfoCalls::origin(TransactionInfo::originCall {})
					.abi_encode(),
			)
			.build_and_unwrap_result();
		let decoded = TransactionInfo::originCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(ALICE_ADDR, H160::from_slice(decoded.as_slice()));
	});
}

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
	Code, Config,
};
use alloy_core::sol_types::{SolCall, SolConstructor, SolValue};
use frame_support::traits::fungible::Mutate;
use hex_literal::hex;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Terminate};
use pretty_assertions::assert_eq;
use test_case::test_case;
use crate::test_utils::DJANGO_ADDR;

/// Decode a contract return value into an error string.
fn decode_error(output: &[u8]) -> String {
	assert!(output.len() >= 4 && &output[..4] == &hex!("08c379a0"));
	String::abi_decode(&output[4..]).unwrap()
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn base_case(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: true, beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::terminateCall { beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();

		assert_eq!(result.data, Vec::<u8>::new());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_in_constructor(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let result = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: false, beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();

		assert!(result.result.did_revert());
		assert_eq!(
			decode_error(result.result.data.as_ref()),
			"terminate pre-compile cannot be called from the constructor"
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_for_direct_delegate(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: true, beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::delegateTerminateCall { beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();

		assert!(result.did_revert());
		assert_eq!(
			decode_error(result.data.as_ref()),
			"illegal to call this pre-compile via delegate call",
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_for_indirect_delegate(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: true, beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::indirectDelegateTerminateCall { beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_result();

		assert!(result.did_revert());
		assert_eq!(
			decode_error(result.data.as_ref()),
			"illegal to call this pre-compile via delegate call",
		);
	});
}

#[test]
fn sent_funds_after_terminate_shall_be_credited_to_beneficiary() {
	let (code, _) = compile_module_with_type("Terminate", FixtureType::Solc).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: true, beneficiary: DJANGO_ADDR.0.into() }.abi_encode())
			.build_and_unwrap_contract();

        let result = builder::bare_call(addr)
            .data(Terminate::terminateCall { beneficiary: DJANGO_ADDR.0.into() }.abi_encode())       // Call terminate function
            .build_and_unwrap_result();
		

		assert_eq!(result.data, Vec::<u8>::new());
	});
}

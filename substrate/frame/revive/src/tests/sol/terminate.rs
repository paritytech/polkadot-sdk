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
use alloy_core::sol_types::{SolCall, SolConstructor};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Terminate};
use pretty_assertions::assert_eq;
use test_case::test_case;

/// Decode a contract return value into an error string.
fn decode_error(output: &[u8]) -> String {
	use alloy_core::sol_types::SolError;
	alloy_core::sol! { error Error(string); }
	Error::abi_decode_validate(output).unwrap().0
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn base_case(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Terminate::constructorCall { skip: true }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::terminateCall {}.abi_encode())
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
			.constructor_data(Terminate::constructorCall { skip: false }.abi_encode())
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
			.constructor_data(Terminate::constructorCall { skip: true }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::delegateTerminateCall {}.abi_encode())
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
			.constructor_data(Terminate::constructorCall { skip: true }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Terminate::indirectDelegateTerminateCall {}.abi_encode())
			.build_and_unwrap_result();

		assert!(result.did_revert());
		assert_eq!(
			decode_error(result.data.as_ref()),
			"illegal to call this pre-compile via delegate call",
		);
	});
}

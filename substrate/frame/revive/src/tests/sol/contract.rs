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

use core::iter;

use crate::{
	address::AddressMapper,
	evm::{decode_revert_reason, fees::InfoT},
	test_utils::{builder::Contract, deposit_limit, ALICE, ALICE_ADDR, BOB_ADDR, GAS_LIMIT},
	tests::{builder, ExtBuilder, MockHandlerImpl, Test},
	BalanceOf, Code, Config, DelegateInfo, DispatchError, Error, ExecConfig, ExecOrigin,
	ExecReturnValue,
};
use alloy_core::{
	primitives::{Bytes, FixedBytes},
	sol_types::{Revert, SolCall, SolError, SolInterface},
};
use frame_support::{
	assert_err,
	traits::fungible::{Balanced, Mutate},
};
use itertools::Itertools;
use pallet_revive_fixtures::{compile_module_with_type, Callee, Caller, FixtureType};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;
use sp_core::H160;
use test_case::test_case;

/// Tests that the `CALL` opcode works as expected by having one contract call another.
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn staticcall_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let magic_number = 42u64;
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::staticCallCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(magic_number, echo_output, "the call must reproduce the magic number");

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::staticCallCall {
					_callee: callee_addr.0.into(),
					_data: Callee::storeCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();
		assert!(!result.success, "Can not store in static call");
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let magic_number = 42u64;
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(magic_number, echo_output, "the call must reproduce the magic number");

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::storeCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the store call must succeed");
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_revert(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Call revert and assert failure
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::revertCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(!result.success, "Call should propagate revert");
		assert!(result.output.len() > 0, "Returned data should contain revert message");

		let data = result.output.as_ref();
		if data.len() >= 4 && &data[..4] == Revert::SELECTOR {
			let reason = decode_revert_reason(data).expect("Failed to decode revert reason");
			assert_eq!(reason, "revert: This is a revert");
		} else {
			panic!("Error selector not found in revert data");
		}
	});
}

#[test]
fn deploy_revert() {
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(Caller::createRevertCall {}.abi_encode())
			.build_and_unwrap_result();

		let data: &[u8] = result.data.as_ref();
		if data.len() >= 72 && &data[68..72] == Revert::SELECTOR {
			let reason = decode_revert_reason(&data[68..]).expect("Failed to decode revert reason");
			assert_eq!(reason, "revert: ChildRevert: revert in constructor");
		} else {
			panic!("Error selector not found at expected position 68");
		}
	});
}

// This test has a `caller` contract calling into a `callee` contract which then executes the
// INVALID opcode. INVALID consumes all gas which means that it will error with OutOfGas.
#[ignore = "TODO: ignore until we decide what is the correct way to handle this"]
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_invalid_opcode(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::invalidCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();

		assert!(!result.success, "Invalid opcode should propagate as error");

		let data = result.output.as_ref();
		assert!(data.iter().all(|&x| x == 0), "Returned data should be empty")
	});
}

#[test]
fn invalid_opcode_evm() {
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let result = builder::bare_call(callee_addr)
			.data(Callee::invalidCall {}.abi_encode().into())
			.build();
		assert_err!(result.result, Error::<Test>::InvalidInstruction);
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_stop_opcode(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::stopCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();

		assert!(result.success);

		let data = result.output.as_ref();
		assert!(data.iter().all(|&x| x == 0), "Returned data should be empty")
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn delegatecall_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let magic_number = 42u64;
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(magic_number, echo_output, "the call must reproduce the magic number");

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: callee_addr.0.into(),
					_data: Callee::whoSenderCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the whoSender call must succeed");
		let decoded = Callee::whoSenderCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(ALICE_ADDR, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn mock_caller_hook_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Set BOB as the mock caller and check whoSender returns BOB's address.
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_data: Callee::whoSenderCall {}.abi_encode().into(),
					_gas: u64::MAX,
					_value: 0,
				}
				.abi_encode(),
			)
			.exec_config(ExecConfig {
				bump_nonce: false,
				collect_deposit_from_hold: None,
				effective_gas_price: None,
				is_dry_run: None,
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_caller: Some(BOB_ADDR),
					mock_call: Default::default(),
					mock_delegate_caller: Default::default(),
				})),
			})
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the whoSender call must succeed");
		let decoded = Callee::whoSenderCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(BOB_ADDR, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn mock_call_hook_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Set up mocked_magic_number to be returned by the mock call handler and check that is
		// returned instead of magic_number.
		let magic_number = 42u64;
		let mocked_magic_number = 99u64;
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.exec_config(ExecConfig {
				bump_nonce: false,
				collect_deposit_from_hold: None,
				effective_gas_price: None,
				is_dry_run: None,
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_caller: None,
					mock_call: iter::once((
						callee_addr,
						ExecReturnValue {
							flags: ReturnFlags::default(),
							data: alloy_core::sol_types::SolValue::abi_encode(&mocked_magic_number)
								.into(),
						},
					))
					.collect(),
					mock_delegate_caller: Default::default(),
				})),
			})
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(mocked_magic_number, echo_output, "the call must reproduce the magic number");
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn mock_delegatecall_hook_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Instantiate the call with an invalid callee and check that the delegatecall uses the
		// mocked callee address.
		let magic_number = 42u64;
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: caller_addr.0.into(), // Wrong callee, should be overridden by the mock hook.
					_value: 0,
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.exec_config(ExecConfig {
				bump_nonce: false,
				collect_deposit_from_hold: None,
				effective_gas_price: None,
				is_dry_run: None,
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_caller: None,
					mock_call: Default::default(),
					mock_delegate_caller: iter::once((
						Callee::echoCall { _data: magic_number }.abi_encode().into(),
						DelegateInfo {
							callee: callee_addr,
							caller: ExecOrigin::<Test>::from_runtime_origin(crate::OriginFor::<Test>::signed(
								<Test as crate::pallet::Config>::AddressMapper::to_fallback_account_id(
									&caller_addr,
								),
							)).expect("Conversion to ExecOrigin must work"),
						},
					))
					.collect(),
				})),
			})
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(magic_number, echo_output, "the call must reproduce the magic number");
	});
}

#[test]
fn create_works() {
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let create_call_data =
			Caller::createCall { initcode: Bytes::from(callee_code.clone()) }.abi_encode();

		let result = builder::bare_call(caller_addr)
			.data(create_call_data)
			.native_value(1_000)
			.build_and_unwrap_result();

		let callee_addr = Caller::createCall::abi_decode_returns(&result.data).unwrap();
		let magic_number = 42u64;

		// Check if the created contract is working
		let echo_result = builder::bare_call(callee_addr.0 .0.into())
			.data(Callee::echoCall { _data: magic_number }.abi_encode())
			.build_and_unwrap_result();

		let echo_output = Callee::echoCall::abi_decode_returns(&echo_result.data).unwrap();

		assert_eq!(magic_number, echo_output, "Callee.echo must return 42");
	});
}

#[test]
fn create2_works() {
	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let salt = [42u8; 32];

		let initcode = Bytes::from(callee_code);
		// Prepare the CREATE2 call
		let create_call_data =
			Caller::create2Call { initcode: initcode.clone(), salt: FixedBytes(salt) }.abi_encode();

		let result = builder::bare_call(caller_addr)
			.data(create_call_data)
			.native_value(1000)
			.build_and_unwrap_result();

		let callee_addr = Caller::create2Call::abi_decode_returns(&result.data).unwrap();

		// Compute expected CREATE2 address
		let expected_addr = crate::address::create2(&caller_addr, &initcode, &[], &salt);

		let callee_addr: H160 = callee_addr.0 .0.into();
		assert_eq!(callee_addr, expected_addr, "CREATE2 address should be deterministic");
		let magic_number = 42u64;

		// Check if the created contract is working
		let echo_result = builder::bare_call(callee_addr)
			.data(Callee::echoCall { _data: magic_number }.abi_encode())
			.build_and_unwrap_result();

		let echo_output = Callee::echoCall::abi_decode_returns(&echo_result.data).unwrap();

		assert_eq!(magic_number, echo_output, "Callee.echo must return 42");
	});
}

#[test]
fn instantiate_from_constructor_works() {
	use pallet_revive_fixtures::CallerWithConstructor::*;

	let (caller_code, _) =
		compile_module_with_type("CallerWithConstructor", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let data = CallerWithConstructorCalls::callBar(callBarCall {}).abi_encode();
		let result = builder::bare_call(addr).data(data).build_and_unwrap_result();
		let result = callBarCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(result, 42u64);
	});
}

/// No resolc caller since the subcall limiting is not implemented on resolc, yet.
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
fn subcall_effectively_limited_substrate_tx(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	let no_collection_config = ExecConfig::new_substrate_tx();
	let mut collection_config = ExecConfig::new_substrate_tx();
	collection_config.collect_deposit_from_hold = Some(Default::default());
	let configs = [no_collection_config, collection_config];

	let call_types = [0u8, 1, 2];

	struct Case {
		deposit_limit: BalanceOf<Test>,
		gas_divisor: u64,
		callee_input: Vec<u8>,
		result: Result<bool, DispatchError>,
		is_store_call: bool,
	}

	let test_cases = [
		Case {
			deposit_limit: deposit_limit::<Test>(),
			gas_divisor: 1,
			callee_input: Callee::consumeAllReftimeCall {}.abi_encode(),
			result: Err(<Error<Test>>::OutOfGas.into()),
			is_store_call: false,
		},
		Case {
			deposit_limit: deposit_limit::<Test>(),
			gas_divisor: 2,
			callee_input: Callee::consumeAllReftimeCall {}.abi_encode(),
			result: Ok(false),
			is_store_call: false,
		},
		Case {
			deposit_limit: deposit_limit::<Test>(),
			gas_divisor: u64::MAX,
			callee_input: Callee::consumeAllReftimeCall {}.abi_encode(),
			result: Ok(false),
			is_store_call: false,
		},
		Case {
			deposit_limit: 130,
			gas_divisor: 1,
			callee_input: Callee::storeCall { _data: 42 }.abi_encode(),
			result: Err(<Error<Test>>::StorageDepositLimitExhausted.into()),
			is_store_call: true,
		},
		Case {
			deposit_limit: 130,
			gas_divisor: 2,
			callee_input: Callee::storeCall { _data: 42 }.abi_encode(),
			result: Ok(false),
			is_store_call: true,
		},
		Case {
			deposit_limit: 130,
			gas_divisor: u64::MAX,
			callee_input: Callee::storeCall { _data: 42 }.abi_encode(),
			result: Ok(false),
			is_store_call: true,
		},
		Case {
			deposit_limit: deposit_limit::<Test>(),
			gas_divisor: 2,
			callee_input: Callee::storeCall { _data: 42 }.abi_encode(),
			result: Ok(true),
			is_store_call: true,
		},
	];

	for ((case, config), call_type) in
		test_cases.iter().cartesian_product(configs).cartesian_product(call_types)
	{
		// the storage stuff won't work on static or delegate call
		if case.is_store_call && call_type != 0 {
			continue
		}

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let fees =
				<Test as Config>::FeeInfo::tx_fee_from_weight(0, &GAS_LIMIT) + case.deposit_limit;
			<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(fees));

			// Instantiate the callee contract, which can echo a value.
			let Contract { addr: callee_addr, .. } =
				builder::bare_instantiate(Code::Upload(callee_code.clone()))
					.build_and_unwrap_contract();

			// Instantiate the caller contract.
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code.clone()))
					.build_and_unwrap_contract();

			let output = builder::bare_call(caller_addr)
				.data(
					Caller::callPartialGasCall {
						_callee: callee_addr.0.into(),
						_data: case.callee_input.clone().into(),
						_gasDivisor: case.gas_divisor,
						_callType: call_type,
					}
					.abi_encode(),
				)
				.exec_config(config)
				.storage_deposit_limit(case.deposit_limit)
				.build();

			let result = output.result.map(|result| {
				Caller::callPartialGasCall::abi_decode_returns(&result.data).unwrap()
			});
			assert_eq!(case.result, result);
		});
	}
}

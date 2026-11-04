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
	BalanceOf, Code, Config, DelegateInfo, DispatchError, Error, ExecConfig, ExecOrigin,
	ExecReturnValue, Weight,
	address::AddressMapper,
	evm::{decode_revert_reason, fees::InfoT},
	metering::TransactionLimits,
	test_utils::{ALICE, ALICE_ADDR, BOB_ADDR, WEIGHT_LIMIT, builder::Contract, deposit_limit},
	tests::{ExtBuilder, MOCK_CODE, MockHandlerImpl, RuntimeOrigin, Test, builder},
};
use alloy_core::{
	primitives::{Bytes, FixedBytes},
	sol_types::{Revert, SolCall, SolError, SolInterface},
};
use frame_support::{
	assert_err,
	traits::fungible::{Balanced, Inspect, Mutate},
};
use itertools::Itertools;
use pallet_revive_fixtures::{Callee, Caller, FixtureType, Host, compile_module_with_type};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;
use sp_core::{H160, H256};
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
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_invalid_opcode(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Pass a large gas stipend to the callee
		let gas_limit = 200_000_000_000u64;

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		// Instantiate the caller contract.
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let contract_result = builder::bare_call(caller_addr)
			.data(
				Caller::normalCall {
					_callee: callee_addr.0.into(),
					_value: 0,
					_data: Callee::invalidCall {}.abi_encode().into(),
					_gas: gas_limit,
				}
				.abi_encode(),
			)
			.build();

		let result = contract_result.result.expect("Outer call should succeed");
		assert!(
			contract_result.gas_consumed > gas_limit as u128,
			"Inner call should consume all forwarded gas. Consumed: {}, Limit: {}",
			contract_result.gas_consumed,
			gas_limit
		);
		let decoded = Caller::normalCall::abi_decode_returns(&result.data)
			.expect("Should decode return data");
		assert!(!decoded.success, "INVALID opcode should cause inner call to fail");
		assert!(decoded.output.is_empty(), "Output should be empty on INVALID opcode");
	});
}

#[test]
fn invalid_opcode_evm() {
	let (callee_code, _) = compile_module_with_type("Callee", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract.
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

		// Instantiate the callee contract.
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
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_caller: Some(BOB_ADDR),
					..Default::default()
				})),
				..Default::default()
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
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_call: iter::once((
						callee_addr,
						ExecReturnValue {
							flags: ReturnFlags::default(),
							data: alloy_core::sol_types::SolValue::abi_encode(&mocked_magic_number)
								.into(),
						},
					))
					.collect(),
					..Default::default()
				})),
				..Default::default()
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
				mock_handler: Some(Box::new(MockHandlerImpl {
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
					..Default::default()
				})),
				..Default::default()
			})
			.build_and_unwrap_result();

		let result = Caller::normalCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "the call must succeed");
		let echo_output = Callee::echoCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(magic_number, echo_output, "the call must reproduce the magic number");
	});
}

#[test]
fn mocked_code_works() {
	let (host_code, _) = compile_module_with_type("Host", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

		let mocked_addr = H160::from_slice(&[0x42; 20]);

		let expected_size = MOCK_CODE.len() as u64;
		let expected_hash = sp_io::hashing::keccak_256(&MOCK_CODE);

		// Test EXTCODESIZE with mocked address
		let result = builder::bare_call(host_addr)
			.data(Host::extcodesizeOpCall { account: mocked_addr.0.into() }.abi_encode())
			.exec_config(ExecConfig {
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_call: iter::once((mocked_addr, ExecReturnValue::default())).collect(),
					..Default::default()
				})),
				..Default::default()
			})
			.build_and_unwrap_result();

		let size = Host::extcodesizeOpCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(
			size, expected_size,
			"EXTCODESIZE should return {} for mocked address",
			expected_size
		);

		// Test EXTCODEHASH with mocked address
		let result = builder::bare_call(host_addr)
			.data(Host::extcodehashOpCall { account: mocked_addr.0.into() }.abi_encode())
			.exec_config(ExecConfig {
				mock_handler: Some(Box::new(MockHandlerImpl {
					mock_call: iter::once((mocked_addr, ExecReturnValue::default())).collect(),
					..Default::default()
				})),
				..Default::default()
			})
			.build_and_unwrap_result();

		let hash = Host::extcodehashOpCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(
			H256::from_slice(hash.as_slice()),
			H256::from_slice(&expected_hash),
			"EXTCODEHASH should return keccak256(MOCK_CODE) for mocked address"
		);

		// Verify that without mock handler, the same address returns 0 for code size
		let result = builder::bare_call(host_addr)
			.data(Host::extcodesizeOpCall { account: mocked_addr.0.into() }.abi_encode())
			.build_and_unwrap_result();

		let size = Host::extcodesizeOpCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(size, 0, "EXTCODESIZE should return 0 for unmocked address without code");
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
		let echo_result = builder::bare_call(callee_addr.0.0.into())
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

		let callee_addr: H160 = callee_addr.0.0.into();
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

/// Root creates a contract via nested CREATE in block N and destroys it via the
/// system precompile in block N+1. Exercises the full `do_terminate` path under
/// Root and confirms the deposit waiver holds across both calls.
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn root_call_can_create_and_destroy_in_next_block(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	use crate::{
		AccountInfo, HoldReason, Pallet,
		address::AddressMapper,
		test_utils::DJANGO_ADDR,
		tests::{System, initialize_block, test_utils::get_balance_on_hold},
	};
	use alloy_core::primitives::Address;
	use pallet_revive_fixtures::{
		NestedChild::{NestedChildCalls, destroyViaPrecompileCall},
		NestedDeployer::{NestedDeployerCalls, deployChildCall},
	};

	let (code, _) = compile_module_with_type("NestedDeployer", caller_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);

		if caller_type == FixtureType::Resolc {
			let (child_code, _) = compile_module_with_type("NestedChild", callee_type).unwrap();
			Pallet::<Test>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				child_code,
				<BalanceOf<Test>>::MAX,
			)
			.unwrap();
		}

		let Contract { addr, account_id } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let _ = <Test as Config>::Currency::set_balance(&account_id, 100_000_000_000_000);

		// Snapshot balances/holds the two Root calls must not touch.
		let storage_hold = HoldReason::StorageDepositReserve.into();
		let upload_hold = HoldReason::CodeUploadDepositReserve.into();
		let pallet_account = Pallet::<Test>::account_id();
		let deployer_storage_hold_before = get_balance_on_hold(&storage_hold, &account_id);
		let deployer_free_before = <<Test as Config>::Currency as Inspect<_>>::balance(&account_id);
		let pallet_upload_hold_before = get_balance_on_hold(&upload_hold, &pallet_account);

		// Block 1: Root creates the child via nested CREATE.
		let create_result = builder::bare_call(addr)
			.origin(RuntimeOrigin::root())
			.data(NestedDeployerCalls::deployChild(deployChildCall {}).abi_encode())
			.build_and_unwrap_result();
		assert!(!create_result.did_revert());
		let returned: Address = deployChildCall::abi_decode_returns(&create_result.data).unwrap();
		let child_addr = H160::from_slice(returned.as_slice());
		assert!(AccountInfo::<Test>::load_contract(&child_addr).is_some());

		// Deposits stayed waived across the Root create.
		let child_id = <Test as crate::Config>::AddressMapper::to_account_id(&child_addr);
		assert_eq!(get_balance_on_hold(&storage_hold, &child_id), 0);
		assert_eq!(get_balance_on_hold(&storage_hold, &account_id), deployer_storage_hold_before);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&account_id),
			deployer_free_before,
		);

		// Block 2: Root tells the child to self-terminate via the system precompile.
		initialize_block(System::block_number() + 1);
		let destroy_result = builder::bare_call(child_addr)
			.origin(RuntimeOrigin::root())
			.data(
				NestedChildCalls::destroyViaPrecompile(destroyViaPrecompileCall {
					beneficiary: DJANGO_ADDR.0.into(),
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!destroy_result.did_revert(), "Root cross-tx terminate should succeed");

		assert!(
			AccountInfo::<Test>::load_contract(&child_addr).is_none(),
			"child must be destroyed by the cross-block terminate call",
		);

		// Deposits stayed waived across both Root calls.
		assert_eq!(get_balance_on_hold(&storage_hold, &child_id), 0);
		assert_eq!(get_balance_on_hold(&storage_hold, &account_id), deployer_storage_hold_before);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&account_id),
			deployer_free_before,
		);
		if caller_type == FixtureType::Solc {
			assert_eq!(
				get_balance_on_hold(&upload_hold, &pallet_account),
				pallet_upload_hold_before
			);
		}
	});
}

/// Sibling of the cross-block test, but using the Solidity `selfdestruct` opcode
/// (`only_if_same_tx: true`). To actually reach `do_terminate` past the EIP-6780
/// gate at [exec.rs] `contracts_to_destroy`, creation and destruction must
/// happen in the same tx — covers the `only_if_same_tx: true` branch.
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn root_call_can_create_and_destroy_in_same_tx(caller_type: FixtureType, callee_type: FixtureType) {
	use crate::{
		AccountInfo, HoldReason, Pallet, address::AddressMapper, test_utils::DJANGO_ADDR,
		tests::test_utils::get_balance_on_hold,
	};
	use alloy_core::primitives::Address;
	use pallet_revive_fixtures::NestedDeployer::{NestedDeployerCalls, deployAndDestroyChildCall};

	let (code, _) = compile_module_with_type("NestedDeployer", caller_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);

		if caller_type == FixtureType::Resolc {
			let (child_code, _) = compile_module_with_type("NestedChild", callee_type).unwrap();
			Pallet::<Test>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				child_code,
				<BalanceOf<Test>>::MAX,
			)
			.unwrap();
		}

		let Contract { addr, account_id } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let _ = <Test as Config>::Currency::set_balance(&account_id, 100_000_000_000_000);

		let storage_hold = HoldReason::StorageDepositReserve.into();
		let upload_hold = HoldReason::CodeUploadDepositReserve.into();
		let pallet_account = Pallet::<Test>::account_id();
		let deployer_storage_hold_before = get_balance_on_hold(&storage_hold, &account_id);
		let deployer_free_before = <<Test as Config>::Currency as Inspect<_>>::balance(&account_id);
		let pallet_upload_hold_before = get_balance_on_hold(&upload_hold, &pallet_account);

		let result = builder::bare_call(addr)
			.origin(RuntimeOrigin::root())
			.data(
				NestedDeployerCalls::deployAndDestroyChild(deployAndDestroyChildCall {
					beneficiary: DJANGO_ADDR.0.into(),
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "Root nested CREATE + SELFDESTRUCT should succeed");

		let returned: Address =
			deployAndDestroyChildCall::abi_decode_returns(&result.data).unwrap();
		let child_addr = H160::from_slice(returned.as_slice());

		// EIP-6780: created-and-destroyed in the same tx must actually remove the
		// contract. Before the do_terminate fix this silently failed under Root and
		// the ContractInfo stayed put.
		assert!(
			AccountInfo::<Test>::load_contract(&child_addr).is_none(),
			"child contract must have been terminated, not silently left on-chain",
		);

		let child_id = <Test as crate::Config>::AddressMapper::to_account_id(&child_addr);
		assert_eq!(get_balance_on_hold(&storage_hold, &child_id), 0);
		assert_eq!(get_balance_on_hold(&storage_hold, &account_id), deployer_storage_hold_before);
		assert_eq!(
			<<Test as Config>::Currency as Inspect<_>>::balance(&account_id),
			deployer_free_before,
		);
		if caller_type == FixtureType::Solc {
			assert_eq!(
				get_balance_on_hold(&upload_hold, &pallet_account),
				pallet_upload_hold_before
			);
		}
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn subcall_effectively_limited_substrate_tx(caller_type: FixtureType, callee_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	let no_collection_config = ExecConfig::new_substrate_tx();
	let mut collection_config = ExecConfig::new_substrate_tx();
	collection_config.collect_deposit_from_hold = Some(Default::default());
	let configs = [no_collection_config, collection_config];

	let call_types =
		[Caller::CallType::Call, Caller::CallType::StaticCall, Caller::CallType::DelegateCall];

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
			// Callee reverts (out of gas), but caller has 1/64 of full gas for cleanup
			result: Ok(false),
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
		// A new storage slot costs 66 here: 2 for the item plus 1 per byte of its 32-byte key and
		// 32-byte value. Under the 63/64 rule the callee may use only 65 of the 67, too little for
		// its write, while the caller's own write fits.
		Case {
			deposit_limit: 67,
			gas_divisor: 1,
			callee_input: Callee::storeCall { _data: 42 }.abi_encode(),
			result: Ok(false),
			is_store_call: true,
		},
	];

	for ((case, config), call_type) in
		test_cases.iter().cartesian_product(&configs).cartesian_product(call_types)
	{
		// the storage stuff won't work on static or delegate call
		if case.is_store_call && !matches!(call_type, Caller::CallType::Call) {
			continue;
		}

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let fees = <Test as Config>::FeeInfo::tx_fee_from_weight(0, &WEIGHT_LIMIT) +
				case.deposit_limit;
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
				.exec_config(config.clone())
				.transaction_limits(TransactionLimits::WeightAndDeposit {
					weight_limit: Weight::from_parts(50_000_000_000, 10 * 1024 * 1024),
					deposit_limit: case.deposit_limit,
				})
				.build();

			let result = output.result.map(|result| {
				Caller::callPartialGasCall::abi_decode_returns(&result.data).unwrap()
			});
			assert_eq!(case.result, result);
		});
	}
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
fn delegatecall_with_large_deposit_limit_succeeds(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Use a very large deposit limit to trigger the bug scenario
		let large_deposit_limit: u128 = u64::MAX as _;

		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: 42 }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.transaction_limits(TransactionLimits::WeightAndDeposit {
				weight_limit: WEIGHT_LIMIT,
				deposit_limit: large_deposit_limit,
			})
			.build();

		// The call must succeed - before the fix, this would fail with OutOfGas
		let exec_result = result.result.expect("call must not fail");
		let decoded = Caller::delegateCall::abi_decode_returns(&exec_result.data).unwrap();
		assert!(decoded.success, "delegatecall must succeed");

		let echo_result = Callee::echoCall::abi_decode_returns(&decoded.output).unwrap();
		assert_eq!(echo_result, 42, "echo must return the magic number");
	});
}

/// Solidity patterns that rely on the EIP-150 63/64 rule, in Ethereum and Substrate mode.
mod eip_150 {
	use super::*;
	use crate::{
		ContractResult, EthTxInfo, Pallet,
		evm::{CallTracer, GenericTransaction},
		test_utils::ETH_GAS_LIMIT,
		tests::{DepositPerByte, test_utils::dispatch_eth_transaction},
		tracing::trace,
	};
	use frame_support::{
		assert_ok,
		storage::{TransactionOutcome, with_transaction},
	};
	use pallet_revive_fixtures::{
		CatchInvalid, Forwarder, GasLeftReceiver, Multicall3, ReentryStorage,
	};
	use pretty_assertions::assert_eq;
	use test_case::test_matrix;

	/// Returns the limits of an Ethereum call with `eth_gas_limit` and `weight_limit`.
	fn ethereum_limits(eth_gas_limit: u64, weight_limit: Weight) -> TransactionLimits<Test> {
		TransactionLimits::EthereumGas {
			eth_gas_limit: eth_gas_limit.into(),
			weight_limit,
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		}
	}

	/// Returns the limits of a Substrate call with `weight_limit` and the default deposit limit.
	fn substrate_limits(weight_limit: Weight) -> TransactionLimits<Test> {
		TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit: deposit_limit::<Test>() }
	}

	/// Funds `ALICE` and deploys the provided contracts, returning their addresses.
	fn deploy<const N: usize>(codes: [Vec<u8>; N]) -> [H160; N] {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		codes.map(|code| {
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract().addr
		})
	}

	/// Runs `operation` in a storage layer that is rolled back afterwards, as a dry run through
	/// the runtime API is.
	fn without_commit<R>(operation: impl FnOnce() -> R) -> R {
		with_transaction(|| TransactionOutcome::Rollback(Ok::<_, DispatchError>(operation())))
			.unwrap()
	}

	/// Runs `test` with a `parameter_types!` static set to `value`, and restores the previous
	/// value when `test` ends, whether it returns or panics.
	///
	/// `get` and `set` are the accessors `parameter_types!` generates for the static, for example
	/// `DepositPerByte::get` and `DepositPerByte::set`.
	fn with_parameter<T: Clone, R>(
		get: fn() -> T,
		set: fn(T),
		value: T,
		test: impl FnOnce() -> R,
	) -> R {
		struct RestoreParameter<T: Clone> {
			set: fn(T),
			previous: T,
		}
		impl<T: Clone> Drop for RestoreParameter<T> {
			fn drop(&mut self) {
				(self.set)(self.previous.clone());
			}
		}
		let _restore = RestoreParameter { set, previous: get() };
		set(value);
		test()
	}

	/// Runs `input` on `contract` in an Ethereum transaction with the gas that a dry run without a
	/// gas limit returned, and returns the output.
	fn eth_transact_with_estimated_gas(contract: H160, input: Vec<u8>) -> Vec<u8> {
		let tx = GenericTransaction {
			from: Some(ALICE_ADDR),
			to: Some(contract),
			input: input.into(),
			..Default::default()
		};
		let dry_run =
			without_commit(|| Pallet::<Test>::dry_run_eth_transact(tx.clone(), None, true, None))
				.unwrap();
		let tx = GenericTransaction { gas: Some(dry_run.eth_gas), ..tx };
		let mut tracer = CallTracer::default();
		assert_ok!(trace(&mut tracer, || dispatch_eth_transaction(tx)));
		tracer.collect_trace().unwrap().output.0
	}

	/// Returns an `aggregate3` call that stores `data` in `callee`.
	fn store_call3(callee: H160, data: u64) -> Multicall3::Call3 {
		Multicall3::Call3 {
			target: callee.0.into(),
			allowFailure: true,
			callData: Callee::storeCall { _data: data }.abi_encode().into(),
		}
	}

	/// Asserts that every call of an `aggregate3` run succeeded, naming the first that did not.
	fn assert_aggregate3_calls_succeeded(output: &[u8]) {
		let results = Multicall3::aggregate3Call::abi_decode_returns(output).unwrap();
		for (index, result) in results.iter().enumerate() {
			assert!(result.success, "call {index} failed, returning {:?}", result.returnData);
		}
	}

	/// Runs `input` on `contract` with exactly the limits a dry run returned, in the mode of
	/// `limits`, and returns the output.
	fn call_with_estimated_limits(
		contract: H160,
		input: Vec<u8>,
		limits: &TransactionLimits<Test>,
	) -> Vec<u8> {
		match limits {
			TransactionLimits::EthereumGas { .. } => {
				eth_transact_with_estimated_gas(contract, input)
			},
			TransactionLimits::WeightAndDeposit { .. } => {
				let dry_run = without_commit(|| {
					builder::bare_call(contract)
						.data(input.clone())
						.exec_config(ExecConfig::new_substrate_tx().with_dry_run(None))
						.build()
				});
				let output = builder::bare_call(contract)
					.data(input)
					.transaction_limits(TransactionLimits::WeightAndDeposit {
						weight_limit: dry_run.weight_required,
						deposit_limit: dry_run.max_storage_deposit.charge_or_zero(),
					})
					.build_and_unwrap_result();
				assert!(!output.did_revert(), "the call reverted with {:?}", output.data);
				output.data
			},
		}
	}

	/// Asserts that `result` used at least 63/64 of its weight limit or, in Ethereum mode, of its
	/// gas limit.
	fn assert_callee_used_up_its_gas(
		result: &ContractResult<ExecReturnValue, BalanceOf<Test>>,
		limits: &TransactionLimits<Test>,
	) {
		let used_up = |consumed: u128, limit: u128| consumed * 64 >= limit * 63;
		let used_up_ref_time = |weight_limit: &Weight| {
			used_up(result.weight_consumed.ref_time().into(), weight_limit.ref_time().into())
		};
		let used_up_its_gas = match limits {
			TransactionLimits::EthereumGas { eth_gas_limit, weight_limit, .. } => {
				used_up(result.gas_consumed, *eth_gas_limit) || used_up_ref_time(weight_limit)
			},
			TransactionLimits::WeightAndDeposit { weight_limit, .. } => {
				used_up_ref_time(weight_limit)
			},
		};
		assert!(
			used_up_its_gas,
			"the call used {} gas and weight {:?}, less than 63/64 of {limits:?}",
			result.gas_consumed, result.weight_consumed
		);
	}

	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn caller_writes_storage_in_catch_after_callee_hits_invalid(
		caller_type: FixtureType,
		callee_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (caller_code, _) = compile_module_with_type("CatchInvalid", caller_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, caller_addr] = deploy([callee_code, caller_code]);

			let result = builder::bare_call(caller_addr)
				.data(CatchInvalid::tryInvalidCall { callee: callee_addr.0.into() }.abi_encode())
				.transaction_limits(limits.clone())
				.build();
			assert_ok!(&result.result);
			assert_callee_used_up_its_gas(&result, &limits);

			let caught = builder::bare_call(caller_addr)
				.data(CatchInvalid::caughtCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(
				CatchInvalid::caughtCall::abi_decode_returns(&caught.data).unwrap(),
				"the catch block must write to storage"
			);
		});
	}

	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn caller_writes_storage_in_catch_after_constructor_hits_invalid(
		fixture_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (creator_code, _) = compile_module_with_type("CatchInvalid", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [creator_addr] = deploy([creator_code]);
			if fixture_type == FixtureType::Resolc {
				let (child_code, _) =
					compile_module_with_type("InvalidConstructor", fixture_type).unwrap();
				assert_ok!(Pallet::<Test>::upload_code(
					RuntimeOrigin::signed(ALICE),
					child_code,
					BalanceOf::<Test>::MAX,
				));
			}

			let result = builder::bare_call(creator_addr)
				.data(CatchInvalid::tryCreateInvalidCall {}.abi_encode())
				.transaction_limits(limits.clone())
				.build();
			assert_ok!(&result.result);
			assert_callee_used_up_its_gas(&result, &limits);

			let caught = builder::bare_call(creator_addr)
				.data(CatchInvalid::caughtCall {}.abi_encode())
				.build_and_unwrap_result();
			assert!(
				CatchInvalid::caughtCall::abi_decode_returns(&caught.data).unwrap(),
				"the catch block must write to storage"
			);
		});
	}

	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc],
		// A tenth of the default gas or weight limit, to shorten the test execution time spent in
		// the callee's endless loop. With `u64::MAX` gas, the weight limit runs out first, as in
		// a dry run of a transaction that sets no gas limit.
		[
			ethereum_limits(ETH_GAS_LIMIT / 10, WEIGHT_LIMIT),
			ethereum_limits(u64::MAX, WEIGHT_LIMIT / 10),
			substrate_limits(WEIGHT_LIMIT / 10)
		]
	)]
	fn aggregate3_continues_after_call_drains_its_gas(
		caller_type: FixtureType,
		callee_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (multicall_code, _) = compile_module_with_type("Multicall3", caller_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, multicall_addr] = deploy([callee_code, multicall_code]);

			let calls = vec![
				Multicall3::Call3 {
					target: callee_addr.0.into(),
					allowFailure: true,
					callData: Callee::consumeAllReftimeCall {}.abi_encode().into(),
				},
				Multicall3::Call3 {
					target: callee_addr.0.into(),
					allowFailure: false,
					callData: Callee::echoCall { _data: 42 }.abi_encode().into(),
				},
			];
			let result = builder::bare_call(multicall_addr)
				.data(Multicall3::aggregate3Call { calls }.abi_encode())
				.transaction_limits(limits.clone())
				.build();
			let output = result.result.as_ref().unwrap();
			assert!(!output.did_revert());
			assert_callee_used_up_its_gas(&result, &limits);

			let results = Multicall3::aggregate3Call::abi_decode_returns(&output.data).unwrap();
			assert!(!results[0].success);
			assert_eq!(Callee::echoCall::abi_decode_returns(&results[1].returnData).unwrap(), 42);
		});
	}

	/// A nested call succeeds when its caller runs with exactly the limits a dry run returned.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn dry_run_estimates_suffice_for_transaction_with_nested_call(
		caller_type: FixtureType,
		callee_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, caller_addr] = deploy([callee_code, caller_code]);
			let input = Caller::normalCall {
				_callee: callee_addr.0.into(),
				_value: 0,
				_data: Callee::storeCall { _data: 7 }.abi_encode().into(),
				_gas: u64::MAX,
			}
			.abi_encode();

			let output = call_with_estimated_limits(caller_addr, input, &limits);
			assert!(Caller::normalCall::abi_decode_returns(&output).unwrap().success);
			let stored = builder::bare_call(callee_addr)
				.data(Callee::storedCall {}.abi_encode())
				.build_and_unwrap_result();
			assert_eq!(Callee::storedCall::abi_decode_returns(&stored.data).unwrap(), 7);
		});
	}

	/// A value transfer without gas gets only the stipend, and succeeds when its caller runs with
	/// exactly the limits a dry run returned.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn dry_run_estimates_suffice_for_transaction_with_value_transfer(
		caller_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
		// An EVM receiver, so the test does not depend on how much PVM code fits in the stipend.
		let (receiver_code, _) =
			compile_module_with_type("DoNothingReceiver", FixtureType::Solc).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [receiver_addr, caller_addr] = deploy([receiver_code, caller_code]);
			let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);
			let _ = <Test as Config>::Currency::set_balance(&caller_account, 1_000_000_000);
			let value = 1;
			let input = Caller::normalCall {
				_callee: receiver_addr.0.into(),
				_value: value,
				_data: Default::default(),
				_gas: 0,
			}
			.abi_encode();

			let receiver_balance = Pallet::<Test>::evm_balance(&receiver_addr);
			let output = call_with_estimated_limits(caller_addr, input, &limits);
			assert!(Caller::normalCall::abi_decode_returns(&output).unwrap().success);
			assert_eq!(Pallet::<Test>::evm_balance(&receiver_addr), receiver_balance + value);
		});
	}

	/// A value call without a gas limit of its own gets the stipend, as one with the highest
	/// explicit gas limit does.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	#[ignore = "a PVM value call with gas u64::MAX gets no stipend"]
	fn value_call_without_gas_limit_gets_the_stipend(
		caller_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
		// The caller's backend decides the stipend; the receiver only reports the gas it gets.
		let (receiver_code, _) =
			compile_module_with_type("GasLeftReceiver", FixtureType::Solc).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [receiver_addr, caller_addr] = deploy([receiver_code, caller_code]);
			let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);
			let _ = <Test as Config>::Currency::set_balance(&caller_account, 1_000_000_000);
			let receiver_gas_left = |gas: u64| {
				let output = builder::bare_call(caller_addr)
					.data(
						Caller::normalCall {
							_callee: receiver_addr.0.into(),
							_value: 1,
							_data: GasLeftReceiver::gasLeftCall {}.abi_encode().into(),
							_gas: gas,
						}
						.abi_encode(),
					)
					.transaction_limits(limits.clone())
					.build_and_unwrap_result();
				let call = Caller::normalCall::abi_decode_returns(&output.data).unwrap();
				assert!(call.success, "the call with gas {gas} failed");
				GasLeftReceiver::gasLeftCall::abi_decode_returns(&call.output).unwrap()
			};

			// Both gas limits exceed the 63/64 share, so each call gets the share plus the stipend.
			assert_eq!(receiver_gas_left(u64::MAX), receiver_gas_left(u64::MAX - 1));
		});
	}

	/// A call that writes storage after an earlier call freed the same amount succeeds when its
	/// caller runs with exactly the limits a dry run returned.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn dry_run_estimates_suffice_for_transaction_writing_after_refund(
		caller_type: FixtureType,
		callee_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (multicall_code, _) = compile_module_with_type("Multicall3", caller_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, multicall_addr] = deploy([callee_code, multicall_code]);
			builder::bare_call(callee_addr)
				.data(Callee::storeCall { _data: 7 }.abi_encode())
				.build_and_unwrap_result();

			let calls = vec![store_call3(callee_addr, 0), store_call3(callee_addr, 7)];
			let output = call_with_estimated_limits(
				multicall_addr,
				Multicall3::aggregate3Call { calls }.abi_encode(),
				&limits,
			);
			assert_aggregate3_calls_succeeded(&output);
		});
	}

	/// A callee's limit is fixed when it is called, so a refund recorded for its caller while it
	/// runs must not lower the estimates a dry run returns.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc],
		[ethereum_limits(ETH_GAS_LIMIT, WEIGHT_LIMIT), substrate_limits(WEIGHT_LIMIT)]
	)]
	fn dry_run_estimates_suffice_for_transaction_with_reentrant_refund(
		storage_type: FixtureType,
		proxy_type: FixtureType,
		limits: TransactionLimits<Test>,
	) {
		let (storage_code, _) = compile_module_with_type("ReentryStorage", storage_type).unwrap();
		let (proxy_code, _) = compile_module_with_type("ReentryProxy", proxy_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [storage_addr, proxy_addr] = deploy([storage_code, proxy_code]);
			// In Ethereum mode, a transaction's gas pays for both its weight and its storage
			// deposits. With the default `DepositPerByte`, the missing deposit is too small to run
			// the transaction out of gas. The value set here is still below that of Asset Hub on
			// Polkadot and Kusama, and far below it relative to weight fees.
			with_parameter(DepositPerByte::get, DepositPerByte::set, 70_000, || {
				builder::bare_call(storage_addr)
					.data(ReentryStorage::writeS0Call {}.abi_encode())
					.build_and_unwrap_result();

				// Writing `s1` first leaves the storage contract with a charge of its own, so that
				// without the fix the refund lowers the Substrate estimate too.
				call_with_estimated_limits(
					storage_addr,
					ReentryStorage::writeFreeAndRewriteViaCall { proxy: proxy_addr.0.into() }
						.abi_encode(),
					&limits,
				);
			});
		});
	}

	/// A refund from an earlier call pays for a later call's storage, so the dry run only adds the
	/// part the caller keeps for that call.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc]
	)]
	fn earlier_deposit_refund_pays_for_later_charges_in_dry_run(
		caller_type: FixtureType,
		callee_type: FixtureType,
	) {
		let (multicall_code, _) = compile_module_with_type("Multicall3", caller_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, multicall_addr] = deploy([callee_code, multicall_code]);
			let dry_run = |calls| {
				let tx = GenericTransaction {
					from: Some(ALICE_ADDR),
					to: Some(multicall_addr),
					input: Multicall3::aggregate3Call { calls }.abi_encode().into(),
					..Default::default()
				};
				without_commit(|| Pallet::<Test>::dry_run_eth_transact(tx, None, true, None))
					.unwrap()
			};

			let without_refund = dry_run(vec![store_call3(callee_addr, 7)]);
			assert_aggregate3_calls_succeeded(&without_refund.data);

			// Store a value in the slot, so the next dry run gets a refund for freeing it.
			builder::bare_call(callee_addr)
				.data(Callee::storeCall { _data: 7 }.abi_encode())
				.build_and_unwrap_result();

			let with_refund =
				dry_run(vec![store_call3(callee_addr, 0), store_call3(callee_addr, 7)]);
			assert_aggregate3_calls_succeeded(&with_refund.data);

			assert_eq!(
				with_refund.max_storage_deposit,
				without_refund.max_storage_deposit - without_refund.storage_deposit,
				"the refund from freeing the slot must pay for writing it again",
			);
		});
	}

	/// A forwarder that checks `gasleft()` after the call, like OpenZeppelin's `ERC2771Forwarder`,
	/// accepts a transaction that leaves its callee the requested gas and rejects one that does
	/// not.
	#[test_matrix(
		[FixtureType::Solc, FixtureType::Resolc],
		[FixtureType::Solc, FixtureType::Resolc]
	)]
	fn forwarder_detects_callee_getting_less_than_requested_gas(
		forwarder_type: FixtureType,
		callee_type: FixtureType,
	) {
		let (forwarder_code, _) = compile_module_with_type("Forwarder", forwarder_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let [callee_addr, forwarder_addr] = deploy([callee_code, forwarder_code]);
			// A tenth of the default gas, to shorten the time spent in the callee's endless loop.
			let requested_gas = ETH_GAS_LIMIT / 10;
			let forward = |eth_gas_limit| {
				builder::bare_call(forwarder_addr)
					.data(
						Forwarder::forwardCall {
							target: callee_addr.0.into(),
							data: Callee::consumeAllReftimeCall {}.abi_encode().into(),
							requestedGas: requested_gas,
						}
						.abi_encode(),
					)
					.transaction_limits(ethereum_limits(eth_gas_limit, WEIGHT_LIMIT))
					.build()
					.result
			};

			// Enough gas for the callee to get the requested gas.
			let output = forward(2 * requested_gas).unwrap();
			assert!(!output.did_revert());
			assert!(
				!Forwarder::forwardCall::abi_decode_returns(&output.data).unwrap(),
				"the forwarded call must run out of gas"
			);

			// Too little for that, so the forwarder hits INVALID.
			let rejected = forward(requested_gas);
			if forwarder_type == FixtureType::Solc {
				assert_err!(rejected, Error::<Test>::InvalidInstruction);
			} else {
				// resolc compiles INVALID to a revert that uses up all gas.
				assert!(rejected.unwrap().did_revert(), "the forwarder must revert");
			}
		});
	}
}

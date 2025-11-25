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
	assert_refcount,
	call_builder::VmBinaryModule,
	debug::DebugSettings,
	evm::{PrestateTrace, PrestateTracer, PrestateTracerConfig},
	test_utils::{builder::Contract, ALICE, ALICE_ADDR, BOB},
	tests::{
		builder,
		test_utils::{contract_base_deposit, ensure_stored, get_contract},
		AllowEvmBytecode, DebugFlag, ExtBuilder, RuntimeOrigin, Test,
	},
	tracing::trace,
	BalanceOf, Code, Config, Error, EthBlockBuilderFirstValues, GenesisConfig, Origin, Pallet,
	PristineCode,
};
use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::{
	assert_err, assert_noop, assert_ok, dispatch::GetDispatchInfo, traits::fungible::Mutate,
};
use pallet_revive_fixtures::{compile_module_with_type, Fibonacci, FixtureType, NestedCounter};
use pretty_assertions::assert_eq;
use sp_runtime::Weight;
use test_case::test_case;

use revm::bytecode::opcode::*;

mod arithmetic;
mod bitwise;
mod block_info;
mod contract;
mod control;
mod host;
mod memory;
mod stack;
mod system;
mod tx_info;

fn make_initcode_from_runtime_code(runtime_code: &Vec<u8>) -> Vec<u8> {
	let runtime_code_len = runtime_code.len();
	assert!(runtime_code_len < 256, "runtime code length must be less than 256 bytes");
	let mut init_code: Vec<u8> = vec![
		vec![PUSH1, 0x80_u8],
		vec![PUSH1, 0x40_u8],
		vec![MSTORE],
		vec![PUSH1, 0x40_u8],
		vec![MLOAD],
		vec![PUSH1, runtime_code_len as u8],
		vec![PUSH1, 0x13_u8],
		vec![DUP3],
		vec![CODECOPY],
		vec![PUSH1, runtime_code_len as u8],
		vec![SWAP1],
		vec![RETURN],
		vec![INVALID],
	]
	.into_iter()
	.flatten()
	.collect();
	init_code.extend(runtime_code);
	init_code
}

#[test]
fn basic_evm_flow_works() {
	let (code, init_hash) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		for i in 1u8..=2 {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
				.salt(Some([i; 32]))
				.build_and_unwrap_contract();

			// check the code exists
			let contract = get_contract(&addr);
			ensure_stored(contract.code_hash);
			let deposit = contract_base_deposit(&addr);
			assert_eq!(contract.total_deposit(), deposit);
			assert_refcount!(contract.code_hash, i as u64);

			let result = builder::bare_call(addr)
				.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
				.build_and_unwrap_result();
			let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
			assert_eq!(55u64, decoded);
		}

		// init code is not stored
		assert!(!PristineCode::<Test>::contains_key(init_hash));
	});
}

#[test]
fn basic_evm_flow_tracing_works() {
	use crate::{
		evm::{CallTrace, CallTracer, CallType},
		tracing::trace,
	};
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let mut tracer = CallTracer::new(Default::default());
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = trace(&mut tracer, || {
			builder::bare_instantiate(Code::Upload(code.clone()))
				.salt(None)
				.build_and_unwrap_contract()
		});

		let contract = get_contract(&addr);
		let runtime_code = PristineCode::<Test>::get(contract.code_hash).unwrap();

		let call_trace = tracer.collect_trace().unwrap();
		assert_eq!(
			call_trace,
			CallTrace {
				from: ALICE_ADDR,
				call_type: CallType::Create,
				to: addr,
				input: code.into(),
				output: runtime_code.into(),
				value: Some(crate::U256::zero()),
				gas: call_trace.gas,
				gas_used: call_trace.gas_used,
				..Default::default()
			}
		);

		let mut call_tracer = CallTracer::new(Default::default());
		let result = trace(&mut call_tracer, || {
			builder::bare_call(addr)
				.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
				.build_and_unwrap_result()
		});

		let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(55u64, decoded);

		let call_trace = call_tracer.collect_trace().unwrap();
		assert_eq!(
			call_trace,
			CallTrace {
				call_type: CallType::Call,
				from: ALICE_ADDR,
				to: addr,
				input: Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 })
					.abi_encode()
					.into(),
				output: result.data.into(),
				value: Some(crate::U256::zero()),
				gas: call_trace.gas,
				gas_used: call_trace.gas_used,
				..Default::default()
			},
		);
	});
}

#[test]
fn eth_contract_too_large() {
	// Generate EVM bytecode that is one byte larger than the EIP-3860 limit.
	let contract_size = u32::try_from(revm::primitives::eip3860::MAX_INITCODE_SIZE + 1)
		.expect("usize value doesn't fit in u32");
	let code = VmBinaryModule::evm_sized(contract_size).code;

	for (allow_unlimited_contract_size, debug_flag) in
		[(true, false), (true, true), (false, false), (false, true)]
	{
		// Set the DebugEnabled flag to the desired value for this iteration of the test.
		DebugFlag::set(debug_flag);

		// Initialize genesis config with allow_unlimited_contract_size
		let genesis_config = GenesisConfig::<Test> {
			debug_settings: Some(DebugSettings::new(allow_unlimited_contract_size, false)),
			..Default::default()
		};

		ExtBuilder::default()
			.genesis_config(Some(genesis_config))
			.build()
			.execute_with(|| {
				let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

				let result = builder::bare_instantiate(Code::Upload(code.clone())).build();

				if allow_unlimited_contract_size && debug_flag {
					// The contract is too large, but the DebugEnabled flag is set and
					// allow_unlimited_contract_size is true.
					assert_ok!(result.result);
				} else {
					// The contract is too large and either the DebugEnabled flag is not set or
					// allow_unlimited_contract_size is false.
					assert_err!(result.result, <Error<Test>>::BlobTooLarge);
				}
			});
	}
}

#[test]
fn upload_evm_runtime_code_works() {
	use crate::{
		exec::Executable,
		primitives::ExecConfig,
		storage::{AccountInfo, ContractInfo},
		Pallet, TransactionMeter,
	};

	let (runtime_code, _runtime_hash) =
		compile_module_with_type("Fibonacci", FixtureType::SolcRuntime).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let deployer = ALICE;
		let deployer_addr = ALICE_ADDR;
		let _ = Pallet::<Test>::set_evm_balance(&deployer_addr, 1_000_000_000.into());

		let (uploaded_blob, _) = Pallet::<Test>::try_upload_code(
			deployer,
			runtime_code.clone(),
			crate::vm::BytecodeType::Evm,
			&mut TransactionMeter::new_from_limits(Weight::MAX, BalanceOf::<Test>::MAX).unwrap(),
			&ExecConfig::new_substrate_tx(),
		)
		.unwrap();

		let contract_address = crate::address::create1(&deployer_addr, 0u32.into());

		let contract_info =
			ContractInfo::<Test>::new(&contract_address, 0u32.into(), *uploaded_blob.code_hash())
				.unwrap();
		AccountInfo::<Test>::insert_contract(&contract_address, contract_info);

		// Call the contract and verify it works
		let result = builder::bare_call(contract_address)
			.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
			.build_and_unwrap_result();
		let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(55u64, decoded, "Contract should correctly compute fibonacci(10)");
	});
}

#[test]
fn upload_and_remove_code_works_for_evm() {
	let (code, code_hash) = compile_module_with_type("Dummy", FixtureType::SolcRuntime).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = Pallet::<Test>::set_evm_balance(&ALICE_ADDR, 5_000_000_000u64.into());

		// Ensure the code is not already stored.
		assert!(!PristineCode::<Test>::contains_key(&code_hash));

		// Upload the code.
		assert_ok!(Pallet::<Test>::upload_code(RuntimeOrigin::signed(ALICE), code, 1000u64));

		// Ensure the contract was stored.
		ensure_stored(code_hash);

		// Remove the code.
		assert_ok!(Pallet::<Test>::remove_code(RuntimeOrigin::signed(ALICE), code_hash));

		// Ensure the code is no longer stored.
		assert!(!PristineCode::<Test>::contains_key(&code_hash));
	});
}

#[test]
fn upload_fails_if_evm_bytecode_disabled() {
	let (code, _) = compile_module_with_type("Dummy", FixtureType::SolcRuntime).unwrap();

	AllowEvmBytecode::set(false); // Disable support for EVM bytecode.
	ExtBuilder::default().build().execute_with(|| {
		// Upload should fail since support for EVM bytecode is disabled.
		assert_err!(
			Pallet::<Test>::upload_code(RuntimeOrigin::signed(ALICE), code, 1000u64),
			<Error<Test>>::CodeRejected
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn dust_work_with_child_calls(fixture_type: FixtureType) {
	use pallet_revive_fixtures::CallSelfWithDust;
	let (code, _) = compile_module_with_type("CallSelfWithDust", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		let value = 1_000_000_000.into();
		builder::bare_call(addr)
			.data(
				CallSelfWithDust::CallSelfWithDustCalls::call(CallSelfWithDust::callCall {})
					.abi_encode(),
			)
			.evm_value(value)
			.build_and_unwrap_result();

		assert_eq!(crate::Pallet::<Test>::evm_balance(&addr), value);
	});
}

#[test]
fn prestate_diff_mode_tracing_works() {
	use alloy_core::hex;

	struct TestCase {
		config: PrestateTracerConfig,
		expected_instantiate_trace_json: &'static str,
		expected_call_trace_json: &'static str,
	}

	let (counter_code, _) = compile_module_with_type("NestedCounter", FixtureType::Solc).unwrap();
	let (contract_runtime_code, _) =
		compile_module_with_type("NestedCounter", FixtureType::SolcRuntime).unwrap();
	let (child_runtime_code, _) =
		compile_module_with_type("Counter", FixtureType::SolcRuntime).unwrap();

	let test_cases = [
		TestCase {
			config: PrestateTracerConfig {
				diff_mode: false,
				disable_storage: false,
				disable_code: false,
			},
			expected_instantiate_trace_json: r#"{
					"{{ALICE_ADDR}}": {
						"balance": "{{ALICE_BALANCE_PRE}}"
					}
				}"#,
			expected_call_trace_json: r#"{
					"{{ALICE_ADDR}}": {
						"balance": "{{ALICE_BALANCE_POST}}",
						"nonce": 1
					},
					"{{CONTRACT_ADDR}}": {
						"balance": "0x0",
						"nonce": 2,
						"code": "{{CONTRACT_CODE}}",
						"storage": {
							"0x0000000000000000000000000000000000000000000000000000000000000000": "{{CHILD_ADDR_PADDED}}",
							"0x0000000000000000000000000000000000000000000000000000000000000001": "0x0000000000000000000000000000000000000000000000000000000000000007"
						}
					},
					"{{CHILD_ADDR}}": {
						"balance": "0x0",
						"nonce": 1,
						"code": "{{CHILD_CODE}}",
						"storage": {
							"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
						}
					}
				}"#,
		},
		TestCase {
			config: PrestateTracerConfig {
				diff_mode: true,
				disable_storage: false,
				disable_code: false,
			},
			expected_instantiate_trace_json: r#"{
					"pre": {
						"{{ALICE_ADDR}}": {
							"balance": "{{ALICE_BALANCE_PRE}}"
						}
					},
					"post": {
						"{{ALICE_ADDR}}": {
							"balance": "{{ALICE_BALANCE_POST}}",
							"nonce": 1
						},
						"{{CONTRACT_ADDR}}": {
							"balance": "0x0",
							"nonce": 2,
							"code": "{{CONTRACT_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "{{CHILD_ADDR_PADDED}}",
								"0x0000000000000000000000000000000000000000000000000000000000000001": "0x0000000000000000000000000000000000000000000000000000000000000007"
							}
						},
						"{{CHILD_ADDR}}": {
							"balance": "0x0",
							"nonce": 1,
							"code": "{{CHILD_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
							}
						}
					}
				}"#,
			expected_call_trace_json: r#"{
					"pre": {
						"{{CONTRACT_ADDR}}": {
							"balance": "0x0",
							"nonce": 2,
							"code": "{{CONTRACT_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000001": "0x0000000000000000000000000000000000000000000000000000000000000007"
							}
						},
						"{{CHILD_ADDR}}": {
							"balance": "0x0",
							"nonce": 1,
							"code": "{{CHILD_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
							}
						}
					},
					"post": {
						"{{CONTRACT_ADDR}}": {
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000001": "0x0000000000000000000000000000000000000000000000000000000000000008"
							}
						},
						"{{CHILD_ADDR}}": {
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x0000000000000000000000000000000000000000000000000000000000000007"
							}
						}
					}
				}"#,
		},
	];

	for test_case in test_cases {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000_000);

			let contract_addr = crate::address::create1(&ALICE_ADDR, 0u64);
			let child_addr = crate::address::create1(&contract_addr, 1u64);

			// Compute balances
			let alice_balance_pre = Pallet::<Test>::convert_native_to_evm(
				1_000_000_000_000 - Pallet::<Test>::min_balance(),
			);

			let replace_placeholders = |json: &str| -> String {
				let alice_balance_post = Pallet::<Test>::evm_balance(&ALICE_ADDR);

				let mut child_addr_bytes = [0u8; 32];
				child_addr_bytes[12..32].copy_from_slice(child_addr.as_bytes());

				json.replace("{{ALICE_ADDR}}", &format!("{:#x}", ALICE_ADDR))
					.replace("{{CONTRACT_ADDR}}", &format!("{:#x}", contract_addr))
					.replace("{{CHILD_ADDR}}", &format!("{:#x}", child_addr))
					.replace("{{ALICE_BALANCE_PRE}}", &format!("{:#x}", alice_balance_pre))
					.replace("{{ALICE_BALANCE_POST}}", &format!("{:#x}", alice_balance_post))
					.replace(
						"{{CONTRACT_CODE}}",
						&format!("0x{}", hex::encode(&contract_runtime_code)),
					)
					.replace("{{CHILD_CODE}}", &format!("0x{}", hex::encode(&child_runtime_code)))
					.replace(
						"{{CHILD_ADDR_PADDED}}",
						&format!("0x{}", hex::encode(child_addr_bytes)),
					)
			};

			let mut tracer = PrestateTracer::<Test>::new(test_case.config.clone());
			let Contract { addr: contract_addr_actual, .. } = trace(&mut tracer, || {
				builder::bare_instantiate(Code::Upload(counter_code.clone()))
					.salt(None)
					.build_and_unwrap_contract()
			});
			assert_eq!(contract_addr, contract_addr_actual, "contract address mismatch");

			let instantiate_trace = tracer.collect_trace();

			let expected_json = replace_placeholders(test_case.expected_instantiate_trace_json);
			let expected_trace: PrestateTrace = serde_json::from_str(&expected_json).unwrap();
			assert_eq!(
				instantiate_trace, expected_trace,
				"unexpected instantiate trace for {:?}",
				test_case.config
			);

			let mut tracer = PrestateTracer::<Test>::new(test_case.config.clone());
			trace(&mut tracer, || {
				builder::bare_call(contract_addr)
					.data(
						NestedCounter::NestedCounterCalls::nestedNumber(
							NestedCounter::nestedNumberCall {},
						)
						.abi_encode(),
					)
					.build_and_unwrap_result();
			});

			let call_trace = tracer.collect_trace();
			let expected_json = replace_placeholders(test_case.expected_call_trace_json);
			let expected_trace: PrestateTrace = serde_json::from_str(&expected_json).unwrap();
			assert_eq!(
				call_trace, expected_trace,
				"unexpected call trace for {:?}",
				test_case.config
			);
		});
	}
}

#[test]
fn eth_substrate_call_dispatches_successfully() {
	use frame_support::traits::fungible::Inspect;
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, 100);

		let transfer_call =
			crate::tests::RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
				dest: BOB,
				value: 50,
			});

		assert!(EthBlockBuilderFirstValues::<Test>::get().is_none());

		assert_ok!(Pallet::<Test>::eth_substrate_call(
			Origin::EthTransaction(ALICE).into(),
			Box::new(transfer_call),
			vec![]
		));

		// Verify balance changed
		assert_eq!(<Test as Config>::Currency::balance(&ALICE), 950);
		assert_eq!(<Test as Config>::Currency::balance(&BOB), 150);

		assert!(EthBlockBuilderFirstValues::<Test>::get().is_some());
	});
}

#[test]
fn eth_substrate_call_requires_eth_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let inner_call = frame_system::Call::remark { remark: vec![] };

		// Should fail with non-EthTransaction origin
		assert_noop!(
			Pallet::<Test>::eth_substrate_call(
				RuntimeOrigin::signed(ALICE),
				Box::new(inner_call.into()),
				vec![]
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn eth_substrate_call_tracks_weight_correctly() {
	use crate::weights::WeightInfo;
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000);

		let inner_call = frame_system::Call::remark { remark: vec![0u8; 100] };
		let transaction_encoded = vec![];
		let transaction_encoded_len = transaction_encoded.len() as u32;

		let result = Pallet::<Test>::eth_substrate_call(
			Origin::EthTransaction(ALICE).into(),
			Box::new(inner_call.clone().into()),
			transaction_encoded,
		);

		assert_ok!(result);
		let post_info = result.unwrap();

		let overhead = <Test as Config>::WeightInfo::eth_substrate_call(transaction_encoded_len);
		let expected_weight = overhead.saturating_add(inner_call.get_dispatch_info().call_weight);
		assert!(
			expected_weight == post_info.actual_weight.unwrap(),
			"expected_weight ({}) should be == actual_weight ({})",
			expected_weight,
			post_info.actual_weight.unwrap(),
		);
	});
}

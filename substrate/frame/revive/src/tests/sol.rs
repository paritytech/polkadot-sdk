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
	evm::{
		CallTrace, CallTracer, CallType, OpcodeStep, OpcodeTrace, OpcodeTracer, OpcodeTracerConfig,
	},
	test_utils::{builder::Contract, ALICE, ALICE_ADDR},
	tests::{
		builder,
		sol::revm_tracing::RevmTracer,
		test_utils::{contract_base_deposit, ensure_stored, get_contract},
		ExtBuilder, Test,
	},
	tracing::trace,
	Code, Config, PristineCode, U256,
};
use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Fibonacci, FixtureType};
use pretty_assertions::assert_eq;
use revm::bytecode::opcode::*;

mod arithmetic;
mod bitwise;
mod block_info;
mod contract;
mod control;
mod host;
mod memory;
mod revm_tracing;
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
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let mut tracer = CallTracer::new(Default::default(), |_| crate::U256::zero());
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
			.with_tracer(&mut tracer)
			.build_and_unwrap_contract();

		let contract = get_contract(&addr);
		let runtime_code = PristineCode::<Test>::get(contract.code_hash).unwrap();

		assert_eq!(
			tracer.collect_trace().unwrap(),
			CallTrace {
				from: ALICE_ADDR,
				call_type: CallType::Create2,
				to: addr,
				input: code.into(),
				output: runtime_code.into(),
				value: Some(crate::U256::zero()),
				..Default::default()
			}
		);

		let mut call_tracer = CallTracer::new(Default::default(), |_| crate::U256::zero());
		let result = trace(&mut call_tracer, || {
			builder::bare_call(addr)
				.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
				.build_and_unwrap_result()
		});

		let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(55u64, decoded);

		assert_eq!(
			call_tracer.collect_trace().unwrap(),
			CallTrace {
				call_type: CallType::Call,
				from: ALICE_ADDR,
				to: addr,
				input: Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 })
					.abi_encode()
					.into(),
				output: result.data.into(),
				value: Some(crate::U256::zero()),
				..Default::default()
			},
		);
	});
}

#[test]
fn opcode_tracing_works() {
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let config = OpcodeTracerConfig {
			enable_memory: false,
			disable_stack: false,
			disable_storage: true,
			enable_return_data: true,
			limit: Some(5),
			memory_word_limit: 16,
		};

		let mut tracer = OpcodeTracer::new(config, |_| sp_core::U256::from(0u64));
		builder::bare_call(addr)
			.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 3u64 }).abi_encode())
			.with_tracer(&mut tracer)
			.build_and_unwrap_result();

		let actual_trace = tracer.collect_trace();
		let expected_trace = OpcodeTrace {
			gas: actual_trace.gas,
			failed: false,
			return_value: crate::evm::Bytes(U256::from(2).to_big_endian().to_vec()),
			struct_logs: vec![
				OpcodeStep {
					pc: 0,
					op: PUSH1,
					gas: sp_core::U256::from(0u64),
					gas_cost: sp_core::U256::from(0u64),
					depth: 1,
					..Default::default()
				},
				OpcodeStep {
					pc: 2,
					op: PUSH1,
					gas: sp_core::U256::from(0u64),
					gas_cost: sp_core::U256::from(0u64),
					depth: 1,
					stack: vec![U256::from(0x80)],
					..Default::default()
				},
				OpcodeStep {
					pc: 4,
					op: MSTORE,
					gas: sp_core::U256::from(0u64),
					gas_cost: sp_core::U256::from(0u64),
					depth: 1,
					stack: vec![U256::from(0x80), U256::from(0x40)],
					..Default::default()
				},
				OpcodeStep {
					pc: 5,
					op: CALLVALUE,
					gas: sp_core::U256::from(0u64),
					gas_cost: sp_core::U256::from(0u64),
					depth: 1,
					..Default::default()
				},
				OpcodeStep {
					pc: 6,
					op: DUP1,
					gas: sp_core::U256::from(0u64),
					gas_cost: sp_core::U256::from(0u64),
					depth: 1,
					stack: vec![U256::from(0)],
					..Default::default()
				},
			],
		};

		assert_eq!(actual_trace, expected_trace);
	});
}

#[test]
fn opcode_tracing_match_revm_works() {
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();
	let config = OpcodeTracerConfig {
		enable_memory: true,
		// flip once https://github.com/paradigmxyz/revm-inspectors/pull/359 merged
		enable_return_data: false,
		disable_stack: false,
		disable_storage: false,
		memory_word_limit: 16,
		limit: None,
	};

	let revive_traces = ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		let mut tracer = OpcodeTracer::new(config.clone(), |_| U256::from(0u64));
		builder::bare_call(addr)
			.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 2u64 }).abi_encode())
			.with_tracer(&mut tracer)
			.build_and_unwrap_result();

		tracer.collect_trace()
	});

	let revm_traces = {
		use revm::{context::TxEnv, context_interface::TransactTo, primitives::Bytes};

		let mut tracer = RevmTracer::new(config);
		let addr = tracer.deploy(TxEnv { data: Bytes::from(code), ..Default::default() });
		tracer.call(TxEnv {
			kind: TransactTo::Call(addr),
			data: Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 2u64 })
				.abi_encode()
				.into(),
			..Default::default()
		})
	};

	assert_eq!(OpcodeTrace::from(revm_traces), revive_traces);
}

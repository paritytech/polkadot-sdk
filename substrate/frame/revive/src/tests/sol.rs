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
	test_utils::{builder::Contract, ALICE},
	tests::{
		builder,
		test_utils::{contract_base_deposit, ensure_stored, get_contract},
		ExtBuilder, Test,
	},
	Code, Config, PristineCode,
};
use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Fibonacci, FixtureType};
use pretty_assertions::assert_eq;

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
				.data(
					Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: U256::from(10u64) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(55u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		}

		// init code is not stored
		assert!(!PristineCode::<Test>::contains_key(init_hash));
	});
}

#[test]
fn basic_evm_flow_tracing_works() {
	use crate::{
		evm::{CallTrace, CallTracer, CallType},
		test_utils::ALICE_ADDR,
		tracing::trace,
	};
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let mut tracer = CallTracer::new(Default::default(), |_| crate::U256::zero());
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = trace(&mut tracer, || {
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract()
		});

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
				.data(
					Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: U256::from(10u64) })
						.abi_encode(),
				)
				.build_and_unwrap_result()
		});

		assert_eq!(
			U256::from(55u32),
			U256::from_be_bytes::<32>(result.data.clone().try_into().unwrap())
		);

		assert_eq!(
			call_tracer.collect_trace().unwrap(),
			CallTrace {
				call_type: CallType::Call,
				from: ALICE_ADDR,
				to: addr,
				input: Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: U256::from(10u64) })
					.abi_encode()
					.into(),
				output: result.data.into(),
				value: Some(crate::U256::zero()),
				..Default::default()
			},
		);
	});
}

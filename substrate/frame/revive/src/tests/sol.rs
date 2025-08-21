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

mod block_info;
mod system;

use crate::{
	test_utils::{builder::Contract, ALICE},
	tests::{
		builder,
		test_utils::{ensure_stored, get_contract_checked},
		ExtBuilder, Test,
	},
	Code, Config,
};
use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Fibonacci, FixtureType};
use pretty_assertions::assert_eq;

#[test]
fn basic_evm_flow_works() {
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		// check the code exists
		let contract = get_contract_checked(&addr).unwrap();
		ensure_stored(contract.code_hash);

		let result = builder::bare_call(addr)
			.data(
				Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: U256::from(10u64) })
					.abi_encode(),
			)
			.build_and_unwrap_result();
		assert_eq!(U256::from(55u32), U256::from_be_bytes::<32>(result.data.try_into().unwrap()));
	});
}

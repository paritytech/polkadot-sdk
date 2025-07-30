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

//! Tests for bitwise comparison operations.

use crate::{
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Bitwise, FixtureType};
use pretty_assertions::assert_eq;

#[test]
fn lt_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 5 < 10 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::lt(Bitwise::ltCall { a: U256::from(5), b: U256::from(10) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"LT(5, 10) should equal 1 for {:?}", fixture_type
			);

			// Test: 10 < 5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::lt(Bitwise::ltCall { a: U256::from(10), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"LT(10, 5) should equal 0 for {:?}", fixture_type
			);

			// Test: 5 < 5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::lt(Bitwise::ltCall { a: U256::from(5), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"LT(5, 5) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

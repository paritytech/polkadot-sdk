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
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Arithmetic, FixtureType};
use pretty_assertions::assert_eq;

#[test]
fn add_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Simple test first - just like the original
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::add(Arithmetic::addCall { a: U256::from(20u32), b: U256::from(22u32) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(42u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ADD(20, 22) should equal 42 for {:?}", fixture_type
			);

			// Test large numbers but not MAX overflow
			let large_a = U256::from(u64::MAX);
			let large_b = U256::from(1000u32);
			let expected = large_a + large_b;
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::add(Arithmetic::addCall { a: large_a, b: large_b })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ADD({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
			);
		});
	}
}

#[test]
fn mul_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Simple test first - just like the original
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::mul(Arithmetic::mulCall { a: U256::from(20u32), b: U256::from(22u32) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(440u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"MUL(20, 22) should equal 440 for {:?}", fixture_type
			);

			// Test large numbers but not MAX overflow
			let large_a = U256::from(u64::MAX);
			let large_b = U256::from(1000u32);
			let expected = large_a * large_b;
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::mul(Arithmetic::mulCall { a: large_a, b: large_b })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"MUL({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
			);
		});
	}
}

#[test]
fn sub_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Simple test first - just like the original
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::sub(Arithmetic::subCall { a: U256::from(20u32), b: U256::from(18u32) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(2u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SUB(20, 18) should equal 2 for {:?}", fixture_type
			);

			// Test large numbers but not MAX overflow
			let large_a = U256::from(u64::MAX);
			let large_b = U256::from(1000u32);
			let expected = large_a - large_b;
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::sub(Arithmetic::subCall { a: large_a, b: large_b })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SUB({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
			);
		});
	}
}
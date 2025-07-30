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

/// Tests for bitwise operations.

use crate::{
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, primitives::I256, sol_types::SolInterface, hex};
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

#[test]
fn gt_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 5 > 10 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::gt(Bitwise::gtCall { a: U256::from(5), b: U256::from(10) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"GT(5, 10) should equal 0 for {:?}", fixture_type
			);

			// Test: 10 > 5 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::gt(Bitwise::gtCall { a: U256::from(10), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"GT(10, 5) should equal 1 for {:?}", fixture_type
			);

			// Test: 5 > 5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::gt(Bitwise::gtCall { a: U256::from(5), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"GT(5, 5) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn eq_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 5 == 10 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::eq(Bitwise::eqCall { a: U256::from(5), b: U256::from(10) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"EQ(5, 10) should equal 0 for {:?}", fixture_type
			);

			// Test: 10 == 5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::eq(Bitwise::eqCall { a: U256::from(10), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"EQ(10, 5) should equal 0 for {:?}", fixture_type
			);

			// Test: 5 == 5 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::eq(Bitwise::eqCall { a: U256::from(5), b: U256::from(5) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"EQ(5, 5) should equal 1 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn slt_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let minus_five = -I256::from_raw(U256::from(5u32));
			let ten = I256::from_raw(U256::from(10u32));
			let five = I256::from_raw(U256::from(5u32));

			// Test: -5 < 10 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::slt(Bitwise::sltCall { a: minus_five, b: ten })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SLT(-5, 10) should equal 1 for {:?}", fixture_type
			);

			// Test: 10 < -5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::slt(Bitwise::sltCall { a: ten, b: minus_five })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SLT(10, -5) should equal 0 for {:?}", fixture_type
			);

			// Test: -5 < 5 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::slt(Bitwise::sltCall { a: minus_five, b: five })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SLT(-5, 5) should equal 1 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn sgt_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let minus_five = -I256::from_raw(U256::from(5u32));
			let ten = I256::from_raw(U256::from(10u32));
			let five = I256::from_raw(U256::from(5u32));

			// Test: -5 > 10 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::sgt(Bitwise::sgtCall { a: minus_five, b: ten })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SGT(-5, 10) should equal 0 for {:?}", fixture_type
			);

			// Test: 10 > -5 should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::sgt(Bitwise::sgtCall { a: ten, b: minus_five })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SGT(10, -5) should equal 1 for {:?}", fixture_type
			);

			// Test: -5 > 5 should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::sgt(Bitwise::sgtCall { a: minus_five, b: five })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SGT(-5, 5) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn and_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 0b1010 & 0b1100 should return 0b1000 (8)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::and(Bitwise::andCall { 
						a: U256::from(0b1010), 
						b: U256::from(0b1100) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0b1000),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"AND(0b1010, 0b1100) should equal 0b1000 for {:?}", fixture_type
			);

			// Test: 0xFF & 0x0F should return 0x0F (15)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::and(Bitwise::andCall { 
						a: U256::from(0xFF), 
						b: U256::from(0x0F) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x0F),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"AND(0xFF, 0x0F) should equal 0x0F for {:?}", fixture_type
			);

			// Test: 0 & anything should return 0
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::and(Bitwise::andCall { 
						a: U256::from(0), 
						b: U256::from(0xFFFFFFFFu32) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"AND(0, 0xFFFFFFFF) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn or_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 0b1010 | 0b1100 should return 0b1110 (14)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::or(Bitwise::orCall { 
						a: U256::from(0b1010), 
						b: U256::from(0b1100) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0b1110),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"OR(0b1010, 0b1100) should equal 0b1110 for {:?}", fixture_type
			);

			// Test: 0xF0 | 0x0F should return 0xFF (255)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::or(Bitwise::orCall { 
						a: U256::from(0xF0), 
						b: U256::from(0x0F) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0xFF),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"OR(0xF0, 0x0F) should equal 0xFF for {:?}", fixture_type
			);

			// Test: 0 | anything should return anything
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::or(Bitwise::orCall { 
						a: U256::from(0), 
						b: U256::from(0x12345678) 
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x12345678),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"OR(0, 0x12345678) should equal 0x12345678 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn xor_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: 0b1010 ^ 0b1100 should return 0b0110 (6)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::xor(Bitwise::xorCall {
						a: U256::from(0b1010),
						b: U256::from(0b1100)
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0b0110),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"XOR(0b1010, 0b1100) should equal 0b0110 for {:?}", fixture_type
			);

			// Test: 0xFF ^ 0xAA should return 0x55 (85)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::xor(Bitwise::xorCall {
						a: U256::from(0xFF),
						b: U256::from(0xAA)
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x55),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"XOR(0xFF, 0xAA) should equal 0x55 for {:?}", fixture_type
			);

			// Test: anything ^ itself should return 0
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::xor(Bitwise::xorCall {
						a: U256::from(0x12345678),
						b: U256::from(0x12345678)
					}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"XOR(0x12345678, 0x12345678) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn not_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: ~0 should return U256::MAX
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::not(Bitwise::notCall { a: U256::from(0) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::MAX,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"NOT(0) should equal U256::MAX for {:?}", fixture_type
			);

			// Test: ~U256::MAX should return 0
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::not(Bitwise::notCall { a: U256::MAX })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"NOT(U256::MAX) should equal 0 for {:?}", fixture_type
			);

			// Test: ~0x0F should return 0xFFFFF...F0 (all bits flipped)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::not(Bitwise::notCall { a: U256::from(0x0F) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			let expected = !U256::from(0x0F);
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"NOT(0x0F) should equal ~0x0F for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn iszero_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: iszero(0) should return 1 (true)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::iszero(Bitwise::iszeroCall { a: U256::from(0) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ISZERO(0) should equal 1 for {:?}", fixture_type
			);

			// Test: iszero(1) should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::iszero(Bitwise::iszeroCall { a: U256::from(1) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ISZERO(1) should equal 0 for {:?}", fixture_type
			);

			// Test: iszero(U256::MAX) should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::iszero(Bitwise::iszeroCall { a: U256::MAX })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ISZERO(U256::MAX) should equal 0 for {:?}", fixture_type
			);

			// Test: iszero(0x12345678) should return 0 (false)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::iszero(Bitwise::iszeroCall { a: U256::from(0x12345678) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"ISZERO(0x12345678) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn clz_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: clz(0) should return 256
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: U256::from(0) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
            println!("CLZ(0) result: {:?}", result.data);
			assert_eq!(
				U256::from(256),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(0) should equal 256 for {:?}", fixture_type
			);

			// Test: clz(1) should return 255 (255 leading zeros)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: U256::from(1) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(255),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(1) should equal 255 for {:?}", fixture_type
			);

			// Test: clz(0xFF) should return 248 (256 - 8 = 248 leading zeros)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: U256::from(0xFF) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(248),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(0xFF) should equal 248 for {:?}", fixture_type
			);

			// Test: clz(U256::MAX) should return 0 (no leading zeros)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: U256::MAX })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(U256::MAX) should equal 0 for {:?}", fixture_type
			);

			// Test: clz(1 << 255) should return 0 (highest bit set)
			let high_bit = U256::from(1) << 255;
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: high_bit })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(1 << 255) should equal 0 for {:?}", fixture_type
			);

			// Test: clz(1 << 254) should return 1 (second highest bit set)
			let second_high_bit = U256::from(1) << 254;
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: second_high_bit })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(1 << 254) should equal 1 for {:?}", fixture_type
			);

			// Test: clz(0x100) should return 247 (256 - 9 = 247 leading zeros)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::clzOp(Bitwise::clzOpCall { a: U256::from(0x100) })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(247),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"CLZ(0x100) should equal 247 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn byte_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: byte(0, 0x1234567890abcdef000000000000000000000000000000000000000000000000) == 0x12
			let value = U256::from_be_bytes::<32>(hex::decode("1234567890abcdef000000000000000000000000000000000000000000000000").unwrap().try_into().unwrap());
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::byteOp(Bitwise::byteOpCall { index: U256::from(0), value })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x12),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"BYTE(0, ...) should equal 0x12 for {:?}", fixture_type
			);

			// Test: byte(7, 0x1234567890abcdef000000000000000000000000000000000000000000000000) == 0xef
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::byteOp(Bitwise::byteOpCall { index: U256::from(7), value })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0xef),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"BYTE(7, ...) should equal 0xef for {:?}", fixture_type
			);

			// Test: byte(31, 0x1234567890abcdef000000000000000000000000000000000000000000000000) == 0x00
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::byteOp(Bitwise::byteOpCall { index: U256::from(31), value })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x00),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"BYTE(31, ...) should equal 0x00 for {:?}", fixture_type
			);

			// Test: byte(32, ...) should return 0 (out of bounds)
			let result = builder::bare_call(addr)
				.data(
					Bitwise::BitwiseCalls::byteOp(Bitwise::byteOpCall { index: U256::from(32), value })
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0x00),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"BYTE(32, ...) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn sar_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: sar(1, 0) == 1
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value: I256::from_raw(U256::from(1)), shift: U256::from(0) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(1, 0) should equal 1 for {:?}", fixture_type
			);

			// Test: sar(1, 1) == 0
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value: I256::from_raw(U256::from(1)), shift: U256::from(1) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(1, 1) should equal 0 for {:?}", fixture_type
			);

			// Test: sar(0x800...0, 1) == 0xc000...0 (arithmetic shift, sign extend)
			let value = I256::from_raw(U256::from_be_bytes::<32>([0x80].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap()));
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value, shift: U256::from(1) }).abi_encode())
				.build_and_unwrap_result();
			let expected = U256::from_be_bytes::<32>([0xC0].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap());
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(0x800...0, 1) should equal 0xc000...0 for {:?}", fixture_type
			);

			// Test: sar(0x800...0, 255) == 0xffff...ff (all bits set, sign extend)
			let value = I256::from_raw(U256::from_be_bytes::<32>([0x80].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap()));
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value, shift: U256::from(255) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::MAX,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(0x800...0, 255) should equal U256::MAX for {:?}", fixture_type
			);

			// Test: sar(0x800...0, 256) == 0xffff...ff (all bits set, sign extend)
			let value = I256::from_raw(U256::from_be_bytes::<32>([0x80].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap()));
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value, shift: U256::from(256) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::MAX,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(0x800...0, 256) should equal U256::MAX for {:?}", fixture_type
			);

			// Test: sar(1, 256) == 0 (shift out all bits)
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::sar(Bitwise::sarCall { value: I256::from_raw(U256::from(1)), shift: U256::from(256) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SAR(1, 256) should equal 0 for {:?}", fixture_type
			);
		});
	}
}
#[test]
fn shl_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: shl(1, 0) == 1
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shl(Bitwise::shlCall { value: U256::from(1), shift: U256::from(0) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHL(1, 0) should equal 1 for {:?}", fixture_type
			);

			// Test: shl(1, 1) == 2
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shl(Bitwise::shlCall { value: U256::from(1), shift: U256::from(1) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(2),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHL(1, 1) should equal 2 for {:?}", fixture_type
			);

			// Test: shl(1, 255) == 0x800...0
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shl(Bitwise::shlCall { value: U256::from(1), shift: U256::from(255) }).abi_encode())
				.build_and_unwrap_result();
			let expected = U256::from_be_bytes::<32>([0x80].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap());
			assert_eq!(
				expected,
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHL(1, 255) should equal 0x800...0 for {:?}", fixture_type
			);

			// Test: shl(1, 256) == 0
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shl(Bitwise::shlCall { value: U256::from(1), shift: U256::from(256) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHL(1, 256) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

#[test]
fn shr_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Bitwise", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Test: shr(1, 0) == 1
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shr(Bitwise::shrCall { value: U256::from(1), shift: U256::from(0) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHR(1, 0) should equal 1 for {:?}", fixture_type
			);

			// Test: shr(2, 1) == 1
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shr(Bitwise::shrCall { value: U256::from(2), shift: U256::from(1) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHR(2, 1) should equal 1 for {:?}", fixture_type
			);

			// Test: shr(0x800...0, 255) == 1
			let value = U256::from_be_bytes::<32>([0x80].iter().chain([0u8; 31].iter()).cloned().collect::<Vec<u8>>().try_into().unwrap());
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shr(Bitwise::shrCall { value, shift: U256::from(255) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(1),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHR(0x800...0, 255) should equal 1 for {:?}", fixture_type
			);

			// Test: shr(1, 256) == 0
			let result = builder::bare_call(addr)
				.data(Bitwise::BitwiseCalls::shr(Bitwise::shrCall { value: U256::from(1), shift: U256::from(256) }).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(0),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"SHR(1, 256) should equal 0 for {:?}", fixture_type
			);
		});
	}
}

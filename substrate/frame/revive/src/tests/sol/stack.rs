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
	tests::{builder, sol::make_initcode_from_runtime_code, ExtBuilder, Test},
	Code, Config, U256,
};
use frame_support::traits::fungible::Mutate;
use pretty_assertions::assert_eq;
use revm::bytecode::opcode::*;

#[test]
fn push_works() {
	let expected_value = 0xfefefefe_u64;
	let runtime_code: Vec<u8> = vec![
		vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
		vec![PUSH0],
		vec![MSTORE],
		vec![PUSH1, 0x20_u8],
		vec![PUSH0],
		vec![RETURN],
	]
	.into_iter()
	.flatten()
	.collect();
	let code = make_initcode_from_runtime_code(&runtime_code);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr).build_and_unwrap_result();

		assert!(!result.did_revert(), "test reverted");
		assert_eq!(
			U256::from_big_endian(&result.data),
			U256::from(expected_value),
			"memory test should return {expected_value}"
		);
	});
}

#[test]
fn pop_works() {
	let expected_value = 0xfefefefe_u64;
	let runtime_code: Vec<u8> = vec![
		vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
		vec![PUSH1, 0xaa],
		vec![POP],
		vec![PUSH0],
		vec![MSTORE],
		vec![PUSH1, 0x20_u8],
		vec![PUSH0],
		vec![RETURN],
	]
	.into_iter()
	.flatten()
	.collect();
	let code = make_initcode_from_runtime_code(&runtime_code);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr).build_and_unwrap_result();

		assert!(!result.did_revert(), "test reverted");
		assert_eq!(
			U256::from_big_endian(&result.data),
			U256::from(expected_value),
			"memory test should return {expected_value}"
		);
	});
}

#[test]
fn dup_works() {
	let expected_value = 0xfefefefe_u64;
	let runtime_code: Vec<u8> = vec![
		vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
		vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
		vec![DUP2],
		vec![PUSH0],
		vec![MSTORE],
		vec![PUSH1, 0x20_u8],
		vec![PUSH0],
		vec![RETURN],
	]
	.into_iter()
	.flatten()
	.collect();
	let code = make_initcode_from_runtime_code(&runtime_code);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr).build_and_unwrap_result();

		assert!(!result.did_revert(), "test reverted");
		assert_eq!(
			U256::from_big_endian(&result.data),
			U256::from(expected_value),
			"memory test should return {expected_value}"
		);
	});
}

#[test]
fn swap_works() {
	let expected_value = 0xfefefefe_u64;
	let runtime_code: Vec<u8> = vec![
		vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
		vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
		vec![SWAP1],
		vec![PUSH0],
		vec![MSTORE],
		vec![PUSH1, 0x20_u8],
		vec![PUSH0],
		vec![RETURN],
	]
	.into_iter()
	.flatten()
	.collect();
	let code = make_initcode_from_runtime_code(&runtime_code);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr).build_and_unwrap_result();

		assert!(!result.did_revert(), "test reverted");
		assert_eq!(
			U256::from_big_endian(&result.data),
			U256::from(expected_value),
			"memory test should return {expected_value}"
		);
	});
}

// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg(test)]

use super::*;
use crate::{
	call_builder::CallSetup,
	exec::Stack,
	tests::{ExtBuilder, Test},
	wasm::WasmBlob,
};
use alloy_core::hex as alloy_hex;
use core::num::NonZero;
use sp_core::hex2array as hex;

type Env<'a> = Stack<'a, Test, WasmBlob<Test>>;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EthConsensusTest {
	input: String,
	expected: String,
	name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EthConsensusFailureTest {
	input: String,
	expected_error: String,
	name: String,
}

/// Convenience function to call a primitive pre-compile for tests.
pub fn run_primitive<P: PrimitivePrecompile<T = Test>>(input: Vec<u8>) -> Result<Vec<u8>, Error> {
	ExtBuilder::default().build().execute_with(|| {
		let mut call_setup = CallSetup::<Test>::default();
		let (mut ext, _) = call_setup.ext();
		assert!(P::MATCHER.is_fixed(), "All pre-compiles we are testing here are fixed");
		let address = P::MATCHER.base_address();
		if P::HAS_CONTRACT_INFO {
			P::call_with_info(&address, input, &mut ext)
		} else {
			P::call(&address, input, &mut ext)
		}
	})
}

/// Tests a precompile against the ethereum consensus tests defined in the given json
/// The JSON format is expected to contain an array of test vectors,
/// where each vector can be deserialized into an "EthConsensusTest".
pub fn run_test_vectors<P: PrimitivePrecompile<T = Test>>(json: &str) {
	let tests: Vec<EthConsensusTest> = serde_json::from_str(json).expect("expected json array");

	for test in tests {
		let input: Vec<u8> =
			alloy_hex::decode(test.input).expect("Could not hex-decode test input data");

		match run_primitive::<P>(input) {
			Ok(data) => {
				assert_eq!(
					alloy_hex::encode(data),
					test.expected,
					"test '{}' failed (different output)",
					test.name
				);
			},
			Err(err) => {
				panic!("Test '{}' returned error: {:?}", test.name, err);
			},
		}
	}
}

pub fn run_failure_test_vectors<P: PrimitivePrecompile<T = Test>>(json: &str) {
	let tests: Vec<EthConsensusFailureTest> =
		serde_json::from_str(json).expect("expected json array");

	for test in tests {
		let input: Vec<u8> =
			alloy_hex::decode(test.input).expect("Could not hex-decode test input data");

		match run_primitive::<P>(input) {
			Err(Error::Error(ExecError { error: DispatchError::Other(reason), .. })) => {
				assert_eq!(
					test.expected_error, reason,
					"Test '{}' failed (different error)",
					test.name
				);
			},
			Err(err) => panic!("Test {} failed with wrong error: {:?}", test.name, err),
			Ok(data) => {
				panic!("Test should failed, got {data:?}");
			},
		}
	}
}

#[test]
fn matching_works() {
	struct Matcher1;
	struct Matcher2;

	impl PrimitivePrecompile for Matcher1 {
		type T = Test;
		const MATCHER: BuiltinAddressMatcher =
			BuiltinAddressMatcher::Fixed(NonZero::new(0x42).unwrap());
		const HAS_CONTRACT_INFO: bool = true;

		fn call(
			address: &[u8; 20],
			_input: Vec<u8>,
			_env: &mut impl Ext<T = Self::T>,
		) -> Result<Vec<u8>, Error> {
			Ok(address.to_vec())
		}
	}

	impl PrimitivePrecompile for Matcher2 {
		type T = Test;
		const MATCHER: BuiltinAddressMatcher =
			BuiltinAddressMatcher::Prefix(NonZero::new(0x88).unwrap());
		const HAS_CONTRACT_INFO: bool = false;

		fn call(
			address: &[u8; 20],
			_input: Vec<u8>,
			_env: &mut impl Ext<T = Self::T>,
		) -> Result<Vec<u8>, Error> {
			Ok(address.to_vec())
		}
	}

	type Col = (Matcher1, Matcher2);

	assert_eq!(
		<Matcher1 as PrimitivePrecompile>::MATCHER.base_address(),
		hex!("0000000000000000000000000000000000000042")
	);
	assert_eq!(
		<Matcher1 as PrimitivePrecompile>::MATCHER.base_address(),
		<Matcher1 as PrimitivePrecompile>::MATCHER.highest_address()
	);

	assert_eq!(
		<Matcher2 as PrimitivePrecompile>::MATCHER.base_address(),
		hex!("0000000000000000000000000000000000000088")
	);
	assert_eq!(
		<Matcher2 as PrimitivePrecompile>::MATCHER.highest_address(),
		hex!("FFFFFFFF00000000000000000000000000000088")
	);

	assert!(Col::get::<Env>(&hex!("1000000000000000000000000000000000000043")).is_none());
	assert_eq!(
		Col::get::<Env>(&hex!("0000000000000000000000000000000000000042"))
			.unwrap()
			.has_contract_info,
		true,
	);
	assert!(Col::get::<Env>(&hex!("1000000000000000000000000000000000000042")).is_none());
	assert_eq!(
		Col::get::<Env>(&hex!("0000000000000000000000000000000000000088"))
			.unwrap()
			.has_contract_info,
		false,
	);
	assert_eq!(
		Col::get::<Env>(&hex!("2200000000000000000000000000000000000088"))
			.unwrap()
			.has_contract_info,
		false,
	);
	assert_eq!(
		Col::get::<Env>(&hex!("0010000000000000000000000000000000000088"))
			.unwrap()
			.has_contract_info,
		false,
	);
	assert!(Col::get::<Env>(&hex!("0000000010000000000000000000000000000088")).is_none());
}

#[test]
fn builtin_matching_works() {
	let _ = <All<Test>>::CHECK_COLLISION;

	assert_eq!(
		<Builtin<Test>>::get::<Env>(&hex!("0000000000000000000000000000000000000001"))
			.unwrap()
			.has_contract_info(),
		false,
	);

	assert_eq!(
		<Builtin<Test>>::get::<Env>(&hex!("0000000000000000000000000000000000000002"))
			.unwrap()
			.has_contract_info(),
		false,
	);

	assert_eq!(
		<Builtin<Test>>::get::<Env>(&hex!("000000000000000000000000000000000000000a"))
			.unwrap()
			.has_contract_info(),
		false,
	);

	#[cfg(feature = "runtime-benchmarks")]
	assert_eq!(
		<Builtin<Test>>::get::<Env>(&hex!("000000000000000000000000000000000000FFFF"))
			.unwrap()
			.has_contract_info(),
		true,
	);

	#[cfg(feature = "runtime-benchmarks")]
	assert_eq!(
		<Builtin<Test>>::get::<Env>(&hex!("000000000000000000000000000000000000EFFF"))
			.unwrap()
			.has_contract_info(),
		false,
	);
	assert!(
		<Builtin<Test>>::get::<Env>(&hex!("700000000000000000000000000000000000FFFF")).is_none()
	);
	assert!(
		<Builtin<Test>>::get::<Env>(&hex!("700000000000000000000000000000000000EFFF")).is_none()
	);
}

#[test]
fn public_matching_works() {
	let matcher_fixed = AddressMatcher::Fixed(NonZero::new(0x42).unwrap());
	let matcher_prefix = AddressMatcher::Prefix(NonZero::new(0x8).unwrap());

	assert_eq!(matcher_fixed.base_address(), hex!("0000000000000000000000000000000000420000"));
	assert_eq!(matcher_fixed.base_address(), matcher_fixed.highest_address());

	assert_eq!(matcher_prefix.base_address(), hex!("0000000000000000000000000000000000080000"));
	assert_eq!(matcher_prefix.highest_address(), hex!("FFFFFFFF00000000000000000000000000080000"));
}

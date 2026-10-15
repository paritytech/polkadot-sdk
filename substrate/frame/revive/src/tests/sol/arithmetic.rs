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

use super::{slope, weight_of_solc_call};
use crate::{
	Code, Config,
	evm::decode_revert_reason,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
	weights::WeightInfo,
};

use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolInterface},
};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{Arithmetic, FixtureType, compile_module_with_type};
use test_case::test_case;

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn arithmetic_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		{
			let result = builder::bare_call(addr)
				.data(
					Arithmetic::ArithmeticCalls::testArithmetic(Arithmetic::testArithmeticCall {})
						.abi_encode(),
				)
				.build_and_unwrap_result();
			if result.did_revert() {
				if let Some(revert_msg) = decode_revert_reason(&result.data) {
					panic!("Revert message: {revert_msg}");
				} else {
					panic!("Revert without message, raw data: {:?}", result.data);
				}
			}
		}
	});
}

#[test]
fn exp_charges_each_exponent_bit_after_the_first() {
	type Weights = <Test as Config>::WeightInfo;
	let weight_of = |exponent: U256| {
		weight_of_solc_call(
			"Arithmetic",
			Arithmetic::exponentiateCall { base: U256::from(3), exponent }.abi_encode(),
		)
	};

	let zero = weight_of(U256::ZERO);
	let one_bit = weight_of(U256::from(1));
	let max_bits = weight_of(U256::from(1) << 255);

	assert!(
		zero.ref_time() < one_bit.ref_time(),
		"a zero exponent must be cheaper than a non-zero one: zero={zero:?} one bit={one_bit:?}",
	);
	assert_eq!(
		one_bit.saturating_add(slope(Weights::evm_exp_zero_opcode)),
		zero.saturating_add(slope(Weights::evm_exp_opcode)),
	);
	assert_eq!(
		max_bits,
		one_bit.saturating_add(slope(Weights::evm_exp_per_bit).saturating_mul(255))
	);
}

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
	evm::decode_revert_reason,
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::sol_types::SolInterface;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Arithmetic, FixtureType};

#[test]
fn arithmetic_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let result = builder::bare_call(addr)
					.data(
						Arithmetic::ArithmeticCalls::testArithmetic(
							Arithmetic::testArithmeticCall {},
						)
						.abi_encode(),
					)
					.build_and_unwrap_result();
				if result.did_revert() {
					if let Some(revert_msg) = decode_revert_reason(&result.data) {
						log::error!("Revert message: {}", revert_msg);
					} else {
						log::error!("Revert without message, raw data: {:?}", result.data);
					}
				}

				assert!(!result.did_revert(), "arithmetic test reverted");
			}
		});
	}
}

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

use alloy_core::sol_types::SolInterface;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, System as SystemFixture};
use pretty_assertions::assert_eq;
use revm::primitives::Bytes;
use sp_io::hashing::keccak_256;

#[test]
fn keccak_256_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc]
		// TODO remove take(1) once Solc supported
		.into_iter()
		.take(1)
	{
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let pre = b"revive";
			let expected = keccak_256(pre);

			let result = builder::bare_call(addr)
				.data(
					SystemFixture::SystemCalls::keccak256Func(SystemFixture::keccak256FuncCall {
						data: Bytes::from(pre),
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();

			assert_eq!(&expected, result.data.as_slice());
		});
	}
}

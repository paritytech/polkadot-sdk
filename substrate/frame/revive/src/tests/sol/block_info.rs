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
	test_utils::{builder::Contract, ALICE, EVE_ADDR},
	tests::{builder, ExtBuilder, System, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, BlockInfo, FixtureType};
use pretty_assertions::assert_eq;

/// Tests that the blocknumber opcode works as expected.
#[test]
fn block_number_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			System::set_block_number(42);

			let result = builder::bare_call(addr)
				.data(
					BlockInfo::BlockInfoCalls::blockNumber(BlockInfo::blockNumberCall {})
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(42u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the coinbase opcode works as expected.
#[test]
fn coinbase_works() {
	let eve_as_u256 = {
		let mut bytes = [0u8; 32];
		bytes[12..32].copy_from_slice(&EVE_ADDR.0);
		U256::from_be_bytes(bytes)
	};
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					BlockInfo::BlockInfoCalls::coinbase(BlockInfo::coinbaseCall {})
						.abi_encode(),
				)
				.build_and_unwrap_result();
			
			// Verify that we got a 32-byte result (address is padded to 32 bytes in EVM)
			assert_eq!(result.data.len(), 32, "Coinbase should return a 32-byte padded address");
			
			// The coinbase opcode should return the current block's beneficiary address
			let coinbase_result = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

			assert_eq!(
				coinbase_result, 
				eve_as_u256,
				"Coinbase should return expected beneficiary address for {:?}",
				fixture_type
			);
		});
	}
}

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
	tests::{builder, Contracts, ExtBuilder, System, Test, Timestamp},
	vm::evm::{U256Converter, BASE_FEE, DIFFICULTY},
	Code, Config,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, BlockInfo, FixtureType};
use pretty_assertions::assert_eq;
use sp_core::H160;

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

/// Tests that the blockauthor opcode works as expected.
#[test]
fn block_author_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(BlockInfo::BlockInfoCalls::coinbase(BlockInfo::coinbaseCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				Contracts::block_author().unwrap(),
				// Padding is used into the 32 bytes
				H160::from_slice(&result.data[12..])
			);
		});
	}
}

/// Tests that the chainid opcode works as expected.
#[test]
fn chainid_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(BlockInfo::BlockInfoCalls::chainid(BlockInfo::chainidCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(<Test as Config>::ChainId::get()),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the timestamp opcode works as expected.
#[test]
fn timestamp_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			Timestamp::set_timestamp(2000);
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					BlockInfo::BlockInfoCalls::timestamp(BlockInfo::timestampCall {}).abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				// Solidity expects timestamps in seconds, whereas pallet_timestamp uses
				// milliseconds.
				U256::from(Timestamp::get() / 1000),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the gaslimit opcode works as expected.
#[test]
fn gaslimit_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(BlockInfo::BlockInfoCalls::gaslimit(BlockInfo::gaslimitCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(
					<Test as frame_system::Config>::BlockWeights::get().max_block.ref_time()
				),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the basefee opcode works as expected.
#[test]
fn basefee_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(BlockInfo::BlockInfoCalls::basefee(BlockInfo::basefeeCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				BASE_FEE.into_revm_u256(),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the difficulty opcode works as expected.
#[test]
fn difficulty_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("BlockInfo", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					BlockInfo::BlockInfoCalls::difficulty(BlockInfo::difficultyCall {})
						.abi_encode(),
				)
				.build_and_unwrap_result();
			assert_eq!(
				// Alligned with the value set for PVM
				U256::from(DIFFICULTY),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

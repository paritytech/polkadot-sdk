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
	address::AddressMapper,
	test_utils::{builder::Contract, ALICE, BOB, BOB_ADDR},
	tests::{builder, test_utils, ExtBuilder, Test},
	Code, Config, Key, System, H256,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::{fungible::Mutate, Get};
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Host};
use pretty_assertions::assert_eq;

fn convert_to_free_balance(total_balance: u128) -> U256 {
	let existential_deposit_planck =
		<Test as pallet_balances::Config>::ExistentialDeposit::get() as u128;
	let native_to_eth = <<Test as Config>::NativeToEthRatio as Get<u32>>::get() as u128;
	U256::from((total_balance - existential_deposit_planck) * native_to_eth)
}

#[test]
fn balance_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let bobs_balance = 123_456_789_000u64;
		let expected_balance = convert_to_free_balance(bobs_balance as u128);
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			<Test as Config>::Currency::set_balance(&BOB, bobs_balance);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let result = builder::bare_call(addr)
					.data(
						Host::HostCalls::balance(Host::balanceCall { account: BOB_ADDR.0.into() })
							.abi_encode(),
					)
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");
				let result = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

				assert_eq!(
					expected_balance, result,
					"BALANCE should return BOB's balance for {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn selfbalance_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let expected_balance = convert_to_free_balance(100_000_000_000);
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

				<Test as Config>::Currency::set_balance(&account_id32, 100_000_000_000);
			}
			{
				let result = builder::bare_call(addr)
					.data(Host::HostCalls::selfbalance(Host::selfbalanceCall {}).abi_encode())
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");
				let result_balance = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

				assert_eq!(
					expected_balance, result_balance,
					"BALANCE should return contract's balance for {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn extcodesize_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let expected_code_size = {
				let contract_info = test_utils::get_contract(&addr);
				let code_hash = contract_info.code_hash;
				U256::from(test_utils::ensure_stored(code_hash))
			};

			{
				let result = builder::bare_call(addr)
					.data(
						Host::HostCalls::extcodesizeOp(Host::extcodesizeOpCall {
							account: addr.0.into(),
						})
						.abi_encode(),
					)
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");

				let result_size = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

				assert_eq!(
					expected_code_size, result_size,
					"EXTCODESIZE should return the code size for {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn extcodehash_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let expected_code_hash = {
				let contract_info = test_utils::get_contract(&addr);
				contract_info.code_hash
			};

			{
				let result = builder::bare_call(addr)
					.data(
						Host::HostCalls::extcodehashOp(Host::extcodehashOpCall {
							account: addr.0.into(),
						})
						.abi_encode(),
					)
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");

				let result_hash = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
				let result_hash = H256::from(result_hash.to_be_bytes());

				assert_eq!(
					expected_code_hash, result_hash,
					"EXTCODEHASH should return the code hash for {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn extcodecopy_works() {
	use pallet_revive_fixtures::{HostEvmOnly, HostEvmOnly::HostEvmOnlyCalls};
	let fixture_type = FixtureType::Solc;

	let (code, _) = compile_module_with_type("HostEvmOnly", fixture_type).unwrap();
	let (dummy_code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	let code_start = 3;
	let code_len = 17;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let Contract { addr: dummy_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_code.clone())).build_and_unwrap_contract();

		let expected_code = {
			let contract_info = test_utils::get_contract(&dummy_addr);
			let code_hash = contract_info.code_hash;
			let expected_code = crate::PristineCode::<Test>::get(&code_hash)
				.map(|bounded_vec| bounded_vec.to_vec())
				.unwrap_or_default();
			expected_code[code_start..code_start + code_len].to_vec()
		};

		let result = builder::bare_call(addr)
			.data(
				HostEvmOnlyCalls::extcodecopyOp(HostEvmOnly::extcodecopyOpCall {
					account: dummy_addr.0.into(),
					offset: U256::from(code_start),
					size: U256::from(code_len),
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "test reverted");
		let actual_code = {
			let length = u32::from_be_bytes(result.data[60..64].try_into().unwrap()) as usize;
			&result.data[64..64 + length]
		};

		assert_eq!(
			expected_code.len(),
			actual_code.len(),
			"EXTCODECOPY should return the correct code length for {:?}",
			fixture_type
		);

		assert_eq!(
			&expected_code, actual_code,
			"EXTCODECOPY should return the correct code for {:?}",
			fixture_type
		);
	});
}

#[test]
fn blockhash_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let block_number_to_test = 5u64;

			System::<Test>::set_block_number(13);

			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let block_hash = [1; 32];
				frame_system::BlockHash::<Test>::insert(
					&crate::BlockNumberFor::<Test>::from(block_number_to_test as u64),
					<Test as frame_system::Config>::Hash::from(&block_hash),
				);
				let result = builder::bare_call(addr)
					.data(
						Host::HostCalls::blockhashOp(Host::blockhashOpCall {
							blockNumber: U256::from(block_number_to_test),
						})
						.abi_encode(),
					)
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");

				let result_hash = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
				let result_hash = H256::from(result_hash.to_be_bytes());

				let expected_block_hash = System::<Test>::block_hash(block_number_to_test);

				assert_eq!(
					expected_block_hash, result_hash,
					"EXTBLOCKHASH should return the block hash for {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn sload_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		let index = U256::from(13);
		let expected_value = U256::from(17);

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let contract_info = test_utils::get_contract(&addr);
				let key = Key::Fix(index.to_be_bytes());
				contract_info
					.write(&key, Some(expected_value.to_be_bytes::<32>().to_vec()), None, false)
					.unwrap();
			}

			{
				let result = builder::bare_call(addr)
					.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode())
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");
				let result = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

				assert_eq!(
					expected_value, result,
					"result should return expected value {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn sstore_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let index = U256::from(13);
			let expected_value = U256::from(17);
			let unexpected_value = U256::from(19);

			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			{
				let contract_info = test_utils::get_contract(&addr);
				let key = Key::Fix(index.to_be_bytes());
				contract_info
					.write(&key, Some(unexpected_value.to_be_bytes::<32>().to_vec()), None, false)
					.unwrap();
			}

			{
				let result = builder::bare_call(addr)
					.data(
						Host::HostCalls::sstoreOp(Host::sstoreOpCall {
							slot: index,
							value: expected_value,
						})
						.abi_encode(),
					)
					.build_and_unwrap_result();
				assert!(!result.did_revert(), "test reverted");

				let written_value = {
					let contract_info = test_utils::get_contract(&addr);
					let key = Key::Fix(index.to_be_bytes());
					let result = contract_info.read(&key).unwrap();
					U256::from_be_bytes::<32>(result.try_into().unwrap())
				};
				assert_eq!(
					expected_value, written_value,
					"result should return expected value {:?}",
					fixture_type
				);
			}
		});
	}
}

#[test]
fn transient_storage_works() {
	use pallet_revive_fixtures::HostTransientMemory;
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("HostTransientMemory", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let slot = U256::from(0);

			let value = U256::from(13);

			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					HostTransientMemory::HostTransientMemoryCalls::transientMemoryTest(
						HostTransientMemory::transientMemoryTestCall { slot, a: value },
					)
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");
			assert_eq!(
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				U256::from(0),
				"transient storage should return zero for {:?}",
				fixture_type
			);
		});
	}
}

#[test]
#[ignore]
fn log_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
		todo!("implement this test");
	}
}

#[test]
fn selfdestruct_works() {
	use pallet_revive_fixtures::HostEvmOnly;
	let fixture_type = FixtureType::Solc;
	let (code, _) = compile_module_with_type("HostEvmOnly", fixture_type).unwrap();

	let expected_bobs_balance = 100_000_000_000u64;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		<Test as Config>::Currency::set_balance(&BOB, 0);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		assert!(test_utils::get_contract_checked(&addr).is_some());
		{
			let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

			<Test as Config>::Currency::set_balance(&account_id32, expected_bobs_balance);
		}

		{
			let result = builder::bare_call(addr)
				.data(
					HostEvmOnly::HostEvmOnlyCalls::selfdestructOp(
						HostEvmOnly::selfdestructOpCall { recipient: BOB_ADDR.0.into() },
					)
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted: {result:?}");

			let bobs_balance = <Test as Config>::Currency::free_balance(&BOB);
			assert_eq!(
				bobs_balance,
				expected_bobs_balance - 1,
				"BOB's balance should be updated after selfdestruct for {:?}",
				fixture_type
			);
		}
		// the contract should not be deleted so check if it is still there.
		assert!(test_utils::get_contract_checked(&addr).is_some());
	});
}

#[test]
#[ignore]
fn selfdestruct_delete_works() {
	use pallet_revive_fixtures::{HostEvmOnly, HostEvmOnlyFactory};
	let fixture_type = FixtureType::Solc;
	let (code, _) = compile_module_with_type("HostEvmOnlyFactory", fixture_type).unwrap();

	let expected_bobs_balance = 100_000_000_000u64;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);
		<Test as Config>::Currency::set_balance(&BOB, 0);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		assert!(test_utils::get_contract_checked(&addr).is_some());
		{
			let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

			<Test as Config>::Currency::set_balance(&account_id32, expected_bobs_balance);
		}

		{
			let result = builder::bare_call(addr)
				.data(
					HostEvmOnlyFactory::HostEvmOnlyFactoryCalls::createAndSelfdestruct(
						HostEvmOnlyFactory::createAndSelfdestructCall {
							recipient: BOB_ADDR.0.into(),
						},
					)
					.abi_encode(),
				)
				.build_and_unwrap_result();
			println!("result: {result:?}");
			assert!(!result.did_revert(), "test reverted: {result:?}");

			let bobs_balance = <Test as Config>::Currency::free_balance(&BOB);
			assert_eq!(
				bobs_balance,
				expected_bobs_balance - 1,
				"BOB's balance should be updated after selfdestruct for {:?}",
				fixture_type
			);
		}
		// the contract should not be deleted so check if it is still there.
		assert!(test_utils::get_contract_checked(&addr).is_some());
	});
}

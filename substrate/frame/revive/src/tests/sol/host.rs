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
	tests::{builder, test_utils, ExtBuilder, RuntimeEvent, Test},
	Code, Config, Key, System, H256,
};

use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolInterface},
};
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

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let Contract { addr: dummy_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_code.clone())).build_and_unwrap_contract();

		let contract_info = test_utils::get_contract(&dummy_addr);
		let code_hash = contract_info.code_hash;
		let full_code = crate::PristineCode::<Test>::get(&code_hash)
			.map(|bounded_vec| bounded_vec.to_vec())
			.unwrap_or_default();

		struct TestCase {
			description: &'static str,
			offset: usize,
			size: usize,
			expected: Vec<u8>,
		}

		// Test cases covering different scenarios
		let test_cases = vec![
			TestCase {
				description: "copy within bounds",
				offset: 3,
				size: 17,
				expected: full_code[3..20].to_vec(),
			},
			TestCase { description: "len = 0", offset: 0, size: 0, expected: vec![] },
			TestCase {
				description: "offset beyond code length",
				offset: full_code.len(),
				size: 10,
				expected: vec![0u8; 10],
			},
			TestCase {
				description: "offset + size beyond code",
				offset: full_code.len().saturating_sub(5),
				size: 20,
				expected: {
					let mut expected = vec![0u8; 20];
					expected[..5].copy_from_slice(&full_code[full_code.len() - 5..]);
					expected
				},
			},
			TestCase {
				description: "size larger than remaining",
				offset: 10,
				size: full_code.len(),
				expected: {
					let mut expected = vec![0u8; full_code.len()];
					expected[..full_code.len() - 10].copy_from_slice(&full_code[10..]);
					expected
				},
			},
		];

		for test_case in test_cases {
			let result = builder::bare_call(addr)
				.data(
					HostEvmOnlyCalls::extcodecopyOp(HostEvmOnly::extcodecopyOpCall {
						account: dummy_addr.0.into(),
						offset: U256::from(test_case.offset),
						size: U256::from(test_case.size),
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();

			assert!(!result.did_revert(), "test reverted for: {}", test_case.description);

			let return_value = HostEvmOnly::extcodecopyOpCall::abi_decode_returns(&result.data)
				.expect("Failed to decode extcodecopyOp return value");
			let actual_code = &return_value.0;

			assert_eq!(
				&test_case.expected, actual_code,
				"EXTCODECOPY content mismatch for {}",
				test_case.description
			);
		}
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
fn sload_error_reading_non_32_byte_value() {
	let (code, _) = compile_module_with_type("Host", FixtureType::Solc).unwrap();

	let index = U256::from(13);
	let expected_value = U256::from(17);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		{
			// Test that reading storage value of 31 bytes results in contract trapped
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(index.to_be_bytes());
			contract_info
				.write(&key, Some(expected_value.to_be_bytes::<32>()[..31].to_vec()), None, false)
				.unwrap();

			let result = builder::bare_call(addr)
				.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode())
				.build();
			assert!(result.result.is_err(), "test should error");
			let err = result.result.unwrap_err();
			let sp_runtime::DispatchError::Module(module_err) = &err else {
				panic!("expected Module error (ContractTrapped), got {:?}", err)
			};
			assert_eq!(module_err.message, Some("ContractTrapped"));
		}

		{
			// Test that reading storage value of 33 bytes results in contract trapped
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(index.to_be_bytes());
			let mut bytes = expected_value.to_be_bytes::<32>().to_vec();
			bytes.push(0u8);
			contract_info.write(&key, Some(bytes), None, false).unwrap();

			let result = builder::bare_call(addr)
				.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode())
				.build();
			assert!(result.result.is_err(), "test should error");
			let err = result.result.unwrap_err();
			let sp_runtime::DispatchError::Module(module_err) = &err else {
				panic!("expected Module error (ContractTrapped), got {:?}", err)
			};
			assert_eq!(module_err.message, Some("ContractTrapped"));
		}
	});
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
fn logs_work() {
	use crate::tests::initialize_block;
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// Drop previous events
			initialize_block(2);

			let result = builder::bare_call(addr)
				.data(Host::HostCalls::logOps(Host::logOpsCall {}).abi_encode())
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");

			let events = System::<Test>::events();
			assert_eq!(
				events,
				vec![
					frame_system::EventRecord {
						phase: frame_system::Phase::Initialization,
						event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
							contract: addr,
							data: vec![0u8; 32],
							topics: vec![],
						}),
						topics: vec![],
					},
					frame_system::EventRecord {
						phase: frame_system::Phase::Initialization,
						event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
							contract: addr,
							data: vec![0u8; 32],
							topics: vec![H256::from_low_u64_be(0x11)],
						}),
						topics: vec![],
					},
					frame_system::EventRecord {
						phase: frame_system::Phase::Initialization,
						event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
							contract: addr,
							data: vec![0u8; 32],
							topics: vec![H256::from_low_u64_be(0x22), H256::from_low_u64_be(0x33)],
						}),
						topics: vec![],
					},
					frame_system::EventRecord {
						phase: frame_system::Phase::Initialization,
						event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
							contract: addr,
							data: vec![0u8; 32],
							topics: vec![
								H256::from_low_u64_be(0x44),
								H256::from_low_u64_be(0x55),
								H256::from_low_u64_be(0x66)
							],
						}),
						topics: vec![],
					},
					frame_system::EventRecord {
						phase: frame_system::Phase::Initialization,
						event: RuntimeEvent::Contracts(crate::Event::ContractEmitted {
							contract: addr,
							data: vec![0u8; 32],
							topics: vec![
								H256::from_low_u64_be(0x77),
								H256::from_low_u64_be(0x88),
								H256::from_low_u64_be(0x99),
								H256::from_low_u64_be(0xaa)
							],
						}),
						topics: vec![],
					},
				]
			);
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
fn logs_denied_for_static_call() {
	use pallet_revive_fixtures::Caller;
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
		let (host_code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Deploy Host contract
			let Contract { addr: host_addr, .. } =
				builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

			// Deploy Caller contract
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

			// Use staticcall from Caller to Host's logOps function
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::CallerCalls::staticCall(Caller::staticCallCall {
						_callee: host_addr.0.into(),
						_data: Host::HostCalls::logOps(Host::logOpsCall {}).abi_encode().into(),
						_gas: U256::MAX,
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let decoded_result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();

			assert_eq!(decoded_result.success, false);
		});
	}
}

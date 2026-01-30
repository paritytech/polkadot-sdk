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
	exec::EMPTY_CODE_HASH,
	metering::TransactionLimits,
	storage::AccountInfo,
	test_utils::{builder::Contract, ALICE, BOB, BOB_ADDR},
	tests::{
		builder, dummy_evm_contract, test_utils, test_utils::get_contract, Contracts, ExtBuilder,
		RuntimeEvent, Test, TestSigner,
	},
	Code, Config, Error, Key, PristineCode, System, H256, U256,
};
use frame_support::{assert_err_ignore_postinfo, assert_ok};

use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::traits::{fungible::Mutate, Get};
use pallet_revive_fixtures::{compile_module_with_type, Caller, FixtureType, Host};
use pretty_assertions::assert_eq;
use sp_core::H160;
use test_case::test_case;

fn convert_to_free_balance(total_balance: u128) -> U256 {
	let existential_deposit_planck =
		<Test as pallet_balances::Config>::ExistentialDeposit::get() as u128;
	let native_to_eth = <<Test as Config>::NativeToEthRatio as Get<u32>>::get() as u128;
	U256::from((total_balance - existential_deposit_planck) * native_to_eth)
}

/// Create a delegated EOA that points to the given target contract
fn create_delegated_eoa(target: &H160) -> H160 {
	let chain_id = U256::from(<Test as Config>::ChainId::get());
	let seed = H256::random();
	let signer = TestSigner::new(&seed.0);
	let authority = signer.address;

	let authority_id = <Test as Config>::AddressMapper::to_account_id(&authority);
	let _ = <Test as Config>::Currency::set_balance(&authority_id, 100_000_000);

	let nonce = U256::from(frame_system::Pallet::<Test>::account_nonce(&authority_id));
	let auth = signer.sign_authorization(chain_id, *target, nonce);

	// Process the authorization to set up delegation
	let result = builder::eth_call_with_authorization_list(*target)
		.authorization_list(vec![auth])
		.eth_gas_limit(1_000_000u64.into())
		.build();
	assert_ok!(result);

	assert!(AccountInfo::<Test>::is_delegated(&authority));
	authority
}

/// Create a funded EOA (not delegated, not a contract)
fn create_funded_eoa() -> H160 {
	let seed = H256::random();
	let address = H160::from_slice(&sp_io::hashing::keccak_256(&seed.0)[12..]);
	let account_id = <Test as Config>::AddressMapper::to_account_id(&address);
	let _ = <Test as Config>::Currency::set_balance(&account_id, 100_000_000);
	address
}

/// Call EXTCODEHASH opcode via the Host contract
fn call_extcodehash(host: &H160, target: &H160) -> H256 {
	let result = builder::bare_call(*host)
		.data(
			Host::HostCalls::extcodehashOp(Host::extcodehashOpCall { account: target.0.into() })
				.abi_encode(),
		)
		.build_and_unwrap_result();
	assert!(!result.did_revert(), "extcodehash call reverted");
	let decoded = Host::extcodehashOpCall::abi_decode_returns(&result.data).unwrap();
	H256::from_slice(decoded.as_slice())
}

/// Call EXTCODESIZE opcode via the Host contract
fn call_extcodesize(host: &H160, target: &H160) -> u64 {
	let result = builder::bare_call(*host)
		.data(
			Host::HostCalls::extcodesizeOp(Host::extcodesizeOpCall { account: target.0.into() })
				.abi_encode(),
		)
		.build_and_unwrap_result();
	assert!(!result.did_revert(), "extcodesize call reverted");
	Host::extcodesizeOpCall::abi_decode_returns(&result.data).unwrap()
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn balance_works(fixture_type: FixtureType) {
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
			let decoded = Host::balanceCall::abi_decode_returns(&result.data).unwrap();

			assert_eq!(
				expected_balance.as_u64(),
				decoded,
				"BALANCE should return BOB's balance for {fixture_type:?}",
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn selfbalance_works(fixture_type: FixtureType) {
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
			let decoded = Host::selfbalanceCall::abi_decode_returns(&result.data).unwrap();

			assert_eq!(
				expected_balance.as_u64(),
				decoded,
				"BALANCE should return contract's balance for {fixture_type:?}",
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn extcodesize_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Deploy the Host contract (used to call EXTCODESIZE)
		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Deploy a target contract for delegation tests
		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		let host_code_size = {
			let info = test_utils::get_contract(&host_addr);
			test_utils::ensure_stored(info.code_hash) as u64
		};

		let target_code_size = {
			let info = test_utils::get_contract(&target_addr);
			test_utils::ensure_stored(info.code_hash) as u64
		};

		// Test case 1: Regular contract - returns its own code size
		{
			let result = call_extcodesize(&host_addr, &host_addr);
			assert_eq!(
				result, host_code_size,
				"EXTCODESIZE for regular contract should return its code size"
			);
		}

		// Test case 2: Delegated EOA - returns target's code size (EIP-7702)
		{
			let delegated_eoa = create_delegated_eoa(&target_addr);
			let result = call_extcodesize(&host_addr, &delegated_eoa);
			assert_eq!(
				result, target_code_size,
				"EXTCODESIZE for delegated EOA should return target's code size"
			);
		}

		// Test case 3: Regular EOA - returns 0
		{
			let eoa = create_funded_eoa();
			let result = call_extcodesize(&host_addr, &eoa);
			assert_eq!(result, 0, "EXTCODESIZE for regular EOA should return 0");
		}

		// Test case 4: Non-existent address - returns 0
		{
			let result = call_extcodesize(&host_addr, &H160::from_low_u64_be(0xdead));
			assert_eq!(result, 0, "EXTCODESIZE for non-existent address should return 0");
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn extcodehash_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Deploy the Host contract (used to call EXTCODEHASH)
		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Deploy a target contract for delegation tests
		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		let host_code_hash = test_utils::get_contract(&host_addr).code_hash;
		let target_code_hash = test_utils::get_contract(&target_addr).code_hash;

		// Test case 1: Regular contract - returns its own code hash
		{
			let result = call_extcodehash(&host_addr, &host_addr);
			assert_eq!(
				result, host_code_hash,
				"EXTCODEHASH for regular contract should return its code hash"
			);
		}

		// Test case 2: Delegated EOA - returns target's code hash (EIP-7702)
		{
			let delegated_eoa = create_delegated_eoa(&target_addr);
			let result = call_extcodehash(&host_addr, &delegated_eoa);
			assert_eq!(
				result, target_code_hash,
				"EXTCODEHASH for delegated EOA should return target's code hash"
			);
		}

		// Test case 3: Regular EOA - returns EMPTY_CODE_HASH
		{
			let eoa = create_funded_eoa();
			let result = call_extcodehash(&host_addr, &eoa);
			assert_eq!(
				result, EMPTY_CODE_HASH,
				"EXTCODEHASH for regular EOA should return EMPTY_CODE_HASH"
			);
		}

		// Test case 4: Non-existent address - returns zero
		{
			let result = call_extcodehash(&host_addr, &H160::from_low_u64_be(0xdead));
			assert_eq!(
				result,
				H256::zero(),
				"EXTCODEHASH for non-existent address should return zero"
			);
		}
	});
}

/// Test Pallet::code() behavior for different account types
#[test]
fn pallet_code_works() {
	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Deploy a target contract
		let Contract { addr: contract_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		// Test case 1: Regular contract - returns actual code
		{
			let code = Contracts::code(&contract_addr);
			assert!(!code.is_empty(), "Pallet::code for contract should return code");
			// The code should be the pristine code stored
			let expected =
				PristineCode::<Test>::get(test_utils::get_contract(&contract_addr).code_hash)
					.unwrap();
			assert_eq!(code, expected, "Pallet::code for contract should return pristine code");
		}

		// Test case 2: Delegated EOA - returns delegation indicator (0xef0100 || target)
		{
			let delegated_eoa = create_delegated_eoa(&contract_addr);
			let code = Contracts::code(&delegated_eoa);

			// EIP-7702: delegation indicator is 0xef0100 followed by the target address
			assert_eq!(code.len(), 23, "Delegation indicator should be 23 bytes (3 + 20)");
			assert_eq!(
				&code[0..3],
				&[0xef, 0x01, 0x00],
				"Delegation indicator should start with 0xef0100"
			);
			assert_eq!(
				&code[3..23],
				contract_addr.as_bytes(),
				"Delegation indicator should contain target address"
			);
		}

		// Test case 3: Regular EOA - returns empty
		{
			let eoa = create_funded_eoa();
			let code = Contracts::code(&eoa);
			assert!(code.is_empty(), "Pallet::code for regular EOA should return empty");
		}

		// Test case 4: Non-existent address - returns empty
		{
			let code = Contracts::code(&H160::from_low_u64_be(0xdead));
			assert!(code.is_empty(), "Pallet::code for non-existent address should return empty");
		}
	});
}

/// EXTCODECOPY does not exist in PVM so we only test Solc caller contract.
#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
fn extcodecopy_works(caller_type: FixtureType, callee_type: FixtureType) {
	use pallet_revive_fixtures::{HostEvmOnly, HostEvmOnly::HostEvmOnlyCalls};

	let (caller_code, _) = compile_module_with_type("HostEvmOnly", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Host", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		let Contract { addr: dummy_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code.clone()))
				.build_and_unwrap_contract();

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
						offset: test_case.offset as u64,
						size: test_case.size as u64,
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn blockhash_works(fixture_type: FixtureType) {
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
				&crate::BlockNumberFor::<Test>::from(block_number_to_test),
				<Test as frame_system::Config>::Hash::from(&block_hash),
			);
			let result = builder::bare_call(addr)
				.data(
					Host::HostCalls::blockhashOp(Host::blockhashOpCall {
						blockNumber: block_number_to_test,
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");

			let decoded = Host::blockhashOpCall::abi_decode_returns(&result.data).unwrap();
			let expected_block_hash = System::<Test>::block_hash(block_number_to_test);

			assert_eq!(
				expected_block_hash,
				H256::from_slice(decoded.as_slice()),
				"EXTBLOCKHASH should return the block hash for {fixture_type:?}",
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn sload_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	let index = 13u64;
	let expected_value = 17u64;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		{
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(U256::from(index).to_big_endian());
			contract_info
				.write(&key, Some(U256::from(expected_value).to_big_endian().to_vec()), None, false)
				.unwrap();
		}

		{
			let result = builder::bare_call(addr)
				.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode())
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");
			let decoded = Host::sloadOpCall::abi_decode_returns(&result.data).unwrap();

			assert_eq!(
				expected_value, decoded,
				"result should return expected value {fixture_type:?}",
			);
		}
	});
}

#[test]
fn sload_error_reading_non_32_byte_value() {
	let (code, _) = compile_module_with_type("Host", FixtureType::Solc).unwrap();

	let index = 13u64;
	let expected_value = U256::from(17);

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		{
			// Test that reading storage value of 31 bytes results in contract trapped
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(U256::from(index).to_big_endian());
			contract_info
				.write(&key, Some(expected_value.to_big_endian()[..31].to_vec()), None, false)
				.unwrap();

			assert_err_ignore_postinfo!(
				builder::call(addr)
					.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode(),)
					.build(),
				Error::<Test>::ContractTrapped
			);
		}

		{
			// Test that reading storage value of 33 bytes results in contract trapped
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(U256::from(index).to_big_endian());
			let mut bytes = expected_value.to_big_endian().to_vec();
			bytes.push(0u8);
			contract_info.write(&key, Some(bytes), None, false).unwrap();

			assert_err_ignore_postinfo!(
				builder::call(addr)
					.data(Host::HostCalls::sloadOp(Host::sloadOpCall { slot: index }).abi_encode())
					.build(),
				Error::<Test>::ContractTrapped
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn sstore_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let index = 13u64;
		let expected_value = 17u64;
		let unexpected_value = 19u64;

		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		{
			let contract_info = test_utils::get_contract(&addr);
			let key = Key::Fix(U256::from(index).to_big_endian());
			contract_info
				.write(
					&key,
					Some(U256::from(unexpected_value).to_big_endian().to_vec()),
					None,
					false,
				)
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
				let key = Key::Fix(U256::from(index).to_big_endian());
				let result = contract_info.read(&key).unwrap();
				U256::from_big_endian(&result)
			};
			assert_eq!(
				U256::from(expected_value),
				written_value,
				"result should return expected value {:?}",
				fixture_type
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn logs_work(fixture_type: FixtureType) {
	use crate::tests::initialize_block;
	let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Drop previous events
		initialize_block(2);

		let result = builder::bare_call(addr)
			.transaction_limits(TransactionLimits::WeightAndDeposit {
				weight_limit: crate::Weight::from_parts(100_000_000_000_000, 50 * 1024 * 1024),
				deposit_limit: Default::default(),
			})
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn transient_storage_works(fixture_type: FixtureType) {
	use pallet_revive_fixtures::HostTransientMemory;
	let (code, _) = compile_module_with_type("HostTransientMemory", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let slot = 0u64;
		let value = 13u64;

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
		let decoded =
			HostTransientMemory::transientMemoryTestCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(0u64, decoded, "transient storage should return zero for {fixture_type:?}");
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn logs_denied_for_static_call(caller_type: FixtureType, callee_type: FixtureType) {
	use pallet_revive_fixtures::Caller;
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (host_code, _) = compile_module_with_type("Host", callee_type).unwrap();

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
					_gas: u64::MAX,
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let decoded_result = Caller::staticCallCall::abi_decode_returns(&result.data).unwrap();

		assert_eq!(decoded_result.success, false);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn reading_empty_storage_item_returns_zero_simple(fixture_type: FixtureType) {
	let (host_code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

		let index = 13u64;

		// read non-existent storage item
		let result = builder::bare_call(host_addr)
			.data(Host::sloadOpCall { slot: index }.abi_encode())
			.build_and_unwrap_result();

		let decoded = Host::sloadOpCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(decoded, 0u64, "sloadOpCall should return zero for empty storage item");
	})
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn reading_empty_storage_item_returns_zero_delegatecall(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (host_code, _) = compile_module_with_type("Host", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let index = 13u64;

		// read non-existent storage item
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: host_addr.0.into(),
					_data: Host::sloadOpCall { slot: index }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "delegateCall reverted");
		let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();

		assert!(result.success, "sloadOpCall did not succeed");
		let decoded = Host::sloadOpCall::abi_decode_returns(&result.output).unwrap();
		assert_eq!(decoded, 0u64, "sloadOpCall should return zero for empty storage item");
	})
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn storage_item_zero_shall_refund_deposit_simple(fixture_type: FixtureType) {
	let (host_code, _) = compile_module_with_type("Host", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

		let index = 13u64;

		let base_deposit = get_contract(&host_addr).total_deposit();

		// write storage item
		let result = builder::bare_call(host_addr)
			.data(Host::sstoreOpCall { slot: index, value: 17u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "sstoreOpCall reverted");

		// 32 for key, 32 for value, 2 for item
		assert_eq!(
			get_contract(&host_addr).total_deposit(),
			base_deposit + 32 + 32 + 2,
			"Unexpected deposit sum charged for non-zero storage item"
		);

		// write storage item to all zeros
		let result = builder::bare_call(host_addr)
			.data(Host::sstoreOpCall { slot: index, value: 0u64 }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "sstoreOpCall reverted");

		assert_eq!(
			get_contract(&host_addr).total_deposit(),
			base_deposit,
			"contract should refund deposit on zeroing storage item"
		);
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn storage_item_zero_shall_refund_deposit_delegatecall(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (host_code, _) = compile_module_with_type("Host", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: host_addr, .. } =
			builder::bare_instantiate(Code::Upload(host_code)).build_and_unwrap_contract();

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		let index = 13u64;

		let base_deposit = get_contract(&caller_addr).total_deposit();

		// write storage item
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: host_addr.0.into(),
					_data: Host::sstoreOpCall { slot: index, value: 17u64 }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "delegateCall did not succeed");

		// 32 for key, 32 for value, 2 for item
		assert_eq!(
			get_contract(&caller_addr).total_deposit(),
			base_deposit + 32 + 32 + 2,
			"Unexpected deposit sum charged for non-zero storage item"
		);

		// write storage item to all zeros
		let result = builder::bare_call(caller_addr)
			.data(
				Caller::delegateCall {
					_callee: host_addr.0.into(),
					_data: Host::sstoreOpCall { slot: index, value: 0u64 }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = Caller::delegateCall::abi_decode_returns(&result.data).unwrap();
		assert!(result.success, "delegateCall did not succeed");

		assert_eq!(
			get_contract(&caller_addr).total_deposit(),
			base_deposit,
			"contract should refund deposit on zeroing storage item"
		);
	});
}

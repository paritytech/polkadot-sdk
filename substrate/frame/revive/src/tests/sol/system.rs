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
	evm::fees::InfoT,
	precompiles::alloy::sol_types::{sol_data::Bool, SolType},
	test_utils::{builder::Contract, deposit_limit, ALICE, ALICE_ADDR, WEIGHT_LIMIT},
	tests::{builder, Contracts, ExtBuilder, Test},
	Code, Config, ExecConfig, TransactionLimits, TransactionMeter, U256,
};

use alloy_core::sol_types::{Revert, SolCall, SolConstructor, SolError};
use frame_support::traits::fungible::{Balanced, Mutate};
use pallet_revive_fixtures::{
	compile_module_with_type, Callee, FixtureType, System as SystemFixture,
};
use pretty_assertions::assert_eq;
use revm::primitives::Bytes;
use sp_core::H160;
use sp_io::hashing::keccak_256;
use test_case::test_case;

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn keccak_256_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let pre = b"revive";
		let expected = keccak_256(pre);

		let result = builder::bare_call(addr)
			.data(SystemFixture::keccak256FuncCall { data: Bytes::from(pre) }.abi_encode())
			.build_and_unwrap_result();

		assert_eq!(&expected, result.data.as_slice());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn address_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::addressFuncCall {}.abi_encode())
			.build_and_unwrap_result();

		let decoded = SystemFixture::addressFuncCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(addr, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn caller_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::callerCall {}.abi_encode())
			.build_and_unwrap_result();

		let decoded = SystemFixture::callerCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(ALICE_ADDR, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn callvalue_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let value = 1337u64;

		let result = builder::bare_call(addr)
			.evm_value(value.into())
			.data(SystemFixture::callvalueCall {}.abi_encode())
			.build_and_unwrap_result();

		let decoded = SystemFixture::callvalueCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(value, decoded);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldataload_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::calldataloadCall { offset: 4u64 }.abi_encode())
			.build_and_unwrap_result();

		// Call calldataload(offset=4) → returns the argument "4"
		let decoded = SystemFixture::calldataloadCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(U256::from(4u32), U256::from_big_endian(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldatasize_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		// calldata = selector + encoded argument
		let result = builder::bare_call(addr)
			.data(SystemFixture::calldatasizeCall {}.abi_encode())
			.build_and_unwrap_result();

		// ABI encodes: 4 (selector) + 0 (no args) = 4
		let decoded = SystemFixture::calldatasizeCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(4u64, decoded);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldatacopy_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let call_data = SystemFixture::calldatacopyCall {
			destOffset: 0u64, // unused
			offset: 4u64,     // skip selector
			size: 64u64,      // copy destOffset + offset
		}
		.abi_encode();

		let result = builder::bare_call(addr).data(call_data.clone()).build_and_unwrap_result();

		let returned_data =
			SystemFixture::calldatacopyCall::abi_decode_returns(&result.data).unwrap();

		let returned_data = returned_data.as_ref();
		assert_eq!(returned_data.len(), 64);

		// The expected data is the slice of the original calldata that was copied.
		let expected_data = &call_data.as_slice()[4..(4 + 64) as usize];
		assert_eq!(expected_data, returned_data);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn codesize_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::codesizeCall {}.abi_encode())
			.build_and_unwrap_result();

		// Now fetch the actual *runtime* code size from storage
		let code = Contracts::code(&addr);

		let decoded = SystemFixture::codesizeCall::abi_decode_returns(&result.data).unwrap();
		let expected_size = code.len() as u64;

		assert_eq!(expected_size, decoded);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn gas_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		// enable txhold collection which we expect to be on when using the evm backend
		let limits = TransactionLimits::WeightAndDeposit {
			weight_limit: WEIGHT_LIMIT,
			deposit_limit: deposit_limit::<Test>(),
		};
		let hold_initial =
			TransactionMeter::<Test>::new(limits.clone()).unwrap().eth_gas_left().unwrap();

		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(hold_initial));
		let mut exec_config = ExecConfig::new_substrate_tx();
		exec_config.collect_deposit_from_hold = Some((0u32.into(), Default::default()));

		let result = builder::bare_call(addr)
			.data(SystemFixture::gasCall {}.abi_encode())
			.exec_config(exec_config)
			.transaction_limits(limits)
			.build_and_unwrap_result();

		let gas_left: u64 = SystemFixture::gasCall::abi_decode_returns(&result.data)
			.unwrap()
			.try_into()
			.unwrap();

		assert!(gas_left > 0);
		assert!(gas_left < hold_initial);
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn returndatasize_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let magic_number = 42u64;
		let result = builder::bare_call(addr)
			.data(
				SystemFixture::returndatasizeCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let decoded = SystemFixture::returndatasizeCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(decoded, 32);
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn returndatacopy_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: false }.abi_encode())
			.build_and_unwrap_contract();

		let magic_number = U256::from(42);
		let result = builder::bare_call(addr)
			.data(
				SystemFixture::returndatacopyCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: 42u64 }.abi_encode().into(),
					_gas: u64::MAX,
					destOffset: 0u64,
					offset: 0u64,
					size: 32u64,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let decoded = SystemFixture::returndatacopyCall::abi_decode_returns(&result.data).unwrap();
		let decoded_value = U256::from_big_endian(decoded.as_ref());
		assert_eq!(magic_number, decoded_value)
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn constructor_with_argument_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let result = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(SystemFixture::constructorCall { panic: true }.abi_encode())
			.build()
			.result
			.unwrap()
			.result;
		assert!(result.did_revert());

		let expected_message = "Reverted because revert=true was set as constructor argument";
		assert_eq!(result.data, Revert::from(expected_message).abi_encode());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn sr25519_verify(fixture_type: FixtureType) {
	use pallet_revive_fixtures::Sr25519Verify;
	let (binary, _) = compile_module_with_type("Sr25519Verify", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the first contract
		let Contract { addr: contract_addr, .. } =
			builder::bare_instantiate(Code::Upload(binary)).build_and_unwrap_contract();

		let call_with = |message: &[u8; 11]| {
			// Alice's signature for "hello world"
			#[rustfmt::skip]
			let signature: [u8; 64] = [
				184, 49, 74, 238, 78, 165, 102, 252, 22, 92, 156, 176, 124, 118, 168, 116, 247,
				99, 0, 94, 2, 45, 9, 170, 73, 222, 182, 74, 60, 32, 75, 64, 98, 174, 69, 55, 83,
				85, 180, 98, 208, 75, 231, 57, 205, 62, 4, 105, 26, 136, 172, 17, 123, 99, 90, 255,
				228, 54, 115, 63, 30, 207, 205, 131,
			];

			// Alice's public key
			#[rustfmt::skip]
			let public_key: [u8; 32] = [
				212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44,
				133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125,
			];

			let result = builder::bare_call(contract_addr)
				.data(
					Sr25519Verify::verifyCall {
						signature: signature.into(),
						message: (*message).into(),
						publicKey: public_key.into(),
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert());
			result
		};
		let result = Bool::abi_decode(&call_with(&b"hello world").data).expect("decoding failed");
		assert!(result);
		let result = Bool::abi_decode(&call_with(&b"hello worlD").data).expect("decoding failed");
		assert!(!result);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn ecdsa_to_eth_address(fixture_type: FixtureType) {
	use pallet_revive_fixtures::EcdsaToEthAddress;
	let (binary, _) = compile_module_with_type("EcdsaToEthAddress", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: contract_addr, .. } =
			builder::bare_instantiate(Code::Upload(binary)).build_and_unwrap_contract();

		let pubkey_compressed = array_bytes::hex2array_unchecked(
			"028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd91",
		);

		let result = builder::bare_call(contract_addr)
			.data(EcdsaToEthAddress::convertCall { publicKey: pubkey_compressed }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(
			result.data[..20],
			array_bytes::hex2array_unchecked::<_, 20>("09231da7b19A016f9e576d23B16277062F4d46A8")
		);
	});
}

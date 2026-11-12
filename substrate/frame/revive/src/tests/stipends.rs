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
	Code, Config,
	test_utils::builder::Contract,
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	FixtureType, SendAndTransferTest, StipendTest, compile_module_with_type,
};
use test_case::test_case;

#[test]
fn evm_call_stipends_work_for_transfers() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_sends() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_transfer_zero() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferZeroCall {}.abi_encode())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_send_zero() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendZeroCall {}.abi_encode())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test_case(FixtureType::Solc, "BalanceReader", false)]
#[test_case(FixtureType::Resolc, "BalanceReader", false)]
#[test_case(FixtureType::Solc, "BalanceOfReader", false)]
#[test_case(FixtureType::Resolc, "BalanceOfReader", false)]
#[test_case(FixtureType::Solc, "SenderBalanceReader", true)]
#[test_case(FixtureType::Resolc, "SenderBalanceReader", true)]
#[test_case(FixtureType::Solc, "SimpleReceiver", false)]
#[test_case(FixtureType::Solc, "TwoEventsReceiver", true)]
#[test_case(FixtureType::Resolc, "TwoEventsReceiver", true)]
fn call_stipends_cover_own_balance_reads_and_evm_events(
	fixture_type: FixtureType,
	receiver_name: &str,
	reverts: bool,
) {
	let (receiver_code, _) = compile_module_with_type(receiver_name, fixture_type).unwrap();
	let (test_code, _) = compile_module_with_type("SendAndTransferTest", fixture_type).unwrap();
	// Half of it moves one native unit plus dust.
	let value = 2_000_002_u128;

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr, account_id } =
			builder::bare_instantiate(Code::Upload(test_code)).build_and_unwrap_contract();
		// Keeps the sender's balance above 0, so `SenderBalanceReader` fails only because of the
		// cost of the balance read.
		<Test as Config>::Currency::mint_into(&account_id, 10_000_000_000_000).unwrap();

		let result = builder::bare_call(addr)
			.data(
				SendAndTransferTest::testSendAndTransferCall { to: receiver.0.into() }.abi_encode(),
			)
			.evm_value(value.into())
			.build_and_unwrap_result();

		assert_eq!(result.did_revert(), reverts);
	});
}

/// `send` and `transfer` reach a PVM receiver of every code size the pallet accepts.
///
/// The receiver runs on the stipend alone. Its `call` export returns at once, so only the size
/// of its code section varies, up to [`crate::call_builder::VmBinaryModule::MAX_CODE_BYTES`].
#[cfg(feature = "runtime-benchmarks")]
#[test_case(FixtureType::Solc, 1024)]
#[test_case(FixtureType::Solc, 32 * 1024)]
#[test_case(FixtureType::Solc, 64 * 1024)]
#[test_case(FixtureType::Solc, 96 * 1024)]
#[test_case(FixtureType::Solc, 128 * 1024)]
#[test_case(FixtureType::Solc, 256 * 1024)]
#[test_case(FixtureType::Solc, crate::call_builder::VmBinaryModule::MAX_CODE_BYTES)]
#[test_case(FixtureType::Resolc, 1024)]
#[test_case(FixtureType::Resolc, 32 * 1024)]
#[test_case(FixtureType::Resolc, 64 * 1024)]
#[test_case(FixtureType::Resolc, 96 * 1024)]
#[test_case(FixtureType::Resolc, 128 * 1024)]
#[test_case(FixtureType::Resolc, 256 * 1024)]
#[test_case(FixtureType::Resolc, crate::call_builder::VmBinaryModule::MAX_CODE_BYTES)]
fn call_stipends_cover_pvm_receivers_of_any_code_size(fixture_type: FixtureType, code_bytes: u32) {
	let receiver_code = crate::call_builder::VmBinaryModule::with_code_bytes(code_bytes).code;
	let (test_code, _) = compile_module_with_type("SendAndTransferTest", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(test_code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				SendAndTransferTest::testSendAndTransferCall { to: receiver.0.into() }.abi_encode(),
			)
			.evm_value(2_000_002_u128.into())
			.build_and_unwrap_result();

		assert!(!result.did_revert(), "{:?}", crate::evm::decode_revert_reason(&result.data));
	});
}

#[test]
fn evm_call_stipends_work_for_calls() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testCallCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipend_prevents_transfer_reentrancy() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferReentrancyCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipend_prevents_send_reentrancy() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendReentrancyCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

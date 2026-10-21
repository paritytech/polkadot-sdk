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
use pallet_revive_fixtures::{BalanceReadTest, FixtureType, StipendTest, compile_module_with_type};
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn evm_call_stipends_work_for_balance_reads(fixture_type: FixtureType) {
	let (receiver_code, _) = compile_module_with_type("BalanceReader", fixture_type).unwrap();
	let (code, _) = compile_module_with_type("BalanceReadTest", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(BalanceReadTest::testBalanceReadCall { to: receiver.0.into() }.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
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

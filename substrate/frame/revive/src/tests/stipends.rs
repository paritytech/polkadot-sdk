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
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::{SolCall, SolConstructor, SolValue};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{FixtureType, StipendSender, StipendTest, compile_module_with_type};
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

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn evm_call_stipend_denies_reentrancy_for_transfer_and_send_only(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("StipendSender", fixture_type).unwrap();
	let (probe_code, _) = compile_module_with_type("ReentrancyProbe", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: probe, .. } =
			builder::bare_instantiate(Code::Upload(probe_code)).build_and_unwrap_contract();
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				StipendSender::constructorCall { _probe: probe.0.into() }.abi_encode(),
			)
			.build_and_unwrap_contract();
		let run = |call: Vec<u8>| {
			let result = builder::bare_call(addr)
				.data(call)
				.evm_value(1_000_000_u128.into())
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "the call into StipendSender should not revert");
			bool::abi_decode(&result.data).unwrap()
		};

		assert!(
			run(StipendSender::isTransferDeniedCall {}.abi_encode()),
			"transfer forwards only the stipend, so its callee must not reenter"
		);
		assert!(
			run(StipendSender::isSendDeniedCall {}.abi_encode()),
			"send forwards only the stipend, so its callee must not reenter"
		);
		assert_eq!(
			run(StipendSender::isCallWithOneGasDeniedCall {}.abi_encode()),
			fixture_type == FixtureType::Resolc,
			"a caller that sets its own gas limit is never guarded, so only cost can deny it: \
			 the stipend reaches a reentry on the EVM but not on PVM, where the hop back pays \
			 another call base and code load"
		);
		assert!(
			run(StipendSender::isSelfSendAllowedCall {}.abi_encode()),
			"the guard stops the callee reentering, not a sender sending to itself"
		);
	});
}

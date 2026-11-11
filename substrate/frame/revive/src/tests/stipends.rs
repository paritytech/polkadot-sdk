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
	Code, Config, EthTxInfo, ExecReturnValue, H160, U256, Weight,
	evm::{CallTracer, fees::InfoT},
	test_utils::{ETH_GAS_LIMIT, builder::Contract},
	tests::{ExtBuilder, Test, builder, sol::eth_extrinsic_revert},
	tracing::trace,
};
use alloy_core::sol_types::SolCall;
use frame_support::{
	assert_ok,
	traits::fungible::{Balanced, Mutate},
};
use pallet_revive_fixtures::{
	FixtureType, SendAndTransferTest, StipendTest, compile_module_with_type,
};
use pallet_revive_uapi::ReturnFlags;
use test_case::test_case;

/// Calls `dest` with weight and deposit limits, configured like the `call` dispatchable.
fn run_call_in_substrate_tx(dest: H160, data: Vec<u8>, value: U256) -> ExecReturnValue {
	builder::bare_call(dest)
		.data(data)
		.evm_value(value)
		.exec_config(crate::Pallet::<Test>::substrate_call_exec_config())
		.build_and_unwrap_result()
}

/// Dispatches `eth_call` to `dest`.
fn run_call_in_eth_tx(dest: H160, data: Vec<u8>, value: U256) -> ExecReturnValue {
	// A real transaction fills this pot from its gas before `eth_call` runs.
	<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
		EthTxInfo::<Test>::new(0, Weight::zero()).max_deposit(ETH_GAS_LIMIT.into()),
	));
	let mut tracer = CallTracer::new(Default::default());
	trace(&mut tracer, || {
		assert_ok!(builder::eth_call(dest).data(data).value(value).build());
	});
	// A failed `eth_call` still dispatches successfully, and reports its failure as an event.
	let flags =
		if eth_extrinsic_revert().is_some() { ReturnFlags::REVERT } else { ReturnFlags::empty() };
	let output = tracer.collect_trace().expect("the transaction runs the call").output;
	ExecReturnValue { flags, data: output.0 }
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx; "eth tx")]
fn evm_call_stipends_work_for_transfers(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result =
			run_call(addr, StipendTest::testTransferCall {}.abi_encode(), 1_000_000_u128.into());

		assert!(!result.did_revert());
	});
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx; "eth tx")]
fn evm_call_stipends_work_for_sends(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result =
			run_call(addr, StipendTest::testSendCall {}.abi_encode(), 1_000_000_u128.into());

		assert!(!result.did_revert());
	});
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx; "eth tx")]
fn evm_call_stipends_work_for_transfer_zero(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result =
			run_call(addr, StipendTest::testTransferZeroCall {}.abi_encode(), U256::zero());

		assert!(!result.did_revert());
	});
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx; "eth tx")]
fn evm_call_stipends_work_for_send_zero(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = run_call(addr, StipendTest::testSendZeroCall {}.abi_encode(), U256::zero());

		assert!(!result.did_revert());
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

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx; "eth tx")]
fn evm_call_stipends_work_for_calls(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result =
			run_call(addr, StipendTest::testCallCall {}.abi_encode(), 1_000_000_u128.into());

		assert!(!result.did_revert());
	});
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx => ignore["fails until #13263 is merged"]; "eth tx")]
fn evm_call_stipend_prevents_transfer_reentrancy(
	run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue,
) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = run_call(
			addr,
			StipendTest::testTransferReentrancyCall {}.abi_encode(),
			1_000_000_u128.into(),
		);

		assert!(!result.did_revert());
	});
}

#[test_case(run_call_in_substrate_tx; "substrate tx")]
#[test_case(run_call_in_eth_tx => ignore["fails until #13263 is merged"]; "eth tx")]
fn evm_call_stipend_prevents_send_reentrancy(run_call: fn(H160, Vec<u8>, U256) -> ExecReturnValue) {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = run_call(
			addr,
			StipendTest::testSendReentrancyCall {}.abi_encode(),
			1_000_000_u128.into(),
		);

		assert!(!result.did_revert());
	});
}

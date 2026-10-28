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
	Code, Config, EthTxInfo, ExecReturnValue, H160, TransactionLimits, U256, Weight,
	evm::fees::InfoT,
	test_utils::{ETH_GAS_LIMIT, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::{Balanced, Mutate};
use pallet_revive_fixtures::{FixtureType, StipendTest, compile_module_with_type};
use test_case::test_case;

/// Calls `dest` with weight and deposit limits, configured like the `call` dispatchable.
fn run_call_in_substrate_tx(dest: H160, data: Vec<u8>, value: U256) -> ExecReturnValue {
	builder::bare_call(dest)
		.data(data)
		.evm_value(value)
		.exec_config(crate::Pallet::<Test>::substrate_call_exec_config())
		.build_and_unwrap_result()
}

/// Calls `dest` with an Ethereum gas limit, configured like the `eth_call` dispatchable.
fn run_call_in_eth_tx(dest: H160, data: Vec<u8>, value: U256) -> ExecReturnValue {
	let eth_tx_info = EthTxInfo::<Test>::new(0, Weight::zero());
	// A real transaction fills this pot from its gas before `eth_call` runs.
	<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
		eth_tx_info.max_deposit(ETH_GAS_LIMIT.into()),
	));
	builder::bare_call(dest)
		.data(data)
		.evm_value(value)
		.transaction_limits(TransactionLimits::EthereumGas {
			eth_gas_limit: ETH_GAS_LIMIT.into(),
			weight_limit: Weight::MAX,
			eth_tx_info,
			authorization_deposit: Default::default(),
		})
		.exec_config(crate::Pallet::<Test>::eth_call_exec_config(0u32.into(), 0, Weight::zero()))
		.build_and_unwrap_result()
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

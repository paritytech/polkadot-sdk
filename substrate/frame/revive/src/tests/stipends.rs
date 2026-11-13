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
	Code, Config, EthTxInfo, TransactionLimits,
	test_utils::{WEIGHT_LIMIT, builder::Contract, deposit_limit},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::{traits::fungible::Mutate, weights::Weight};
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

#[test_case(FixtureType::Solc, "BalanceReader", true, true)]
#[test_case(FixtureType::Resolc, "BalanceReader", true, true)]
#[test_case(FixtureType::Solc, "BalanceOfReader", true, true)]
#[test_case(FixtureType::Resolc, "BalanceOfReader", true, true)]
#[test_case(FixtureType::Solc, "SenderBalanceReader", false, false)]
#[test_case(FixtureType::Resolc, "SenderBalanceReader", false, false)]
#[test_case(FixtureType::Solc, "SimpleReceiver", true, true)]
// These receivers are charged more proof size than the stipend's weight contains.
#[test_case(FixtureType::Solc, "TwoEventsReceiver", false, true)]
#[test_case(FixtureType::Resolc, "TwoEventsReceiver", false, true)]
#[test_case(FixtureType::Solc, "ThreeWordEventReceiver", false, true)]
#[test_case(FixtureType::Resolc, "ThreeWordEventReceiver", false, true)]
fn call_stipends_cover_own_balance_reads_and_evm_events(
	fixture_type: FixtureType,
	receiver_name: &str,
	succeeds_in_substrate_mode: bool,
	succeeds_in_ethereum_mode: bool,
) {
	let (receiver_code, _) = compile_module_with_type(receiver_name, fixture_type).unwrap();
	let (test_code, _) = compile_module_with_type("SendAndTransferTest", fixture_type).unwrap();
	// Half of it moves one native unit plus dust.
	let value = 2_000_002_u128;
	let modes = [
		(
			"Substrate mode",
			TransactionLimits::WeightAndDeposit {
				weight_limit: WEIGHT_LIMIT,
				deposit_limit: deposit_limit::<Test>(),
			},
			succeeds_in_substrate_mode,
		),
		(
			"Ethereum mode",
			TransactionLimits::EthereumGas {
				eth_gas_limit: u64::MAX.into(),
				weight_limit: WEIGHT_LIMIT,
				// The base weight is only proof size, so proof size is the larger cost, and the
				// receiver can use the proof size that the stipend's price pays for.
				eth_tx_info: EthTxInfo::new(0, Weight::from_parts(0, WEIGHT_LIMIT.proof_size())),
				authorization_deposit: Default::default(),
			},
			succeeds_in_ethereum_mode,
		),
	];

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

		for (mode, limits, succeeds) in modes {
			let result = builder::bare_call(addr)
				.data(
					SendAndTransferTest::testSendAndTransferCall { to: receiver.0.into() }
						.abi_encode(),
				)
				.evm_value(value.into())
				.transaction_limits(limits)
				.build_and_unwrap_result();

			assert_eq!(!result.did_revert(), succeeds, "{mode}");
		}
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

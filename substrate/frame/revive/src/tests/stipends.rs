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
	Caller, FixtureType, GasLeftReceiver, SendAndTransferTest, StipendTest,
	compile_module_with_type,
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
	let modes = [
		(
			"Substrate mode",
			TransactionLimits::WeightAndDeposit {
				weight_limit: WEIGHT_LIMIT,
				deposit_limit: deposit_limit::<Test>(),
			},
		),
		// The base weight is only proof size, so proof size decides the fee and the receiver can
		// use no more proof size than the stipend has, as in Substrate mode.
		(
			"Ethereum mode",
			TransactionLimits::EthereumGas {
				eth_gas_limit: u64::MAX.into(),
				weight_limit: WEIGHT_LIMIT,
				eth_tx_info: EthTxInfo::new(0, Weight::from_parts(0, WEIGHT_LIMIT.proof_size())),
				authorization_deposit: Default::default(),
			},
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

		for (mode, limits) in modes {
			let result = builder::bare_call(addr)
				.data(
					SendAndTransferTest::testSendAndTransferCall { to: receiver.0.into() }
						.abi_encode(),
				)
				.evm_value(value.into())
				.transaction_limits(limits)
				.build_and_unwrap_result();

			assert_eq!(result.did_revert(), reverts, "{mode}");
		}
	});
}

/// In Ethereum mode, the receiver of a value call without gas gets as gas what the stipend's weight
/// adds to the transaction's fee, which is more when ref_time is the larger cost of the
/// transaction than when proof size is.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn value_call_receiver_gets_more_gas_when_ref_time_costs_more(caller_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (receiver_code, _) =
		compile_module_with_type("GasLeftReceiver", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);
		let Contract { addr: receiver_addr, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr: caller_addr, account_id: caller_account } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		let _ = <Test as Config>::Currency::set_balance(&caller_account, 1_000_000_000);
		let receiver_gas_left = |base_weight| {
			let output = builder::bare_call(caller_addr)
				.data(
					Caller::normalCall {
						_callee: receiver_addr.0.into(),
						_value: 1,
						_data: GasLeftReceiver::gasLeftCall {}.abi_encode().into(),
						_gas: 0,
					}
					.abi_encode(),
				)
				.transaction_limits(TransactionLimits::EthereumGas {
					eth_gas_limit: u64::MAX.into(),
					weight_limit: WEIGHT_LIMIT,
					eth_tx_info: EthTxInfo::new(0, base_weight),
					authorization_deposit: Default::default(),
				})
				.build_and_unwrap_result();
			let call = Caller::normalCall::abi_decode_returns(&output.data).unwrap();
			assert!(call.success, "the value call must succeed");
			GasLeftReceiver::gasLeftCall::abi_decode_returns(&call.output).unwrap()
		};

		let gas_when_ref_time_costs_more =
			receiver_gas_left(Weight::from_parts(WEIGHT_LIMIT.ref_time(), 0));
		let gas_when_proof_size_costs_more =
			receiver_gas_left(Weight::from_parts(0, WEIGHT_LIMIT.proof_size()));
		// The stipend's ref_time costs more than its proof size.
		assert!(gas_when_ref_time_costs_more > gas_when_proof_size_costs_more);
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

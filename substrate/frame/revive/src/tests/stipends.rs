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
	test_utils::{builder::Contract, ALICE, BOB_ADDR},
	tests::{builder, ExtBuilder, RuntimeEvent, Test},
	BalanceWithDust, Code, Config, Pallet, System,
};
use alloy_core::sol_types::{SolCall, SolEvent};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	compile_module_with_type, ComplexReceiver, FixtureType, SimpleReceiver, StipendSender,
};
use sp_core::H160;
use test_case::test_case;

enum Receiver {
	Contract(&'static str),
	EOA(H160),
}

#[derive(Clone, Copy)]
enum TestCase {
	Bob,
	DoNothingReceiver,
	SimpleReceiver,
	ComplexReceiver,
}

impl TestCase {
	fn receiver(&self) -> Receiver {
		match self {
			&TestCase::Bob => Receiver::EOA(BOB_ADDR),
			&TestCase::DoNothingReceiver => Receiver::Contract("DoNothingReceiver"),
			&TestCase::SimpleReceiver => Receiver::Contract("SimpleReceiver"),
			&TestCase::ComplexReceiver => Receiver::Contract("ComplexReceiver"),
		}
	}
}

fn get_contract_events() -> Vec<(H160, Vec<u8>, Vec<[u8; 32]>)> {
	let events = System::<Test>::events();
	events
		.into_iter()
		.filter_map(|e| match e.event {
			RuntimeEvent::Contracts(crate::Event::ContractEmitted { contract, data, topics }) =>
				Some((
					contract,
					data,
					topics.into_iter().map(|t| t.to_fixed_bytes()).collect::<Vec<_>>(),
				)),
			_ => None,
		})
		.collect()
}

#[test_case(TestCase::Bob; "EOA")]
#[test_case(TestCase::DoNothingReceiver; "DoNothingReceiver")]
#[test_case(TestCase::SimpleReceiver; "SimpleReceiver")]
#[test_case(TestCase::ComplexReceiver; "ComplexReceiver")]
fn evm_call_stipends_work_for_transfers(test_case: TestCase) {
	let (expect_receive_event, expect_success) = match test_case {
		TestCase::Bob => (false, true),
		TestCase::DoNothingReceiver => (false, true),
		TestCase::SimpleReceiver => (true, true),
		TestCase::ComplexReceiver => (false, false),
	};

	let (code, _) = compile_module_with_type("StipendSender", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr: stipend_sender_address, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let receiver_addr = match test_case.receiver() {
			Receiver::Contract(name) => {
				let (receiver_code, _) = compile_module_with_type(name, FixtureType::Solc).unwrap();
				let Contract { addr: receiver_addr, .. } =
					builder::bare_instantiate(Code::Upload(receiver_code))
						.build_and_unwrap_contract();

				receiver_addr
			},

			Receiver::EOA(address) => address,
		};

		let balance_before = Pallet::<Test>::evm_balance(&receiver_addr);

		let amount = Pallet::<Test>::convert_native_to_evm(1_000_000_000_000);

		let result = builder::bare_call(stipend_sender_address)
			.data(StipendSender::sendViaTransferCall { to: receiver_addr.0.into() }.abi_encode())
			.evm_value(amount)
			.build_and_unwrap_result();

		let balance_after = Pallet::<Test>::evm_balance(&receiver_addr);

		let mut contract_events = get_contract_events().into_iter();

		if expect_receive_event {
			let (_contract, data, topics) = contract_events.next().unwrap();
			let decoded_event =
				SimpleReceiver::Received::decode_raw_log(topics, data.as_slice()).unwrap();
			assert_eq!(decoded_event.from, stipend_sender_address.0);
			assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
		}

		if expect_success {
			assert!(!result.did_revert());
			assert_eq!(amount, balance_after.saturating_sub(balance_before));

			let (_contract, data, topics) = contract_events.next().unwrap();
			let decoded_event =
				StipendSender::TransferSuccess::decode_raw_log(topics, data.as_slice()).unwrap();
			assert_eq!(decoded_event.method, "transfer");
			assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
			assert_eq!(decoded_event.to, receiver_addr.0);
		} else {
			assert!(result.did_revert());
			assert_eq!(balance_after, balance_before);
		}
	});
}

#[test_case(TestCase::Bob; "EOA")]
#[test_case(TestCase::DoNothingReceiver; "DoNothingReceiver")]
#[test_case(TestCase::SimpleReceiver; "SimpleReceiver")]
#[test_case(TestCase::ComplexReceiver; "ComplexReceiver")]
fn evm_call_stipends_work_for_sends(test_case: TestCase) {
	let (expect_receive_event, expect_success) = match test_case {
		TestCase::Bob => (false, true),
		TestCase::DoNothingReceiver => (false, true),
		TestCase::SimpleReceiver => (true, true),
		TestCase::ComplexReceiver => (false, false),
	};

	let (code, _) = compile_module_with_type("StipendSender", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr: stipend_sender_address, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let receiver_addr = match test_case.receiver() {
			Receiver::Contract(name) => {
				let (receiver_code, _) = compile_module_with_type(name, FixtureType::Solc).unwrap();
				let Contract { addr: receiver_addr, .. } =
					builder::bare_instantiate(Code::Upload(receiver_code))
						.build_and_unwrap_contract();

				receiver_addr
			},

			Receiver::EOA(address) => address,
		};

		let balance_before = Pallet::<Test>::evm_balance(&receiver_addr);

		let amount = Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(
			1_000_000_000_000,
			0,
		));

		let result = builder::bare_call(stipend_sender_address)
			.data(StipendSender::sendViaSendCall { to: receiver_addr.0.into() }.abi_encode())
			.evm_value(amount)
			.build_and_unwrap_result();

		let balance_after = Pallet::<Test>::evm_balance(&receiver_addr);

		let mut contract_events = get_contract_events().into_iter();

		if expect_receive_event {
			let (_contract, data, topics) = contract_events.next().unwrap();
			let decoded_event =
				SimpleReceiver::Received::decode_raw_log(topics, data.as_slice()).unwrap();
			assert_eq!(decoded_event.from, stipend_sender_address.0);
			assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
		}

		if expect_success {
			assert!(!result.did_revert());
			assert_eq!(amount, balance_after.saturating_sub(balance_before));

			let (_contract, data, topics) = contract_events.next().unwrap();
			let decoded_event =
				StipendSender::TransferSuccess::decode_raw_log(topics, data.as_slice()).unwrap();
			assert_eq!(decoded_event.method, "send");
			assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
			assert_eq!(decoded_event.to, receiver_addr.0);
		} else {
			assert!(!result.did_revert());
			assert_eq!(balance_after, balance_before);

			let (_contract, data, topics) = contract_events.next().unwrap();
			let decoded_event =
				StipendSender::TransferFailed::decode_raw_log(topics, data.as_slice()).unwrap();
			assert_eq!(decoded_event.method, "send");
			assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
			assert_eq!(decoded_event.to, receiver_addr.0);
		}
	});
}

#[test_case(TestCase::Bob; "EOA")]
#[test_case(TestCase::DoNothingReceiver; "DoNothingReceiver")]
#[test_case(TestCase::SimpleReceiver; "SimpleReceiver")]
#[test_case(TestCase::ComplexReceiver; "ComplexReceiver")]
fn evm_call_stipends_work_for_calls(test_case: TestCase) {
	let (expect_receive_event, expect_simple_receive_event) = match test_case {
		TestCase::Bob => (false, true),
		TestCase::DoNothingReceiver => (false, true),
		TestCase::SimpleReceiver => (true, true),
		TestCase::ComplexReceiver => (true, false),
	};

	let (code, _) = compile_module_with_type("StipendSender", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr: stipend_sender_address, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let receiver_addr = match test_case.receiver() {
			Receiver::Contract(name) => {
				let (receiver_code, _) = compile_module_with_type(name, FixtureType::Solc).unwrap();
				let Contract { addr: receiver_addr, .. } =
					builder::bare_instantiate(Code::Upload(receiver_code))
						.build_and_unwrap_contract();

				receiver_addr
			},

			Receiver::EOA(address) => address,
		};

		let balance_before = Pallet::<Test>::evm_balance(&receiver_addr);

		let amount = Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(
			1_000_000_000_000,
			0,
		));

		let result = builder::bare_call(stipend_sender_address)
			.data(StipendSender::sendViaCallCall { to: receiver_addr.0.into() }.abi_encode())
			.evm_value(amount)
			.build_and_unwrap_result();

		let balance_after = Pallet::<Test>::evm_balance(&receiver_addr);

		let mut contract_events = get_contract_events().into_iter();

		if expect_receive_event {
			let (_contract, data, topics) = contract_events.next().unwrap();
			if expect_simple_receive_event {
				let decoded_event =
					SimpleReceiver::Received::decode_raw_log(topics, data.as_slice()).unwrap();
				assert_eq!(decoded_event.from, stipend_sender_address.0);
				assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
			} else {
				let decoded_event =
					ComplexReceiver::Received::decode_raw_log(topics, data.as_slice()).unwrap();
				assert_eq!(decoded_event.from, stipend_sender_address.0);
				assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
				assert_eq!(decoded_event.newCounter.try_into(), Ok(1));
			}
		}

		assert!(!result.did_revert());
		assert_eq!(amount, balance_after.saturating_sub(balance_before));

		let (_contract, data, topics) = contract_events.next().unwrap();
		let decoded_event =
			StipendSender::TransferSuccess::decode_raw_log(topics, data.as_slice()).unwrap();
		assert_eq!(decoded_event.method, "call");
		assert_eq!(decoded_event.amount.as_le_slice(), amount.to_little_endian());
		assert_eq!(decoded_event.to, receiver_addr.0);
	});
}

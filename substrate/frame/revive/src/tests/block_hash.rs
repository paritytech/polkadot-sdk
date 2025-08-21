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

//! The pallet-revive ETH block hash specific integration test suite.

use crate::{
	test_utils::{builder::Contract, deposit_limit, ALICE},
	tests::{assert_ok, builder, Contracts, ExtBuilder, RuntimeOrigin, Test},
	BalanceWithDust, Code, Config, EthBlock, EthereumBlock, Event, InflightEthTransactions,
	InflightEthTxEvents, Pallet, ReceiptGasInfo, ReceiptInfoData, Weight, H256,
};

use frame_support::traits::{fungible::Mutate, Hooks};
use pallet_revive_fixtures::compile_module;

#[test]
fn on_initialize_clears_storage() {
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let receipt_data =
			vec![ReceiptGasInfo { effective_gas_price: 1.into(), gas_used: 1.into() }];
		ReceiptInfoData::<Test>::put(receipt_data.clone());
		assert_eq!(ReceiptInfoData::<Test>::get(), receipt_data);

		let event =
			Event::ContractEmitted { contract: Default::default(), data: vec![1], topics: vec![] };
		InflightEthTxEvents::<Test>::put(vec![event.clone()]);
		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![event.clone()]);

		let transactions = vec![(vec![1, 2, 3], 1, vec![event], true, Weight::zero())];
		InflightEthTransactions::<Test>::put(transactions.clone());
		assert_eq!(InflightEthTransactions::<Test>::get(), transactions);

		let block = EthBlock { number: 1.into(), ..Default::default() };
		EthereumBlock::<Test>::put(block.clone());
		assert_eq!(EthereumBlock::<Test>::get(), block);

		Contracts::on_initialize(0);

		assert_eq!(ReceiptInfoData::<Test>::get(), vec![]);
		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);
		assert_eq!(InflightEthTransactions::<Test>::get(), vec![]);
		assert_eq!(EthereumBlock::<Test>::get(), Default::default());
	});
}

#[test]
fn transactions_are_captured() {
	let (binary, _) = compile_module("dummy").unwrap();
	let (gas_binary, _code_hash) = compile_module("run_out_of_gas").unwrap();

	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		Contracts::on_initialize(0);

		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(binary.clone())).build_and_unwrap_contract();
		let balance =
			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

		assert_ok!(builder::eth_call(addr).value(balance).build());
		assert_ok!(builder::eth_instantiate_with_code(binary).value(balance).build());

		// Call is not captured.
		assert_ok!(builder::call(addr).value(1).build());
		// Instantiate with code is not captured.
		assert_ok!(builder::instantiate_with_code(gas_binary).value(1).build());

		let transactions = InflightEthTransactions::<Test>::get();
		assert_eq!(transactions.len(), 2);
		assert_eq!(transactions[0].0, vec![1]); // payload set to 1 for eth_call
		assert_eq!(transactions[0].1, 0); // tx index
		assert_eq!(transactions[0].2, vec![]); // no events emitted
		assert_eq!(transactions[0].3, true); // successful

		assert_eq!(transactions[1].0, vec![2]); // payload set to 2 for eth_instantiate_with_code
		assert_eq!(transactions[1].1, 0); // tx index
		assert_eq!(transactions[1].2, vec![]); // no events emitted
		assert_eq!(transactions[1].3, true); // successful

		Contracts::on_finalize(0);

		assert_eq!(InflightEthTransactions::<Test>::get(), vec![]);
	});
}

#[test]
fn events_are_captured() {
	let (binary, code_hash) = compile_module("event_and_return_on_deploy").unwrap();

	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			binary.clone(),
			deposit_limit::<Test>(),
		));

		Contracts::on_initialize(1);

		// Bare call must not be captured.
		let Contract { addr, .. } = builder::bare_instantiate(Code::Existing(code_hash.clone()))
			.build_and_unwrap_contract();
		let balance =
			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

		// Capture the EthInstantiate.
		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);
		assert_ok!(builder::eth_instantiate_with_code(binary).value(balance).build());
		// Events are cleared out by storing the transaction.
		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);

		let transactions = InflightEthTransactions::<Test>::get();
		assert_eq!(transactions.len(), 1);
		assert_eq!(transactions[0].0, vec![2]); // payload set to 1 for eth_instantiate_with_code
		assert_eq!(transactions[0].1, 0); // tx index
		match &transactions[0].2[0] {
			crate::Event::ContractEmitted { contract, data, topics } => {
				assert_ne!(contract, &addr);
				assert_eq!(data, &vec![1, 2, 3, 4]);
				assert_eq!(topics, &vec![H256::repeat_byte(42)]);
			},
			event => panic!("Event {event:?} unexpected"),
		};
		assert_eq!(transactions[0].3, true); // successful

		Contracts::on_finalize(0);

		assert_eq!(InflightEthTransactions::<Test>::get(), vec![]);
		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);
	});
}

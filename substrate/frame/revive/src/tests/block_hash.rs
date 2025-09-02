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
	eth_block_storage,
	evm::block_hash::EventLog,
	test_utils::{builder::Contract, ALICE},
	tests::{assert_ok, builder, Contracts, ExtBuilder, Test},
	BalanceWithDust, Code, Config, EthBlock, EthereumBlock, Pallet, ReceiptGasInfo,
	ReceiptInfoData,
};

use frame_support::traits::{fungible::Mutate, Hooks};
use pallet_revive_fixtures::compile_module;

impl PartialEq for EventLog {
	// Dont care about the contract address, since eth instantiate cannot expose it.
	fn eq(&self, other: &Self) -> bool {
		self.data == other.data && self.topics == other.topics
	}
}

#[test]
fn on_initialize_clears_storage() {
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let receipt_data = vec![ReceiptGasInfo { gas_used: 1.into() }];
		ReceiptInfoData::<Test>::put(receipt_data.clone());
		assert_eq!(ReceiptInfoData::<Test>::get(), receipt_data);

		let block = EthBlock { number: 1.into(), ..Default::default() };
		EthereumBlock::<Test>::put(block.clone());
		assert_eq!(EthereumBlock::<Test>::get(), block);

		Contracts::on_initialize(0);

		// RPC queried storage is cleared out.
		assert_eq!(ReceiptInfoData::<Test>::get(), vec![]);
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

		// assert_eq!(eth_block_storage::INCREMENTAL_BUILDER.borrow_mut().tx_hashes.len(), 2);

		// let transactions = InflightEthTransactions::<Test>::get();
		// let expected = vec![
		// 	TransactionDetails {
		// 		transaction_encoded: TransactionSigned::TransactionLegacySigned(Default::default())
		// 			.signed_payload(),
		// 		logs: vec![],
		// 		success: true,
		// 		gas_used: Weight::zero(),
		// 	},
		// 	TransactionDetails {
		// 		transaction_encoded: TransactionSigned::Transaction4844Signed(Default::default())
		// 			.signed_payload(),
		// 		logs: vec![],
		// 		success: true,
		// 		gas_used: Weight::zero(),
		// 	},
		// ];
		// assert_eq!(transactions, expected);

		Contracts::on_finalize(0);

		// assert_eq!(eth_block_storage::INCREMENTAL_BUILDER.borrow_mut().tx_hashes.len(), 0);
	});
}

// #[test]
// fn events_are_captured() {
// 	let (binary, code_hash) = compile_module("event_and_return_on_deploy").unwrap();

// 	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
// 		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

// 		assert_ok!(Contracts::upload_code(
// 			RuntimeOrigin::signed(ALICE),
// 			binary.clone(),
// 			deposit_limit::<Test>(),
// 		));

// 		Contracts::on_initialize(1);

// 		// Bare call must not be captured.
// 		builder::bare_instantiate(Code::Existing(code_hash)).build_and_unwrap_contract();
// 		let balance =
// 			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

// 		// Capture the EthInstantiate.
// 		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);
// 		assert_ok!(builder::eth_instantiate_with_code(binary).value(balance).build());
// 		// Events are cleared out by storing the transaction.
// 		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);

// 		let transactions = InflightEthTransactions::<Test>::get();
// 		let expected = vec![TransactionDetails {
// 			transaction_encoded: TransactionSigned::Transaction4844Signed(Default::default())
// 				.signed_payload(),
// 			logs: vec![EventLog {
// 				data: vec![1, 2, 3, 4],
// 				topics: vec![H256::repeat_byte(42)],
// 				contract: Default::default(),
// 			}],
// 			success: true,
// 			gas_used: Weight::zero(),
// 		}];

// 		assert_eq!(transactions, expected);

// 		Contracts::on_finalize(0);

// 		assert_eq!(InflightEthTransactions::<Test>::get(), vec![]);
// 		assert_eq!(InflightEthTxEvents::<Test>::get(), vec![]);
// 	});
// }

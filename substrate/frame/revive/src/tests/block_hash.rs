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
	evm::{block_hash::EthereumBlockBuilder, fees::InfoT, Block, TransactionSigned},
	test_utils::{builder::Contract, deposit_limit, ALICE},
	tests::{assert_ok, builder, Contracts, ExtBuilder, RuntimeOrigin, Test},
	BalanceWithDust, Code, Config, EthBlock, EthBlockBuilderFirstValues, EthBlockBuilderIR,
	EthereumBlock, Pallet, ReceiptGasInfo, ReceiptInfoData,
};

use frame_support::traits::{
	fungible::{Balanced, Mutate},
	Hooks,
};
use pallet_revive_fixtures::compile_module;

use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::{FixedBytes, Log as AlloyLog};

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

		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(binary.clone())).build_and_unwrap_contract();
		let balance =
			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(5_000_000_000));

		assert_ok!(builder::eth_call(addr).value(balance).build());
		assert_ok!(builder::eth_instantiate_with_code(binary).value(balance).build());

		// Call is not captured.
		assert_ok!(builder::call(addr).value(1).build());
		// Instantiate with code is not captured.
		assert_ok!(builder::instantiate_with_code(gas_binary).value(1).build());

		let block_builder = EthBlockBuilderIR::<Test>::get();
		// Only 2 transactions were captured.
		assert_eq!(block_builder.gas_info.len(), 2);

		let expected_payloads = vec![
			// Signed payload of eth_call.
			TransactionSigned::TransactionLegacySigned(Default::default()).signed_payload(),
			// Signed payload of eth_instantiate_with_code.
			TransactionSigned::Transaction4844Signed(Default::default()).signed_payload(),
		];
		let expected_tx_root = Block::compute_trie_root(&expected_payloads);

		// Double check the trie root hash.
		let mut builder = EthereumBlockBuilder::<Test>::from_ir(block_builder);

		let first_values = EthBlockBuilderFirstValues::<Test>::get().unwrap();
		builder.transaction_root_builder.set_first_value(first_values.0);

		let tx_root = builder.transaction_root_builder.finish();
		assert_eq!(tx_root, expected_tx_root.0.into());

		Contracts::on_finalize(0);

		// Builder is killed on finalize.
		let block_builder = EthBlockBuilderIR::<Test>::get();
		assert_eq!(block_builder.gas_info.len(), 0);
	});
}

#[test]
fn events_are_captured() {
	let (binary, code_hash) = compile_module("event_and_return_on_deploy").unwrap();

	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);

		assert_ok!(Contracts::upload_code(
			RuntimeOrigin::signed(ALICE),
			binary.clone(),
			deposit_limit::<Test>(),
		));

		Contracts::on_initialize(1);

		// Bare call must not be captured.
		builder::bare_instantiate(Code::Existing(code_hash)).build_and_unwrap_contract();
		let balance =
			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
			500_000_000_000,
		));

		assert_ok!(builder::eth_instantiate_with_code(binary).value(balance).build());

		// The contract address is not exposed by the `eth_instantiate_with_code` call.
		// Instead, extract the address from the frame system's last event.
		let events = frame_system::Pallet::<Test>::events();
		let contract = events
			.into_iter()
			.filter_map(|event_record| match event_record.event {
				crate::tests::RuntimeEvent::Contracts(crate::Event::Instantiated {
					contract,
					..
				}) => Some(contract),
				_ => None,
			})
			.last()
			.expect("Contract address must be found from events");

		let expected_payloads = vec![
			// Signed payload of eth_instantiate_with_code.
			TransactionSigned::Transaction4844Signed(Default::default()).signed_payload(),
		];
		let expected_tx_root = Block::compute_trie_root(&expected_payloads);

		let block_builder = EthBlockBuilderIR::<Test>::get();
		let gas_used = block_builder.gas_info[0].gas_used;

		let logs = vec![AlloyLog::new_unchecked(
			contract.0.into(),
			vec![FixedBytes::from([42u8; 32])],
			vec![1, 2, 3, 4].into(),
		)];
		let receipt = alloy_consensus::Receipt {
			status: true.into(),
			cumulative_gas_used: gas_used.as_u64(),
			logs,
		};

		let receipt_bloom = receipt.bloom_slow();
		// Receipt starts with encoded tx type which is 3 for 4844 transactions.
		let mut encoded_receipt = vec![3];
		receipt.rlp_encode_with_bloom(&receipt_bloom, &mut encoded_receipt);
		let expected_receipt_root = Block::compute_trie_root(&[encoded_receipt.clone()]);

		let block_builder = EthBlockBuilderIR::<Test>::get();
		// 1 transaction captured.
		assert_eq!(block_builder.gas_info.len(), 1);

		let mut builder = EthereumBlockBuilder::<Test>::from_ir(block_builder);
		builder.transaction_root_builder.set_first_value(expected_payloads[0].clone());
		let tx_root = builder.transaction_root_builder.finish();
		assert_eq!(tx_root, expected_tx_root.0.into());

		builder.receipts_root_builder.set_first_value(encoded_receipt.clone());
		let receipt_root = builder.receipts_root_builder.finish();
		assert_eq!(receipt_root, expected_receipt_root.0.into());

		Contracts::on_finalize(0);

		let block_builder = EthBlockBuilderIR::<Test>::get();
		assert_eq!(block_builder.gas_info.len(), 0);
	});
}

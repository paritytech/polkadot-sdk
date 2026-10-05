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
	BalanceWithDust, Code, Config, EthBlock, EthBlockBuilderFirstValues, EthBlockBuilderIR,
	EthereumBlock, Pallet, ReceiptGasInfo, ReceiptInfoData,
	evm::{Block, TransactionSigned, block_hash::EthereumBlockBuilder, fees::InfoT},
	exec::Key,
	test_utils::{ALICE, builder::Contract, deposit_limit},
	tests::{
		Contracts, ExtBuilder, RuntimeEvent, RuntimeOrigin, System, Test, Timestamp, assert_ok,
		builder, test_utils::get_contract,
	},
};
use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::{FixedBytes, Log as AlloyLog};
use frame_support::traits::{
	Hooks,
	fungible::{Balanced, Mutate},
};
use pallet_revive_fixtures::compile_module;
use revm::bytecode::opcode::*;
use sp_core::{H160, H256, U256};
use sp_state_machine::BasicExternalities;

const CHILD_LOG_ID: u8 = 0x22;
const PARENT_LOG_ID: u8 = 0x33;
const OUTER_LOG_ID: u8 = 0x44;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedLog {
	address: H160,
	topic: H256,
	data: Vec<u8>,
}

fn push32(code: &mut Vec<u8>, value: [u8; 32]) {
	code.push(PUSH32);
	code.extend_from_slice(&value);
}

fn evm_init_code(runtime: &[u8]) -> Vec<u8> {
	let len: u16 = runtime.len().try_into().expect("runtime must fit into PUSH2");
	let [hi, lo] = len.to_be_bytes();
	let offset = 14u8;
	let mut code =
		vec![PUSH2, hi, lo, PUSH1, offset, PUSH1, 0, CODECOPY, PUSH2, hi, lo, PUSH1, 0, RETURN];
	code.extend_from_slice(runtime);
	code
}

fn emit_log(code: &mut Vec<u8>, id: u8) {
	push32(code, [id; 32]);
	code.extend_from_slice(&[PUSH0, MSTORE]);
	push32(code, [id; 32]);
	code.extend_from_slice(&[PUSH1, 32, PUSH0, LOG1]);
}

fn child_runtime(revert: bool) -> Vec<u8> {
	let mut code = vec![PUSH1, 1, PUSH0, SSTORE];
	emit_log(&mut code, CHILD_LOG_ID);
	finish_runtime(&mut code, revert);
	code
}

fn caller_runtime(log_id: u8, forward_second_address: bool, revert: bool) -> Vec<u8> {
	let mut code = if forward_second_address {
		vec![PUSH1, 32, PUSH1, 32, PUSH0, CALLDATACOPY]
	} else {
		Vec::new()
	};
	code.extend_from_slice(&[PUSH0, PUSH0]); // output length, output offset
	if forward_second_address {
		code.extend_from_slice(&[PUSH1, 32]);
	} else {
		code.push(PUSH0);
	}
	code.extend_from_slice(&[
		PUSH0,        // input offset
		PUSH0,        // value
		PUSH0,        // calldata offset for callee address
		CALLDATALOAD, // callee address
		GAS,
		CALL,
		POP,
	]);
	emit_log(&mut code, log_id);
	finish_runtime(&mut code, revert);
	code
}

fn finish_runtime(code: &mut Vec<u8>, revert: bool) {
	if revert {
		code.extend_from_slice(&[PUSH0, PUSH0, REVERT]);
	} else {
		code.push(STOP);
	}
}

fn address_calldata(address: H160) -> Vec<u8> {
	let mut data = vec![0u8; 12];
	data.extend_from_slice(address.as_bytes());
	data
}

fn address_pair_calldata(first: H160, second: H160) -> Vec<u8> {
	let mut data = address_calldata(first);
	data.extend_from_slice(&address_calldata(second));
	data
}

fn tx_payload(tag: u8) -> Vec<u8> {
	vec![0x80, tag]
}

fn fund_eth_context() {
	let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000_000);
	<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
		10_000_000_000_000_000,
	));
}

fn deploy_evm(runtime: Vec<u8>, salt: u8) -> H160 {
	builder::bare_instantiate(Code::Upload(evm_init_code(&runtime)))
		.salt(Some([salt; 32]))
		.build_and_unwrap_contract()
		.addr
}

fn contract_emitted_events() -> Vec<ExpectedLog> {
	frame_system::Pallet::<Test>::events()
		.into_iter()
		.filter_map(|event| match event.event {
			RuntimeEvent::Contracts(crate::Event::ContractEmitted { contract, data, topics }) => {
				let [topic] = topics.as_slice() else {
					panic!("test contracts emit exactly one topic");
				};
				Some(ExpectedLog { address: contract, topic: *topic, data })
			},
			_ => None,
		})
		.collect()
}

fn expected_receipt(logs: &[ExpectedLog], gas_used: U256) -> (Vec<u8>, [u8; 256]) {
	let logs = logs
		.iter()
		.map(|log| {
			AlloyLog::new_unchecked(
				log.address.0.into(),
				vec![FixedBytes::from(log.topic.0)],
				log.data.clone().into(),
			)
		})
		.collect();
	let receipt = alloy_consensus::Receipt {
		status: true.into(),
		cumulative_gas_used: gas_used.as_u64(),
		logs,
	};
	let bloom = receipt.bloom_slow();
	let mut encoded = Vec::new();
	receipt.rlp_encode_with_bloom(&bloom, &mut encoded);
	(encoded, *bloom.0)
}

fn assert_receipt_block_and_events(expected_logs: &[ExpectedLog]) {
	let block_builder = EthBlockBuilderIR::<Test>::get();
	assert_eq!(block_builder.gas_info.len(), 1);
	let gas_used = block_builder.gas_info[0].gas_used;
	let (_, actual_receipt) =
		EthBlockBuilderFirstValues::<Test>::get().expect("first tx and receipt must exist");
	let (expected_receipt, bloom) = expected_receipt(expected_logs, gas_used);
	let expected_receipt_root = Block::compute_trie_root(&[expected_receipt.clone()]).0;

	assert_eq!(actual_receipt, expected_receipt);
	assert_eq!(contract_emitted_events(), expected_logs);

	Contracts::on_finalize(1);

	let block = EthereumBlock::<Test>::get();
	assert_eq!(block.logs_bloom.0.as_slice(), bloom.as_slice());
	assert_eq!(block.receipts_root, expected_receipt_root.into());
}

fn expected_log(address: H160, id: u8) -> ExpectedLog {
	ExpectedLog { address, topic: H256([id; 32]), data: vec![id; 32] }
}

fn child_storage(address: H160) -> Option<Vec<u8>> {
	get_contract(&address).read(&Key::Fix([0u8; 32]))
}

fn in_eth_context(test: impl FnOnce()) {
	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		fund_eth_context();
		Contracts::on_initialize(1);
		test();
	});
}

fn call_with_address_data(caller: H160, data: Vec<u8>, tag: u8) {
	assert_ok!(
		builder::eth_call(caller)
			.data(data)
			.transaction_encoded(tx_payload(tag))
			.build()
	);
}

#[test]
fn on_initialize_clears_storage() {
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let receipt_data =
			vec![ReceiptGasInfo { gas_used: 1.into(), effective_gas_price: 1.into() }];
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
fn genesis_block_number_and_timestamp_fetched_from_storage() {
	let mut ext = BasicExternalities::new_empty();
	ext.execute_with(|| {
		System::set_block_number(10);
		Timestamp::set_timestamp(10000000);
	});
	let storage = ext.into_storages();

	ExtBuilder::default()
		.with_genesis_state_overrides(storage)
		.build()
		.execute_with(|| {
			let block = EthereumBlock::<Test>::get();
			// The timestamp is divided by 1000 (converted to seconds)
			assert_eq!(block.timestamp, 10000.into());
			assert_eq!(block.number, 10.into());
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
		let Contract { addr: addr2, .. } =
			builder::bare_instantiate(Code::Upload(gas_binary.clone())).build_and_unwrap_contract();
		let balance =
			Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(100, 10));

		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(5_000_000_000));

		// eth calls are captured.
		assert_ok!(builder::eth_call(addr).transaction_encoded(vec![1]).value(balance).build());
		assert_ok!(
			builder::eth_instantiate_with_code(binary)
				.value(balance)
				.transaction_encoded(vec![2])
				.build()
		);
		assert_ok!(builder::eth_call(addr2).transaction_encoded(vec![3]).build());

		// non-eth calls are not captured.
		assert_ok!(builder::call(addr).value(1).build());
		assert_ok!(builder::instantiate_with_code(gas_binary).salt(Some([1u8; 32])).build());

		let block_builder = EthBlockBuilderIR::<Test>::get();
		assert_eq!(block_builder.gas_info.len(), 3, "3 transactions were captured");

		let expected_payloads = vec![vec![1u8], vec![2u8], vec![3u8]];
		let expected_tx_root = Block::compute_trie_root(&expected_payloads);

		// Double check the trie root hash.
		let mut builder = EthereumBlockBuilder::<Test>::from_ir(block_builder);

		let first_values = EthBlockBuilderFirstValues::<Test>::get().unwrap();
		builder.transaction_root_builder.set_first_value(first_values.0);

		let tx_root = builder.transaction_root_builder.finish();
		assert_eq!(tx_root, expected_tx_root.0.into());

		Contracts::on_finalize(0);
		assert_eq!(crate::EthereumBlock::<Test>::get().transactions.len(), 3);

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

#[test]
fn reverted_child_logs_are_not_in_parent_receipt() {
	in_eth_context(|| {
		let child = deploy_evm(child_runtime(true), 0xC1);
		let parent = deploy_evm(caller_runtime(PARENT_LOG_ID, false, false), 0xC2);

		call_with_address_data(parent, address_calldata(child), 0xC1);
		assert_eq!(child_storage(child), None);
		assert_receipt_block_and_events(&[expected_log(parent, PARENT_LOG_ID)]);
	});
}

#[test]
fn successful_child_logs_are_in_parent_receipt() {
	in_eth_context(|| {
		let child = deploy_evm(child_runtime(false), 0xD1);
		let parent = deploy_evm(caller_runtime(PARENT_LOG_ID, false, false), 0xD2);

		call_with_address_data(parent, address_calldata(child), 0xD1);
		assert!(child_storage(child).is_some());
		assert_receipt_block_and_events(&[
			expected_log(child, CHILD_LOG_ID),
			expected_log(parent, PARENT_LOG_ID),
		]);
	});
}

#[test]
fn ancestor_revert_discards_successful_descendant_logs() {
	in_eth_context(|| {
		let child = deploy_evm(child_runtime(false), 0xE1);
		let parent = deploy_evm(caller_runtime(PARENT_LOG_ID, false, true), 0xE2);
		let outer = deploy_evm(caller_runtime(OUTER_LOG_ID, true, false), 0xE3);

		call_with_address_data(outer, address_pair_calldata(parent, child), 0xE1);
		assert_eq!(child_storage(child), None);
		assert_receipt_block_and_events(&[expected_log(outer, OUTER_LOG_ID)]);
	});
}

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
	EthereumBlock, H160, H256, Pallet, ReceiptGasInfo, ReceiptInfoData, SyntheticReceiptInfo, U256,
	evm::{
		Block, HashesOrTransactionInfos, TransactionSigned, block_hash::EthereumBlockBuilder,
		fees::InfoT,
	},
	test_utils::{ALICE, builder::Contract, deposit_limit},
	tests::{Contracts, ExtBuilder, RuntimeOrigin, System, Test, Timestamp, assert_ok, builder},
};
use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::{FixedBytes, Log as AlloyLog};
use frame_support::traits::{
	Hooks,
	fungible::{Balanced, Mutate},
};
use pallet_revive_fixtures::compile_module;
use sp_crypto_hashing::keccak_256;
use sp_state_machine::BasicExternalities;

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
fn receipt_data_written_before_the_synthetic_transaction_existed_still_reads() {
	// A runtime upgrade enacts mid-block: block N's `on_finalize` runs the old code, while a
	// runtime API call at N runs the new code against what that wrote. So the entries a runtime
	// without the synthetic transaction left behind have to keep decoding, with no synthetic
	// transaction reported for them.
	//
	// Spelled out as bytes rather than encoded through the current types, so that any change to
	// what `ReceiptInfoData` holds — folding the synthetic transaction back in, or a new field on
	// `ReceiptGasInfo` — fails here instead of round-tripping.
	let mut old_bytes = vec![0x04]; // `Compact(1)`: one entry follows.
	old_bytes.extend([0x08, 0x52]); // `gas_used`: 21_000, little-endian…
	old_bytes.extend([0u8; 30]); // …zero-padded to 32 bytes.
	old_bytes.push(0x07); // `effective_gas_price`: 7…
	old_bytes.extend([0u8; 31]); // …likewise.

	ExtBuilder::default().build().execute_with(|| {
		sp_io::storage::set(&ReceiptInfoData::<Test>::hashed_key(), &old_bytes);

		assert_eq!(
			Pallet::<Test>::eth_receipt_data(),
			vec![ReceiptGasInfo { gas_used: 21_000.into(), effective_gas_price: 7.into() }],
		);
		assert!(Pallet::<Test>::eth_synthetic_transaction().is_none());
	});
}

#[test]
fn a_mirrored_log_shares_its_block_with_a_real_transaction() {
	let (binary, _) = compile_module("dummy").unwrap();

	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		Contracts::on_initialize(0);

		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(binary)).build_and_unwrap_contract();
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(5_000_000_000));

		let real_payload = vec![1u8];
		assert_ok!(builder::eth_call(addr).transaction_encoded(real_payload.clone()).build());

		// A mirror fired from a plain extrinsic: no ethereum transaction is open, so it is buffered
		// for the block's synthetic transaction instead of reaching a receipt.
		Pallet::<Test>::emit_contract_log_outside_frame(
			H160::from_low_u64_be(0xA1),
			vec![H256::repeat_byte(0x11)].try_into().unwrap(),
			vec![1, 2, 3].try_into().unwrap(),
		);

		Contracts::on_finalize(0);

		let synthetic_payload = crate::evm::synthetic_log_transaction(
			U256::zero(),
			U256::from(<Test as Config>::ChainId::get()),
		);
		let block = EthereumBlock::<Test>::get();

		// Both tries were fed both transactions, with the synthetic one trailing. The single-
		// transaction case cannot show this: `process_transaction` stashes a lone transaction as
		// the builders' first value and returns, so only a second one reaches them at all.
		let expected_tx_root =
			Block::compute_trie_root(&[real_payload.clone(), synthetic_payload.clone()]);
		assert_eq!(block.transactions_root, expected_tx_root.0.into());

		let hashes = match block.transactions {
			HashesOrTransactionInfos::Hashes(hashes) => hashes,
			_ => panic!("expected transaction hashes"),
		};
		assert_eq!(hashes.len(), 2);
		assert_eq!(hashes[0], H256(keccak_256(&real_payload)), "the real transaction comes first");
		assert_eq!(hashes[1], H256(keccak_256(&synthetic_payload)), "the synthetic one trails it");

		// And the gas entries the serving layer reconciles against keep the same split: the real
		// transaction's is the only one paired with the block body.
		assert_eq!(ReceiptInfoData::<Test>::get().len(), 1, "one entry per ethereum transaction");
		let synthetic = SyntheticReceiptInfo::<Test>::get().expect("reported apart");
		assert_eq!(synthetic.log_count, 1, "with the log count the block committed to it");
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

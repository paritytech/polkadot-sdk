// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet integration tests for pallet-revive.
//!
//! This crate contains integration tests that use Zombienet to test
//! pallet-revive functionality in a realistic multi-node environment.
use crate::{TestEnvironment, BEST_BLOCK_METRIC};
use pallet_revive::evm::{
	Account, Block as EvmBlock, BlockNumberOrTag, BlockTag, GenericTransaction, ReceiptInfo,
	TransactionInfo,
};
use pallet_revive_eth_rpc::{
	example::TransactionBuilder,
	subxt_client::{self},
	EthRpcClient,
};
use sp_core::{H256, U256};
use subxt::{self, ext::subxt_rpcs::rpc_params};
// use zombienet_sdk::subxt::{
// 	self, backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig,
// };

const ROOT_FROM_NO_DATA: &str = "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";

pub fn print_receipt_info(receipt: &ReceiptInfo) {
	println!("Receipt:");
	println!("- Block number:        {}", receipt.block_number);
	println!("- Block hash:          {}", receipt.block_hash);
	println!("- Gas used:            {}", receipt.gas_used);
	println!("- From:                {}", receipt.from);
	println!("- To:                  {:?}", receipt.to);
	println!("- Contract address:    {:?}", receipt.contract_address);
	println!("- Cumulative gas used: {}", receipt.cumulative_gas_used);
	println!("- Success:             {:?}", receipt.status);
}

pub async fn assert_block(
	test_env: &TestEnvironment,
	block_number_or_tag: BlockNumberOrTag,
	should_be_empty: bool,
) {
	let TestEnvironment { eth_rpc_client, collator_rpc_client, collator_client, .. } = test_env;

	println!("Asserting block {block_number_or_tag:?} should_be_empty: {should_be_empty}");
	let eth_rpc_block = eth_rpc_client
		.get_block_by_number(block_number_or_tag.clone(), false)
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch block {block_number_or_tag:?}: {err:?}"))
		.expect(&format!("Expected block {block_number_or_tag:?} not found"));

	println!("eth block number: {:?} hash: {:?}", eth_rpc_block.number, eth_rpc_block.hash);

	if should_be_empty {
		// Blocks with no transactions and no state should have the same roots
		assert_eq!(hex::encode(&eth_rpc_block.transactions_root), ROOT_FROM_NO_DATA);
		assert_eq!(hex::encode(&eth_rpc_block.receipts_root), ROOT_FROM_NO_DATA);
		assert_eq!(hex::encode(&eth_rpc_block.state_root), ROOT_FROM_NO_DATA);
	}

	let substrate_block_hash: H256 = collator_rpc_client
		.request("chain_getBlockHash", rpc_params![eth_rpc_block.number])
		.await
		.unwrap_or_else(|err| {
			panic!("Failed to get block hash for block {:?}: {err:?}", eth_rpc_block.number)
		});

	println!("substrate block number: {:?} hash: {substrate_block_hash:?}", eth_rpc_block.number);
	let storage = collator_client.storage().at(substrate_block_hash);

	let query = subxt_client::storage().revive().ethereum_block();
	let evm_block: EvmBlock = storage
		.fetch(&query)
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch EvmBlock from storage: {err:?}"))
		.expect("EvmBlock not found in storage")
		.0;
	assert_eq!(eth_rpc_block, evm_block);

	let number_u256 = subxt::utils::Static(eth_rpc_block.number);
	let query = subxt_client::storage().revive().block_hash(number_u256);

	let block_hash_from_storage: H256 = storage
		.fetch(&query)
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch block hash from storage: {err:?}"))
		.expect(&format!("Block number {:?} hash not found in storage", eth_rpc_block.number));
	assert_eq!(eth_rpc_block.hash, block_hash_from_storage);
}

pub async fn assert_transactions(
	test_env: &TestEnvironment,
	signer: Account,
	transactions: Vec<(H256, GenericTransaction, ReceiptInfo)>,
) {
	let TestEnvironment { eth_rpc_client, collator_rpc_client, collator_client, .. } = test_env;

	for (idx, (tx_hash, tx, receipt)) in transactions.into_iter().enumerate() {
		let block_number = receipt.block_number;
		let block_hash = receipt.block_hash;
		let tx_unsigned = tx
			.try_into_unsigned()
			.unwrap_or_else(|err| panic!("Failed to convert transaction: {err:?}"));
		let tx_signed = signer.sign_transaction(tx_unsigned);
		let expected_tx_info = TransactionInfo::new(&receipt, tx_signed);

		let tx_by_hash = eth_rpc_client
			.get_transaction_by_hash(tx_hash)
			.await
			.unwrap_or_else(|err| panic!("Failed to fetch tx by hash {tx_hash:?}: {err:?}"))
			.expect(&format!("Expected transaction {tx_hash:?} not found"));
		let tx_by_block_number_and_index = eth_rpc_client
			.get_transaction_by_block_number_and_index(
				BlockNumberOrTag::U256(block_number.into()),
				idx.into(),
			)
			.await
			.unwrap_or_else(|err| {
				panic!(
					"Failed to fetch tx by block number {block_number:?} and index {idx:?} {err:?}",
				)
			})
			.expect(&format!(
				"Expected transaction at block number {block_number:?} and index {idx:?} not found"
			));
		let tx_by_block_hash_and_index = eth_rpc_client
			.get_transaction_by_block_hash_and_index(block_hash, idx.into())
			.await
			.unwrap_or_else(|err| {
				panic!("Failed to fetch tx by block hash {block_hash:?} and index {idx:?} {err:?}",)
			})
			.expect(&format!(
				"Expected transaction at block hash {block_hash:?} and index {idx:?} not found",
			));

		assert_eq!(expected_tx_info, tx_by_hash);
		assert_eq!(expected_tx_info, tx_by_block_number_and_index);
		assert_eq!(expected_tx_info, tx_by_block_hash_and_index);
	}
}

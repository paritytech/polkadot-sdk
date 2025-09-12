// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use pallet_revive::evm::{
	Account, Block as EvmBlock, BlockNumberOrTag, BlockTag, GenericTransaction, ReceiptInfo,
	TransactionInfo,
};
use pallet_revive_eth_rpc::{
	example::TransactionBuilder,
	subxt_client::{self},
	EthRpcClient,
};
use pallet_revive_zombienet::{TestEnvironment, BEST_BLOCK_METRIC};
use sp_core::{H256, U256};
use subxt::{self, ext::subxt_rpcs::rpc_params};
// use zombienet_sdk::subxt::{
// 	self, backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig,
// };

const COLLATOR_RPC_PORT: u16 = 9944;
const ETH_RPC_URL: &str = "http://localhost:8545";

const ROOT_FROM_NO_DATA: &str = "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_dont_spawn_zombienet() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let test_env = TestEnvironment::without_zombienet(COLLATOR_RPC_PORT, ETH_RPC_URL)
		.await
		.unwrap_or_else(|err| panic!("Failed to create test environment: {err:?}"));

	// TODO: block zero is reconstructed from substrate
	// sanity_block_check(&test_env, BlockNumberOrTag::U256(0.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::U256(1.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Earliest), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Finalized), true).await;

	test_transfer(&test_env).await;
	test_deployment(&test_env).await;
}

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_with_zombienet_spawning() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let test_env = TestEnvironment::with_zombienet(COLLATOR_RPC_PORT, ETH_RPC_URL)
		.await
		.unwrap_or_else(|err| panic!("Failed to create test environment: {err:?}"));
	let zombienet = test_env.zombienet.as_ref().unwrap();

	// TODO: block zero is reconstructed from substrate
	// sanity_block_check(&test_env, BlockNumberOrTag::U256(0.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::U256(1.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Earliest), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Finalized), true).await;

	// test_transfer(&test_env).await;

	// TODO remove after tests are implemented
	let alice = zombienet
		.network
		.get_node("alice-westend-validator")
		.unwrap_or_else(|err| panic!("Failed to get node: {err:?}"));
	assert!(alice
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 600.0, 3600u64)
		.await
		.is_ok());
}

fn print_receipt_info(receipt: &ReceiptInfo) {
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

async fn assert_block(
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

async fn assert_transactions(
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

async fn test_transfer(test_env: &TestEnvironment) {
	let TestEnvironment { eth_rpc_client, collator_rpc_client, collator_client, .. } = test_env;

	let alith = Account::default();
	let alith_address = alith.address();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = 1_000_000_000_000_000_000_000u128.into();

	let alith_balance_before = eth_rpc_client
		.get_balance(alith_address, BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to get Alith's balance: {err:?}"));
	let ethan_balance_before = eth_rpc_client
		.get_balance(ethan.address(), BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to get Ethan's balance: {err:?}"));

	println!("\n\n=== Transferring  ===\n\n");

	let tx = TransactionBuilder::new(&eth_rpc_client)
		.signer(alith.clone())
		.value(amount)
		.to(ethan.address())
		.send()
		.await
		.unwrap_or_else(|err| panic!("Failed to send transaction: {err:?}"));
	println!("Tx hash: {:?}", tx.hash());

	let receipt = tx
		.wait_for_receipt()
		.await
		.unwrap_or_else(|err| panic!("Failed while waiting for receipt: {err:?}"));
	print_receipt_info(&receipt);

	let alith_balance_after = eth_rpc_client
		.get_balance(alith_address, BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to get Alith's balance: {err:?}"));
	let ethan_balance_after = eth_rpc_client
		.get_balance(ethan.address(), BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to get Ethan's balance: {err:?}"));
	println!("Balances before:");
	println!("  Alith: {alith_balance_before:?}");
	println!("  Ethan: {ethan_balance_before:?}");
	println!("Balances after:");
	println!(
		"  Alith: {alith_balance_after:?} gas:{:?}",
		alith_balance_before.saturating_sub(alith_balance_after).saturating_sub(amount)
	);
	println!("  Ethan: {ethan_balance_after:?}");

	// TODO:
	//  Should Alith's balance reduced by amount and gas used?
	//  Currently gas used is zero
	// assert_eq!(
	// 	alith_balance_after,
	// 	alith_balance_before.saturating_sub(amount).saturating_sub(gas_used)
	// );
	assert_eq!(ethan_balance_after, ethan_balance_before.saturating_add(amount));
	assert_block(test_env, BlockNumberOrTag::U256(receipt.block_number), false).await;
	assert_transactions(test_env, alith, vec![(tx.hash(), tx.generic_transaction(), receipt)])
		.await;
}

async fn test_deployment(test_env: &TestEnvironment) {
	let TestEnvironment { eth_rpc_client, collator_rpc_client, collator_client, .. } = test_env;

	let account = Account::default();

	let data = vec![];
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")
		.unwrap_or_else(|err| panic!("Failed to compile dummy contract: {err:?}"));
	let input = bytes.into_iter().chain(data.clone()).collect::<Vec<u8>>();

	println!("Account:");
	println!("- address: {:?}", account.address());
	println!("- substrate: {}", account.substrate_account());

	println!("\n\n=== Deploying contract ===\n\n");

	let nonce = eth_rpc_client
		.get_transaction_count(account.address(), BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to get transactions count: {err:?}"));

	let tx = TransactionBuilder::new(&eth_rpc_client)
		.signer(account.clone())
		.value(5_000_000_000_000u128.into())
		.input(input)
		.send()
		.await
		.unwrap_or_else(|err| panic!("Failed to send transaction: {err:?}"));
	println!("Tx hash: {:?}", tx.hash());

	let receipt = tx
		.wait_for_receipt()
		.await
		.unwrap_or_else(|err| panic!("Failed while waiting for receipt: {err:?}"));
	print_receipt_info(&receipt);

	let contract_address = receipt.contract_address.unwrap();

	assert_eq!(
		contract_address,
		pallet_revive::create1(&account.address(), nonce.try_into().unwrap())
	);
	assert_block(test_env, BlockNumberOrTag::U256(receipt.block_number), false).await;
	assert_transactions(
		test_env,
		account.clone(),
		vec![(tx.hash(), tx.generic_transaction(), receipt)],
	)
	.await;

	println!("\n\n=== Calling contract ===\n\n");
	let tx = TransactionBuilder::new(&eth_rpc_client)
		.value(U256::from(1_000_000u32))
		.to(contract_address)
		.send()
		.await
		.unwrap_or_else(|err| panic!("Failed to send transaction: {err:?}"));
	println!("Tx hash: {:?}", tx.hash());
	let receipt = tx
		.wait_for_receipt()
		.await
		.unwrap_or_else(|err| panic!("Failed while waiting for receipt: {err:?}"));
	print_receipt_info(&receipt);

	assert_eq!(contract_address, receipt.to.unwrap());
	assert_block(test_env, BlockNumberOrTag::U256(receipt.block_number), false).await;
	assert_transactions(test_env, account, vec![(tx.hash(), tx.generic_transaction(), receipt)])
		.await;
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use cumulus_zombienet_sdk_helpers::submit_extrinsic_with_params_and_wait_for_finalization_success;
use pallet_revive::evm::{Account, BlockNumberOrTag, BlockTag};
use pallet_revive_eth_rpc::{example::TransactionBuilder, EthRpcClient};
use pallet_revive_zombienet::{utils::*, TestEnvironment, BEST_BLOCK_METRIC};
use sp_core::U256;
use subxt::{self, config::polkadot::PolkadotExtrinsicParamsBuilder, dynamic::Value};
use subxt_signer::sr25519::dev;

const COLLATOR_RPC_PORT: u16 = 9944;
const ETH_RPC_URL: &str = "http://localhost:8545";

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

	// test_single_transfer(&test_env).await;
	// test_deployment(&test_env).await;
	// test_parallel_transfers(&test_env, 5).await;
	test_mixed_evm_substrate_transactions(&test_env, 3, 2).await;
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
	// assert_block(&test_env, BlockNumberOrTag::U256(1.into()), true).await;
	// assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Earliest), true).await;
	// assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Finalized), true).await;

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

async fn test_single_transfer(test_env: &TestEnvironment) {
	let TestEnvironment { eth_rpc_client, .. } = test_env;

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
	let TestEnvironment { eth_rpc_client, .. } = test_env;

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

async fn test_parallel_transfers(test_env: &TestEnvironment, num_transactions: usize) {
	println!("\n\n=== Testing Parallel Transfers ===\n\n");

	let TestEnvironment { eth_rpc_client, .. } = test_env;
	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(1_000_000_000_000_000_000u128);

	println!("Creating {} parallel transfer transactions", num_transactions);
	let mut nonce = eth_rpc_client
		.get_transaction_count(alith.address(), BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch account nonce: {err:?}"));

	let mut transactions = Vec::new();
	for i in 0..num_transactions {
		let tx_builder = TransactionBuilder::new(eth_rpc_client)
			.signer(alith.clone())
			.nonce(nonce)
			.value(amount)
			.to(ethan.address());

		transactions.push(tx_builder);
		println!("Prepared transaction {}/{num_transactions} with nonce: {nonce:?}", i + 1);
		nonce = nonce.saturating_add(U256::one());
	}

	println!("Submitting and waiting for {} transactions in parallel", num_transactions);
	let start_time = std::time::Instant::now();

	let results = eth_rpc_submit_and_wait_for_transactions_parallel(transactions)
		.await
		.unwrap_or_else(|err| {
			panic!("Failed to submit or wait for parallel transactions: {err:?}")
		});

	let duration = start_time.elapsed();
	println!(
		"Completed {} transactions in {:?} ({:.2} tx/sec)",
		results.len(),
		duration,
		results.len() as f64 / duration.as_secs_f64()
	);

	println!("Successfully completed {} parallel transactions", results.len());

	let mut blocks = vec![];
	let mut txs = vec![];

	// Verify all transactions were successful
	for (i, (hash, generic_tx, receipt)) in results.into_iter().enumerate() {
		println!("Transaction {}: hash={hash:?}, block={}", i + 1, receipt.block_number);
		assert_eq!(
			receipt.status.unwrap_or(U256::zero()),
			U256::one(),
			"Transaction should be successful"
		);
		let block = BlockNumberOrTag::U256(receipt.block_number);

		if !blocks.contains(&block) {
			blocks.push(block);
		}
		txs.push((hash, generic_tx, receipt));
	}

	for block in blocks {
		assert_block(test_env, block, false).await;
	}
	assert_transactions(test_env, alith, txs).await;
}

async fn test_mixed_evm_substrate_transactions(
	test_env: &TestEnvironment,
	num_evm_txs: usize,
	num_substrate_txs: usize,
) {
	println!("\n\n=== Testing Mixed EVM and Substrate Transactions ===\n\n");

	let TestEnvironment { eth_rpc_client, collator_client, .. } = test_env;
	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(500_000_000_000_000_000u128);

	// Prepare EVM transactions
	println!("Creating {} EVM transfer transactions", num_evm_txs);
	let mut nonce = eth_rpc_client
		.get_transaction_count(alith.address(), BlockTag::Latest.into())
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch account nonce: {err:?}"));

	let mut evm_transactions = Vec::new();
	for i in 0..num_evm_txs {
		let tx_builder = TransactionBuilder::new(eth_rpc_client)
			.signer(alith.clone())
			.nonce(nonce)
			.value(amount)
			.to(ethan.address());

		evm_transactions.push(tx_builder);
		println!("Prepared EVM transaction {}/{num_evm_txs} with nonce: {nonce:?}", i + 1);
		nonce = nonce.saturating_add(U256::one());
	}

	// Prepare substrate transactions (simple balance transfers)
	println!("Creating {} substrate transfer transactions", num_substrate_txs);
	let alice_signer = dev::alice();

	// Prepare all substrate transfer calls first
	let mut substrate_calls = Vec::new();
	for i in 0..num_substrate_txs {
		let call = subxt::dynamic::tx("System", "remark", vec![Value::from_bytes("Hello there")]);
		substrate_calls.push(call);
		println!("Prepared substrate transaction {}/{num_substrate_txs}", i + 1);
	}

	// Create futures for all substrate transfer calls
	let mut substrate_tx_futures = Vec::new();
	let mut nonce = collator_client
		.tx()
		.account_nonce(&alice_signer.public_key().into())
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch account nonce: {err:?}"));
	for call in &substrate_calls {
		let extensions = PolkadotExtrinsicParamsBuilder::new().nonce(nonce).immortal().build();
		let future = submit_extrinsic_with_params_and_wait_for_finalization_success(
			collator_client,
			call,
			&alice_signer,
			extensions,
		);
		substrate_tx_futures.push(future);
		nonce += 1;
	}

	println!(
		"Submitting {} EVM and {} substrate transactions in parallel",
		num_evm_txs, num_substrate_txs
	);
	let start_time = std::time::Instant::now();

	// Submit all transactions in parallel
	let (evm_results, substrate_results) = tokio::join!(
		eth_rpc_submit_and_wait_for_transactions_parallel(evm_transactions),
		futures::future::join_all(substrate_tx_futures)
	);

	let duration = start_time.elapsed();

	// Handle results
	let evm_results = evm_results
		.unwrap_or_else(|err| panic!("Failed to submit or wait for EVM transactions: {err:?}"));

	let substrate_success_count = substrate_results.iter().filter(|result| result.is_ok()).count();

	let substrate_failed_count = substrate_results.len() - substrate_success_count;

	println!(
		"Completed {} EVM and {} substrate transactions ({} substrate failed) in {:?} ({:.2} total tx/sec)",
		evm_results.len(),
		substrate_success_count,
		substrate_failed_count,
		duration,
		(evm_results.len() + substrate_success_count) as f64 / duration.as_secs_f64()
	);

	// Report any substrate transaction failures
	for (i, result) in substrate_results.iter().enumerate() {
		if let Err(err) = result {
			println!("Substrate transaction {} failed: {err:?}", i + 1);
		}
	}

	// Verify EVM transactions
	let mut blocks = vec![];
	let mut evm_txs = vec![];

	for (i, (hash, generic_tx, receipt)) in evm_results.into_iter().enumerate() {
		println!("EVM Transaction {}: hash={hash:?}, block={}", i + 1, receipt.block_number);
		assert_eq!(
			receipt.status.unwrap_or(U256::zero()),
			U256::one(),
			"EVM transaction should be successful"
		);
		let block = BlockNumberOrTag::U256(receipt.block_number);

		if !blocks.contains(&block) {
			blocks.push(block);
		}
		evm_txs.push((hash, generic_tx, receipt));
	}

	// Verify blocks contain the transactions
	for block in blocks {
		assert_block(test_env, block, false).await;
	}
	assert_transactions(test_env, alith, evm_txs).await;

	println!(
		"Successfully completed mixed transaction test with {} EVM and {} substrate transactions",
		num_evm_txs, substrate_success_count
	);
}

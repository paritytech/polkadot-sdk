// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use pallet_revive::evm::{Account, BlockNumberOrTag, BlockTag};
use pallet_revive_eth_rpc::{example::TransactionBuilder, EthRpcClient};
use pallet_revive_zombienet::{utils::*, TestEnvironment};
use sp_core::U256;
use subxt::{self, dynamic::Value};
use subxt_signer::sr25519::dev;

const COLLATOR_RPC_PORT: u16 = 9944;
const ETH_RPC_URL: &str = "http://localhost:8545";

// This test is useful when developing, one can spawn substrate-node and eth-rpc separately
// and does not need to wait until zombienet's parachain nodes are ready to interact with.
#[tokio::test(flavor = "multi_thread")]
async fn test_dont_spawn_zombienet() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let test_env = TestEnvironment::without_zombienet(COLLATOR_RPC_PORT, ETH_RPC_URL)
		.await
		.unwrap_or_else(|err| panic!("Failed to create test environment: {err:?}"));

	// TODO: block zero is reconstructed from substrate
	assert_block(&test_env, BlockNumberOrTag::U256(1.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Earliest), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Finalized), true).await;

	test_single_transfer(&test_env).await;
	test_deployment(&test_env).await;
	test_parallel_transfers(&test_env, 5).await;
	test_mixed_evm_substrate_transactions(&test_env, 3, 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_with_zombienet_spawning() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let test_env = TestEnvironment::with_zombienet(COLLATOR_RPC_PORT, ETH_RPC_URL)
		.await
		.unwrap_or_else(|err| panic!("Failed to create test environment: {err:?}"));
	let _zombienet = test_env.zombienet.as_ref().unwrap();

	// TODO: block zero is reconstructed from substrate
	assert_block(&test_env, BlockNumberOrTag::U256(1.into()), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Earliest), true).await;
	assert_block(&test_env, BlockNumberOrTag::BlockTag(BlockTag::Finalized), true).await;

	test_single_transfer(&test_env).await;
	test_deployment(&test_env).await;
	test_parallel_transfers(&test_env, 5).await;
	test_mixed_evm_substrate_transactions(&test_env, 3, 2).await;
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

	println!(
		"Submitting {} transactions synchronously, then waiting in parallel",
		num_transactions
	);
	let start_time = std::time::Instant::now();

	// Submit all transactions synchronously first
	let submitted_txs = eth_rpc_submit_transactions(transactions)
		.await
		.unwrap_or_else(|err| panic!("Failed to submit transactions: {err:?}"));

	// Wait for all receipts in parallel
	let results = eth_rpc_wait_for_receipts(submitted_txs)
		.await
		.unwrap_or_else(|err| panic!("Failed to wait for parallel transactions: {err:?}"));

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

	// Prepare substrate transactions (simple remarks)
	println!("Creating {} substrate remark transactions", num_substrate_txs);
	let alice_signer = dev::alice();

	let mut substrate_calls = Vec::new();
	for i in 0..num_substrate_txs {
		let call = subxt::dynamic::tx("System", "remark", vec![Value::from_bytes("Hello there")]);
		substrate_calls.push(call);
		println!("Prepared substrate transaction {}/{num_substrate_txs}", i + 1);
	}

	let substrate_nonce = collator_client
		.tx()
		.account_nonce(&alice_signer.public_key().into())
		.await
		.unwrap_or_else(|err| panic!("Failed to fetch account nonce: {err:?}"));

	println!(
		"Submitting {} EVM and {} substrate transactions synchronously, then waiting in parallel",
		num_evm_txs, num_substrate_txs
	);
	let start_time = std::time::Instant::now();

	// Submit transactions
	let evm_submitted = eth_rpc_submit_transactions(evm_transactions)
		.await
		.unwrap_or_else(|err| panic!("Failed to submit EVM transactions: {err:?}"));
	let substrate_submitted = substrate_submit_extrinsics(
		collator_client,
		substrate_calls,
		&alice_signer,
		substrate_nonce,
	)
	.await
	.unwrap_or_else(|err| panic!("Failed to submit substrate transactions: {err:?}"));

	// Wait for all transactions in parallel
	let (evm_results, substrate_results) = tokio::join!(
		eth_rpc_wait_for_receipts(evm_submitted),
		substrate_wait_for_finalization(substrate_submitted)
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

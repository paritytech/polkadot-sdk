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
//! Test the eth-rpc cli with the kitchensink node.
//! This only includes basic transaction tests, most of the other tests are in the
//! [evm-test-suite](https://github.com/paritytech/evm-test-suite) repository.

use crate::{
	BlockInfoProvider, ChainMetadata, EthRpcClient, ReceiptExtractor, ReceiptProvider,
	SubxtBlockInfoProvider, SyncLabel,
	cli::{self, CliCommand},
	client::{Client, connect},
	example::TransactionBuilder,
	subxt_client::{
		self, SrcChainConfig, src_chain::runtime_types::pallet_revive::primitives::Code,
	},
};
use anyhow::anyhow;
use clap::Parser;
use jsonrpsee::{
	core::ClientError,
	ws_client::{WsClient, WsClientBuilder},
};
use pallet_revive::{
	create1,
	evm::{
		Account, Block, BlockHeader, BlockNumberOrTag, BlockNumberOrTagOrHash, BlockTag,
		BoundedOneOrMany, Filter, FilterResults, GenericTransaction, H256,
		HashesOrTransactionInfos, Log, SubscriptionItem, SubscriptionKind, SubscriptionOptions,
		TransactionInfo, TransactionUnsigned, U256,
	},
};
use sp_runtime::BoundedVec;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{sync::Arc, thread};
use subxt::{
	OnlineClient,
	backend::rpc::RpcClient,
	ext::subxt_rpcs::rpc_params,
	tx::{SubmittableTransaction, TxStatus},
};
use subxt_signer::eth::Keypair;

const LOG_TARGET: &str = "eth-rpc-tests";

/// Create a websocket client with a 120s timeout.
async fn ws_client_with_retry(url: &str) -> WsClient {
	let timeout = tokio::time::Duration::from_secs(120);
	tokio::time::timeout(timeout, async {
		loop {
			if let Ok(client) = WsClientBuilder::default().build(url).await {
				return client;
			} else {
				tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
			}
		}
	})
	.await
	.expect("Hit timeout")
}

struct SharedResources {
	_node_handle: std::thread::JoinHandle<()>,
	_rpc_handle: std::thread::JoinHandle<()>,
}

impl SharedResources {
	fn start() -> Self {
		// Start revive-dev-node
		let _node_handle = thread::spawn(move || {
			if let Err(e) = revive_dev_node::command::run_with_args(vec![
				"--dev".to_string(),
				"--rpc-port=45789".to_string(),
				"-lerror,sc_rpc_server=info,runtime::revive=debug".to_string(),
			]) {
				panic!("Node exited with error: {e:?}");
			}
		});

		// Start the rpc server.
		let args = CliCommand::parse_from([
			"--dev",
			"--rpc-port=45788",
			"--node-rpc-url=ws://localhost:45789",
			"--no-prometheus",
			"-linfo,eth-rpc=debug",
			"--eth-pruning=256",
		]);

		let _rpc_handle = thread::spawn(move || {
			if let Err(e) = cli::run(args) {
				panic!("eth-rpc exited with error: {e:?}");
			}
		});

		Self { _node_handle, _rpc_handle }
	}

	async fn client() -> WsClient {
		ws_client_with_retry("ws://localhost:45788").await
	}

	async fn node_client() -> OnlineClient<SrcChainConfig> {
		OnlineClient::<SrcChainConfig>::from_url(Self::node_rpc_url()).await.unwrap()
	}

	fn node_rpc_url() -> &'static str {
		"ws://localhost:45789"
	}
}

macro_rules! unwrap_call_err(
	($err:expr) => {
		match $err.downcast_ref::<jsonrpsee::core::client::Error>().unwrap() {
			jsonrpsee::core::client::Error::Call(call) => call,
			_ => panic!("Expected Call error"),
		}
	}
);

// Helper functions
/// Prepare multiple EVM transfer transactions with nonce in descending order
async fn prepare_evm_transactions<Client: EthRpcClient + Sync + Send>(
	client: Arc<Client>,
	signer: Account,
	recipient: pallet_revive::evm::Address,
	amount: U256,
	count: usize,
) -> anyhow::Result<Vec<TransactionBuilder<Client>>> {
	let start_nonce =
		client.get_transaction_count(signer.address(), BlockTag::Latest.into()).await?;

	let mut transactions = Vec::new();
	for i in (0..count).rev() {
		let nonce = start_nonce.saturating_add(U256::from(i as u64));
		let tx_builder = TransactionBuilder::new(Arc::clone(&client))
			.signer(signer.clone())
			.nonce(nonce)
			.value(amount)
			.to(recipient);

		transactions.push(tx_builder);
		log::trace!(target: LOG_TARGET, "Prepared EVM transaction {}/{count} with nonce: {nonce:?}", i + 1);
	}

	Ok(transactions)
}

/// Prepare multiple Substrate transfer transactions with sequential nonces
async fn prepare_substrate_transactions(
	node_client: &OnlineClient<SrcChainConfig>,
	signer: &subxt_signer::sr25519::Keypair,
	count: usize,
) -> anyhow::Result<Vec<SubmittableTransaction<SrcChainConfig, OnlineClient<SrcChainConfig>>>> {
	let mut nonce = node_client.tx().account_nonce(&signer.public_key().into()).await?;
	let mut substrate_txs = Vec::new();
	for i in 0..count {
		let remark_data = format!("Hello from test {}", i);
		let call = subxt::dynamic::tx(
			"System",
			"remark",
			vec![subxt::dynamic::Value::from_bytes(remark_data.as_bytes())],
		);

		let params = subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new()
			.nonce(nonce)
			.build();

		let tx = node_client.tx().create_signed(&call, signer, params).await?;
		substrate_txs.push(tx);
		log::trace!(target: LOG_TARGET, "Prepared substrate transaction {i}/{count} with nonce: {nonce}");
		nonce += 1 as u64;
	}
	Ok(substrate_txs)
}

/// Submit multiple transactions and return them without waiting for receipts
async fn submit_evm_transactions<Client: EthRpcClient + Sync + Send>(
	transactions: Vec<TransactionBuilder<Client>>,
) -> anyhow::Result<
	Vec<(
		H256,
		pallet_revive::evm::GenericTransaction,
		crate::example::SubmittedTransaction<Client>,
	)>,
> {
	let mut submitted_txs = Vec::new();

	for tx_builder in transactions {
		let tx = tx_builder.send().await?;
		let hash = tx.hash();
		let generic_tx = tx.generic_transaction();
		submitted_txs.push((hash, generic_tx, tx));
	}

	Ok(submitted_txs)
}

/// Submit substrate transactions and return futures for waiting
async fn submit_substrate_transactions(
	substrate_txs: Vec<SubmittableTransaction<SrcChainConfig, OnlineClient<SrcChainConfig>>>,
) -> Vec<impl std::future::Future<Output = Result<(), anyhow::Error>>> {
	let mut futures = Vec::new();

	for (i, tx) in substrate_txs.into_iter().enumerate() {
		let fut = async move {
			match tx.submit_and_watch().await {
				Ok(mut progress) => {
					log::trace!(target: LOG_TARGET, "Substrate tx {i} submitted");
					while let Some(status) = progress.next().await {
						match status {
							Ok(TxStatus::InFinalizedBlock(block)) |
							Ok(TxStatus::InBestBlock(block)) => {
								log::trace!(target: LOG_TARGET,
									"Substrate tx {i} included in block {:?}",
									block.block_hash()
								);
								return Ok(());
							},
							Err(e) => return Err(anyhow::anyhow!("Substrate tx {i} error: {e}")),
							Ok(status) => {
								log::trace!(target: LOG_TARGET, "Substrate tx {i} status {:?}", status);
							},
						}
					}
					Err(anyhow::anyhow!(
						"Failed to get status of submitted substrate tx {i}, assuming error"
					))
				},
				Err(e) => Err(anyhow::anyhow!("Failed to submit substrate tx {i}: {e}")),
			}
		};
		futures.push(fut);
	}

	futures
}

/// Verify all given transaction hashes are in the specified block and accessible via RPC
async fn verify_transactions_in_single_block(
	client: &Arc<WsClient>,
	block_number: U256,
	expected_tx_hashes: &[H256],
) -> anyhow::Result<()> {
	// Fetch the block
	let block = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number), false)
		.await?
		.ok_or_else(|| anyhow!("Block {block_number} should exist"))?;

	let block_tx_hashes = match &block.transactions {
		HashesOrTransactionInfos::Hashes(hashes) => hashes.clone(),
		HashesOrTransactionInfos::TransactionInfos(infos) => {
			infos.iter().map(|info| info.hash).collect()
		},
	};

	if let Some(missing_hash) =
		expected_tx_hashes.iter().find(|hash| !block_tx_hashes.contains(hash))
	{
		return Err(anyhow!("Transaction {missing_hash:?} not found in block {block_number}"));
	}

	Ok(())
}

#[tokio::test]
async fn run_all_eth_rpc_tests() -> anyhow::Result<()> {
	// Set up a 2-minute timeout for the entire test
	let timeout_duration = tokio::time::Duration::from_secs(120);
	let result = tokio::time::timeout(timeout_duration, run_all_eth_rpc_tests_inner()).await;

	match result {
		Ok(inner_result) => inner_result,
		Err(_) => {
			log::error!(target: LOG_TARGET, "Test timed out after 2 minutes!");
			std::process::exit(1);
		},
	}
}

async fn run_all_eth_rpc_tests_inner() -> anyhow::Result<()> {
	// start node and rpc server
	let _shared = SharedResources::start();
	// Wait for servers to be ready
	let _ = SharedResources::client().await;

	macro_rules! run_tests {
		($($test:ident),+ $(,)?) => {
			$(
				{
					let test_name = stringify!($test);
					log::debug!(target: LOG_TARGET, "Running test: {}", test_name);
					match $test().await {
						Ok(()) => log::debug!(target: LOG_TARGET, "Test passed: {}", test_name),
						Err(err) => panic!("Test {} failed: {err:?}", test_name),
					}
				}
			)+
		};
	}

	run_tests!(
		test_fibonacci_call_via_runtime_api,
		test_transfer,
		test_deploy_and_call,
		test_runtime_api_dry_run_addr_works,
		test_invalid_transaction,
		test_evm_blocks_should_match,
		test_evm_blocks_hydrated_should_match,
		test_block_hash_for_tag_with_proper_ethereum_block_hash_works,
		test_block_hash_for_tag_with_invalid_ethereum_block_hash_fails,
		test_block_hash_for_tag_with_block_number_works,
		test_block_hash_for_tag_with_block_tags_works,
		test_multiple_transactions_in_block,
		test_mixed_evm_substrate_transactions,
		test_runtime_pallets_address_upload_code,
		test_subscribe_new_heads,
		test_subscribe_new_heads_multiple_blocks,
		test_subscribe_logs,
		test_subscribe_logs_with_address_filter,
		test_subscribe_logs_with_topic_filter,
		test_subscribe_logs_address_filter_excludes_non_matching,
		test_subscribe_logs_with_multiple_addresses_filter,
		test_subscribe_logs_no_event_transaction_ignored,
		test_subscribe_with_invalid_params_rejected,
		test_estimate_gas_of_contract_with_consume_all_gas,
		test_gas_estimation_for_contract_requiring_binary_search,
		test_gas_estimation_with_no_funds_no_gas_specified,
		test_gas_estimation_with_no_funds_and_with_gas_specified,
		test_block_sync_fresh,
		test_block_sync_resume_interrupted,
		test_block_sync_detects_corruption,
		test_block_sync_picks_up_new_blocks,
	);

	log::debug!(target: LOG_TARGET, "All tests completed successfully!");
	Ok(())
}

async fn test_transfer() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let initial_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;

	let value = 1_000_000_000_000_000_000_000u128.into();
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;

	let receipt = tx.wait_for_receipt().await?;
	assert_eq!(
		Some(ethan.address()),
		receipt.to,
		"Receipt should have the correct contract address."
	);

	let balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	assert_eq!(
		Some(value),
		balance.checked_sub(initial_balance),
		"Ethan {:?} {balance:?} should have increased by {value:?} from {initial_balance}.",
		ethan.address()
	);
	Ok(())
}

async fn test_deploy_and_call() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	// Balance transfer
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let initial_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	let value = 1_000_000_000_000_000_000_000u128.into();
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;

	let receipt = tx.wait_for_receipt().await?;
	assert_eq!(
		Some(ethan.address()),
		receipt.to,
		"Receipt should have the correct contract address."
	);

	let balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	assert_eq!(
		Some(value),
		balance.checked_sub(initial_balance),
		"Ethan {:?} {balance:?} should have increased by {value:?} from {initial_balance}.",
		ethan.address()
	);

	// Deploy contract
	let data = b"hello world".to_vec();
	let value = U256::from(5_000_000_000_000u128);
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let input = bytes.into_iter().chain(data.clone()).collect::<Vec<u8>>();
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).value(value).input(input).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(
		Some(contract_address),
		receipt.contract_address,
		"Contract should be deployed at {contract_address:?}."
	);

	let nonce_after_deploy =
		client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;

	assert_eq!(nonce_after_deploy - nonce, U256::from(1), "Nonce should have increased by 1");

	let initial_balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(
		value, initial_balance,
		"Contract {contract_address:?} balance should be the same as the value sent ({value})."
	);

	// Call contract
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;
	let receipt = tx.wait_for_receipt().await?;

	assert_eq!(
		Some(contract_address),
		receipt.to,
		"Receipt should have the correct contract address {contract_address:?}."
	);

	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(
		Some(value),
		balance.checked_sub(initial_balance),
		"Contract {contract_address:?} Balance {balance} should have increased from {initial_balance} by {value}."
	);

	// Balance transfer to contract
	let initial_balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;

	tx.wait_for_receipt().await?;

	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;

	assert_eq!(
		Some(value),
		balance.checked_sub(initial_balance),
		"Balance {balance} should have increased from {initial_balance} by {value}."
	);
	Ok(())
}

async fn test_runtime_api_dry_run_addr_works() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let node_client = SharedResources::node_client().await;
	let account = Account::default();
	let origin: [u8; 32] = account.substrate_account().into();
	let data = b"hello world".to_vec();
	let value = 5_000_000_000_000u128;
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;

	let payload = subxt_client::apis().revive_api().instantiate(
		subxt::utils::AccountId32(origin),
		value,
		None,
		None,
		Code::Upload(bytes),
		data,
		None,
	);

	// runtime_api.at_latest() uses the latest finalized block, query nonce accordingly
	let nonce = client
		.get_transaction_count(account.address(), BlockTag::Finalized.into())
		.await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());

	let res = node_client
		.runtime_api()
		.at_latest()
		.await?
		.call(payload)
		.await?
		.result
		.unwrap();

	assert_eq!(res.addr, contract_address);
	Ok(())
}

async fn test_invalid_transaction() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());

	let err = TransactionBuilder::new(client.clone())
		.value(U256::from(1_000_000_000_000u128))
		.to(ethan.address())
		.mutate(|tx| match tx {
			TransactionUnsigned::TransactionLegacyUnsigned(tx) => tx.chain_id = Some(42u32.into()),
			TransactionUnsigned::Transaction1559Unsigned(tx) => tx.chain_id = 42u32.into(),
			TransactionUnsigned::Transaction2930Unsigned(tx) => tx.chain_id = 42u32.into(),
			TransactionUnsigned::Transaction4844Unsigned(tx) => tx.chain_id = 42u32.into(),
			TransactionUnsigned::Transaction7702Unsigned(tx) => tx.chain_id = 42u32.into(),
		})
		.send()
		.await
		.unwrap_err();

	let call_err = unwrap_call_err!(err.source().unwrap());
	assert_eq!(call_err.message(), "Invalid Transaction");

	Ok(())
}

async fn get_evm_block_from_storage(
	node_client: &OnlineClient<SrcChainConfig>,
	node_rpc_client: &RpcClient,
	block_number: U256,
) -> anyhow::Result<Block> {
	let block_hash: H256 = node_rpc_client
		.request("chain_getBlockHash", rpc_params![block_number])
		.await
		.unwrap();

	let query = subxt_client::storage().revive().ethereum_block();
	let Some(block) = node_client.storage().at(block_hash).fetch(&query).await.unwrap() else {
		return Err(anyhow!("EVM block {block_hash:?} not found"));
	};
	Ok(block.0)
}

async fn test_evm_blocks_should_match() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let node_client = SharedResources::node_client().await;
	let node_rpc_client = RpcClient::from_url(SharedResources::node_rpc_url()).await?;

	// Deploy a contract to have some interesting blocks
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.input(bytes.to_vec())
		.send()
		.await?;

	let receipt = tx.wait_for_receipt().await?;
	let block_number = receipt.block_number;
	let block_hash = receipt.block_hash;
	log::trace!(target: LOG_TARGET, "block_number = {block_number:?}");
	log::trace!(target: LOG_TARGET, "tx hash = {:?}", tx.hash());

	let evm_block_from_storage =
		get_evm_block_from_storage(&node_client, &node_rpc_client, block_number).await?;

	// Fetch the block immediately (should come from storage EthereumBlock)
	let evm_block_from_rpc_by_number = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number.into()), false)
		.await?
		.expect("Block should exist");
	let evm_block_from_rpc_by_hash =
		client.get_block_by_hash(block_hash, false).await?.expect("Block should exist");

	assert!(
		matches!(
			evm_block_from_rpc_by_number.transactions,
			pallet_revive::evm::HashesOrTransactionInfos::Hashes(_)
		),
		"Block should not have hydrated transactions"
	);

	// All EVM blocks must match
	assert_eq!(evm_block_from_storage, evm_block_from_rpc_by_number, "EVM blocks should match");
	assert_eq!(evm_block_from_storage, evm_block_from_rpc_by_hash, "EVM blocks should match");

	Ok(())
}

async fn test_evm_blocks_hydrated_should_match() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	// Deploy a contract to have some transactions in the block
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let signer = Account::default();
	let signer_copy = Account::default();
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.signer(signer)
		.input(bytes.to_vec())
		.send()
		.await?;

	let receipt = tx.wait_for_receipt().await?;
	let block_number = receipt.block_number;
	let block_hash = receipt.block_hash;
	log::trace!(target: LOG_TARGET, "block_number = {block_number:?}");
	log::trace!(target: LOG_TARGET, "tx hash = {:?}", tx.hash());

	// Fetch the block with hydrated transactions via RPC (by number and by hash)
	let evm_block_from_rpc_by_number = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number.into()), true)
		.await?
		.expect("Block should exist");
	let evm_block_from_rpc_by_hash =
		client.get_block_by_hash(block_hash, true).await?.expect("Block should exist");

	// Both blocks should be identical
	assert_eq!(
		evm_block_from_rpc_by_number, evm_block_from_rpc_by_hash,
		"Hydrated EVM blocks should match"
	);

	// Verify transaction info
	let unsigned_tx = tx
		.generic_transaction()
		.try_into_unsigned()
		.expect("Transaction shall be converted");
	let signed_tx = signer_copy.sign_transaction(unsigned_tx);
	let expected_tx_info = TransactionInfo::new(&receipt, signed_tx);

	let tx_info = if let HashesOrTransactionInfos::TransactionInfos(tx_infos) =
		evm_block_from_rpc_by_number.transactions
	{
		tx_infos[0].clone()
	} else {
		panic!("Expected hydrated transactions");
	};
	assert_eq!(expected_tx_info, tx_info, "TransationInfos should match");

	Ok(())
}

async fn test_block_hash_for_tag_with_proper_ethereum_block_hash_works() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	// Deploy a transaction to create a block with transactions
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.input(bytes.to_vec())
		.send()
		.await?;

	let receipt = tx.wait_for_receipt().await?;
	let ethereum_block_hash = receipt.block_hash;

	log::trace!(target: LOG_TARGET, "Testing with Ethereum block hash: {ethereum_block_hash:?}");

	let block_by_hash = client
		.get_block_by_hash(ethereum_block_hash, false)
		.await?
		.expect("Block should exist");

	let account = Account::default();
	let balance = client.get_balance(account.address(), ethereum_block_hash.into()).await?;

	assert!(balance >= U256::zero(), "Balance should be retrievable with Ethereum hash");
	assert_eq!(block_by_hash.hash, ethereum_block_hash, "Block hash should match");

	Ok(())
}

async fn test_block_hash_for_tag_with_invalid_ethereum_block_hash_fails() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let fake_eth_hash = H256::from([0x42u8; 32]);

	log::trace!(target: LOG_TARGET, "Testing with fake Ethereum hash: {fake_eth_hash:?}");

	let account = Account::default();
	let result = client.get_balance(account.address(), fake_eth_hash.into()).await;

	assert!(result.is_err(), "Should fail with non-existent Ethereum hash");

	Ok(())
}

async fn test_block_hash_for_tag_with_block_number_works() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;

	log::trace!(target: LOG_TARGET, "Testing with block number: {block_number}");

	let account = Account::default();
	let balance = client
		.get_balance(account.address(), BlockNumberOrTagOrHash::BlockNumber(block_number))
		.await?;

	assert!(balance >= U256::zero(), "Balance should be retrievable with block number");
	Ok(())
}

async fn test_block_hash_for_tag_with_block_tags_works() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let tags = vec![
		BlockTag::Latest,
		BlockTag::Finalized,
		BlockTag::Safe,
		BlockTag::Earliest,
		BlockTag::Pending,
	];

	for tag in tags {
		let balance = client.get_balance(account.address(), tag.into()).await?;

		assert!(balance >= U256::zero(), "Balance should be retrievable with tag {tag:?}");
	}

	Ok(())
}

async fn test_multiple_transactions_in_block() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let num_transactions = 20;
	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(1_000_000_000_000_000_000u128);

	// Prepare EVM transfer transactions
	let transactions =
		prepare_evm_transactions(client.clone(), alith, ethan.address(), amount, num_transactions)
			.await?;

	// Submit all transactions
	let submitted_txs = submit_evm_transactions(transactions).await?;
	let tx_hashes: Vec<H256> = submitted_txs.iter().map(|(hash, _, _)| *hash).collect();
	log::trace!(target: LOG_TARGET, "Submitted {} transactions", submitted_txs.len());

	// All transactions should be included in the same block since nonces are in descending order
	let first_receipt = submitted_txs[0].2.wait_for_receipt().await?;

	// Fetch and verify block contains all transactions
	verify_transactions_in_single_block(&client, first_receipt.block_number, &tx_hashes).await?;
	Ok(())
}

async fn test_mixed_evm_substrate_transactions() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let node_client = SharedResources::node_client().await;
	let num_evm_txs = 10;
	let num_substrate_txs = 7;

	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(500_000_000_000_000_000u128);

	// Prepare EVM transactions
	log::trace!(target: LOG_TARGET, "Creating {num_evm_txs} EVM transfer transactions");
	let evm_transactions =
		prepare_evm_transactions(client.clone(), alith, ethan.address(), amount, num_evm_txs)
			.await?;

	// Prepare substrate transactions (simple remarks)
	log::trace!(target: LOG_TARGET, "Creating {num_substrate_txs} substrate remark transactions");
	let alice_signer = subxt_signer::sr25519::dev::alice();

	let substrate_txs =
		prepare_substrate_transactions(&node_client, &alice_signer, num_substrate_txs).await?;

	log::trace!(target: LOG_TARGET, "Submitting {num_evm_txs} EVM and {num_substrate_txs} substrate transactions");

	// Submit EVM transactions
	let evm_submitted = submit_evm_transactions(evm_transactions).await?;
	let evm_tx_hashes: Vec<H256> = evm_submitted.iter().map(|(hash, _, _)| *hash).collect();

	// Submit substrate transactions
	let substrate_futures = submit_substrate_transactions(substrate_txs).await;

	// Wait for first EVM receipt and all substrate transactions in parallel
	let (evm_first_receipt_result, _substrate_results) = tokio::join!(
		async { evm_submitted[0].2.wait_for_receipt().await },
		futures::future::join_all(substrate_futures)
	);
	// Handle the EVM receipt result
	let evm_first_receipt = evm_first_receipt_result?;

	// Fetch and verify block contains all transactions
	verify_transactions_in_single_block(&client, evm_first_receipt.block_number, &evm_tx_hashes)
		.await?;

	Ok(())
}

async fn test_runtime_pallets_address_upload_code() -> anyhow::Result<()> {
	let client = Arc::new(SharedResources::client().await);
	let node_client = SharedResources::node_client().await;
	let node_rpc_client = RpcClient::from_url(SharedResources::node_rpc_url()).await?;

	let (bytecode, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let signer = Account::default();

	// Helper function to get substrate block hash from EVM block number
	let get_substrate_block_hash = |block_number: U256| {
		let rpc_client = node_rpc_client.clone();
		async move {
			rpc_client
				.request::<sp_core::H256>("chain_getBlockHash", rpc_params![block_number])
				.await
		}
	};

	// Step 1: Encode the Substrate upload_code call
	let upload_call = subxt::dynamic::tx(
		"Revive",
		"upload_code",
		vec![
			subxt::dynamic::Value::from_bytes(&bytecode),
			subxt::dynamic::Value::u128(u128::max_value()), // storage_deposit_limit
		],
	);
	let encoded_call = node_client.tx().call_data(&upload_call)?;

	// Step 2: Send the encoded call to RUNTIME_PALLETS_ADDR
	let tx = TransactionBuilder::new(client.clone())
		.signer(signer.clone())
		.to(pallet_revive::RUNTIME_PALLETS_ADDR)
		.input(encoded_call.clone())
		.send()
		.await?;

	// Step 3: Wait for receipt
	let receipt = tx.wait_for_receipt().await?;

	// Step 4: Verify transaction was successful
	assert_eq!(
		receipt.status.unwrap_or(U256::zero()),
		U256::one(),
		"Transaction should be successful"
	);

	// Step 5: Verify the code was actually uploaded
	let code_hash = H256(sp_io::hashing::keccak_256(&bytecode));
	let query = subxt_client::storage().revive().pristine_code(code_hash);
	let block_hash: sp_core::H256 = get_substrate_block_hash(receipt.block_number).await?;
	let stored_code = node_client.storage().at(block_hash).fetch(&query).await?;
	assert!(stored_code.is_some(), "Code with hash {code_hash:?} should exist in storage");
	assert_eq!(stored_code.unwrap(), bytecode, "Stored code should match the uploaded bytecode");

	Ok(())
}

/// Verify that subscribing to `newHeads` delivers a block header matching the
/// corresponding block fetched via `eth_getBlockByNumber` after a transaction
/// triggers a new block.
async fn test_subscribe_new_heads() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let value = U256::from(1_000_000_000_000u128);

	let mut sub = client.eth_subscribe(SubscriptionKind::NewBlockHeaders, None).await?;

	// Act
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;
	tx.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for newHeads notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let header = match notification {
		SubscriptionItem::BlockHeader(header) => header,
		other => panic!("Expected BlockHeader, got: {other:?}"),
	};

	let block = client
		.get_block_by_number(BlockNumberOrTag::U256(header.number), false)
		.await?
		.expect("Block should exist");

	// Assert
	assert!(header.number > U256::zero(), "Block number should be > 0");
	assert_ne!(header.hash, H256::zero(), "Block hash should not be zero");
	assert_ne!(header.parent_hash, H256::zero(), "Parent hash should not be zero");

	let expected_header = BlockHeader::from(block);
	assert_eq!(
		header, expected_header,
		"Subscription header should match the block header from RPC"
	);

	drop(sub);

	Ok(())
}

/// Verify that subscribing to `logs` delivers a log matching the corresponding
/// log fetched via `eth_getLogs` after a contract emits an event.
async fn test_subscribe_logs() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(Some(contract_address), receipt.contract_address);

	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, None).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;
	let call_receipt = tx.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for logs notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let log = match notification {
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	let filter = Filter { block_hash: Some(call_receipt.block_hash), ..Default::default() };
	let rpc_logs = client.get_logs(Some(filter)).await?;
	let rpc_logs: Vec<Log> = match rpc_logs {
		FilterResults::Logs(logs) => logs,
		other => panic!("Expected Logs from eth_getLogs, got: {other:?}"),
	};

	// Assert
	let event_signature = H256(sp_io::hashing::keccak_256(b"Received(address,uint256)"));
	assert_eq!(log.address, contract_address, "Log address should be the contract address");
	assert!(!log.topics.is_empty(), "Log should have at least one topic");
	assert_eq!(log.topics[0], event_signature, "First topic should be the event signature hash");
	assert_eq!(
		log.block_hash, call_receipt.block_hash,
		"Log block hash should match receipt block hash"
	);
	assert_eq!(
		log.transaction_hash, call_receipt.transaction_hash,
		"Log tx hash should match receipt tx hash"
	);
	assert!(rpc_logs.contains(&log), "Subscription log should match eth_getLogs result");

	drop(sub);
	Ok(())
}

/// Verify that subscribing to `logs` with an address filter only delivers logs
/// emitted from the specified contract address.
async fn test_subscribe_logs_with_address_filter() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(Some(contract_address), receipt.contract_address);

	let options = SubscriptionOptions::LogsOptions {
		address: Some(BoundedOneOrMany::One(contract_address)),
		topics: None,
	};
	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, Some(options)).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;
	tx.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for logs notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let log = match notification {
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	// Assert
	assert_eq!(log.address, contract_address, "Log address should match the filter address");

	drop(sub);
	Ok(())
}

/// Verify that subscribing to `logs` with a topic filter delivers logs whose
/// first topic matches the computed event signature hash.
async fn test_subscribe_logs_with_topic_filter() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(Some(contract_address), receipt.contract_address);

	let event_signature = H256(sp_io::hashing::keccak_256(b"Received(address,uint256)"));
	let options = SubscriptionOptions::LogsOptions {
		address: None,
		topics: Some(
			BoundedVec::try_from(vec![Some(BoundedOneOrMany::One(event_signature))])
				.expect("Single topic filter is within bounds"),
		),
	};
	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, Some(options)).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;
	tx.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for logs notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let log = match notification {
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	// Assert
	assert_eq!(
		log.topics[0], event_signature,
		"First topic should match the computed event signature"
	);
	assert_eq!(log.address, contract_address, "Log should come from the deployed contract");

	drop(sub);
	Ok(())
}

/// Verify that sending two sequential transactions yields two `newHeads`
/// notifications whose block numbers are increasing and whose parent hashes
/// chain correctly (the second header's `parent_hash` equals the first
/// header's `hash`).
async fn test_subscribe_new_heads_multiple_blocks() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let value = U256::from(1_000_000_000_000u128);

	let mut sub = client.eth_subscribe(SubscriptionKind::NewBlockHeaders, None).await?;

	// Act
	let tx1 = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;
	tx1.wait_for_receipt().await?;

	let tx2 = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;
	tx2.wait_for_receipt().await?;

	let header1 = match tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for first newHeads notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error")
	{
		SubscriptionItem::BlockHeader(h) => h,
		other => panic!("Expected BlockHeader, got: {other:?}"),
	};

	let header2 = match tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for second newHeads notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error")
	{
		SubscriptionItem::BlockHeader(h) => h,
		other => panic!("Expected BlockHeader, got: {other:?}"),
	};

	// Assert
	assert!(
		header2.number > header1.number,
		"Second block number ({}) should be greater than first ({})",
		header2.number,
		header1.number,
	);
	assert_eq!(
		header2.parent_hash, header1.hash,
		"Second header's parent_hash should equal first header's hash"
	);

	drop(sub);
	Ok(())
}

/// Verify that a `logs` subscription with an address filter does NOT deliver
/// logs emitted by a different contract. Two `SimpleReceiver` contracts are
/// deployed. The subscription is filtered to contract A's address. An event
/// is triggered on contract B first, then on contract A. The first
/// notification received must be from contract A, proving B's log was
/// correctly filtered out.
async fn test_subscribe_logs_address_filter_excludes_non_matching() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;

	let nonce_a = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx_a = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt_a = tx_a.wait_for_receipt().await?;
	let contract_a = create1(&account.address(), nonce_a.try_into().unwrap());
	assert_eq!(Some(contract_a), receipt_a.contract_address);

	let nonce_b = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx_b = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt_b = tx_b.wait_for_receipt().await?;
	let contract_b = create1(&account.address(), nonce_b.try_into().unwrap());
	assert_eq!(Some(contract_b), receipt_b.contract_address);
	assert_ne!(contract_a, contract_b, "The two contracts must have different addresses");

	let options = SubscriptionOptions::LogsOptions {
		address: Some(BoundedOneOrMany::One(contract_a)),
		topics: None,
	};
	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, Some(options)).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let tx_b_call = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_b)
		.send()
		.await?;
	tx_b_call.wait_for_receipt().await?;

	let tx_a_call = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_a)
		.send()
		.await?;
	tx_a_call.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for logs notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let log = match notification {
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	// Assert
	assert_eq!(log.address, contract_a, "Log must come from contract A, not contract B");
	assert_ne!(log.address, contract_b, "Log should NOT come from contract B");

	drop(sub);
	Ok(())
}

/// Verify that a `logs` subscription with a multiple-address filter (OR
/// semantics) delivers logs from both specified contracts. Two
/// `SimpleReceiver` contracts are deployed and the subscription filter
/// includes both addresses. An event is triggered on each contract and
/// both logs must be received.
async fn test_subscribe_logs_with_multiple_addresses_filter() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;

	let nonce_a = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx_a = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt_a = tx_a.wait_for_receipt().await?;
	let contract_a = create1(&account.address(), nonce_a.try_into().unwrap());
	assert_eq!(Some(contract_a), receipt_a.contract_address);

	let nonce_b = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx_b = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt_b = tx_b.wait_for_receipt().await?;
	let contract_b = create1(&account.address(), nonce_b.try_into().unwrap());
	assert_eq!(Some(contract_b), receipt_b.contract_address);

	let options = SubscriptionOptions::LogsOptions {
		address: Some(BoundedOneOrMany::Many(
			BoundedVec::try_from(vec![contract_a, contract_b])
				.expect("Two addresses is within bounds"),
		)),
		topics: None,
	};
	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, Some(options)).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let tx_a_call = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_a)
		.send()
		.await?;
	tx_a_call.wait_for_receipt().await?;

	let tx_b_call = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_b)
		.send()
		.await?;
	tx_b_call.wait_for_receipt().await?;

	let log1 = match tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for first log notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error")
	{
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	let log2 = match tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for second log notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error")
	{
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	// Assert
	let mut received_addresses = vec![log1.address, log2.address];
	received_addresses.sort();
	let mut expected_addresses = vec![contract_a, contract_b];
	expected_addresses.sort();
	assert_eq!(received_addresses, expected_addresses, "Should receive one log from each contract");

	drop(sub);
	Ok(())
}

/// Verify that a plain ETH transfer between EOAs (which emits no events)
/// does not produce a log subscription notification. The subscription must
/// only deliver the log triggered by the subsequent contract call.
async fn test_subscribe_logs_no_event_transaction_ignored() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"SimpleReceiver",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(Some(contract_address), receipt.contract_address);

	let mut sub = client.eth_subscribe(SubscriptionKind::Logs, None).await?;

	// Act
	let value = U256::from(1_000_000_000_000u128);
	let plain_tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(ethan.address())
		.send()
		.await?;
	plain_tx.wait_for_receipt().await?;

	let contract_tx = TransactionBuilder::new(client.clone())
		.value(value)
		.to(contract_address)
		.send()
		.await?;
	contract_tx.wait_for_receipt().await?;

	let notification = tokio::time::timeout(tokio::time::Duration::from_secs(10), sub.next())
		.await
		.expect("Timed out waiting for log notification")
		.expect("Subscription stream ended unexpectedly")
		.expect("Subscription returned an error");

	let log = match notification {
		SubscriptionItem::Log(log) => log,
		other => panic!("Expected Log, got: {other:?}"),
	};

	// Assert
	assert_eq!(
		log.address, contract_address,
		"First log notification must come from the contract call, not the plain transfer"
	);
	assert_eq!(
		log.transaction_hash,
		contract_tx.hash(),
		"Log transaction hash must match the contract call, not the plain transfer"
	);

	drop(sub);
	Ok(())
}

/// Verify that calling `eth_subscribe("newHeads")` with `LogsOptions`
/// returns an error, since `newHeads` does not accept filter options.
async fn test_subscribe_with_invalid_params_rejected() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);

	let options = SubscriptionOptions::LogsOptions {
		address: Some(BoundedOneOrMany::One(Account::default().address())),
		topics: None,
	};

	// Act
	let result = client.eth_subscribe(SubscriptionKind::NewBlockHeaders, Some(options)).await;

	// Assert
	assert!(result.is_err(), "newHeads with LogsOptions should be rejected");

	Ok(())
}

async fn test_estimate_gas_of_contract_with_consume_all_gas() -> anyhow::Result<()> {
	// Arrange
	let code = pallet_revive_fixtures::compile_module_with_type(
		"ContractWithConsumeAllGas",
		pallet_revive_fixtures::FixtureType::Resolc,
	)?
	.0;
	let client = Arc::new(SharedResources::client().await);
	let account = Account::default();

	let receipt = TransactionBuilder::new(client.clone())
		.input(code)
		.send()
		.await?
		.wait_for_receipt()
		.await?;
	let contract_address = receipt
		.contract_address
		.expect("Expected the transaction to publish a contract");

	// Act
	let test_function_selector = [0xf8, 0xa8, 0xfd, 0x6d].to_vec();
	let transaction = GenericTransaction {
		from: Some(account.address()),
		input: test_function_selector.into(),
		to: Some(contract_address),
		chain_id: Some(client.chain_id().await?),
		nonce: Some(
			client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?,
		),
		r#type: Some(0u8.into()),
		..Default::default()
	};
	let dry_run_result = client.estimate_gas(transaction, None).await;

	// Assert
	dry_run_result.expect("Dry run of this transaction must succeed");

	Ok(())
}

async fn test_gas_estimation_for_contract_requiring_binary_search() -> anyhow::Result<()> {
	// Arrange
	let code = pallet_revive_fixtures::compile_module_with_type(
		"ContractRequiringBinarySearchForGasEstimation",
		pallet_revive_fixtures::FixtureType::Resolc,
	)?
	.0;
	let client = Arc::new(SharedResources::client().await);

	let receipt = TransactionBuilder::new(client.clone())
		.input(code)
		.send()
		.await?
		.wait_for_receipt()
		.await?;
	let contract_address = receipt
		.contract_address
		.expect("Expected the transaction to publish a contract");

	// Act
	let main_function_selector = [0xdf, 0xfe, 0xad, 0xd0];
	let receipt = TransactionBuilder::new(client.clone())
		.to(contract_address)
		.input(main_function_selector.to_vec())
		.send()
		.await?
		.wait_for_receipt()
		.await?;

	// Assert
	assert!(receipt.is_success());

	Ok(())
}

/// Test that deploys and calls the Fibonacci contract via Substrate APIs works
async fn test_fibonacci_call_via_runtime_api() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Fibonacci;

	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"Fibonacci",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;

	let node_client =
		OnlineClient::<SrcChainConfig>::from_url(SharedResources::node_rpc_url()).await?;
	let signer = subxt_signer::sr25519::dev::alice();
	let origin: [u8; 32] = signer.public_key().0;

	// Deploy the Fibonacci contract via Substrate API
	log::trace!(target: LOG_TARGET, "Deploying Fibonacci contract via Substrate API");
	let dry_run_result = node_client
		.runtime_api()
		.at_latest()
		.await?
		.call(subxt_client::apis().revive_api().instantiate(
			subxt::utils::AccountId32(origin),
			0u128, // value
			None,  // gas_limit
			None,  // storage_deposit_limit
			subxt_client::src_chain::runtime_types::pallet_revive::primitives::Code::Upload(
				bytes.clone(),
			),
			vec![], // data (constructor args)
			None,   // salt
		))
		.await;

	assert!(dry_run_result.is_ok(), "Dry-run instantiate failed: {dry_run_result:?}");
	let dry_run = dry_run_result.unwrap();
	let instantiate_result = dry_run.result.expect("Dry-run should succeed");

	log::trace!(
		target: LOG_TARGET,
		"Dry-run succeeded: address: {:?}, gas_consumed: {:?}, weight_required: {:?}",
		instantiate_result.addr,
		dry_run.gas_consumed,
		dry_run.weight_required
	);

	// Now submit the actual instantiate extrinsic
	let events = node_client
		.tx()
		.sign_and_submit_then_watch_default(
			&subxt_client::tx().revive().instantiate_with_code(
				0u128,                   // value
				dry_run.weight_required, // weight_limit from dry-run
				u128::MAX,               // storage_deposit_limit
				bytes,                   // code
				vec![],                  // data
				None,                    // salt
			),
			&subxt_signer::sr25519::dev::alice(),
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Extract the contract address from the Instantiated event
	let instantiated_event = events
		.find_first::<subxt_client::revive::events::Instantiated>()?
		.expect("Instantiated event should be present");

	let contract_address = instantiated_event.contract;
	log::trace!(target: LOG_TARGET, "Contract deployed via Substrate at: {contract_address:?}");

	// Verify that the dry-run predicted address matches the actual deployed address
	assert_eq!(
		instantiate_result.addr, contract_address,
		"Dry-run predicted address should match actual deployed address"
	);

	// Call the deployed contract using runtime API
	let call_data = Fibonacci::fibCall { n: 3u64 }.abi_encode();
	let call_payload = subxt_client::apis().revive_api().call(
		subxt::utils::AccountId32(origin),
		contract_address,
		0u128, // value
		None,  // gas_limit
		None,  // storage_deposit_limit
		call_data,
	);

	let result = node_client.runtime_api().at_latest().await?.call(call_payload).await;

	assert!(result.is_ok(), "Contract call failed: {result:?}");
	let call_result = result.unwrap();
	let exec_result = call_result.result.expect("fib(3) should succeed");

	let decoded = Fibonacci::fibCall::abi_decode_returns(&exec_result.data)
		.expect("Failed to decode return value");
	assert_eq!(decoded, 2u64, "fib(3) should return 2");

	// Verify that large Fibonacci values run out of gas
	let call_data = Fibonacci::fibCall { n: 100u64 }.abi_encode();
	let call_payload = subxt_client::apis().revive_api().call(
		subxt::utils::AccountId32(origin),
		contract_address,
		0u128, // value
		None,  // gas_limit
		None,  // storage_deposit_limit
		call_data,
	);

	let result = node_client.runtime_api().at_latest().await?.call(call_payload).await;
	assert!(result.is_ok(), "Runtime API call failed: {result:?}");
	let call_result = result.unwrap();
	assert!(call_result.result.is_err(), "fib(100) should run out of gas");

	Ok(())
}

async fn test_gas_estimation_with_no_funds_no_gas_specified() -> anyhow::Result<()> {
	// Arrange
	let code = pallet_revive_fixtures::compile_module_with_type(
		"ContractWithConsumeAllGas",
		pallet_revive_fixtures::FixtureType::Resolc,
	)?
	.0;
	let client = Arc::new(SharedResources::client().await);
	let account = Account::from(Keypair::from_seed([0xFF; 16].as_slice()).unwrap());

	let receipt = TransactionBuilder::new(client.clone())
		.input(code)
		.send()
		.await?
		.wait_for_receipt()
		.await?;
	let contract_address = receipt
		.contract_address
		.expect("Expected the transaction to publish a contract");

	// Act
	let test_function_selector = [0xf8, 0xa8, 0xfd, 0x6d].to_vec();
	let transaction = GenericTransaction {
		from: Some(account.address()),
		input: test_function_selector.into(),
		to: Some(contract_address),
		chain_id: Some(client.chain_id().await?),
		nonce: Some(
			client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?,
		),
		r#type: Some(0u8.into()),
		..Default::default()
	};
	let dry_run_result = client.estimate_gas(transaction, None).await;

	// Assert
	dry_run_result.expect("Expected this dry run to succeed");

	Ok(())
}

/// Submit `count` EVM transfer transactions and wait for inclusion.
async fn submit_evm_transfers(count: usize) -> anyhow::Result<()> {
	let ws_client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let transactions = prepare_evm_transactions(
		ws_client.clone(),
		Account::default(),
		ethan.address(),
		U256::from(1_000_000_000_000u128),
		count,
	)
	.await?;
	let submitted = submit_evm_transactions(transactions).await?;
	submitted[0].2.wait_for_receipt().await?;
	Ok(())
}

/// Create a [`Client`] for block-sync testing.
///
/// Connects to the same dev-node that [`SharedResources`] started, but uses its own
/// in-memory SQLite database so that sync labels written by the test do not interfere
/// with the eth-rpc server's internal database (and vice versa).
async fn create_sync_test_client() -> anyhow::Result<Client> {
	use sc_cli::{RPC_DEFAULT_MAX_REQUEST_SIZE_MB, RPC_DEFAULT_MAX_RESPONSE_SIZE_MB};

	let node_url = SharedResources::node_rpc_url();
	let max_request_size = RPC_DEFAULT_MAX_REQUEST_SIZE_MB * 1024 * 1024;
	let max_response_size = RPC_DEFAULT_MAX_RESPONSE_SIZE_MB * 1024 * 1024;
	let (api, rpc_client, rpc) = connect(node_url, max_request_size, max_response_size).await?;
	let block_provider = SubxtBlockInfoProvider::new(api.clone(), rpc.clone()).await?;

	let pool = SqlitePoolOptions::new()
		.max_connections(1)
		.idle_timeout(None)
		.max_lifetime(None)
		.connect_with(SqliteConnectOptions::new().in_memory(true))
		.await?;

	let receipt_extractor = ReceiptExtractor::new(api.clone()).await?;
	let receipt_provider =
		ReceiptProvider::new(pool, block_provider.clone(), receipt_extractor, None).await?;

	let client = Client::new(api, rpc_client, rpc, block_provider, receipt_provider, true).await?;
	Ok(client)
}

/// Fresh sync: labels, hash mappings, and re-sync idempotency.
async fn test_block_sync_fresh() -> anyhow::Result<()> {
	use crate::block_sync::SyncCheckpoint;

	// Submit a transaction so the chain has at least one block with EVM data to sync.
	submit_evm_transfers(1).await?;

	let client = create_sync_test_client().await?;

	// Fresh DB — sync_state table should be empty.
	for label in [SyncLabel::Tail, SyncLabel::Head] {
		assert!(
			client.receipt_provider().get_sync_label(label).await?.is_none(),
			"sync_state[{label}] should be absent on fresh DB"
		);
	}
	for label in [ChainMetadata::Genesis, ChainMetadata::FirstEvmBlock] {
		assert!(
			client.receipt_provider().get_sync_label(label).await?.is_none(),
			"sync_state[{label}] should be absent on fresh DB"
		);
	}

	// Capture finalized before sync — Head will be set to this snapshot.
	let finalized_before_sync = client.latest_finalized_block().await.number();

	// Run the full backward sync.
	client.sync_backward().await?;

	// Genesis label must match the chain.
	let genesis = client
		.receipt_provider()
		.get_sync_label(ChainMetadata::Genesis)
		.await?
		.expect("Genesis label should be set after sync");
	assert_eq!(
		genesis,
		SyncCheckpoint::new(0, client.api().genesis_hash()),
		"Stored genesis should match chain genesis"
	);

	// Head should be exactly the finalized block at sync start.
	let sync_head = client
		.receipt_provider()
		.get_sync_label(SyncLabel::Head)
		.await?
		.expect("Head should be set after sync");
	assert_eq!(
		sync_head.block_number, finalized_before_sync,
		"Head should equal finalized at sync start"
	);

	// Tail should be genesis (block 0) — on the dev node all blocks have EVM hashes.
	let sync_tail = client
		.receipt_provider()
		.get_sync_label(SyncLabel::Tail)
		.await?
		.expect("Tail should be set after sync");
	assert_eq!(sync_tail, genesis, "Tail should be genesis");

	// On the dev node all blocks (including genesis) have EVM hashes
	let evm_first = client.receipt_provider().get_sync_label(ChainMetadata::FirstEvmBlock).await?;
	assert!(evm_first.is_none(), "FirstEvmBlock should not be set when all blocks are EVM");
	assert_eq!(client.receipt_provider().first_evm_block(), None);

	// Block hash mappings should be queryable after sync.
	let finalized = client.latest_finalized_block().await;
	let substrate_hash = finalized.hash();
	let ethereum_hash = client.receipt_provider().get_ethereum_hash(&substrate_hash).await;
	assert!(
		ethereum_hash.is_some(),
		"Finalized block #{} should have an ethereum hash mapping after sync",
		finalized.number(),
	);
	assert_eq!(
		client.receipt_provider().get_substrate_hash(&ethereum_hash.unwrap()).await,
		Some(substrate_hash),
		"Reverse mapping should resolve back to the substrate hash"
	);

	// Re-syncing should complete without errors (exercises the resume path).
	client.sync_backward().await?;
	let sync_head_after = client
		.receipt_provider()
		.get_sync_label(SyncLabel::Head)
		.await?
		.expect("Head should exist after re-sync");
	assert!(
		sync_head_after.block_number >= sync_head.block_number,
		"Head should not regress after re-sync"
	);

	Ok(())
}

async fn test_gas_estimation_with_no_funds_and_with_gas_specified() -> anyhow::Result<()> {
	// Arrange
	let code = pallet_revive_fixtures::compile_module_with_type(
		"ContractWithConsumeAllGas",
		pallet_revive_fixtures::FixtureType::Resolc,
	)?
	.0;
	let client = Arc::new(SharedResources::client().await);
	let account = Account::from(Keypair::from_seed([0xFF; 16].as_slice()).unwrap());

	let receipt = TransactionBuilder::new(client.clone())
		.input(code)
		.send()
		.await?
		.wait_for_receipt()
		.await?;
	let contract_address = receipt
		.contract_address
		.expect("Expected the transaction to publish a contract");

	// Act
	let test_function_selector = [0xf8, 0xa8, 0xfd, 0x6d].to_vec();
	let transaction = GenericTransaction {
		from: Some(account.address()),
		input: test_function_selector.into(),
		to: Some(contract_address),
		chain_id: Some(client.chain_id().await?),
		nonce: Some(
			client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?,
		),
		r#type: Some(0u8.into()),
		gas: Some(U256::from(100_000_000u64)),
		..Default::default()
	};
	let dry_run_result = client.estimate_gas(transaction, None).await;

	// Assert
	assert!(matches!(
		dry_run_result, Err(ClientError::Call(error_object))
		if error_object.message().contains("insufficient funds for gas")
	));

	Ok(())
}

/// Simulate an interrupted sync by manually setting both Head and Tail
/// to create a top gap and a bottom gap, then verify that `resume_sync` fills both.
async fn test_block_sync_resume_interrupted() -> anyhow::Result<()> {
	use crate::block_sync::SyncCheckpoint;

	// Submit transactions so the chain has enough blocks for the 1/3 and 2/3 split.
	submit_evm_transfers(6).await?;

	let client = create_sync_test_client().await?;

	// Complete a fresh sync so the DB has all blocks and labels.
	client.sync_backward().await?;

	// Pick two blocks to simulate partial coverage: tail at 1/3, head at 2/3.
	let chain_len = client.latest_finalized_block().await.number();

	let tail_num = chain_len / 3;
	let tail_block = client
		.block_provider()
		.block_by_number(tail_num)
		.await?
		.expect("Tail block should exist");

	let head_num = chain_len * 2 / 3;
	let head_block = client
		.block_provider()
		.block_by_number(head_num)
		.await?
		.expect("Head block should exist");

	// Overwrite both labels to simulate an interrupted sync with a partial range.
	let interrupted_tail = SyncCheckpoint::new(tail_block.number(), tail_block.hash());
	let interrupted_head = SyncCheckpoint::new(head_block.number(), head_block.hash());

	client
		.receipt_provider()
		.set_sync_label(SyncLabel::Tail, interrupted_tail)
		.await?;
	client
		.receipt_provider()
		.set_sync_label(SyncLabel::Head, interrupted_head)
		.await?;

	// Capture finalized before resume — Head will be set to this snapshot.
	let finalized_before_resume = client.latest_finalized_block().await.number();

	// Resume sync — fills top gap and bottom gap.
	client.sync_backward().await?;

	// After resume, Head should be at the finalized block that was current at resume start.
	let new_head = client
		.receipt_provider()
		.get_sync_label(SyncLabel::Head)
		.await?
		.expect("Head should exist after resume");
	assert_eq!(
		new_head.block_number, finalized_before_resume,
		"Head should equal finalized at resume start",
	);

	// Tail should reach genesis (bottom gap fully filled).
	let new_tail = client
		.receipt_provider()
		.get_sync_label(SyncLabel::Tail)
		.await?
		.expect("Tail should exist after resume");
	assert_eq!(
		new_tail.block_number, 0,
		"Tail should be 0 after resume fills the bottom gap, got #{}",
		new_tail.block_number,
	);

	log::debug!(
		target: LOG_TARGET,
		"Resume interrupted OK: simulated partial range #{}..#{}, \
		 after resume tail=#{}, head=#{}",
		interrupted_tail.block_number,
		interrupted_head.block_number,
		new_tail.block_number,
		new_head.block_number,
	);

	Ok(())
}

/// Corrupted sync labels should be detected on resume:
/// - Fake Genesis hash → `ChainMismatch`
/// - Fake Head hash → `SyncBoundaryMismatch`
async fn test_block_sync_detects_corruption() -> anyhow::Result<()> {
	use crate::{block_sync::SyncCheckpoint, client::ClientError};

	// Submit transactions so the chain has enough blocks for the boundary test.
	submit_evm_transfers(2).await?;

	let client = create_sync_test_client().await?;

	// Complete a fresh sync so all labels are stored.
	client.sync_backward().await?;

	// --- ChainMismatch: overwrite Genesis with a fake hash ---
	let fake_genesis = SyncCheckpoint::new(0, H256::from([0xdeu8; 32]));
	client
		.receipt_provider()
		.set_sync_label(ChainMetadata::Genesis, fake_genesis)
		.await?;

	let err = client.sync_backward().await.unwrap_err();
	assert!(matches!(err, ClientError::ChainMismatch), "Expected ChainMismatch, got: {err:?}");

	// Restore the real genesis so we can test the next corruption.
	let real_genesis = SyncCheckpoint::new(0, client.api().genesis_hash());
	client
		.receipt_provider()
		.set_sync_label(ChainMetadata::Genesis, real_genesis)
		.await?;

	// --- SyncBoundaryMismatch: corrupted Head hash ---
	let chain_len = client.latest_finalized_block().await.number();
	let corrupted_upper = SyncCheckpoint::new(chain_len / 2, H256::from([0xbau8; 32]));
	client
		.receipt_provider()
		.set_sync_label(SyncLabel::Head, corrupted_upper)
		.await?;

	let err = client.sync_backward().await.unwrap_err();
	assert!(
		matches!(err, ClientError::SyncBoundaryMismatch),
		"Expected SyncBoundaryMismatch, got: {err:?}"
	);

	Ok(())
}

/// Syncing a second client after new transactions have been submitted
/// should include the newer blocks.
async fn test_block_sync_picks_up_new_blocks() -> anyhow::Result<()> {
	// First sync: snapshot the current chain state.
	let client1 = create_sync_test_client().await?;
	let finalized1 = client1.latest_finalized_block().await.number();

	client1.sync_backward().await?;

	// Submit a transaction to produce at least one new block.
	submit_evm_transfers(1).await?;

	// Second sync: new client with fresh DB should see the new blocks.
	let client2 = create_sync_test_client().await?;
	let finalized2 = client2.latest_finalized_block().await;

	client2.sync_backward().await?;
	assert!(
		finalized2.number() > finalized1,
		"Second finalized #{} should be higher than first #{finalized1}",
		finalized2.number(),
	);

	// The new block should have an ethereum hash mapping in client2's DB.
	assert!(
		client2.receipt_provider().get_ethereum_hash(&finalized2.hash()).await.is_some(),
		"New finalized block #{} should be synced in client2",
		finalized2.number(),
	);

	log::debug!(
		target: LOG_TARGET,
		"Picks up new blocks OK: client2 synced up to #{}, earliest=#{}",
		finalized2.number(),
		client2.receipt_provider().first_evm_block().unwrap_or(0),
	);

	Ok(())
}

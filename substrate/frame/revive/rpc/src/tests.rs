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
	cli::{self, CliCommand},
	client,
	example::TransactionBuilder,
	subxt_client::{
		self, src_chain::runtime_types::pallet_revive::primitives::Code, SrcChainConfig,
	},
	EthRpcClient,
};
use anyhow::anyhow;
use clap::Parser;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use pallet_revive::{
	create1,
	evm::{
		Account, Block, BlockNumberOrTag, BlockNumberOrTagOrHash, BlockTag,
		HashesOrTransactionInfos, TransactionInfo, H256, U256,
	},
};
use static_init::dynamic;
use std::{collections::BTreeMap, sync::Arc, thread};
use subxt::{
	backend::rpc::RpcClient,
	ext::subxt_rpcs::rpc_params,
	tx::{SubmittableTransaction, TxStatus},
	OnlineClient,
};

const LOG_TARGET: &str = "eth-rpc-tests";

/// Create a websocket client with a 120s timeout.
async fn ws_client_with_retry(url: &str) -> WsClient {
	let timeout = tokio::time::Duration::from_secs(120);
	tokio::time::timeout(timeout, async {
		loop {
			if let Ok(client) = WsClientBuilder::default().build(url).await {
				return client
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

	fn node_rpc_url() -> &'static str {
		"ws://localhost:45789"
	}
}

#[dynamic(lazy)]
static mut SHARED_RESOURCES: SharedResources = SharedResources::start();

macro_rules! unwrap_call_err(
	($err:expr) => {
		match $err.downcast_ref::<jsonrpsee::core::client::Error>().unwrap() {
			jsonrpsee::core::client::Error::Call(call) => call,
			_ => panic!("Expected Call error"),
		}
	}
);

// Helper functions
/// Prepare multiple EVM transfer transactions with sequential nonces
async fn prepare_evm_transactions<Client: EthRpcClient + Sync + Send>(
	client: &Arc<Client>,
	signer: Account,
	recipient: pallet_revive::evm::Address,
	amount: U256,
	count: usize,
) -> anyhow::Result<Vec<TransactionBuilder<Client>>> {
	let mut nonce = client.get_transaction_count(signer.address(), BlockTag::Latest.into()).await?;

	let mut transactions = Vec::new();
	for i in 0..count {
		let tx_builder = TransactionBuilder::new(client)
			.signer(signer.clone())
			.nonce(nonce)
			.value(amount)
			.to(recipient);

		transactions.push(tx_builder);
		log::trace!(target: LOG_TARGET, "Prepared EVM transaction {}/{count} with nonce: {nonce:?}", i + 1);
		nonce = nonce.saturating_add(U256::one());
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

/// Wait for all submitted transactions to be included in blocks
async fn wait_for_receipts<Client: EthRpcClient + Sync + Send>(
	submitted_txs: Vec<(
		H256,
		pallet_revive::evm::GenericTransaction,
		crate::example::SubmittedTransaction<Client>,
	)>,
) -> anyhow::Result<
	Vec<(H256, pallet_revive::evm::GenericTransaction, pallet_revive::evm::ReceiptInfo)>,
> {
	let wait_futures: Vec<_> = submitted_txs
		.into_iter()
		.map(|(hash, generic_tx, tx)| async move {
			let receipt = tx.wait_for_receipt().await?;
			Ok::<_, anyhow::Error>((hash, generic_tx, receipt))
		})
		.collect();

	let results = futures::future::join_all(wait_futures).await;
	results.into_iter().collect()
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

/// Verify that each transaction exists in its block and is accessible via RPC
/// Takes a map of block_number -> transaction_hashes for efficient verification
async fn verify_transactions_in_blocks<Client: EthRpcClient + Sync>(
	client: &Arc<Client>,
	blocks_map: &BTreeMap<U256, Vec<H256>>,
) -> anyhow::Result<()> {
	for (block_number, expected_tx_hashes) in blocks_map {
		// Fetch the block once for all transactions
		let block = client
			.get_block_by_number(BlockNumberOrTag::U256(*block_number), false)
			.await?
			.ok_or_else(|| anyhow!("Block {block_number} should exist"))?;

		let block_tx_hashes = match &block.transactions {
			pallet_revive::evm::HashesOrTransactionInfos::Hashes(hashes) => hashes.clone(),
			pallet_revive::evm::HashesOrTransactionInfos::TransactionInfos(infos) =>
				infos.iter().map(|info| info.hash).collect(),
		};

		// Verify each expected transaction is in the block
		for expected_hash in expected_tx_hashes {
			assert!(
				block_tx_hashes.contains(expected_hash),
				"Block {block_number} should contain transaction {expected_hash:?}"
			);

			// Verify we can fetch the transaction by hash
			let tx = client.get_transaction_by_hash(*expected_hash).await?.ok_or_else(|| {
				anyhow!("Transaction {expected_hash:?} should be retrievable by hash")
			})?;

			assert_eq!(
				tx.block_number, *block_number,
				"Transaction {expected_hash:?} should be in block {block_number}"
			);
		}
	}

	Ok(())
}

#[tokio::test]
async fn transfer() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);

	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let initial_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;

	let value = 1_000_000_000_000_000_000_000u128.into();
	let tx = TransactionBuilder::new(&client).value(value).to(ethan.address()).send().await?;

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

#[tokio::test]
async fn deploy_and_call() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = std::sync::Arc::new(SharedResources::client().await);
	let account = Account::default();

	// Balance transfer
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let initial_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	let value = 1_000_000_000_000_000_000_000u128.into();
	let tx = TransactionBuilder::new(&client).value(value).to(ethan.address()).send().await?;

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
	let tx = TransactionBuilder::new(&client).value(value).input(input).send().await?;
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
	let tx = TransactionBuilder::new(&client)
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
	assert_eq!(Some(value), balance.checked_sub(initial_balance), "Contract {contract_address:?} Balance {balance} should have increased from {initial_balance} by {value}.");

	// Balance transfer to contract
	let initial_balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(&client)
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

#[tokio::test]
async fn runtime_api_dry_run_addr_works() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = std::sync::Arc::new(SharedResources::client().await);

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

	let c = OnlineClient::<SrcChainConfig>::from_url("ws://localhost:45789").await?;
	let res = c.runtime_api().at_latest().await?.call(payload).await?.result.unwrap();

	assert_eq!(res.addr, contract_address);
	Ok(())
}

#[tokio::test]
async fn invalid_transaction() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);
	let ethan = Account::from(subxt_signer::eth::dev::ethan());

	let err = TransactionBuilder::new(&client)
		.value(U256::from(1_000_000_000_000u128))
		.to(ethan.address())
		.mutate(|tx| tx.chain_id = Some(42u32.into()))
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

#[tokio::test]
async fn evm_blocks_should_match() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = std::sync::Arc::new(SharedResources::client().await);

	let (node_client, node_rpc_client, _) =
		client::connect(SharedResources::node_rpc_url()).await.unwrap();

	// Deploy a contract to have some interesting blocks
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let tx = TransactionBuilder::new(&client)
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

#[tokio::test]
async fn evm_blocks_hydrated_should_match() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = std::sync::Arc::new(SharedResources::client().await);

	// Deploy a contract to have some transactions in the block
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let signer = Account::default();
	let signer_copy = Account::default();
	let tx = TransactionBuilder::new(&client)
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

#[tokio::test]
async fn block_hash_for_tag_with_proper_ethereum_block_hash_works() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);

	// Deploy a transaction to create a block with transactions
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let value = U256::from(5_000_000_000_000u128);
	let tx = TransactionBuilder::new(&client)
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

#[tokio::test]
async fn block_hash_for_tag_with_invalid_ethereum_block_hash_fails() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);

	let fake_eth_hash = H256::from([0x42u8; 32]);

	log::trace!(target: LOG_TARGET, "Testing with fake Ethereum hash: {fake_eth_hash:?}");

	let account = Account::default();
	let result = client.get_balance(account.address(), fake_eth_hash.into()).await;

	assert!(result.is_err(), "Should fail with non-existent Ethereum hash");

	Ok(())
}

#[tokio::test]
async fn block_hash_for_tag_with_block_number_works() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
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

#[tokio::test]
async fn block_hash_for_tag_with_block_tags_works() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
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
		let balance = client.get_balance(account.address(), tag.clone().into()).await?;

		assert!(balance >= U256::zero(), "Balance should be retrievable with tag {tag:?}");
	}

	Ok(())
}

#[tokio::test]
async fn test_multiple_transactions_in_block() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);

	let num_transactions = 20;
	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(1_000_000_000_000_000_000u128);

	// Prepare EVM transfer transactions
	let transactions =
		prepare_evm_transactions(&client, alith, ethan.address(), amount, num_transactions).await?;

	// Submit all transactions
	let submitted_txs = submit_evm_transactions(transactions).await?;
	log::trace!(target: LOG_TARGET, "Submitted {} transactions", submitted_txs.len());

	// Wait for all receipts in parallel
	let results = wait_for_receipts(submitted_txs).await?;

	// Verify all transactions were successful and group by block
	// Most of the times all transactions will be in a single block,
	// but we cannot guarantee that
	let mut blocks_map: BTreeMap<U256, Vec<H256>> = BTreeMap::new();
	let mut total_txs = 0;

	for (i, (hash, _generic_tx, receipt)) in results.into_iter().enumerate() {
		log::trace!(target: LOG_TARGET, "Transaction {}: hash={hash:?}, block={}", i + 1, receipt.block_number);
		assert_eq!(
			receipt.status.unwrap_or(U256::zero()),
			U256::one(),
			"Transaction should be successful"
		);

		blocks_map.entry(receipt.block_number).or_insert_with(Vec::new).push(hash);
		total_txs += 1;
	}

	log::trace!(target: LOG_TARGET, "All {} transactions successful, spanning {} block(s):", total_txs, blocks_map.len());

	// Print transaction distribution across blocks (BTreeMap keeps them sorted)
	for (block_num, tx_hashes) in &blocks_map {
		log::trace!(target: LOG_TARGET, "  Block {}: {} transaction(s) - {:?}", block_num, tx_hashes.len(), tx_hashes);
	}

	// Verify each transaction exists in its respective block
	verify_transactions_in_blocks(&client, &blocks_map).await?;

	Ok(())
}

#[tokio::test]
async fn test_mixed_evm_substrate_transactions() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);

	let num_evm_txs = 10;
	let num_substrate_txs = 7;

	let alith = Account::default();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let amount = U256::from(500_000_000_000_000_000u128);

	// Prepare EVM transactions
	log::trace!(target: LOG_TARGET, "Creating {num_evm_txs} EVM transfer transactions");
	let evm_transactions =
		prepare_evm_transactions(&client, alith, ethan.address(), amount, num_evm_txs).await?;

	// Prepare substrate transactions (simple remarks)
	log::trace!(target: LOG_TARGET, "Creating {num_substrate_txs} substrate remark transactions");
	let alice_signer = subxt_signer::sr25519::dev::alice();
	let (node_client, _, _) = client::connect(SharedResources::node_rpc_url()).await.unwrap();

	let substrate_txs =
		prepare_substrate_transactions(&node_client, &alice_signer, num_substrate_txs).await?;

	log::trace!(target: LOG_TARGET, "Submitting {num_evm_txs} EVM and {num_substrate_txs} substrate transactions");

	// Submit EVM transactions
	let evm_submitted = submit_evm_transactions(evm_transactions).await?;

	// Submit substrate transactions
	let substrate_futures = submit_substrate_transactions(substrate_txs).await;

	// Wait for all transactions in parallel
	let (evm_results, _substrate_results) = tokio::join!(
		wait_for_receipts(evm_submitted),
		futures::future::join_all(substrate_futures)
	);

	// Handle results
	let evm_results = evm_results?;

	// Verify all EVM transactions were successful and group by block
	// Most of the times all transactions will be in a single block,
	// but we cannot guarantee that
	let mut blocks_map: BTreeMap<U256, Vec<H256>> = BTreeMap::new();
	let mut total_txs = 0;

	for (i, (hash, _generic_tx, receipt)) in evm_results.into_iter().enumerate() {
		log::trace!(target: LOG_TARGET, "EVM Transaction {}: hash={hash:?}, block={}", i + 1, receipt.block_number);
		assert_eq!(
			receipt.status.unwrap_or(U256::zero()),
			U256::one(),
			"EVM transaction should be successful"
		);

		blocks_map.entry(receipt.block_number).or_insert_with(Vec::new).push(hash);
		total_txs += 1;
	}

	log::trace!(target: LOG_TARGET,
		"All {} EVM transactions successful, spanning {} block(s):",
		total_txs,
		blocks_map.len()
	);

	// Print transaction distribution across blocks
	for (block_num, tx_hashes) in &blocks_map {
		log::trace!(target: LOG_TARGET, "Block {}: {} transaction(s) - {:?}", block_num, tx_hashes.len(), tx_hashes);
	}

	// Verify each EVM transaction exists in its respective block
	verify_transactions_in_blocks(&client, &blocks_map).await?;

	Ok(())
}

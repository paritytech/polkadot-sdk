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
		Account, AddressStateOverride, Block, BlockNumberOrTag, BlockNumberOrTagOrHash,
		BlockOverrides, BlockTag, Bytes, Bytes32, GenericTransaction, HashesOrTransactionInfos,
		Log, SimulationCallResult, SimulationParameters, SimulationPayload, StateOverrides,
		StorageOverrides, TransactionInfo, TransactionUnsigned, H160, H256, U256,
	},
};
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
		test_fibonacci_large_value_runs_out_of_gas,
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
		test_simulate_v1_simple,
		test_simulate_v1_simple_multi_block,
		test_simulate_v1_insufficient_funds,
		test_simulate_v1_block_overrides,
		test_simulate_v1_block_number_order,
		test_simulate_v1_state_override_balance,
		test_simulate_v1_state_override_code,
		test_simulate_v1_state_override_nonce,
		test_simulate_v1_state_override_balance_and_code,
		test_simulate_v1_validation_nonce_too_high,
		test_simulate_v1_validation_success,
		test_simulate_v1_chain_linkage,
		test_simulate_v1_tx_sender,
		test_simulate_v1_block_overrides_base_fee,
		test_simulate_v1_filler_blocks,
		test_simulate_v1_block_overrides_fee_recipient,
		test_simulate_v1_nonce_management,
		test_simulate_v1_per_block_state_overrides,
		test_simulate_v1_contract_deploy,
		test_simulate_v1_contract_deploy_then_call,
		test_simulate_v1_contract_revert,
		test_simulate_v1_storage_persistence_across_calls,
		test_simulate_v1_call_with_value_to_contract,
		test_simulate_v1_call_to_empty_address,
		test_simulate_v1_trace_transfers_simple,
		test_simulate_v1_trace_transfers_disabled,
		test_simulate_v1_trace_transfers_multiple_transfers,
		test_simulate_v1_trace_transfers_zero_value_no_log,
		test_simulate_v1_trace_transfers_multi_block,
		test_simulate_v1_trace_transfers_contract_with_value,
		test_simulate_v1_trace_transfers_reverted_call,
		test_simulate_v1_block_override_timestamp,
		test_simulate_v1_block_override_gas_limit,
		test_simulate_v1_block_override_prev_randao,
		test_simulate_v1_block_override_all_fields,
		test_simulate_v1_timestamp_ordering_failure,
		test_simulate_v1_timestamp_equal_allowed,
		test_simulate_v1_state_override_storage_state_diff,
		test_simulate_v1_state_override_multiple_accounts,
		test_simulate_v1_state_override_balance_zero,
		test_simulate_v1_state_override_nonce_reset,
		test_simulate_v1_state_override_code_replace_existing,
		test_simulate_v1_state_override_balance_nonce_code_combined,
		test_simulate_v1_state_changes_persist_but_overrides_dont,
		test_simulate_v1_validation_nonce_too_low,
		test_simulate_v1_validation_fee_cap_too_low,
		test_simulate_v1_validation_tip_above_fee_cap,
		test_simulate_v1_validation_insufficient_funds,
		test_simulate_v1_conflicting_fee_fields,
		test_simulate_v1_chain_id_mismatch,
		test_simulate_v1_validation_not_applied_without_flag,
		test_simulate_v1_filler_blocks_chain_linkage,
		test_simulate_v1_block_capacity_exceeded,
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
	assert_eq!(Some(value), balance.checked_sub(initial_balance), "Contract {contract_address:?} Balance {balance} should have increased from {initial_balance} by {value}.");

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
		let balance = client.get_balance(account.address(), tag.clone().into()).await?;

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

async fn test_fibonacci_large_value_runs_out_of_gas() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Fibonacci;

	let client = Arc::new(SharedResources::client().await);
	let (bytes, _) = pallet_revive_fixtures::compile_module_with_type(
		"Fibonacci",
		pallet_revive_fixtures::FixtureType::Solc,
	)?;

	let account = Account::default();
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(client.clone()).input(bytes.to_vec()).send().await?;
	let receipt = tx.wait_for_receipt().await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(Some(contract_address), receipt.contract_address);

	let result = TransactionBuilder::new(client.clone())
		.to(contract_address)
		.input(Fibonacci::fibCall { n: 100u64 }.abi_encode())
		.eth_call()
		.await;

	let err = result.expect_err("fib(100) should run out of gas");
	assert!(err.to_string().contains("OutOfGas"), "expected OutOfGas error, got: {err}");

	Ok(())
}

/// State build-up over calls within a single block: first transfer uses state-overridden balance,
/// second transfer uses balance received from the first call.
///
/// Reference: Geth TestSimulateV1 "simple" test case.
async fn test_simulate_v1_simple() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xa1; 20]);
	let receiver1 = H160::from([0xa2; 20]);
	let receiver2 = H160::from([0xa3; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![
				GenericTransaction {
					from: Some(sender),
					to: Some(receiver1),
					value: Some(U256::from(1000)),
					..Default::default()
				},
				GenericTransaction {
					from: Some(receiver1),
					to: Some(receiver2),
					value: Some(U256::from(1000)),
					..Default::default()
				},
				GenericTransaction {
					from: Some(sender),
					to: Some(receiver2),
					..Default::default()
				},
			],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 3);
	for (i, call) in response.0[0].calls.iter().enumerate() {
		assert!(matches!(call, SimulationCallResult::Success { .. }), "Call {i} should succeed");
	}

	Ok(())
}

/// State build-up across multiple blocks: second block's state override zeroes out a balance,
/// while another account uses balance it received in the first block.
///
/// Reference: Geth TestSimulateV1 "simple-multi-block" test case.
async fn test_simulate_v1_simple_multi_block() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xa1; 20]);
	let receiver1 = H160::from([0xa2; 20]);
	let receiver2 = H160::from([0xa3; 20]);
	let receiver3 = H160::from([0xa4; 20]);

	let mut block0_overrides = BTreeMap::new();
	block0_overrides.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let mut block1_overrides = BTreeMap::new();
	block1_overrides.insert(
		receiver3,
		AddressStateOverride {
			balance: Some(U256::zero()),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block0_overrides)),
				calls: vec![
					GenericTransaction {
						from: Some(sender),
						to: Some(receiver1),
						value: Some(U256::from(1000)),
						..Default::default()
					},
					GenericTransaction {
						from: Some(sender),
						to: Some(receiver3),
						value: Some(U256::from(1000)),
						..Default::default()
					},
				],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block1_overrides)),
				calls: vec![GenericTransaction {
					from: Some(receiver1),
					to: Some(receiver2),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	assert_eq!(response.0[0].calls.len(), 2);
	assert_eq!(response.0[1].calls.len(), 1);
	for block in &response.0 {
		for (i, call) in block.calls.iter().enumerate() {
			assert!(
				matches!(call, SimulationCallResult::Success { .. }),
				"Call {i} should succeed"
			);
		}
	}
	assert_eq!(
		response.0[1].number,
		response.0[0].number + U256::one(),
		"Block numbers should be sequential"
	);

	Ok(())
}

/// Transfer from an unfunded account should result in an error.
///
/// Reference: Geth TestSimulateV1 "insufficient-funds" test case.
async fn test_simulate_v1_insufficient_funds() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let empty_account = H160::from([0xcc; 20]);
	let recipient = H160::from([0xdd; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(empty_account),
				to: Some(recipient),
				value: Some(U256::from(1_000_000_000_000_000_000u128)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await?;

	// Assert
	assert!(result.0.first().unwrap().calls.first().unwrap().is_failure());

	Ok(())
}

/// Block overrides for number and fee recipient are applied to simulated blocks.
///
/// Reference: Geth TestSimulateV1 "block-overrides" test case.
async fn test_simulate_v1_block_overrides() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let fee_recipient = H160::from([0x0c; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					number: Some(block_number + U256::one()),
					fee_recipient: Some(fee_recipient),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	assert_eq!(response.0[0].number, block_number + U256::one());
	assert_eq!(response.0[0].miner, fee_recipient);
	assert_eq!(response.0[1].number, block_number + U256::from(2));

	Ok(())
}

/// Block numbers must be in ascending order; passing a lower number after a higher one fails.
///
/// Reference: Geth TestSimulateV1 "block-number-order" test case.
async fn test_simulate_v1_block_number_order() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					number: Some(block_number + U256::from(3)),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					number: Some(block_number + U256::from(2)),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Block numbers out of order should fail");

	Ok(())
}

/// A state override giving balance to an unfunded account allows it to send a transfer.
///
/// Reference: Geth TestSimulateV1 "simple" test case (state override balance component).
async fn test_simulate_v1_state_override_balance() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let random_sender = H160::from([0xaa; 20]);
	let recipient = H160::from([0xbb; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		random_sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(random_sender),
				to: Some(recipient),
				value: Some(U256::from(1_000_000_000_000u128)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }));

	Ok(())
}

/// A state override deploying code to an address allows calling it as a contract.
///
/// Reference: Geth TestSimulateV1 "storage-contract" / "evm-error" test cases.
async fn test_simulate_v1_state_override_code() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc0; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(caller.address()),
				to: Some(contract_addr),
				input: Callee::echoCall { _data: 42 }.abi_encode().into(),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }));
	if let SimulationCallResult::Success { return_data, .. } = &response.0[0].calls[0] {
		assert!(!return_data.0.is_empty(), "Contract should return data");
		assert_eq!(return_data.0[31], 42, "echo(42) should return 42");
	}

	Ok(())
}

/// A state override setting a nonce is applied before execution.
///
/// Reference: Geth TestSimulateV1 "validation-checks-from-contract" test case (nonce override).
async fn test_simulate_v1_state_override_nonce() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let recipient = H160::from([0xb2; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000_000u128)),
			nonce: Some(U256::from(5)),
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				nonce: Some(U256::from(5)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Transfer with matching overridden nonce should succeed"
	);

	Ok(())
}

/// A state override combining balance and code on the same account allows contract interaction
/// with funded calls.
///
/// Reference: Geth TestSimulateV1 "simple" test case (balance + code on same account).
async fn test_simulate_v1_state_override_balance_and_code() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc1; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(caller.address()),
				to: Some(contract_addr),
				input: Callee::echoCall { _data: 42 }.abi_encode().into(),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }));

	Ok(())
}

/// In validation mode, a nonce that is too high causes a simulation error.
///
/// Reference: Geth TestSimulateV1 "validation-checks" test case.
async fn test_simulate_v1_validation_nonce_too_high() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				nonce: Some(U256::from(999_999)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Simulation with wrong nonce in validation mode should fail");

	Ok(())
}

/// In validation mode with proper balance, nonce, gas pricing, and base fee override,
/// the simulation succeeds.
///
/// Reference: Geth TestSimulateV1 "validation-checks-success" test case.
async fn test_simulate_v1_validation_success() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xd1; 20]);
	let recipient = H160::from([0xd2; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::one()),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				max_fee_per_gas: Some(U256::from(50000000000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Validation mode with sufficient balance and correct fees should succeed"
	);

	Ok(())
}

/// Simulated blocks maintain proper cryptographic parent-child hash linkage.
///
/// Reference: Geth TestSimulateV1ChainLinkage test.
async fn test_simulate_v1_chain_linkage() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let recipient = H160::from([0xee; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(recipient),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(recipient),
					value: Some(U256::from(2000)),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(recipient),
					value: Some(U256::from(3000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 3);
	assert_eq!(response.0[1].parent_hash, response.0[0].hash);
	assert_eq!(response.0[2].parent_hash, response.0[1].hash);
	assert_ne!(response.0[0].hash, H256::zero());
	assert_ne!(response.0[1].hash, H256::zero());
	assert_ne!(response.0[2].hash, H256::zero());

	Ok(())
}

/// Multiple senders across multiple blocks are handled correctly.
///
/// Reference: Geth TestSimulateV1TxSender test.
async fn test_simulate_v1_tx_sender() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender1 = Account::default();
	let sender2 = Account::from(subxt_signer::eth::dev::baltathar());
	let sender3 = Account::from(subxt_signer::eth::dev::ethan());
	let recipient = H160::from([0xbb; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![
					GenericTransaction {
						from: Some(sender1.address()),
						to: Some(recipient),
						value: Some(U256::from(1000)),
						..Default::default()
					},
					GenericTransaction {
						from: Some(sender2.address()),
						to: Some(recipient),
						value: Some(U256::from(2000)),
						..Default::default()
					},
					GenericTransaction {
						from: Some(sender3.address()),
						to: Some(recipient),
						value: Some(U256::from(3000)),
						..Default::default()
					},
				],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(sender2.address()),
					to: Some(recipient),
					value: Some(U256::from(4000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	assert_eq!(response.0[0].calls.len(), 3);
	assert_eq!(response.0[1].calls.len(), 1);
	for (i, call) in response.0[0].calls.iter().enumerate() {
		assert!(matches!(call, SimulationCallResult::Success { .. }), "Block 0 call {i}");
	}
	assert!(matches!(&response.0[1].calls[0], SimulationCallResult::Success { .. }));

	Ok(())
}

/// Block override for base_fee_per_gas is applied to the simulated block.
///
/// Reference: Geth TestSimulateV1 "basefee-non-validation" test case.
async fn test_simulate_v1_block_overrides_base_fee() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let base_fee_override = U256::from(42);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(base_fee_override),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].base_fee_per_gas, base_fee_override);

	Ok(())
}

/// When block number overrides create a gap, filler blocks are generated to fill it.
///
/// Reference: Geth TestSimulateV1 "blockhash-opcode" test case.
async fn test_simulate_v1_filler_blocks() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let gap = U256::from(5);
	let override_number = block_number + gap;

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(override_number),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	let total_blocks = gap.as_usize();
	assert_eq!(response.0.len(), total_blocks, "Expected {total_blocks} blocks");
	for filler in &response.0[..total_blocks - 1] {
		assert!(filler.calls.is_empty(), "Filler blocks should have no calls");
	}
	let user_block = &response.0[total_blocks - 1];
	assert_eq!(user_block.number, override_number);
	assert_eq!(user_block.calls.len(), 1);
	assert!(matches!(&user_block.calls[0], SimulationCallResult::Success { .. }));
	for i in 1..response.0.len() {
		assert_eq!(
			response.0[i].number,
			response.0[i - 1].number + U256::one(),
			"Block numbers should be sequential"
		);
	}

	Ok(())
}

/// Block override for fee_recipient (miner/coinbase) is applied to the simulated block.
///
/// Reference: Geth TestSimulateV1 "block-overrides" test case (FeeRecipient component).
async fn test_simulate_v1_block_overrides_fee_recipient() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let fee_recipient = H160::from([0xff; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				fee_recipient: Some(fee_recipient),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].miner, fee_recipient);

	Ok(())
}

/// Sequential calls from the same sender within a block automatically get incrementing nonces,
/// and nonces carry over across blocks.
///
/// Reference: Geth TestSimulateV1 nonce management behavior.
async fn test_simulate_v1_nonce_management() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let recipient = H160::from([0xee; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![
					GenericTransaction {
						from: Some(alith.address()),
						to: Some(recipient),
						value: Some(U256::from(1000)),
						..Default::default()
					},
					GenericTransaction {
						from: Some(alith.address()),
						to: Some(recipient),
						value: Some(U256::from(1000)),
						..Default::default()
					},
				],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(recipient),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	assert_eq!(response.0[0].calls.len(), 2);
	assert_eq!(response.0[1].calls.len(), 1);
	for (block_idx, block) in response.0.iter().enumerate() {
		for (call_idx, call) in block.calls.iter().enumerate() {
			assert!(
				matches!(call, SimulationCallResult::Success { .. }),
				"Block {block_idx} call {call_idx} should succeed"
			);
		}
	}

	Ok(())
}

/// Per-block state overrides are applied independently: block 1 overrides do not carry to block 0.
///
/// Reference: Geth TestSimulateV1 "simple-multi-block" test case (per-block state override scope).
async fn test_simulate_v1_per_block_state_overrides() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let funded = H160::from([0xe1; 20]);
	let recipient = H160::from([0xe2; 20]);
	let zeroed = H160::from([0xe3; 20]);

	let mut block0_overrides = BTreeMap::new();
	block0_overrides.insert(
		funded,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let mut block1_overrides = BTreeMap::new();
	block1_overrides.insert(
		zeroed,
		AddressStateOverride {
			balance: Some(U256::zero()),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block0_overrides)),
				calls: vec![
					GenericTransaction {
						from: Some(funded),
						to: Some(recipient),
						value: Some(U256::from(5000)),
						..Default::default()
					},
					GenericTransaction {
						from: Some(funded),
						to: Some(zeroed),
						value: Some(U256::from(5000)),
						..Default::default()
					},
				],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block1_overrides)),
				calls: vec![GenericTransaction {
					from: Some(recipient),
					to: Some(funded),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	assert_eq!(response.0[0].calls.len(), 2);
	assert_eq!(response.0[1].calls.len(), 1);
	for block in &response.0 {
		for (i, call) in block.calls.iter().enumerate() {
			assert!(
				matches!(call, SimulationCallResult::Success { .. }),
				"Call {i} should succeed"
			);
		}
	}

	Ok(())
}

/// Deploy a contract via simulation (no `to` field, `input` = contract bytecode).
async fn test_simulate_v1_contract_deploy() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let (bytecode, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let sender = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(sender.address()),
				to: None,
				input: bytecode.into(),
				value: Some(U256::from(5_000_000_000_000u128)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Contract deploy should succeed"
	);

	Ok(())
}

/// Deploy code via override in block 0, then call it in block 1 — code persists across blocks.
async fn test_simulate_v1_contract_deploy_then_call() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc2; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	// Block 0: override code on contract_addr with Callee runtime bytecode
	let mut block0_overrides = BTreeMap::new();
	block0_overrides.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let echo_calldata: Vec<u8> = Callee::echoCall { _data: 42 }.abi_encode();

	let payload = SimulationParameters {
		block_state_calls: vec![
			// Block 0: call the contract (code set via override)
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block0_overrides)),
				calls: vec![GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: echo_calldata.clone().into(),
					..Default::default()
				}],
			},
			// Block 1: call the same contract — code persists from block 0 override
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: echo_calldata.into(),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	for (i, blk) in response.0.iter().enumerate() {
		assert_eq!(blk.calls.len(), 1);
		assert!(
			matches!(&blk.calls[0], SimulationCallResult::Success { .. }),
			"Block {i} call should succeed"
		);
	}
	// Both blocks should return echo(42) = 42
	if let SimulationCallResult::Success { return_data, .. } = &response.0[0].calls[0] {
		assert!(!return_data.0.is_empty(), "Block 0 should return data");
		assert_eq!(return_data.0[31], 42, "Block 0 should return 42");
	}
	if let SimulationCallResult::Success { return_data, .. } = &response.0[1].calls[0] {
		assert!(!return_data.0.is_empty(), "Block 1 should return data (code persists)");
		assert_eq!(return_data.0[31], 42, "Block 1 should return 42");
	}

	Ok(())
}

/// Call a contract function that always REVERTs.
async fn test_simulate_v1_contract_revert() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc3; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(caller.address()),
				to: Some(contract_addr),
				input: Callee::revertCall {}.abi_encode().into(),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(response.0[0].calls[0].is_failure(), "REVERT contract should produce a Failed result");

	Ok(())
}

/// First call writes to storage, second call reads it back within the same block.
async fn test_simulate_v1_storage_persistence_across_calls() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc4; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			// Contract needs balance for storage deposit when executing SSTORE
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![
				// Call 1: store(42) — write 42 to the `stored` state variable
				GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: Callee::storeCall { _data: 42 }.abi_encode().into(),
					..Default::default()
				},
				// Call 2: stored() — read back the stored value
				GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: Callee::storedCall {}.abi_encode().into(),
					..Default::default()
				},
			],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 2);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"store(42) call should succeed"
	);
	assert!(
		matches!(&response.0[0].calls[1], SimulationCallResult::Success { .. }),
		"stored() call should succeed"
	);
	if let SimulationCallResult::Success { return_data, .. } = &response.0[0].calls[1] {
		assert_eq!(return_data.0.len(), 32, "stored() should return 32 bytes");
		assert_eq!(return_data.0[31], 42, "stored() should return stored value 42");
	}

	Ok(())
}

/// Send ETH value along with a contract call to a payable function.
async fn test_simulate_v1_call_with_value_to_contract() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::CallSelfWithDust;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xc5; 20]);

	let (contract_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"CallSelfWithDust",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(contract_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(caller.address()),
				to: Some(contract_addr),
				input: CallSelfWithDust::fCall {}.abi_encode().into(),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Contract call with value to payable function should succeed"
	);

	Ok(())
}

/// Call an address with no code and no value (empty call to EOA).
async fn test_simulate_v1_call_to_empty_address() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let empty_addr = H160::from([0xde; 20]);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(caller.address()),
				to: Some(empty_addr),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Empty call to EOA should succeed"
	);

	Ok(())
}

/// Override block timestamp and verify it's reflected in the response.
async fn test_simulate_v1_block_override_timestamp() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	// Fetch the current block's timestamp and add an offset to ensure the override is
	// in the future. Substrate timestamps are in milliseconds.
	let current_block = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number), false)
		.await?
		.ok_or_else(|| anyhow!("Current block should exist"))?;
	let timestamp_override = current_block.timestamp + U256::from(10_000u64);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				time: Some(timestamp_override),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(
		response.0[0].timestamp, timestamp_override,
		"Block timestamp should match override"
	);

	Ok(())
}

/// Override block gas limit and verify it's reflected in the response.
async fn test_simulate_v1_block_override_gas_limit() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let gas_limit_override = U256::from(30_000_000u64);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				gas_limit: Some(gas_limit_override),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(
		response.0[0].gas_limit, gas_limit_override,
		"Block gas limit should match override"
	);

	Ok(())
}

/// Override prev_randao and verify difficulty reflects it.
async fn test_simulate_v1_block_override_prev_randao() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let prev_randao = U256::from(12345u64);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				prev_randao: Some(prev_randao),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(
		response.0[0].difficulty, prev_randao,
		"Block difficulty should reflect prev_randao override"
	);

	Ok(())
}

/// Set ALL block override fields at once and verify each one.
async fn test_simulate_v1_block_override_all_fields() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let fee_recipient = H160::from([0xab; 20]);
	let override_number = block_number + U256::one();
	// Use the current block's timestamp + offset to ensure the override is in the future.
	let current_block = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number), false)
		.await?
		.ok_or_else(|| anyhow!("Current block should exist"))?;
	let override_time = current_block.timestamp + U256::from(20_000u64);
	let override_gas_limit = U256::from(50_000_000u64);
	let override_base_fee = U256::from(999u64);
	let override_prev_randao = U256::from(777u64);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(override_number),
				time: Some(override_time),
				gas_limit: Some(override_gas_limit),
				base_fee_per_gas: Some(override_base_fee),
				prev_randao: Some(override_prev_randao),
				fee_recipient: Some(fee_recipient),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	let blk = &response.0[0];
	assert_eq!(blk.number, override_number, "Block number should match override");
	assert_eq!(blk.timestamp, override_time, "Timestamp should match override");
	assert_eq!(blk.gas_limit, override_gas_limit, "Gas limit should match override");
	assert_eq!(blk.base_fee_per_gas, override_base_fee, "Base fee should match override");
	assert_eq!(blk.difficulty, override_prev_randao, "Difficulty should match prev_randao");
	assert_eq!(blk.miner, fee_recipient, "Miner should match fee_recipient");

	Ok(())
}

/// Block timestamps going backwards should fail.
async fn test_simulate_v1_timestamp_ordering_failure() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	// Use the current block's timestamp to compute valid future timestamps.
	let current_block = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number), false)
		.await?
		.ok_or_else(|| anyhow!("Current block should exist"))?;
	let large_timestamp = current_block.timestamp + U256::from(20_000u64);
	let small_timestamp = current_block.timestamp + U256::from(10_000u64);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					time: Some(large_timestamp),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					time: Some(small_timestamp),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Timestamps going backwards should fail");

	Ok(())
}

/// Equal timestamps across blocks are allowed (>= not >).
async fn test_simulate_v1_timestamp_equal_allowed() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	// Use the current block's timestamp + offset so the value is in the future.
	let current_block = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number), false)
		.await?
		.ok_or_else(|| anyhow!("Current block should exist"))?;
	let same_time = current_block.timestamp + U256::from(10_000u64);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					time: Some(same_time),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: Some(BlockOverrides {
					time: Some(same_time),
					..Default::default()
				}),
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(alith.address()),
					to: Some(H160::from([0xdd; 20])),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2, "Both blocks should be produced");
	assert_eq!(response.0[0].timestamp, same_time);
	assert_eq!(response.0[1].timestamp, same_time);

	Ok(())
}

/// Override specific storage slots via StateDiff on a contract.
async fn test_simulate_v1_state_override_storage_state_diff() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xd5; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	// Override storage slot 0 (Callee's `stored` variable) with value 42
	let slot_0 = H256::zero();
	let mut value_bytes = [0u8; 32];
	value_bytes[31] = 42;

	let mut storage_diff = BTreeMap::new();
	storage_diff.insert(slot_0, Bytes32(value_bytes));

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(callee_code.into()),
			storage: Some(StorageOverrides::SpecificStorageSlots { overrides: storage_diff }),
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![
				// Call stored() getter to read the overridden storage slot
				GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: Callee::storedCall {}.abi_encode().into(),
					..Default::default()
				},
			],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"stored() call should succeed"
	);
	if let SimulationCallResult::Success { return_data, .. } = &response.0[0].calls[0] {
		assert_eq!(return_data.0.len(), 32, "Should return 32 bytes");
		assert_eq!(return_data.0[31], 42, "Should return overridden storage value 42");
	}

	Ok(())
}

/// Override 3 accounts simultaneously in one block — all transfers succeed.
async fn test_simulate_v1_state_override_multiple_accounts() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let account_a = H160::from([0x1a; 20]);
	let account_b = H160::from([0x1b; 20]);
	let account_c = H160::from([0x1c; 20]);
	let recipient = H160::from([0x1d; 20]);

	let mut state_overrides_map = BTreeMap::new();
	for account in [account_a, account_b, account_c] {
		state_overrides_map.insert(
			account,
			AddressStateOverride {
				balance: Some(U256::from(10_000_000_000_000_000u128)),
				nonce: None,
				code: None,
				storage: None,
				move_precompile_to_address: None,
			},
		);
	}

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![
				GenericTransaction {
					from: Some(account_a),
					to: Some(recipient),
					value: Some(U256::from(1000)),
					..Default::default()
				},
				GenericTransaction {
					from: Some(account_b),
					to: Some(recipient),
					value: Some(U256::from(2000)),
					..Default::default()
				},
				GenericTransaction {
					from: Some(account_c),
					to: Some(recipient),
					value: Some(U256::from(3000)),
					..Default::default()
				},
			],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 3);
	for (i, call) in response.0[0].calls.iter().enumerate() {
		assert!(
			matches!(call, SimulationCallResult::Success { .. }),
			"Transfer from account {i} should succeed"
		);
	}

	Ok(())
}

/// Override balance to zero on a funded dev account, preventing value transfers.
async fn test_simulate_v1_state_override_balance_zero() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	let recipient = H160::from([0xbb; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		alith.address(),
		AddressStateOverride {
			balance: Some(U256::zero()),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(recipient),
				value: Some(U256::from(1_000_000_000_000u128)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(response.0[0].calls[0].is_failure(), "Transfer from zero-balance account should fail");

	Ok(())
}

/// Override nonce to 0 on an account with validation mode and matching nonce.
async fn test_simulate_v1_state_override_nonce_reset() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xf1; 20]);
	let recipient = H160::from([0xf2; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: Some(U256::zero()),
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::one()),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				nonce: Some(U256::zero()),
				max_fee_per_gas: Some(U256::from(50_000_000_000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Nonce reset to 0 with matching tx nonce should succeed"
	);

	Ok(())
}

/// Override code with different bytecodes in block 0 and block 1 — code is replaced per block.
async fn test_simulate_v1_state_override_code_replace_existing() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = Account::default();
	let contract_addr = H160::from([0xd3; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;
	let (counter_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Counter",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	// Block 0: override code with Callee runtime bytecode
	let mut block0_overrides = BTreeMap::new();
	block0_overrides.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	// Block 1: override code with Counter runtime bytecode (different contract)
	let mut block1_overrides = BTreeMap::new();
	block1_overrides.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(counter_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block0_overrides)),
				calls: vec![GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					input: Callee::echoCall { _data: 42 }.abi_encode().into(),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block1_overrides)),
				calls: vec![GenericTransaction {
					from: Some(caller.address()),
					to: Some(contract_addr),
					// Counter::number() selector — reads storage slot 0 (returns 0 since no
					// constructor ran)
					input: pallet_revive_fixtures::Counter::numberCall {}.abi_encode().into(),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	// Block 0: Callee echo(42) should return 42
	assert!(matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }));
	if let SimulationCallResult::Success { return_data, .. } = &response.0[0].calls[0] {
		assert_eq!(return_data.0[31], 42, "Block 0 Callee echo(42) should return 42");
	}
	// Block 1: Counter number() should return 0 (code was replaced, storage is fresh)
	assert!(matches!(&response.0[1].calls[0], SimulationCallResult::Success { .. }));
	if let SimulationCallResult::Success { return_data, .. } = &response.0[1].calls[0] {
		assert_eq!(return_data.0.len(), 32, "Block 1 Counter number() should return 32 bytes");
		assert!(
			return_data.0.iter().all(|&b| b == 0),
			"Block 1 Counter number() should return 0 (fresh storage, no constructor)"
		);
	}

	Ok(())
}

/// Override ALL state fields (balance, nonce, code) on the same account simultaneously.
async fn test_simulate_v1_state_override_balance_nonce_code_combined() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let caller = H160::from([0xf3; 20]);
	let contract_addr = H160::from([0xd4; 20]);

	let (callee_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	// Override caller with balance + nonce
	state_overrides_map.insert(
		caller,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: Some(U256::from(42)),
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);
	// Override contract with code + balance
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000u128)),
			nonce: None,
			code: Some(callee_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::one()),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(caller),
				to: Some(contract_addr),
				input: Callee::echoCall { _data: 42 }.abi_encode().into(),
				nonce: Some(U256::from(42)),
				max_fee_per_gas: Some(U256::from(50_000_000_000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Combined balance+nonce+code override should succeed"
	);

	Ok(())
}

/// State changes from block 0 calls persist to block 1, but block 0's state overrides
/// don't automatically re-apply.
async fn test_simulate_v1_state_changes_persist_but_overrides_dont() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xf4; 20]);
	let recipient = H160::from([0xf5; 20]);

	let balance = U256::from(10_000u128);
	let transfer_amount = U256::from(3000u128);

	// Block 0: override sender balance, transfer some to recipient
	let mut block0_overrides = BTreeMap::new();
	block0_overrides.insert(
		sender,
		AddressStateOverride {
			balance: Some(balance),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(StateOverrides(block0_overrides)),
				calls: vec![GenericTransaction {
					from: Some(sender),
					to: Some(recipient),
					value: Some(transfer_amount),
					..Default::default()
				}],
			},
			// Block 1: no state overrides. Recipient uses funds received in block 0.
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(recipient),
					to: Some(sender),
					value: Some(U256::from(1000)),
					..Default::default()
				}],
			},
		],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);
	// Block 0 transfer should succeed (sender has overridden balance)
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Block 0 transfer should succeed"
	);
	// Block 1: recipient received funds in block 0, so it should be able to send back
	assert!(
		matches!(&response.0[1].calls[0], SimulationCallResult::Success { .. }),
		"Block 1: recipient should have funds from block 0 transfer"
	);

	Ok(())
}

/// In validation mode, nonce lower than account's state nonce fails.
async fn test_simulate_v1_validation_nonce_too_low() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb3; 20]);
	let recipient = H160::from([0xb4; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: Some(U256::from(5)),
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::one()),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				nonce: Some(U256::from(3)),
				max_fee_per_gas: Some(U256::from(50_000_000_000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Nonce too low should fail in validation mode");

	Ok(())
}

/// In validation mode, max_fee_per_gas below base fee fails.
async fn test_simulate_v1_validation_fee_cap_too_low() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb5; 20]);
	let recipient = H160::from([0xb6; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::from(100u64)),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				max_fee_per_gas: Some(U256::from(1u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Fee cap too low should fail in validation mode");

	Ok(())
}

/// In validation mode, max_priority_fee_per_gas > max_fee_per_gas fails.
async fn test_simulate_v1_validation_tip_above_fee_cap() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb7; 20]);
	let recipient = H160::from([0xb8; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(10_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::one()),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1000)),
				max_fee_per_gas: Some(U256::from(10u64)),
				max_priority_fee_per_gas: Some(U256::from(20u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Tip above fee cap should fail in validation mode");

	Ok(())
}

/// In validation mode, balance < gas_limit * max_fee_per_gas + value fails.
async fn test_simulate_v1_validation_insufficient_funds() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb9; 20]);
	let recipient = H160::from([0xba; 20]);

	// Give just 100 wei — not enough for gas + value
	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(100u64)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				base_fee_per_gas: Some(U256::from(1_000_000u64)),
				..Default::default()
			}),
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(recipient),
				value: Some(U256::from(1_000_000_000_000u128)),
				gas: Some(U256::from(21000u64)),
				max_fee_per_gas: Some(U256::from(1_000_000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: true,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert — the call should either return an error or a failed result
	// (depending on whether InsufficientFunds is caught at validation or execution)
	match result {
		Err(_) => {}, // Validation error
		Ok(response) => {
			assert!(
				response.0[0].calls[0].is_failure(),
				"Insufficient funds should produce failure"
			);
		},
	}

	Ok(())
}

/// gas_price combined with max_fee_per_gas always fails (even without validation mode).
async fn test_simulate_v1_conflicting_fee_fields() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				gas_price: Some(U256::from(10u64)),
				max_fee_per_gas: Some(U256::from(10u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Conflicting fee fields should always fail");

	Ok(())
}

/// Wrong chain_id always fails (even without validation mode).
async fn test_simulate_v1_chain_id_mismatch() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				chain_id: Some(U256::from(999_999u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "Chain ID mismatch should always fail");

	Ok(())
}

/// With validation=false, nonce/fee errors are NOT triggered.
async fn test_simulate_v1_validation_not_applied_without_flag() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				nonce: Some(U256::from(999_999u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert — should succeed (or at least not produce a nonce validation error)
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	// The call may succeed or fail for execution reasons, but NOT for nonce validation
	// If it succeeded, great. If it failed, it shouldn't be a NonceTooHigh error.
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Without validation flag, wrong nonce should not cause failure"
	);

	Ok(())
}

/// Filler blocks maintain proper parent-child hash linkage.
async fn test_simulate_v1_filler_blocks_chain_linkage() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();
	// Create a gap of 3 filler blocks + 1 user block = 4 total
	let gap = U256::from(4);
	let override_number = block_number + gap;

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(override_number),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				value: Some(U256::from(1000)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	let total_blocks = gap.as_usize();
	assert_eq!(response.0.len(), total_blocks, "Expected {total_blocks} blocks (fillers + user)");

	// Verify parent-child linkage for all consecutive pairs
	for i in 1..response.0.len() {
		assert_eq!(
			response.0[i].parent_hash,
			response.0[i - 1].hash,
			"Block {i} parent_hash should equal block {} hash",
			i - 1
		);
	}

	// Verify all hashes are non-zero
	for (i, blk) in response.0.iter().enumerate() {
		assert_ne!(blk.hash, H256::zero(), "Block {i} hash should be non-zero");
	}

	// Filler blocks should have no calls, user block should have 1
	for filler in &response.0[..total_blocks - 1] {
		assert!(filler.calls.is_empty(), "Filler blocks should have no calls");
	}
	assert_eq!(response.0[total_blocks - 1].calls.len(), 1);
	assert!(matches!(&response.0[total_blocks - 1].calls[0], SimulationCallResult::Success { .. }));

	Ok(())
}

/// More than 256 simulated blocks causes BlockCapacityExceeded error.
async fn test_simulate_v1_block_capacity_exceeded() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let alith = Account::default();

	// The capacity is 256 (0x100). Override the block number to create a gap
	// requiring more than 256 blocks total.
	let override_number = block_number + U256::from(257u64);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: Some(BlockOverrides {
				number: Some(override_number),
				..Default::default()
			}),
			state_overrides: None,
			calls: vec![GenericTransaction {
				from: Some(alith.address()),
				to: Some(H160::from([0xdd; 20])),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let result = client.simulate_v1(payload, block).await;

	// Assert
	assert!(result.is_err(), "More than 256 blocks should cause BlockCapacityExceeded");

	Ok(())
}

/// Helper: extract logs from a successful simulation call result.
fn extract_logs<E: core::fmt::Debug>(call: &SimulationCallResult<E>) -> &Vec<Log> {
	match call {
		SimulationCallResult::Success { logs, .. } => logs,
		SimulationCallResult::Failed { error, .. } => {
			panic!("Expected Success, got Failed: {error:?}")
		},
	}
}

/// Helper: assert that a log is a valid synthetic ERC-20 Transfer log per ERC-7528.
/// Checks emitter address, topic count, event signature, from, to, and value data.
fn assert_transfer_log(log: &Log, from: H160, to: H160, value: U256) {
	let emitter = H160([0xee; 20]);
	let transfer_topic =
		H256::from(sp_io::hashing::keccak_256(b"Transfer(address,address,uint256)"));

	assert_eq!(log.address, emitter, "Emitter should be ERC-7528 native token");
	assert_eq!(log.topics.len(), 3, "Transfer log should have 3 topics");
	assert_eq!(log.topics[0], transfer_topic, "topic[0] should be Transfer sig");
	assert_eq!(log.topics[1], H256::from(from), "topic[1] should be from address");
	assert_eq!(log.topics[2], H256::from(to), "topic[2] should be to address");
	assert_eq!(
		log.data,
		Some(Bytes(value.to_big_endian().to_vec())),
		"data should be ABI-encoded value"
	);
}

/// Simple ETH value transfer with trace_transfers enabled produces a synthetic
/// ERC-20 Transfer log with the correct emitter, topics, and data.
///
/// Reference: ERC-7528 / eth_simulateV1 traceTransfers specification.
async fn test_simulate_v1_trace_transfers_simple() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let receiver = H160::from([0xb2; 20]);
	let transfer_value = U256::from(1_000_000u64);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(receiver),
				value: Some(transfer_value),
				..Default::default()
			}],
		}],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	let logs = extract_logs(&response.0[0].calls[0]);
	assert_eq!(logs.len(), 1, "Expected exactly 1 synthetic Transfer log");
	assert_transfer_log(&logs[0], sender, receiver, transfer_value);

	Ok(())
}

/// Same value transfer but with trace_transfers=false produces no synthetic logs.
async fn test_simulate_v1_trace_transfers_disabled() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let receiver = H160::from([0xb2; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(receiver),
				value: Some(U256::from(1_000_000u64)),
				..Default::default()
			}],
		}],
		trace_transfers: false,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	let logs = extract_logs(&response.0[0].calls[0]);
	assert!(logs.is_empty(), "No synthetic logs should be emitted when trace_transfers=false");

	Ok(())
}

/// Multiple value transfers in one block produce one synthetic Transfer log each,
/// in the correct order with correct from/to/value.
async fn test_simulate_v1_trace_transfers_multiple_transfers() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let receiver1 = H160::from([0xb2; 20]);
	let receiver2 = H160::from([0xb3; 20]);
	let value1 = U256::from(1_000u64);
	let value2 = U256::from(2_000u64);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![
				GenericTransaction {
					from: Some(sender),
					to: Some(receiver1),
					value: Some(value1),
					..Default::default()
				},
				GenericTransaction {
					from: Some(sender),
					to: Some(receiver2),
					value: Some(value2),
					..Default::default()
				},
			],
		}],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 2);

	// First call: sender -> receiver1, value1
	let logs1 = extract_logs(&response.0[0].calls[0]);
	assert_eq!(logs1.len(), 1, "First transfer should produce 1 log");
	assert_transfer_log(&logs1[0], sender, receiver1, value1);

	// Second call: sender -> receiver2, value2
	let logs2 = extract_logs(&response.0[0].calls[1]);
	assert_eq!(logs2.len(), 1, "Second transfer should produce 1 log");
	assert_transfer_log(&logs2[0], sender, receiver2, value2);

	Ok(())
}

/// Zero-value transfer with trace_transfers=true produces no synthetic log,
/// because the transfer function returns early when value is zero.
async fn test_simulate_v1_trace_transfers_zero_value_no_log() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let receiver = H160::from([0xb2; 20]);

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(receiver),
				value: Some(U256::zero()),
				..Default::default()
			}],
		}],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	let logs = extract_logs(&response.0[0].calls[0]);
	assert!(logs.is_empty(), "Zero-value transfer should not produce a synthetic log");

	Ok(())
}

/// Transfer logs are produced per-block: transfers in block 0 and block 1
/// each produce their own logs in their respective blocks.
async fn test_simulate_v1_trace_transfers_multi_block() -> anyhow::Result<()> {
	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let receiver1 = H160::from([0xb2; 20]);
	let receiver2 = H160::from([0xb3; 20]);
	let value1 = U256::from(500u64);
	let value2 = U256::from(700u64);

	let state_overrides = |addr: H160| {
		let mut map = BTreeMap::new();
		map.insert(
			addr,
			AddressStateOverride {
				balance: Some(U256::from(1_000_000_000_000_000u128)),
				nonce: None,
				code: None,
				storage: None,
				move_precompile_to_address: None,
			},
		);
		StateOverrides(map)
	};

	let payload = SimulationParameters {
		block_state_calls: vec![
			SimulationPayload {
				block_overrides: None,
				state_overrides: Some(state_overrides(sender)),
				calls: vec![GenericTransaction {
					from: Some(sender),
					to: Some(receiver1),
					value: Some(value1),
					..Default::default()
				}],
			},
			SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![GenericTransaction {
					from: Some(sender),
					to: Some(receiver2),
					value: Some(value2),
					..Default::default()
				}],
			},
		],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 2);

	// Block 0: sender -> receiver1
	let logs0 = extract_logs(&response.0[0].calls[0]);
	assert_eq!(logs0.len(), 1, "Block 0 should have 1 Transfer log");
	assert_transfer_log(&logs0[0], sender, receiver1, value1);

	// Block 1: sender -> receiver2
	let logs1 = extract_logs(&response.0[1].calls[0]);
	assert_eq!(logs1.len(), 1, "Block 1 should have 1 Transfer log");
	assert_transfer_log(&logs1[0], sender, receiver2, value2);

	Ok(())
}

/// Contract call with value transfer also produces a synthetic Transfer log
/// when trace_transfers is enabled.
async fn test_simulate_v1_trace_transfers_contract_with_value() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::CallSelfWithDust;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let contract_addr = H160::from([0xcc; 20]);
	let transfer_value = U256::from(5_000u64);

	let (contract_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"CallSelfWithDust",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(contract_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(contract_addr),
				value: Some(transfer_value),
				input: CallSelfWithDust::fCall {}.abi_encode().into(),
				..Default::default()
			}],
		}],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Success { .. }),
		"Contract call with value should succeed"
	);

	let logs = extract_logs(&response.0[0].calls[0]);
	// The value transfer from sender to contract should produce exactly 1 synthetic
	// Transfer log. CallSelfWithDust::f() is just `external payable {}` — no internal
	// transfers, so there should be no additional synthetic logs.
	assert_eq!(logs.len(), 1, "Contract call with value should produce exactly 1 Transfer log");
	assert_transfer_log(&logs[0], sender, contract_addr, transfer_value);

	Ok(())
}

/// Per Geth spec: "if the transaction sends ETH but the execution reverts, no log gets issued."
/// A contract call with value that REVERTs should produce a Failed result with no logs.
/// The SimulationCallResult::Failed variant has no logs field by design, matching this spec.
async fn test_simulate_v1_trace_transfers_reverted_call() -> anyhow::Result<()> {
	use pallet_revive::precompiles::alloy::sol_types::SolCall;
	use pallet_revive_fixtures::Callee;

	// Arrange
	let client = Arc::new(SharedResources::client().await);
	let block_number = client.block_number().await?;
	let sender = H160::from([0xb1; 20]);
	let contract_addr = H160::from([0xcc; 20]);

	let (contract_code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Callee",
		pallet_revive_fixtures::FixtureType::SolcRuntime,
	)?;

	let mut state_overrides_map = BTreeMap::new();
	state_overrides_map.insert(
		sender,
		AddressStateOverride {
			balance: Some(U256::from(1_000_000_000_000_000u128)),
			nonce: None,
			code: None,
			storage: None,
			move_precompile_to_address: None,
		},
	);
	state_overrides_map.insert(
		contract_addr,
		AddressStateOverride {
			balance: None,
			nonce: None,
			code: Some(contract_code.into()),
			storage: None,
			move_precompile_to_address: None,
		},
	);

	let payload = SimulationParameters {
		block_state_calls: vec![SimulationPayload {
			block_overrides: None,
			state_overrides: Some(StateOverrides(state_overrides_map)),
			calls: vec![GenericTransaction {
				from: Some(sender),
				to: Some(contract_addr),
				value: Some(U256::from(5_000u64)),
				input: Callee::revertCall {}.abi_encode().into(),
				..Default::default()
			}],
		}],
		trace_transfers: true,
		validation: false,
		return_full_transactions: false,
	};

	// Act
	let block = Some(BlockNumberOrTagOrHash::BlockNumber(block_number));
	let response = client.simulate_v1(payload, block).await?;

	// Assert
	assert_eq!(response.0.len(), 1);
	assert_eq!(response.0[0].calls.len(), 1);
	// The call should have failed (contract reverted). SimulationCallResult::Failed
	// has no `logs` field, so no synthetic Transfer logs are returned — matching
	// the Geth spec that reverted calls discard all logs.
	assert!(
		matches!(&response.0[0].calls[0], SimulationCallResult::Failed { .. }),
		"Contract call that REVERTs should produce Failed result (no logs)"
	);

	Ok(())
}

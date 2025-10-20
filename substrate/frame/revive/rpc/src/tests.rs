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
		HashesOrTransactionInfos, TransactionInfo, TransactionUnsigned, H256, U256,
	},
};
use static_init::dynamic;
use std::{sync::Arc, thread};
use substrate_cli_test_utils::*;
use subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient};

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
		// Start the node.
		let _node_handle = thread::spawn(move || {
			if let Err(e) = start_node_inline(vec![
				"--dev",
				"--rpc-port=45789",
				"--no-telemetry",
				"--no-prometheus",
				"-lerror,evm=debug,sc_rpc_server=info,runtime::revive=trace",
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

	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
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

#[tokio::test]
async fn evm_blocks_should_match() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = std::sync::Arc::new(SharedResources::client().await);

	let (node_client, node_rpc_client, _) = client::connect("ws://localhost:45789").await.unwrap();

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
	println!("block_number = {block_number:?}");
	println!("tx hash = {:?}", tx.hash());

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
	println!("block_number = {block_number:?}");
	println!("tx hash = {:?}", tx.hash());

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

	println!("Testing with Ethereum block hash: {ethereum_block_hash:?}");

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

	println!("Testing with fake Ethereum hash: {fake_eth_hash:?}");

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

	println!("Testing with block number: {block_number}");

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

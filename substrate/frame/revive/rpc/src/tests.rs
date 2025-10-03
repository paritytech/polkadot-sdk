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
use clap::Parser;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use pallet_revive::{
	create1,
	evm::{Account, BlockNumberOrTag, BlockTag, U256},
};
use static_init::dynamic;
use std::{sync::Arc, thread};
use substrate_cli_test_utils::*;
use subxt::OnlineClient;

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
	eth_rpc_port: u32,
	node_rpc_port: u32,
	_node_handle: std::thread::JoinHandle<()>,
	_rpc_handle: std::thread::JoinHandle<()>,
}

impl SharedResources {
	fn start_advanced(
		node_rpc_port: u32,
		eth_rpc_port: u32,
		node_extra_args: Vec<&'static str>,
	) -> Self {
		let node_rpc_port_arg = format!("--rpc-port={node_rpc_port}");
		let _node_handle = thread::spawn(move || {
			let args = vec![
				"--dev",
				&node_rpc_port_arg,
				"--no-telemetry",
				"--no-prometheus",
				"-lerror,evm=debug,sc_rpc_server=info,runtime::revive=trace",
			];
			let combined_args = [args, node_extra_args].concat();
			if let Err(e) = start_node_inline(combined_args) {
				panic!("Node exited with error: {e:?}");
			}
		});

		let eth_rpc_port_arg = format!("--rpc-port={eth_rpc_port}");
		let node_rpc_url = format!("--node-rpc-url=ws://localhost:{node_rpc_port}");
		// Start the rpc server.
		let args = CliCommand::parse_from([
			"--dev",
			&eth_rpc_port_arg,
			&node_rpc_url,
			"--no-prometheus",
			"-linfo,eth-rpc=debug",
		]);

		let _rpc_handle = thread::spawn(move || {
			if let Err(e) = cli::run(args) {
				panic!("eth-rpc exited with error: {e:?}");
			}
		});
		Self { eth_rpc_port, node_rpc_port, _node_handle, _rpc_handle }
	}

	fn start() -> Self {
		Self::start_advanced(45789, 45788, vec![])
	}

	async fn client(&self) -> WsClient {
		let url = format!("ws://localhost:{}", self.eth_rpc_port);
		ws_client_with_retry(&url).await
	}

	async fn node_client(&self) -> OnlineClient<SrcChainConfig> {
		let url = format!("ws://localhost:{}", self.node_rpc_port);
		OnlineClient::<SrcChainConfig>::from_url(url)
			.await
			.expect("Failed to get online client")
	}
}

#[dynamic(lazy)]
static mut SHARED_RESOURCES: SharedResources = SharedResources::start();

// TODO maybe it is ok to run single shared resource for all tests?
// Setting state-pruning to low value, to not wait long for state pruning, which is required for
// some EVM reconstruction tests
#[dynamic(lazy)]
static mut SHARED_RESOURCES_ADVANCED: SharedResources =
	SharedResources::start_advanced(55789, 55788, vec!["--state-pruning=8"]);

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
	let shared_resources = SHARED_RESOURCES.write();
	let client = Arc::new(shared_resources.client().await);

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
	let shared_resources = SHARED_RESOURCES.write();
	let client = Arc::new(shared_resources.client().await);
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
	assert_eq!(
		Some(value),
		balance.checked_sub(initial_balance),
		"Contract {contract_address:?}
Balance {balance} should have increased from {initial_balance} by {value}."
	);

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
	let shared_resources = SHARED_RESOURCES.write();
	let client = Arc::new(shared_resources.client().await);

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
	let shared_resources = SHARED_RESOURCES.write();
	let client = Arc::new(shared_resources.client().await);
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

// Wait until state is pruned, it is assumed that initially a state is available
// at latest best block.
async fn wait_until_state_pruned(client: OnlineClient<SrcChainConfig>) -> anyhow::Result<()> {
	let query = subxt_client::storage().revive().ethereum_block();
	let mut blocks = client.blocks().subscribe_best().await?;

	// Get current best block
	let block_hash = if let Some(Ok(block)) = blocks.next().await {
		block.hash()
	} else {
		return Err(anyhow::anyhow!("Failed to fetch next block"));
	};
	let storage = client.storage().at(block_hash);

	// Inspect storage at best block we got until state is pruned
	loop {
		match blocks.next().await {
			Some(Ok(block)) => {
				println!("block current = {:?} {:?} ", block.number(), block.hash());
				// Break only on when state discarded error message appears
				match storage.fetch(&query).await {
					Ok(_) => {
						// State still available, continue waiting
					},
					Err(err) if format!("{:?}", err).contains("State already discarded") => {
						println!("storage pruned: {:?}", err);
						return Ok(());
					},
					Err(err) => {
						return Err(anyhow::anyhow!("Error fetching storage: {:?}", err));
					},
				}
			},
			Some(Err(e)) => return Err(anyhow::anyhow!("Error subscribing to blocks: {:?}", e)),
			None => return Err(anyhow::anyhow!("Block subscription ended unexpectedly")),
		}
	}
}

#[tokio::test]
async fn reconstructed_block_matches_storage_block() -> anyhow::Result<()> {
	// let client = Arc::new(ws_client_with_retry("ws://localhost:8545").await);
	let shared_resources = SHARED_RESOURCES_ADVANCED.write();
	let client = Arc::new(shared_resources.client().await);

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

	// Fetch the block immediately (should come from storage EthereumBlock)
	let storage_block_by_number = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number.into()), true)
		.await?
		.expect("Block should exist");
	let storage_block_by_hash =
		client.get_block_by_hash(block_hash, true).await?.expect("Block should exist");

	// All storage blocks must match
	assert_eq!(
		storage_block_by_number, storage_block_by_hash,
		"Storage blocks by number and hash should match"
	);

	// Wait for state pruning (8 blocks + buffer)
	// wait_until_block(&client, block_number + U256::from(10)).await?;
	wait_until_state_pruned(shared_resources.node_client().await).await?;

	// Fetch the same block again - it should be reconstructed now
	let reconstructed_block_number = client
		.get_block_by_number(BlockNumberOrTag::U256(block_number.into()), true)
		.await?
		.expect("Block should still exist");
	let reconstructed_block_by_hash =
		client.get_block_by_hash(block_hash, true).await?.expect("Block should exist");

	// All reconstructed blocks must match
	assert_eq!(
		reconstructed_block_number, reconstructed_block_by_hash,
		"Reconstructed blocks by number and hash should match"
	);

	// Reconstructed and storage blocks must matchs
	assert_eq!(
		storage_block_by_number, reconstructed_block_number,
		"Reconstructed block should match storage block exactly"
	);
	Ok(())
}

	Ok(())
}

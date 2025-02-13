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

use crate::{
	cli::{self, CliCommand},
	example::TransactionBuilder,
	EthRpcClient,
};
use clap::Parser;
use ethabi::Token;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use pallet_revive::{
	create1,
	evm::{Account, BlockTag, U256},
};
use static_init::dynamic;
use std::{sync::Arc, thread};
use substrate_cli_test_utils::*;

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

fn get_contract(name: &str) -> anyhow::Result<(Vec<u8>, ethabi::Contract)> {
	let pvm_dir: std::path::PathBuf = "./examples/js/pvm".into();
	let abi_dir: std::path::PathBuf = "./examples/js/abi".into();
	let bytecode = std::fs::read(pvm_dir.join(format!("{}.polkavm", name)))?;

	let abi = std::fs::read(abi_dir.join(format!("{}.json", name)))?;
	let contract = ethabi::Contract::load(abi.as_slice())?;

	Ok((bytecode, contract))
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

	let increase =
		client.get_balance(ethan.address(), BlockTag::Latest.into()).await? - initial_balance;
	assert_eq!(value, increase);
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

	let updated_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	assert_eq!(value, updated_balance - initial_balance);

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
		"Contract should be deployed with the correct address."
	);

	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(value, balance, "Contract balance should be the same as the value sent.");

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
		"Receipt should have the correct contract address."
	);

	let increase = client.get_balance(contract_address, BlockTag::Latest.into()).await? - balance;
	assert_eq!(value, increase, "contract's balance should have increased by the value sent.");

	// Balance transfer to contract
	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	let tx = TransactionBuilder::new(&client)
		.value(value)
		.to(contract_address)
		.send()
		.await?;

	tx.wait_for_receipt().await?;
	let increase = client.get_balance(contract_address, BlockTag::Latest.into()).await? - balance;
	assert_eq!(value, increase, "contract's balance should have increased by the value sent.");
	Ok(())
}

#[tokio::test]
async fn revert_call() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);
	let (bytecode, contract) = get_contract("Errors")?;
	let receipt = TransactionBuilder::new(&client)
		.input(bytecode)
		.send()
		.await?
		.wait_for_receipt()
		.await?;

	let err = TransactionBuilder::new(&client)
		.to(receipt.contract_address.unwrap())
		.input(contract.function("triggerRequireError")?.encode_input(&[])?.to_vec())
		.send()
		.await
		.unwrap_err();

	let call_err = unwrap_call_err!(err.source().unwrap());
	assert_eq!(call_err.message(), "execution reverted: revert: This is a require error");
	assert_eq!(call_err.code(), 3);
	Ok(())
}

#[tokio::test]
async fn event_logs() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);
	let (bytecode, contract) = get_contract("EventExample")?;
	let receipt = TransactionBuilder::new(&client)
		.input(bytecode)
		.send()
		.await?
		.wait_for_receipt()
		.await?;

	let receipt = TransactionBuilder::new(&client)
		.to(receipt.contract_address.unwrap())
		.input(contract.function("triggerEvent")?.encode_input(&[])?.to_vec())
		.send()
		.await?
		.wait_for_receipt()
		.await?;
	assert_eq!(receipt.logs.len(), 1, "There should be one log.");
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

#[tokio::test]
async fn native_evm_ratio_works() -> anyhow::Result<()> {
	let _lock = SHARED_RESOURCES.write();
	let client = Arc::new(SharedResources::client().await);
	let (bytecode, contract) = get_contract("PiggyBank")?;
	let contract_address = TransactionBuilder::new(&client)
		.input(bytecode)
		.send()
		.await?
		.wait_for_receipt()
		.await?
		.contract_address
		.unwrap();

	let value = 10_000_000_000_000_000_000u128; // 10 eth
	TransactionBuilder::new(&client)
		.to(contract_address)
		.input(contract.function("deposit")?.encode_input(&[])?.to_vec())
		.value(value.into())
		.send()
		.await?
		.wait_for_receipt()
		.await?;

	let contract_value = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(contract_value, value.into());

	let withdraw_value = 1_000_000_000_000_000_000u128; // 1 eth
	TransactionBuilder::new(&client)
		.to(contract_address)
		.input(
			contract
				.function("withdraw")?
				.encode_input(&[Token::Uint(withdraw_value.into())])?
				.to_vec(),
		)
		.send()
		.await?
		.wait_for_receipt()
		.await?;

	let contract_value = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(contract_value, (value - withdraw_value).into());

	Ok(())
}

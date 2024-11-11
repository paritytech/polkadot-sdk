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
	example::{wait_for_successful_receipt, TransactionBuilder},
	EthRpcClient,
};
use clap::Parser;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use pallet_revive::{
	create1,
	evm::{Account, BlockTag, U256},
};
use std::thread;
use substrate_cli_test_utils::*;

/// Create a websocket client with a 30s timeout.
async fn ws_client_with_retry(url: &str) -> WsClient {
	let timeout = tokio::time::Duration::from_secs(30);
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
	const PVM_CONTRACTS: &str = include_str!("../examples/js/pvm-contracts.json");
	let pvm_contract: serde_json::Value = serde_json::from_str(PVM_CONTRACTS)?;
	let pvm_contract = pvm_contract[name].as_object().unwrap();
	let bytecode = pvm_contract["bytecode"].as_str().unwrap();
	let bytecode = hex::decode(bytecode)?;

	let abi = pvm_contract["abi"].clone();
	let abi = serde_json::to_string(&abi)?;
	let contract = ethabi::Contract::load(abi.as_bytes())?;

	Ok((bytecode, contract))
}

#[tokio::test]
async fn test_jsonrpsee_server() -> anyhow::Result<()> {
	// Start the node.
	let _ = thread::spawn(move || {
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
	let _ = thread::spawn(move || {
		if let Err(e) = cli::run(args) {
			panic!("eth-rpc exited with error: {e:?}");
		}
	});

	let client = ws_client_with_retry("ws://localhost:45788").await;
	let account = Account::default();

	// Balance transfer
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let ethan_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	assert_eq!(U256::zero(), ethan_balance);

	let value = 1_000_000_000_000_000_000_000u128.into();
	let hash = TransactionBuilder::default()
		.value(value)
		.to(ethan.address())
		.send(&client)
		.await?;

	let receipt = wait_for_successful_receipt(&client, hash).await?;
	assert_eq!(
		Some(ethan.address()),
		receipt.to,
		"Receipt should have the correct contract address."
	);

	let ethan_balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
	assert_eq!(value, ethan_balance, "ethan's balance should be the same as the value sent.");

	// Deploy contract
	let data = b"hello world".to_vec();
	let value = U256::from(5_000_000_000_000u128);
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let input = bytes.into_iter().chain(data.clone()).collect::<Vec<u8>>();
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let hash = TransactionBuilder::default().value(value).input(input).send(&client).await?;
	let receipt = wait_for_successful_receipt(&client, hash).await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(
		Some(contract_address),
		receipt.contract_address,
		"Contract should be deployed with the correct address."
	);

	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(value, balance, "Contract balance should be the same as the value sent.");

	// Call contract
	let hash = TransactionBuilder::default()
		.value(value)
		.to(contract_address)
		.send(&client)
		.await?;
	let receipt = wait_for_successful_receipt(&client, hash).await?;

	assert_eq!(
		Some(contract_address),
		receipt.to,
		"Receipt should have the correct contract address."
	);

	let increase = client.get_balance(contract_address, BlockTag::Latest.into()).await? - balance;
	assert_eq!(value, increase, "contract's balance should have increased by the value sent.");

	// Balance transfer to contract
	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	let hash = TransactionBuilder::default()
		.value(value)
		.to(contract_address)
		.send(&client)
		.await?;

	wait_for_successful_receipt(&client, hash).await?;
	let increase = client.get_balance(contract_address, BlockTag::Latest.into()).await? - balance;
	assert_eq!(value, increase, "contract's balance should have increased by the value sent.");

	// Deploy revert
	let (bytecode, contract) = get_contract("revert")?;
	let receipt = TransactionBuilder::default()
		.input(contract.constructor.clone().unwrap().encode_input(bytecode, &[]).unwrap())
		.send_and_wait_for_receipt(&client)
		.await?;

	// Call doRevert
	let res = TransactionBuilder::default()
		.to(receipt.contract_address.unwrap())
		.input(contract.function("doRevert")?.encode_input(&[])?.to_vec())
		.send(&client)
		.await;

	let err = res.unwrap_err();
	let err = err.source().unwrap();
	let err = err.downcast_ref::<jsonrpsee::core::client::Error>().unwrap();
	match err {
		jsonrpsee::core::client::Error::Call(call) =>
			assert_eq!(call.message(), "Execution reverted: revert message"),
		_ => panic!("Expected Call error"),
	}

	// Deploy event
	let (bytecode, contract) = get_contract("event")?;
	let receipt = TransactionBuilder::default()
		.input(bytecode)
		.send_and_wait_for_receipt(&client)
		.await?;

	// Call triggerEvent
	let receipt = TransactionBuilder::default()
		.to(receipt.contract_address.unwrap())
		.input(contract.function("triggerEvent")?.encode_input(&[])?.to_vec())
		.send_and_wait_for_receipt(&client)
		.await?;

	assert_eq!(receipt.logs.len(), 1, "There should be one log.");
	Ok(())
}

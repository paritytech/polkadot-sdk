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

// We require the `riscv` feature to get access to the compiled fixtures.
#![cfg(feature = "riscv")]
use crate::{
	cli,
	example::{send_transaction, wait_for_receipt},
	EthRpcClient,
};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use pallet_revive::{
	create1,
	evm::{Account, BlockTag, Bytes, U256},
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

#[tokio::test]
async fn test_jsonrpsee_server() -> anyhow::Result<()> {
	// Start the node.
	let _ = thread::spawn(move || match start_node_inline(vec!["--dev", "--rpc-port=45789"]) {
		Ok(_) => {},
		Err(e) => {
			panic!("Node exited with error: {}", e);
		},
	});

	// Start the rpc server.
	tokio::spawn(cli::run(cli::CliCommand {
		rpc_port: "45788".to_string(),
		node_rpc_url: "ws://localhost:45789".to_string(),
	}));

	let client = ws_client_with_retry("ws://localhost:45788").await;
	let account = Account::default();

	// Deploy contract
	let data = b"hello world".to_vec();
	let value = U256::from(5_000_000_000_000u128);
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let input = bytes.into_iter().chain(data.clone()).collect::<Vec<u8>>();
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let hash = send_transaction(&account, &client, value, input.into(), None).await?;
	let receipt = wait_for_receipt(&client, hash).await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(
		Some(contract_address),
		receipt.contract_address,
		"Contract should be deployed with the correct address."
	);

	let balance = client.get_balance(contract_address, BlockTag::Latest.into()).await?;
	assert_eq!(
		value * 1_000_000,
		balance,
		"Contract balance should be the same as the value sent."
	);

	// Call contract
	let hash =
		send_transaction(&account, &client, U256::zero(), Bytes::default(), Some(contract_address))
			.await?;
	let receipt = wait_for_receipt(&client, hash).await?;
	assert_eq!(
		Some(contract_address),
		receipt.to,
		"Receipt should have the correct contract address."
	);

	Ok(())
}

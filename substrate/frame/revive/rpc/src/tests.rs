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
	example::{send_transaction, wait_for_receipt},
	EthRpcClient,
};
use assert_cmd::cargo::cargo_bin;
use jsonrpsee::ws_client::WsClientBuilder;
use pallet_revive::{
	create1,
	evm::{Account, BlockTag, Bytes, U256},
};
use std::{
	io::{BufRead, BufReader},
	process::{self, Child, Command},
};
use substrate_cli_test_utils::*;

/// Start eth-rpc server, and return the child process and the WebSocket URL.
fn start_eth_rpc_server(node_ws_url: &str) -> (Child, String) {
	let mut child = Command::new(cargo_bin("eth-rpc"))
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::piped())
		.env("RUST_LOG", "info,eth-rpc=debug")
		.args(["--rpc-port=45788", &format!("--node-rpc-url={node_ws_url}")])
		.spawn()
		.unwrap();

	let mut data = String::new();
	let ws_url = BufReader::new(child.stdout.take().unwrap())
		.lines()
		.find_map(|line| {
			let line = line.expect("failed to obtain next line while extracting node info");
			data.push_str(&line);
			data.push('\n');

			// does the line contain our port (we expect this specific output from eth-rpc).
			let sock_addr = match line.split_once("Running JSON-RPC server: addr=") {
				None => return None,
				Some((_, after)) => after.trim(),
			};

			Some(format!("ws://{}", sock_addr))
		})
		.unwrap_or_else(|| {
			eprintln!("Observed eth-rpc output:\n{}", data);
			panic!("We should get a WebSocket address")
		});

	(child, ws_url)
}

#[tokio::test]
async fn test_jsonrpsee_server() -> anyhow::Result<()> {
	let mut node_child = substrate_cli_test_utils::start_node();

	let _ = std::thread::spawn(move || {
		match common::start_node_inline(vec!["--dev", "--rpc-port=45788"]) {
			Ok(_) => {},
			Err(e) => {
				panic!("Node exited with error: {}", e);
			},
		}
	});

	let (_rpc_child, ws_url) = start_eth_rpc_server("ws://localhost:45788");

	let client = WsClientBuilder::default().build(ws_url).await?;
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

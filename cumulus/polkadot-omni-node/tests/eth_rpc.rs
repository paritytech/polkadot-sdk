// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Runs `polkadot-omni-node --dev --eth-rpc` on Penpal, which includes `pallet-revive`, and talks
//! to the embedded ETH RPC server over its own port.

#![cfg(all(unix, feature = "experimental-eth-rpc-in-node"))]

use jsonrpsee::{
	core::client::ClientT,
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use pallet_revive::evm::{Account, U256};
use pallet_revive_eth_rpc::{example::TransactionBuilder, EthRpcClient};
use sc_chain_spec::{ChainType, GenericChainSpec, NoExtension};
use std::{
	net::TcpListener,
	path::Path,
	process::{Child, Command, Stdio},
	sync::Arc,
	time::{Duration, Instant},
};

/// Penpal's `pallet_revive::Config::ChainId`.
const PENPAL_CHAIN_ID: u64 = 420_420_999;

struct KillChildOnDrop(Child);

impl Drop for KillChildOnDrop {
	fn drop(&mut self) {
		let _ = self.0.kill();
		let _ = self.0.wait();
	}
}

fn free_port() -> u16 {
	TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Penpal's development preset, with Alith's mapped account endowed so it can pay for an
/// Ethereum transaction.
fn write_chain_spec(dir: &Path) -> std::path::PathBuf {
	let spec = GenericChainSpec::<NoExtension>::builder(
		penpal_runtime::WASM_BINARY.expect("penpal is built with its wasm"),
		None,
	)
	.with_name("Penpal ETH RPC")
	.with_id("penpal-eth-rpc")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.build();

	let mut spec: serde_json::Value = serde_json::from_str(&spec.as_json(false).unwrap()).unwrap();
	spec["relay_chain"] = "rococo-local".into();
	spec["para_id"] = 2000.into();

	let balances = spec["genesis"]["runtimeGenesis"]["patch"]["balances"]["balances"]
		.as_array_mut()
		.expect("the dev preset endows accounts");
	balances.push(serde_json::json!([Account::default().substrate_account(), 1u128 << 60]));

	let path = dir.join("penpal-eth-rpc.json");
	std::fs::write(&path, serde_json::to_vec(&spec).unwrap()).unwrap();
	path
}

/// The server comes up only once the embedded client has synced its metadata.
async fn connect(url: &str, node: &mut Child) -> WsClient {
	let deadline = Instant::now() + Duration::from_secs(300);
	loop {
		if let Ok(client) = WsClientBuilder::default().build(url).await {
			return client;
		}
		assert!(node.try_wait().unwrap().is_none(), "the node exited before serving ETH RPC");
		assert!(Instant::now() < deadline, "no ETH RPC server on {url}");
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
}

fn spawn_node(base_dir: &Path, rpc_port: u16, eth_rpc_port: u16) -> KillChildOnDrop {
	let chain_spec = write_chain_spec(base_dir);
	KillChildOnDrop(
		Command::new(env!("CARGO_BIN_EXE_polkadot-omni-node"))
			.stdout(Stdio::null())
			.stderr(Stdio::inherit())
			.arg("--chain")
			.arg(&chain_spec)
			.arg("--base-path")
			.arg(base_dir)
			.args(["--dev", "--dev-block-time", "1000", "--no-prometheus"])
			.args(["--rpc-port", &rpc_port.to_string(), "--port", &free_port().to_string()])
			.args(["--eth-rpc", "--eth-rpc-port", &eth_rpc_port.to_string()])
			.spawn()
			.unwrap(),
	)
}

async fn best_block(client: &WsClient) -> u64 {
	let header: serde_json::Value = client.request("chain_getHeader", rpc_params![]).await.unwrap();
	let number = header["number"].as_str().unwrap().trim_start_matches("0x");
	u64::from_str_radix(number, 16).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn dev_node_serves_eth_rpc() {
	let base_dir = tempfile::tempdir().unwrap();
	let eth_rpc_port = free_port();
	let mut node = spawn_node(base_dir.path(), free_port(), eth_rpc_port);

	let client = Arc::new(connect(&format!("ws://127.0.0.1:{eth_rpc_port}"), &mut node.0).await);

	assert_eq!(client.chain_id().await.unwrap(), U256::from(PENPAL_CHAIN_ID));
	assert_eq!(client.accounts().await.unwrap().first(), Some(&Account::default().address()));

	let to = Account::from(subxt_signer::eth::dev::baltathar()).address();
	let tx = TransactionBuilder::new(client.clone())
		.to(to)
		.value(U256::from(10).pow(18.into()))
		.send()
		.await
		.unwrap();
	let receipt = tx.wait_for_receipt().await.unwrap();

	assert_eq!(receipt.transaction_hash, tx.hash());
	assert_eq!(receipt.to, Some(to));
}

#[tokio::test(flavor = "multi_thread")]
async fn eth_rpc_failure_leaves_the_node_running() {
	let base_dir = tempfile::tempdir().unwrap();
	// A regular file where the receipt database directory should be makes `create_dir_all` fail.
	let chain_dir = base_dir.path().join("chains/penpal-eth-rpc");
	std::fs::create_dir_all(&chain_dir).unwrap();
	std::fs::write(chain_dir.join("eth-rpc"), b"").unwrap();

	let rpc_port = free_port();
	let eth_rpc_port = free_port();
	let mut node = spawn_node(base_dir.path(), rpc_port, eth_rpc_port);

	let deadline = Instant::now() + Duration::from_secs(300);
	let rpc = loop {
		if let Ok(client) =
			WsClientBuilder::default().build(format!("ws://127.0.0.1:{rpc_port}")).await
		{
			break client;
		}
		assert!(node.0.try_wait().unwrap().is_none(), "the node exited");
		assert!(Instant::now() < deadline, "the node's RPC never came up");
		tokio::time::sleep(Duration::from_millis(500)).await;
	};

	let start = best_block(&rpc).await;
	while best_block(&rpc).await < start + 3 {
		assert!(Instant::now() < deadline, "the node stopped producing blocks");
		tokio::time::sleep(Duration::from_millis(500)).await;
	}

	assert!(node.0.try_wait().unwrap().is_none(), "the node exited");
	assert!(
		WsClientBuilder::default()
			.build(format!("ws://127.0.0.1:{eth_rpc_port}"))
			.await
			.is_err(),
		"the ETH RPC server should not be listening"
	);
}

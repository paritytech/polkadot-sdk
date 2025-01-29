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
use clap::Parser;
use jsonrpsee::http_client::HttpClientBuilder;
use pallet_revive::evm::{Account, BlockTag, ReceiptInfo};
use pallet_revive_eth_rpc::{
	example::{wait_for_receipt, TransactionBuilder},
	EthRpcClient,
};
use tokio::{
	io::{AsyncBufReadExt, BufReader},
	process::{Child, ChildStderr, Command},
	signal::unix::{signal, SignalKind},
};

const DOCKER_CONTAINER_NAME: &str = "eth-rpc-test";

#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The parity docker image e.g eth-rpc:master-fb2e414f
	#[clap(long, default_value = "eth-rpc:master-fb2e414f")]
	docker_image: String,

	/// The docker binary
	/// Either docker or podman
	#[clap(long, default_value = "docker")]
	docker_bin: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let CliCommand { docker_bin, docker_image, .. } = CliCommand::parse();

	let mut docker_process = start_docker(&docker_bin, &docker_image)?;
	let stderr = docker_process.stderr.take().unwrap();

	tokio::select! {
		result = docker_process.wait() => {
			println!("docker failed: {result:?}");
		}
		_ = interrupt() => {
			kill_docker().await?;
		}
		_ = test_eth_rpc(stderr) => {
			kill_docker().await?;
		}
	}

	Ok(())
}

async fn interrupt() {
	let mut sigint = signal(SignalKind::interrupt()).expect("failed to listen for SIGINT");
	let mut sigterm = signal(SignalKind::terminate()).expect("failed to listen for SIGTERM");

	tokio::select! {
		_ = sigint.recv() => {},
		_ = sigterm.recv() => {},
	}
}

fn start_docker(docker_bin: &str, docker_image: &str) -> anyhow::Result<Child> {
	let docker_process = Command::new(docker_bin)
		.args([
			"run",
			"--name",
			DOCKER_CONTAINER_NAME,
			"--rm",
			"-p",
			"8545:8545",
			&format!("docker.io/paritypr/{docker_image}"),
			"--node-rpc-url",
			"wss://westend-asset-hub-rpc.polkadot.io",
			"--rpc-cors",
			"all",
			"--unsafe-rpc-external",
			"--log=sc_rpc_server:info",
		])
		.stderr(std::process::Stdio::piped())
		.kill_on_drop(true)
		.spawn()?;

	Ok(docker_process)
}

async fn kill_docker() -> anyhow::Result<()> {
	Command::new("docker").args(["kill", DOCKER_CONTAINER_NAME]).output().await?;
	Ok(())
}

async fn test_eth_rpc(stderr: ChildStderr) -> anyhow::Result<()> {
	let mut reader = BufReader::new(stderr).lines();
	while let Some(line) = reader.next_line().await? {
		println!("{line}");
		if line.contains("Running JSON-RPC server") {
			break;
		}
	}

	let account = Account::default();
	let data = vec![];
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let input = bytes.into_iter().chain(data).collect::<Vec<u8>>();

	println!("Account:");
	println!("- address: {:?}", account.address());
	let client = HttpClientBuilder::default().build("http://localhost:8545")?;

	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let balance = client.get_balance(account.address(), BlockTag::Latest.into()).await?;
	println!("-  nonce: {nonce:?}");
	println!("-  balance: {balance:?}");

	println!("\n\n=== Deploying dummy contract ===\n\n");
	let hash = TransactionBuilder::default().input(input).send(&client).await?;

	println!("Hash: {hash:?}");
	println!("Waiting for receipt...");
	let ReceiptInfo { block_number, gas_used, contract_address, .. } =
		wait_for_receipt(&client, hash).await?;

	let contract_address = contract_address.unwrap();
	println!("\nReceipt:");
	println!("Block explorer: https://westend-asset-hub-eth-explorer.parity.io/{hash:?}");
	println!("- Block number: {block_number}");
	println!("- Gas used: {gas_used}");
	println!("- Address: {contract_address:?}");

	println!("\n\n=== Calling dummy contract ===\n\n");
	let hash = TransactionBuilder::default().to(contract_address).send(&client).await?;

	println!("Hash: {hash:?}");
	println!("Waiting for receipt...");

	let ReceiptInfo { block_number, gas_used, to, .. } = wait_for_receipt(&client, hash).await?;
	println!("\nReceipt:");
	println!("Block explorer: https://westend-asset-hub-eth-explorer.parity.io/{hash:?}");
	println!("- Block number: {block_number}");
	println!("- Gas used: {gas_used}");
	println!("- To: {to:?}");
	Ok(())
}

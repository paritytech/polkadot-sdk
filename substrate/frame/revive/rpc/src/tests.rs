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
				Some((_, after)) => after.split_once(",").unwrap().0,
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
	let (info, _) = extract_info_from_output(node_child.stderr.take().unwrap());
	let (_rpc_child, ws_url) = start_eth_rpc_server(&info.ws_url);

	let data = b"hello world".to_vec();
	let (bytes, _) = pallet_revive_fixtures::compile_module("dummy")?;
	let input = bytes.into_iter().chain(data.clone()).collect::<Vec<u8>>();

	let account = Account::default();
	let client = WsClientBuilder::default().build(ws_url).await?;

	// Deploy contract
	let nonce = client.get_transaction_count(account.address(), BlockTag::Latest.into()).await?;
	let hash = send_transaction(&account, &client, U256::zero(), input.into(), None).await?;
	let receipt = wait_for_receipt(&client, hash).await?;
	let contract_address = create1(&account.address(), nonce.try_into().unwrap());
	assert_eq!(contract_address, receipt.contract_address.unwrap());

	// Call contract
	let hash =
		send_transaction(&account, &client, U256::zero(), Bytes::default(), Some(contract_address))
			.await?;
	let receipt = wait_for_receipt(&client, hash).await?;
	assert_eq!(contract_address, receipt.to.unwrap());

	Ok(())
}

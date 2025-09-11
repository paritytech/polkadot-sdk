// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use pallet_revive::evm::{Account, Block as EvmBlock, BlockNumberOrTag, BlockTag, ReceiptInfo};
use pallet_revive_eth_rpc::{example::TransactionBuilder, subxt_client, EthRpcClient};
use pallet_revive_zombienet::{EthRpcServer, ZombienetNetwork, BEST_BLOCK_METRIC};
use sp_core::H256;
use std::sync::Arc;
use subxt::{
	self, backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig,
};
// use zombienet_sdk::subxt::{
// 	self, backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig,
// };

const COLLATOR_RPC_PORT: u16 = 9944;
const ROOT_FROM_NO_DATA: &str = "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";
const ETH_RPC_URL: &str = "http://localhost:8545";

async fn transfer() -> Result<(), anyhow::Error> {
	let client = Arc::new(HttpClientBuilder::default().build("http://localhost:8545")?);

	let alith = Account::default();
	let alith_address = alith.address();
	let ethan = Account::from(subxt_signer::eth::dev::ethan());
	let value = 1_000_000_000_000_000_000_000u128.into();

	let print_balance = || async {
		let balance = client.get_balance(alith_address, BlockTag::Latest.into()).await?;
		println!("Alith     {alith_address:?} balance: {balance:?}");
		let balance = client.get_balance(ethan.address(), BlockTag::Latest.into()).await?;
		println!("ethan {:?} balance: {balance:?}", ethan.address());
		anyhow::Result::<()>::Ok(())
	};

	print_balance().await?;
	println!("\n\n=== Transferring  ===\n\n");

	let tx = TransactionBuilder::new(&client)
		.signer(alith)
		.value(value)
		.to(ethan.address())
		.send()
		.await?;
	println!("Transaction hash: {:?}", tx.hash());

	let ReceiptInfo { block_number, gas_used, status, .. } = tx.wait_for_receipt().await?;
	println!("Receipt: ");
	println!("- Block number: {block_number}");
	println!("- Gas used: {gas_used}");
	println!("- Success: {status:?}");

	print_balance().await?;
	Ok(())
}

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_dont_spawn_zombienet() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let collator_ws_uri = format!("ws://127.0.0.1:{COLLATOR_RPC_PORT}");
	let collator_rpc_client = RpcClient::from_insecure_url(collator_ws_uri)
		.await
		.unwrap_or_else(|err| panic!("Failed to create RPC client: {err:?}"));
	let eth_rpc_client = Arc::new(
		HttpClientBuilder::default()
			.build(ETH_RPC_URL)
			.unwrap_or_else(|err| panic!("Failed to connect to eth-rpc server: {err:?}")),
	);

	sanity_check(collator_rpc_client, eth_rpc_client).await;
}

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_with_zombienet_spawning() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let zn = ZombienetNetwork::launch(COLLATOR_RPC_PORT)
		.await
		.unwrap_or_else(|err| panic!("Failed to spawn zombienet, {err:?}"));
	let base_dir = zn.network.base_dir().unwrap();

	let collator_name = "asset-hub-westend-collator1";
	let collator = zn
		.network
		.get_node(collator_name)
		.unwrap_or_else(|err| panic!("Failed to get collator node: {err:?}"));

	let _eth_rpc = EthRpcServer::launch(collator.ws_uri(), base_dir)
		.unwrap_or_else(|err| panic!("Failed to spawn ETH-RPC server: {err:?}"));

	// TODO: use below approach once subxt versions used here and in zombienet-sdk match
	// let collator_rpc_client = collator.rpc().await.unwrap_or_else(|err| {
	// 	panic!("Failed to get the RPC client for the collator {collator_name}: {err:?}")
	// });
	let collator_rpc_client = RpcClient::from_insecure_url(collator.ws_uri())
		.await
		.unwrap_or_else(|err| panic!("Failed to create RPC client: {err:?}"));

	let eth_rpc_client = Arc::new(
		HttpClientBuilder::default()
			.build(ETH_RPC_URL)
			.unwrap_or_else(|err| panic!("Failed to connect to eth-rpc server: {err:?}")),
	);

	sanity_check(collator_rpc_client, eth_rpc_client).await;

	assert!(zn
		.network
		.get_node("alice-westend-validator")
		.unwrap_or_else(|err| panic!("Failed to get node: {err:?}"))
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 600.0, 3600u64)
		.await
		.is_ok());
}

async fn sanity_check(collator_rpc_client: RpcClient, eth_rpc_client: Arc<HttpClient>) {
	let collator: OnlineClient<PolkadotConfig> =
		OnlineClient::from_rpc_client(collator_rpc_client.clone())
			.await
			.unwrap_or_else(|err| panic!("Failed to create client from RPC client: {err:?}"));
	for block_number_or_tag in [
		// TODO: block zero is reconstructed from substrate
		// BlockNumberOrTag::U256(0.into()),
		BlockNumberOrTag::U256(1.into()),
		BlockNumberOrTag::BlockTag(BlockTag::Earliest),
		BlockNumberOrTag::BlockTag(BlockTag::Finalized),
	] {
		println!("checking block: {block_number_or_tag:?}");
		let eth_rpc_block = eth_rpc_client
			.get_block_by_number(block_number_or_tag.clone(), false)
			.await
			.unwrap_or_else(|err| panic!("Failed to fetch block {block_number_or_tag:?}: {err:?}"))
			.expect("Expected block {block_number_or_tag:?} not found");

		println!("eth block number: {:?} hash: {:?}", eth_rpc_block.number, eth_rpc_block.hash);

		// Blocks with no transactions and no state should have the same roots
		assert_eq!(hex::encode(&eth_rpc_block.transactions_root), ROOT_FROM_NO_DATA);
		assert_eq!(hex::encode(&eth_rpc_block.receipts_root), ROOT_FROM_NO_DATA);
		assert_eq!(hex::encode(&eth_rpc_block.state_root), ROOT_FROM_NO_DATA);

		let substrate_block_hash: H256 = collator_rpc_client
			.request("chain_getBlockHash", rpc_params![eth_rpc_block.number])
			.await
			.unwrap_or_else(|err| {
				panic!("Failed to get block hash for block {:?}: {err:?}", eth_rpc_block.number)
			});

		println!(
			"substrate block number: {:?} hash: {substrate_block_hash:?}",
			eth_rpc_block.number
		);
		let storage = collator.storage().at(substrate_block_hash);

		let query = subxt_client::storage().revive().ethereum_block();
		let evm_block: EvmBlock = storage
			.fetch(&query)
			.await
			.unwrap_or_else(|err| panic!("Failed to fetch EvmBlock from storage: {err:?}"))
			.expect("EvmBlock not found in storage")
			.0;
		assert_eq!(eth_rpc_block, evm_block);

		let number_u256 = subxt::utils::Static(eth_rpc_block.number);
		let query = subxt_client::storage().revive().block_hash(number_u256);

		let block_hash_from_storage: H256 = storage
			.fetch(&query)
			.await
			.unwrap_or_else(|err| panic!("Failed to fetch block hash from storage: {err:?}"))
			.expect("Block hash not found in storage");
		assert_eq!(eth_rpc_block.hash, block_hash_from_storage);
	}
}

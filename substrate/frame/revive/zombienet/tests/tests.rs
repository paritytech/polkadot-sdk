// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use pallet_revive_zombienet::{EthRpcServer, ZombienetNetwork, BEST_BLOCK_METRIC};
const COLLATOR_RPC_PORT: u16 = 9944;

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_1() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let zn = ZombienetNetwork::launch(COLLATOR_RPC_PORT).await?;
	let base_dir = zn.network.base_dir().unwrap();

	let eth_rpc = EthRpcServer::launch("127.0.0.1", COLLATOR_RPC_PORT, base_dir)?;

	assert!(zn
		.network
		.get_node("alice-westend-validator")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 200.0, 1800u64)
		.await
		.is_ok());
	Ok(())
}

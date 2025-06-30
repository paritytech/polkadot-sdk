// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig, NetworkNode};

pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

pub async fn initialize_network(
	config: NetworkConfig,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	// Spawn network
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// Do not terminate network after the test is finished.
	// This is needed for CI to get logs from k8s.
	// Network shall be terminated from CI after logs are downloaded.
	// NOTE! For local execution (native provider) below call has no effect.
	network.detach().await;

	Ok(network)
}

pub async fn wait_node_is_up(
	node: &NetworkNode,
	timeout_secs: impl Into<u64>,
) -> Result<(), anyhow::Error> {
	node.wait_metric_with_timeout("process_start_time_seconds", |b| b >= 1.0, timeout_secs)
		.await
		.map_err(|err| anyhow::anyhow!("{}: {:?}", node.name(), err))
}

pub async fn wait_network_is_up(
	nodes: &[&NetworkNode],
	timeout_secs: u64,
) -> Result<(), anyhow::Error> {
	let handles = nodes.iter().map(|&node| wait_node_is_up(node, timeout_secs));

	futures::future::try_join_all(handles).await?;

	Ok(())
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig};

pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";
pub const FINALIZED_BLOCK_METRIC: &str = "substrate_block_height{status=\"finalized\"}";
pub const BEEFY_BEST_BLOCK_METRIC: &str = "substrate_beefy_best_block";
pub const DEFAULT_SUBSTRATE_IMAGE: &str = "docker.io/paritypr/substrate:latest";
pub const NODE_ROLE_METRIC: &str = "node_roles";
pub const PEER_COUNT_METRIC: &str = "substrate_sub_libp2p_peers_count";

pub const DEFAULT_DB_SNAPSHOT_URL: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/substrate/0001-basic-warp-sync/chains-0bb3f0be2ce41b5615b224215bcc8363aa0416a6.tgz";
pub const DEFAULT_CHAIN_SPEC: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/substrate/chain-spec.json";

pub const FULLNODE_ROLE_VALUE: f64 = 1.0;
pub const VALIDATOR_ROLE_VALUE: f64 = 4.0;

pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const DB_SNAPSHOT_ENV: &str = "DB_SNAPSHOT";
pub const CHAIN_SPEC_ENV: &str = "WARP_CHAIN_SPEC_PATH";
pub const DB_BLOCK_HEIGHT_ENV: &str = "DB_BLOCK_HEIGHT";

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

pub fn env_or_default(var: &str, default: &str) -> String {
	std::env::var(var).unwrap_or_else(|_| default.to_string())
}

pub fn db_snapshot_height_override_from_env() -> Option<f64> {
	std::env::var(DB_BLOCK_HEIGHT_ENV)
		.ok()
		.and_then(|value| value.parse::<f64>().ok())
}

pub async fn resolve_db_snapshot_height(
	network: &Network<LocalFileSystem>,
	node_name: &str,
) -> anyhow::Result<f64> {
	if let Some(override_height) = db_snapshot_height_override_from_env() {
		return Ok(override_height);
	}

	let node = network.get_node(node_name)?;
	let height = node.reports(BEST_BLOCK_METRIC).await?;
	Ok(height)
}

pub fn log_line_at_least_once(timeout_secs: u64) -> LogLineCountOptions {
	LogLineCountOptions::new(|count| count >= 1, Duration::from_secs(timeout_secs), false)
}

pub fn log_line_exactly_once(timeout_secs: u64) -> LogLineCountOptions {
	LogLineCountOptions::new(|count| count == 1, Duration::from_secs(timeout_secs), false)
}

pub fn log_line_absent(timeout_secs: u64) -> LogLineCountOptions {
	LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(timeout_secs))
}

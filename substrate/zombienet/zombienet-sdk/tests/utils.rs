// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig};

pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";
pub const FINALIZED_BLOCK_METRIC: &str = "substrate_block_height{status=\"finalized\"}";
pub const BEEFY_BEST_BLOCK_METRIC: &str = "substrate_beefy_best_block";
pub const DEFAULT_SUBSTRATE_IMAGE: &str = "docker.io/paritypr/substrate:latest";

pub const DEFAULT_DB_SNAPSHOT_URL: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/substrate/0001-basic-warp-sync/chains-0bb3f0be2ce41b5615b224215bcc8363aa0416a6.tgz";
pub const DEFAULT_CHAIN_SPEC: &str =
	"https://raw.githubusercontent.com/paritytech/polkadot-sdk/refs/heads/master/substrate/zombienet/0001-basic-warp-sync/chain-spec.json";

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

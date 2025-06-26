// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The zombienet spawner for integration tests for a transaction pool. Holds shared logic used
//! across integration tests for transaction pool.

use anyhow::anyhow;
use tracing_subscriber::EnvFilter;
use txtesttool::scenario::{ChainType, ScenarioBuilder};
use zombienet_sdk::{
	subxt::SubstrateConfig, LocalFileSystem, Network, NetworkConfig, NetworkConfigExt,
};

/// Gathers TOML files paths for relaychains and for parachains' (that use rococo-local based
/// relaychains) zombienet network specs for testing in relation to fork aware transaction pool.
pub mod relaychain_rococo_local_network_spec {
	pub const HIGH_POOL_LIMIT_FATP: &'static str =
		"tests/zombienet/network-specs/rococo-local-high-pool-limit-fatp.toml";

	/// Network specs used for fork-aware tx pool testing of parachains.
	pub mod parachain_asset_hub_network_spec {
		pub const LOW_POOL_LIMIT_FATP: &'static str =
			"tests/zombienet/network-specs/asset-hub-low-pool-limit-fatp.toml";
		pub const HIGH_POOL_LIMIT_FATP: &'static str =
			"tests/zombienet/network-specs/asset-hub-high-pool-limit-fatp.toml";
	}
}

/// Default time that we expect to need for a full run of current tests that send future and ready
/// txs to parachain or relaychain networks.
pub const DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS: u64 = 1500;

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("Network initialization failure: {0}")]
	NetworkInit(anyhow::Error),
	#[error("Node couldn't be found as part of the network: {0}")]
	NodeNotFound(anyhow::Error),
	#[error("Failed to get node online client")]
	FailedToGetOnlineClinet,
	#[error("Failed to get node blocks stream")]
	FailedToGetBlocksStream,
}

/// Result of work related to network spawning.
pub type Result<T> = std::result::Result<T, Error>;

/// Provides logic to spawn a network based on a Zombienet toml file.
pub struct NetworkSpawner {
	network: Network<LocalFileSystem>,
}

impl NetworkSpawner {
	/// Initialize the network spawner based on a Zombienet toml file
	pub async fn from_toml_with_env_logger(toml_path: &'static str) -> Result<NetworkSpawner> {
		// Initialize the subscriber with a default log level of INFO if RUST_LOG is not set
		let env_filter =
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
		// Set up the subscriber with the formatter and the environment filter
		tracing_subscriber::fmt()
			.with_env_filter(env_filter) // Use the env filter
			.init();

		let net_config = NetworkConfig::load_from_toml(toml_path).map_err(Error::NetworkInit)?;
		Ok(NetworkSpawner {
			network: net_config
				.spawn_native()
				.await
				.map_err(|err| Error::NetworkInit(anyhow!(err.to_string())))?,
		})
	}

	/// Returns the spawned network.
	pub fn network(&self) -> &Network<LocalFileSystem> {
		&self.network
	}

	/// Waits for blocks production/import to kick-off on given node.
	pub async fn wait_for_block_production(&self, node_name: &str) -> Result<()> {
		let node = self
			.network
			.get_node(node_name)
			.map_err(|_| Error::NodeNotFound(anyhow!("{node_name}")))?;
		let client = node
			.wait_client::<SubstrateConfig>()
			.await
			.map_err(|_| Error::FailedToGetOnlineClinet)?;
		let mut stream = client
			.blocks()
			.subscribe_best()
			.await
			.map_err(|_| Error::FailedToGetBlocksStream)?;
		// It should take at most two iterations to return with the best block, if any.
		for _ in 0..=1 {
			let Some(block) = stream.next().await else {
				continue;
			};

			if let Some(block) = block.ok().filter(|block| block.number() == 1) {
				tracing::info!("[{node_name}] found first best block: {:#?}", block.hash());
				break;
			}

			tracing::info!("[{node_name}] waiting for first best block");
		}
		Ok(())
	}

	/// Get the network filesystem base dir path.
	pub fn base_dir_path(&self) -> Option<&str> {
		self.network.base_dir()
	}

	/// Get a certain node rpc uri.
	pub fn node_rpc_uri(&self, node_name: &str) -> Result<String> {
		self.network
			.get_node(node_name)
			.and_then(|node| Ok(node.ws_uri().to_string()))
			.map_err(|_| Error::NodeNotFound(anyhow!("{node_name}")))
	}
}

/// Shared params usually set in same way for most of the scenarios.
pub struct ScenarioBuilderSharedParams {
	watched_txs: bool,
	does_block_monitoring: bool,
	send_threshold: usize,
	chain_type: ChainType,
}

impl Default for ScenarioBuilderSharedParams {
	fn default() -> Self {
		Self {
			watched_txs: true,
			does_block_monitoring: false,
			send_threshold: 20000,
			chain_type: ChainType::Sub,
		}
	}
}

/// Creates a [`txtesttool::scenario::ScenarioBuilder`] with a set of default parameters defined
/// with [`ScenarioBuilderSharedParams::default`].
pub fn default_zn_scenario_builder(net_spawner: &NetworkSpawner) -> ScenarioBuilder {
	let shared_params = ScenarioBuilderSharedParams::default();
	ScenarioBuilder::new()
		.with_watched_txs(shared_params.watched_txs)
		.with_send_threshold(shared_params.send_threshold)
		.with_block_monitoring(shared_params.does_block_monitoring)
		.with_chain_type(shared_params.chain_type)
		.with_base_dir_path(net_spawner.base_dir_path().unwrap().to_string())
}

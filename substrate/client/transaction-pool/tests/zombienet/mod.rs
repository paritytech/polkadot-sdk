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
use tokio::sync::OnceCell;
use txtesttool::scenario::{ChainType, ScenarioBuilder};
use zombienet_sdk::{
	subxt::{OnlineClient, SubstrateConfig},
	LocalFileSystem, Network, NetworkConfig, NetworkConfigExt,
};

pub const ASSET_HUB_LOW_POOL_LIMIT_FATP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-low-pool-limit-fatp.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_FATP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-fatp.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_OLDP_3_COLLATORS_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-oldp-3-collators.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_OLDP_4_COLLATORS_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-oldp-4-collators.toml";

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

static LOGGER: OnceCell<()> = OnceCell::const_new();
async fn init_logger() {
	LOGGER
		.get_or_init(|| async {
			let _ = env_logger::try_init_from_env(
				env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
			);
		})
		.await;
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
		init_logger().await;
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

	/// Returns a node client and waits for blocks productio to kick-off.
	pub async fn wait_collator_client(
		&self,
		node_name: &str,
	) -> Result<OnlineClient<SubstrateConfig>> {
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
		loop {
			let Some(block) = stream.next().await else {
				continue;
			};

			if let Ok(_) =
				block.and_then(|block| Ok(tracing::info!("found best block: {:#?}", block.hash())))
			{
				break;
			}
		}

		Ok(client)
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
	chain_type: ChainType::Sub,
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
pub fn default_zn_scenario_builder() -> ScenarioBuilder {
	let shared_params = ScenarioBuilderSharedParams::default();
	ScenarioBuilder::new()
		.with_watched_txs(shared_params.watched_txs)
		.with_send_threshold(shared_params.send_threshold)
		.with_block_monitoring(shared_params.does_block_monitoring)
		.with_chain_type(shared_params.chain_type)
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use serde_json::json;
use std::process::{Child, Command};
// use zombienet_orchestrator::network::{self, node::LogLineCountOptions};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	LocalFileSystem, Network, NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 1000;
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

const NODE_RPC_PORT: u16 = 9944;

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_1() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let network = spawn_network().await?;

	log::info!("Checking if network nodes are up");
	let result = network.wait_until_is_up(200u64).await;
	assert!(result.is_ok(), "Network is not up: {:?}", result.unwrap_err());

	let base_dir = network.base_dir().unwrap();
	let collator1 = network.get_node("asset-hub-westend-collator1")?;
	let collator_client: OnlineClient<PolkadotConfig> = collator1.wait_client().await?;

	// let eth_rpc = launch_eth_rpc_server("127.0.0.1", NODE_RPC_PORT, base_dir)?;

	assert!(network
		.get_node("alice-westend-validator")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 200.0, 180u64)
		.await
		.is_ok());
	Ok(())
}

// Using below approach instead of '*.toml' because toml does not allow to
// unset some fields when patching ("devStakers" in this particular case).
async fn spawn_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,xcm=trace").into()])
				.with_node(|node| {
					node.with_name("alice-westend-validator").with_initial_balance(2000000000000)
				})
				.with_node(|node| {
					node.with_name("bob-westend-validator").with_initial_balance(2000000000000)
				})
		})
		.with_parachain(|p| {
			p.with_id(1000)
				.with_chain("asset-hub-westend-local")
				.cumulus_based(true)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime::revive=debug").into()])
				// Unset devStakers
				// https://matrix.to/#/!fBnLYzzPKYqhYEWXWL:parity.io/$YJwq5jVe22ehGBukwwt2eYYICJFc_6bqoMpwbzlU4lU?via=parity.io&via=matrix.org&via=web3.foundation
				// https://matrix.to/#/!fBnLYzzPKYqhYEWXWL:parity.io/$9scJvupMsoM837t-L6Ww-anY-L40dUipPrncbmDkjbI?via=parity.io&via=matrix.org&via=web3.foundation
				.with_genesis_overrides(json!({
					"staking": {
						"devStakers": null,
					}
				}))
				.with_collator(|n| {
					n.with_name("asset-hub-westend-collator1")
						// eth-rpc will connect to this port
						.with_rpc_port(NODE_RPC_PORT)
				})
				.with_collator(|n| n.with_name("asset-hub-westend-collator2"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

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

pub struct EthRpcHandle {
	child: Child,
}

impl EthRpcHandle {
	pub fn kill(&mut self) -> Result<(), anyhow::Error> {
		log::info!("Terminating eth-rpc server");
		self.child
			.kill()
			.map_err(|e| anyhow!("Failed to kill eth-rpc process: {}", e))?;
		self.child
			.wait()
			.map_err(|e| anyhow!("Failed to wait for eth-rpc process: {}", e))?;
		Ok(())
	}
}

impl Drop for EthRpcHandle {
	fn drop(&mut self) {
		if let Err(e) = self.kill() {
			log::error!("Failed to terminate eth-rpc server in drop: {}", e);
		}
	}
}

pub fn launch_eth_rpc_server(
	node_rpc_ip: &str,
	node_rpc_port: u16,
	log_path: &str,
) -> Result<EthRpcHandle, anyhow::Error> {
	let node_rpc_url = format!("ws://{node_rpc_ip}:{node_rpc_port}");
	log::info!("Launching eth-rpc server with node RPC URL: {}", node_rpc_url);

	let child = Command::new("eth-rpc")
		.arg("--node-rpc-url")
		.arg(node_rpc_url)
		.arg("--dev")
		.spawn()
		.map_err(|e| anyhow!("Failed to spawn eth-rpc process: {}", e))?;

	log::info!("eth-rpc server launched with PID: {}", child.id());

	Ok(EthRpcHandle { child })
}

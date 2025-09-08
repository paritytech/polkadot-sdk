// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// use zombienet_orchestrator::network::{self, node::LogLineCountOptions};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	GlobalSettingsBuilder, LocalFileSystem, Network, NetworkConfig,
};

async fn initialize_network(
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

// This tests makes sure that RPC collator is able to build blocks
#[tokio::test(flavor = "multi_thread")]
async fn test_1() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let settings = {
		let mut builder = GlobalSettingsBuilder::new();
		if let Ok(base_dir) = std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			builder = builder.with_base_dir(base_dir);
		}
		builder.build().unwrap()
	};
	let config = NetworkConfig::load_from_toml_with_settings(
		"examples/westend_local_network.toml",
		&settings,
	)?;

	log::info!("Spawning network");
	let network = initialize_network(config).await?;

	log::info!("Checking if network nodes are up");
	let result = network.wait_until_is_up(200u64).await;
	assert!(result.is_ok(), "Network is not up: {:?}", result.unwrap_err());

	let collator1 = network.get_node("asset-hub-westend-collator1")?;
	let collator_client: OnlineClient<PolkadotConfig> = collator1.wait_client().await?;

	Ok(())
}

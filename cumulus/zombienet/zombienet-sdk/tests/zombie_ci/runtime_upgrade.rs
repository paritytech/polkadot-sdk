// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use crate::utils::BEST_BLOCK_METRIC;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;

use polkadot_primitives::Id as ParaId;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_configuration::types::AssetLocation;
use zombienet_sdk::{
	tx_helper::{ChainUpgrade, RuntimeUpgradeOptions},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const WASM_WITH_SPEC_VERSION_INCREMENTED: &str =
	"/tmp/wasm_binary_spec_version_incremented.rs.compact.compressed.wasm";

// This tests makes sure that it is possible to upgrade parachain's runtime
// and parachain produces blocks after such upgrade.
#[tokio::test(flavor = "multi_thread")]
async fn runtime_upgrade() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let network = initialize_network().await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_throughput(
		&alice_client,
		20,
		[(ParaId::from(PARA_ID), 2..40)].into_iter().collect(),
	)
	.await?;

	let timeout_secs: u64 = 250;
	let charlie = network.get_node("charlie")?;

	let charlie_client: OnlineClient<PolkadotConfig> = charlie.wait_client().await?;
	let current_spec_version =
		charlie_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version {current_spec_version}");

	log::info!("Performing runtime upgrade");
	network
		.parachain(PARA_ID)
		.unwrap()
		.perform_runtime_upgrade(
			charlie,
			RuntimeUpgradeOptions::new(AssetLocation::from(WASM_WITH_SPEC_VERSION_INCREMENTED)),
		)
		.await?;

	let current_best_block = charlie.reports(BEST_BLOCK_METRIC).await?;
	log::info!("Current parachain best block {current_best_block}");

	log::info!("Checking block production");
	assert!(network
		.get_node("dave")?
		.wait_metric_with_timeout(
			BEST_BLOCK_METRIC,
			|b| b >= current_best_block + 10.0,
			timeout_secs
		)
		.await
		.is_ok());

	let incremented_spec_version =
		charlie_client.backend().current_runtime_version().await?.spec_version;

	log::info!("Incremented runtime spec version {incremented_spec_version}");

	assert_eq!(
		current_spec_version + 1,
		incremented_spec_version,
		"Unexpected runtime spec version"
	);

	Ok(())
}

async fn initialize_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice   - validator
	// 	 - bob     - validator
	// - parachain nodes
	//   - charlie - validator
	//   - dave    - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("charlie")
						.validator(true)
						.with_args(vec![("-lparachain=debug").into()])
				})
				.with_collator(|n| n.with_name("dave").validator(false))
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

	Ok(network)
}

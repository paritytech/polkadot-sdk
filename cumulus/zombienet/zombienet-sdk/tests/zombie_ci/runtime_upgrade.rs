// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::time::Duration;

use crate::utils::initialize_network;

use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_upgrade};
use polkadot_primitives::Id as ParaId;
use zombienet_configuration::types::AssetLocation;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	tx_helper::{ChainUpgrade, RuntimeUpgradeOptions},
	NetworkConfig, NetworkConfigBuilder,
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
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain making progress");
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

	let dave = network.get_node("dave")?;
	let dave_client: OnlineClient<PolkadotConfig> = dave.wait_client().await?;
	let expected_spec_version = current_spec_version + 1;

	log::info!(
		"Waiting (up to {timeout_secs}s) for parachain runtime upgrade to version {}",
		expected_spec_version
	);
	tokio::time::timeout(
		Duration::from_secs(timeout_secs),
		wait_for_upgrade(dave_client, expected_spec_version),
	)
	.await
	.expect("Timeout waiting for runtime upgrade")?;

	let spec_version_from_charlie =
		charlie_client.backend().current_runtime_version().await?.spec_version;
	assert_eq!(expected_spec_version, spec_version_from_charlie, "Unexpected runtime spec version");

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
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

	Ok(config)
}

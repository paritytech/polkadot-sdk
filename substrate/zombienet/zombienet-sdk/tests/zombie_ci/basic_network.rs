// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use anyhow::anyhow;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn basic_network_spawns_and_produces_blocks() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	for node in [alice, bob, charlie] {
		log::info!("Ensuring {} is connected to at least 1 peer", node.name());
		assert!(node
			.wait_metric_with_timeout("sub_libp2p_peers_count", |p| p >= 1.0, 60u64)
			.await
			.is_ok());
	}

	for (node, block_height, timeout) in
		[(alice, 5.0, 60u64), (bob, 5.0, 60u64), (charlie, 5.0, 60u64)]
	{
		log::info!(
			"Ensuring {} produced/imported blocks beyond height {}",
			node.name(),
			block_height
		);
		assert!(node
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > block_height, timeout)
			.await
			.is_ok());
	}

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	log::info!("Building network config");

	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - Nodes:
	//   - alice (validator)
	//   - bob (validator)
	//   - charlie (full node)
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("dev")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_node(|node| node.with_name("alice").validator(true))
				.with_node(|node| node.with_name("bob").validator(true))
				.with_node(|node| node.with_name("charlie").validator(false))
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

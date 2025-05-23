// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use cumulus_zombienet_sdk_helpers::wait_for_nth_session_change;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				// Not strictly necessary for the test, but to keep it consistent
				// with the parachain part we also pass `--no-mdns` to the relaychain.
				.with_default_args(vec!["-lparachain=debug".into(), "--no-mdns".into()])
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));
			(1..3).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(1000)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("asset-hub-rococo-local")
				// Do not put bootnodes into the chain-spec nor command line arguments.
				.without_default_bootnodes()
				// Disable mdns to rely only on DHT bootnode discovery mechanism.
				.with_default_args(vec![
					"-lbootnodes=trace".into(),
					"--no-mdns".into(),
					"--discover-local".into(),
					"--".into(),
					"--no-mdns".into(),
				])
				.with_collator(|n| n.with_name("alpha"))
				.with_collator(|n| n.with_name("beta"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}

#[tokio::test(flavor = "multi_thread")]
async fn dht_bootnodes_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let mut network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	let alpha = network.get_node("alpha")?;

	// Make sure the collators connect to each other.
	alpha
		.wait_metric_with_timeout("substrate_sync_peers", |count| count == 1.0, 300u64)
		.await?;

	let log_line_options = LogLineCountOptions::new(|n| n == 1, Duration::from_secs(30), false);

	// Make sure the DHT bootnode discovery was successful.
	let result = alpha
		.wait_log_line_count_with_timeout(
			".* Parachain bootnode discovery on the relay chain DHT succeeded",
			false,
			log_line_options.clone(),
		)
		.await?;
	assert!(result.success());

	log::info!(
		"First two collators successfully connected via DHT bootnodes. \
		 Waiting for two full sessions (~3min) before spawning a third collator."
	);

	// Wait for two full sessions (three session changes) and spawn a new collator to check the
	// bootnode is also advertised with the new epoch key (republishing works).
	let mut blocks_sub = relay_client.blocks().subscribe_all().await?;
	wait_for_nth_session_change(&mut blocks_sub, 3).await?;
	drop(blocks_sub);

	log::info!("Spawning the third collator.");
	network.add_collator("gamma", Default::default(), 1000).await?;

	let gamma = network.get_node("gamma")?;

	// Make sure the new collator has connected to the existing collators.
	gamma
		.wait_metric_with_timeout("substrate_sync_peers", |count| count == 2.0, 300u64)
		.await?;

	// Make sure the DHT bootnode discovery was successful.
	let result = gamma
		.wait_log_line_count_with_timeout(
			".* Parachain bootnode discovery on the relay chain DHT succeeded",
			false,
			log_line_options,
		)
		.await?;

	assert!(result.success());

	log::info!("Test finished successfully.");

	Ok(())
}

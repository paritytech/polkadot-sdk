// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use subxt::{OnlineClient, PolkadotConfig};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

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
				.with_collator(|n| n.with_name("collator-0"))
				.with_collator(|n| n.with_name("collator-1"))
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
	let network = spawn_fn(config).await?;

	let para_node = network.get_node("collator-0")?;
	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;

	let mut blocks_sub = para_client.blocks().subscribe_all().await?;

	// Skip the genesis block.
	let genesis_block = blocks_sub.next().await.expect("to receive genesis block")?;
	assert_eq!(genesis_block.number(), 0);

	// Producing the first block is a good indicator the colators have connected to each other.
	let first_block = blocks_sub.next().await.expect("to receive first block")?;
	assert_eq!(first_block.number(), 1);

	// Make sure we are connected to another collator. This can be not the case if the collator
	// produced a block locally without actually talking to another collator.
	assert!(para_node.assert("substrate_sync_peers", 1).await?);

	log::info!("Test finished successfully");

	Ok(())
}

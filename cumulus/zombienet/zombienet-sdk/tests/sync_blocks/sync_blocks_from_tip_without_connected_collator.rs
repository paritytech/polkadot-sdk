// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

#[tokio::test(flavor = "multi_thread")]
async fn sync_blocks_from_tip_without_connected_collator() -> Result<(), anyhow::Error> {
	let network = initialize_network().await?;

	let relay_alice = network.get_node("alice")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;

	log::info!("Ensuring parachain making progress");
	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 9..11)].into_iter().collect(),
	)
	.await?;

	let para_ferdie = network.get_node("ferdie")?;
	let para_eve = network.get_node("eve")?;

	log::info!("Ensuring ferdie and eve are connected to 1 peer only");
	assert!(para_eve.assert("sub_libp2p_peers_count", 1).await?);
	assert!(para_ferdie.assert("sub_libp2p_peers_count", 1).await?);

	log::info!("Ensuring ferdie and eve are syncing");
	assert!(para_ferdie
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 12.0, 250u64)
		.await
		.is_ok());
	assert!(para_eve
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 12.0, 250u64)
		.await
		.is_ok());

	Ok(())
}

async fn initialize_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	log::info!("Spawning network");

	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain Nodes:
	// 	 - alice
	// 	 - bob
	// - parachain Nodes:
	//   - charlie - collator
	//   - dave    - full node
	//   - eve     - full node; connected only to dave,
	//   - ferdie  - full node; connected only to dave; gets relay chain data only from alice
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(2).with_request_memory("2G")
				})
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("dave").validator(false))
				.with_collator(|n| n.with_name("charlie").validator(true))
				.with_collator(|n| {
					n.with_name("eve").validator(false).with_args(vec![
						"--reserved-only".into(),
						("--reserved-nodes", "{{ZOMBIE:dave:multiAddress}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("ferdie").validator(false).with_args(vec![
						"--reserved-only".into(),
						("--reserved-nodes", "{{ZOMBIE:dave:multiaddr}}").into(),
						("--relay-chain-rpc-url", "{{ZOMBIE:alice:ws_uri}}").into(),
					])
				})
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

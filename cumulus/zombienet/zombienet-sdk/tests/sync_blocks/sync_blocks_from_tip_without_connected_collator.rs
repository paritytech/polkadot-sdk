// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, create_assign_core_call,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{client::OfflineClientT, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::{AddCollatorOptions, NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn sync_blocks_from_tip_without_connected_collator() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let mut network = spawn_fn(config).await?;

	// Spawn remaining collators, those that are referring to the running ones

	let alice_ws_uri = network.get_node("alice")?.ws_uri().to_string();
	let dave_multi_address = network.get_node("dave")?.multi_addr().to_string();

	let eve_options = AddCollatorOptions {
		is_validator: false,
		args: vec![
			("--reserved-only").into(),
			(format!("--reserved-nodes={dave_multi_address}").as_str()).into(),
		],
		..Default::default()
	};
	network.add_collator("eve", eve_options, PARA_ID).await?;

	let ferdie_options = AddCollatorOptions {
		is_validator: false,
		args: vec![
			("--reserved-only").into(),
			(format!("--reserved-nodes={dave_multi_address}").as_str()).into(),
			(format!("--relay-chain-rpc-url={alice_ws_uri}").as_str()).into(),
		],
		..Default::default()
	};
	network.add_collator("ferdie", ferdie_options, PARA_ID).await?;

	let relay_node = network.get_node("alice")?;
	let para_node_ferdie = network.get_node("ferdie")?;
	let para_node_eve = network.get_node("eve")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Ensure parachains are registered.
	assert_para_throughput(&relay_client, 2, [(ParaId::from(PARA_ID), 2..3)].into_iter().collect())
		.await?;

	// Ensure parachains made progress.
	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 9..11)].into_iter().collect(),
	)
	.await?;

	let para_client: OnlineClient<PolkadotConfig> = para_node_ferdie.wait_client().await?;
	// Ensure parachains made progress.
	assert_para_throughput(
		&para_client,
		10,
		[(ParaId::from(PARA_ID), 9..11)].into_iter().collect(),
	)
	.await?;

	assert_finality_lag(&para_node_ferdie.wait_client().await?, 12).await?;
	assert_finality_lag(&para_node_eve.wait_client().await?, 12).await?;

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(2).with_request_memory("2G")
				})
				// Nodes are validators by default
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				// .cumulus_based()
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				// .with_chain("sync-blocks")
				.with_default_args(vec![("-lparachain=debug").into()])
				// charlie - parachain collator
				.with_collator(|n| n.with_name("charlie").validator(true))
				// dave - parachain full node
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
		})
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use polkadot_primitives::Id as ParaId;
use subxt::{OnlineClient, PolkadotConfig};

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::assert_para_is_backed;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;
const DB_SNAPSHOT_RELAYCHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/cumulus/0007-full_node_warp_sync/relaychain-12523fe793bff9f6d68651816879a09eec2c1462.tgz";
const DB_SNAPSHOT_PARACHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/cumulus/0007-full_node_warp_sync/parachain-f9bb05af6bef7afa50368540f5033f62359e9e5d.tgz";

#[tokio::test(flavor = "multi_thread")]
async fn full_node_warp_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is backed");
	assert_para_is_backed(&alice_client, ParaId::from(PARA_ID), 30).await?;

	for name in ["two", "three", "four", "five"] {
		log::info!("Checking full node {name} is syncing");
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, 225u64)
			.await
			.is_ok());
	}

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice    - validator
	// 	 - bob      - validator
	// 	 - charlie  - validator
	// 	 - dave     - validator
	// - parachain nodes
	//   - eve      - collator
	//   - ferdie   - collator
	//   - one      - collator
	//   - two      - full node
	//   - three    - full node
	//   - four     - full node
	//   - five     - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-relaychain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("alice").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN))
				.with_node(|node| node.with_name("bob").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN))
				.with_node(|node| {
					node.with_name("charlie").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN)
				})
				.with_node(|node| {
					node.with_name("dave").with_args(vec![
						("-lparachain=debug").into(),
						("--no-beefy").into(),
						("--reserved-only").into(),
						(
							"--reserved-nodes",
							vec![
								"{{ZOMBIE:alice:multiaddr}}",
								"{{ZOMBIE:bob:multiaddr}}",
								"{{ZOMBIE:charlie:multiaddr}}",
							],
						)
							.into(),
						("--sync", "warp").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-parachain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("eve").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| n.with_name("ferdie").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| n.with_name("one").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| {
					n.with_name("two").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--").into(),
						("--sync", "warp").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("three").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--relay-chain-rpc-urls", "{{ZOMBIE:alice:ws_uri}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("four").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--relay-chain-rpc-urls", "{{ZOMBIE:dave:ws_uri}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("five").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--relay-chain-light-client").into(),
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

	Ok(config)
}

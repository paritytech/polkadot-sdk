// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use crate::utils::BEST_BLOCK_METRIC;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;

use polkadot_primitives::Id as ParaId;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;

// This tests makes sure that parachain full nodes are synchronizing with the validator
// and report expected block height.
#[tokio::test(flavor = "multi_thread")]
async fn full_node_catching_up() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let network = initialize_network().await?;

	let relay_alice = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_throughput(&relay_client, 3, [(ParaId::from(PARA_ID), 2..4)].into_iter().collect())
		.await?;

	for (name, timeout_secs) in [("dave", 250u64), ("eve", 250u64), ("ferdie", 250u64)] {
		log::info!("Ensuring {name} reports expected block height");
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 7.0, timeout_secs)
			.await
			.is_ok());
	}

	// We want to make sure that none of the consensus hook checks fail, even if the chain makes
	// progress. If below log line occurred 1 or more times then test failed.
	log::info!("Ensuring none of the consensus hook checks fail");
	let result = network
		.get_node("charlie")?
		.wait_log_line_count_with_timeout(
			"set_validation_data inherent needs to be present in every block",
			false,
			LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
		)
		.await?;

	assert!(result.success());

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
	//   - dave    - full node; synchronizes only with charlie
	//   - eve     - RPC full node; synchronizes only with charlie
	//   - ferdie  - light client; synchronizes only with charlie
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
				.with_collator(|n| {
					n.with_name("dave").validator(false).with_args(vec![
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:charlie:multiaddr}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("eve").validator(false).with_args(vec![
						("--relay-chain-rpc-url", "{{ZOMBIE:alice:ws_uri}}").into(),
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:charlie:multiaddr}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("ferdie").validator(false).with_args(vec![
						("--relay-chain-light-client").into(),
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:charlie:multiaddr}}").into(),
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

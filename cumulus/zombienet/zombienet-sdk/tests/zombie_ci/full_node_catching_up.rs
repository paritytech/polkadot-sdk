// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

// This tests makes sure that parachain full nodes are synchronizing with the validator
// and report expected block height.
#[tokio::test(flavor = "multi_thread")]
async fn full_node_catching_up() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_alice = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;

	log::info!("Ensuring parachain making progress");
	assert_para_throughput(
		&relay_client,
		20,
		[(ParaId::from(PARA_ID), 2..40)].into_iter().collect(),
	)
	.await?;

	for (name, timeout_secs) in [("dave", 250u64), ("eve", 250u64)] {
		log::info!("Ensuring {name} reports expected block height");
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 7.0, timeout_secs)
			.await
			.is_ok());
	}

	// We want to make sure that none of the consensus hook checks fail, even if the chain makes
	// progress. If below log line occurred 1 or more times then test failed.
	log::info!("Ensuring none of the consensus hook checks fail at charlie");
	let result = network
		.get_node("charlie")?
		.wait_log_line_count_with_timeout(
			"set_validation_data inherent needs to be present in every block",
			false,
			LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
		)
		.await?;

	assert!(result.success(), "Consensus hook failed at charlie: {result:?}");

	// Check that AURA authorities tracking is working as expected.

	// Authorities must be fetched from runtime exactly once.
	let result = network
		.get_node("eve")?
		.wait_log_line_count_with_timeout(
			"Authorities for block .* at number 1 not found in cache, fetching from runtime",
			false,
			// Do not wait for more logs, everything has happened already.
			LogLineCountOptions::new(|n| n == 1, Duration::from_secs(0), false),
		)
		.await?;
	assert!(result.success(), "Authorities tracking failed for eve: {result:?}");

	// From there on, the authorities should be cached.
	let result = network
		.get_node("eve")?
		.wait_log_line_count_with_timeout(
			"Authorities for block .* at number .* found in cache",
			false,
			// Do not wait for more logs, everything has happened already.
			LogLineCountOptions::new(|n| n > 1, Duration::from_secs(0), false),
		)
		.await?;
	assert!(result.success(), "Authorities tracking failed for eve: {result:?}");

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
	//   - dave    - full node; synchronizes only with charlie
	//   - eve     - RPC full node; synchronizes only with charlie
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
						("-laura::authorities_tracker=debug").into(),
						("--relay-chain-rpc-url", "{{ZOMBIE:alice:ws_uri}}").into(),
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

	Ok(config)
}

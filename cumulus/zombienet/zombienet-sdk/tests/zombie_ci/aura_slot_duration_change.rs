// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{self, dynamic::Value, OnlineClient, PolkadotConfig, SubstrateConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

// This test makes sure that parachain nodes can keep synchronizing after the slot duration
// changes.
#[tokio::test(flavor = "multi_thread")]
async fn aura_slot_duration_change() -> Result<(), anyhow::Error> {
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

	log::info!("Ensuring dave reports expected block height");
	assert!(network
		.get_node("dave")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 7.0, 250u64)
		.await
		.is_ok());

	// Change the slot duration.
	log::info!("Submitting extrinsic to halve the slot duration");
	let call = subxt::dynamic::tx("TestPallet", "halve_slot_duration", Vec::<Value>::new());
	let charlie_client: OnlineClient<SubstrateConfig> =
		network.get_node("charlie")?.wait_client().await?;
	let res = charlie_client
		.tx()
		.sign_and_submit_then_watch_default(&call, &dev::alice())
		.await;
	assert!(res.is_ok(), "Extrinsic failed to submit: {:?}", res.unwrap_err());
	res.unwrap().wait_for_finalized_success().await.unwrap();
	log::info!("Extrinsic finalized");

	log::info!("Ensuring dave reports expected block height after slot duration change");
	assert!(network
		.get_node("dave")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 45.0, 250u64)
		.await
		.is_ok());

	// Check that the slot duration change was correctly detected.
	let result = network
		.get_node("dave")?
		.wait_log_line_count_with_timeout(
			"Slot duration changed from 6000ms to 3000ms",
			false,
			LogLineCountOptions::new(|n| n == 1, Duration::from_secs(0), false),
		)
		.await?;
	assert!(result.success(), "Did not detect slot duration change");

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

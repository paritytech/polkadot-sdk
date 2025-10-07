// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde_json::json;
use std::time::Duration;

use crate::utils::initialize_network;

use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, assign_cores, runtime_upgrade, wait_for_upgrade,
};
use polkadot_primitives::Id as ParaId;
use rstest::rstest;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const WASM_WITH_ELASTIC_SCALING: &str =
	"/tmp/wasm_binary_elastic_scaling.rs.compact.compressed.wasm";

const WASM_WITH_ELASTIC_SCALING_12S_SLOT: &str =
	"/tmp/wasm_binary_elastic_scaling_12s_slot.rs.compact.compressed.wasm";

// This test ensures that we can upgrade the parachain's runtime to support elastic scaling
// and that the parachain produces 3 blocks per slot after the upgrade.

// Covers both sync and async backing parachains.
#[tokio::test(flavor = "multi_thread")]
#[rstest]
#[case(true)]
#[case(false)]
async fn elastic_scaling_upgrade_to_3_cores(
	#[case] async_backing: bool,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config(async_backing).await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("validator0")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	assign_cores(alice, PARA_ID, vec![0]).await?;

	if async_backing {
		log::info!("Ensuring parachain makes progress making 6s blocks");
		assert_para_throughput(
			&alice_client,
			20,
			[(ParaId::from(PARA_ID), 15..21)].into_iter().collect(),
		)
		.await?;
	} else {
		log::info!("Ensuring parachain makes progress making 12s blocks");
		assert_para_throughput(
			&alice_client,
			20,
			[(ParaId::from(PARA_ID), 7..12)].into_iter().collect(),
		)
		.await?;
	}

	assign_cores(alice, PARA_ID, vec![1, 2]).await?;
	let timeout_secs: u64 = 250;
	let collator0 = network.get_node("collator0")?;
	let collator0_client: OnlineClient<PolkadotConfig> = collator0.wait_client().await?;

	let current_spec_version =
		collator0_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version {current_spec_version}");

	let wasm =
		if async_backing { WASM_WITH_ELASTIC_SCALING } else { WASM_WITH_ELASTIC_SCALING_12S_SLOT };

	runtime_upgrade(&network, collator0, PARA_ID, wasm).await?;

	let collator1 = network.get_node("collator1")?;
	let collator1_client: OnlineClient<PolkadotConfig> = collator1.wait_client().await?;
	let expected_spec_version = current_spec_version + 1;

	log::info!(
		"Waiting (up to {timeout_secs}s) for parachain runtime upgrade to version {}",
		expected_spec_version
	);
	tokio::time::timeout(
		Duration::from_secs(timeout_secs),
		wait_for_upgrade(collator1_client, expected_spec_version),
	)
	.await
	.expect("Timeout waiting for runtime upgrade")?;

	let spec_version_from_collator0 =
		collator0_client.backend().current_runtime_version().await?.spec_version;
	assert_eq!(
		expected_spec_version, spec_version_from_collator0,
		"Unexpected runtime spec version"
	);

	log::info!("Ensure elastic scaling works, 3 blocks should be produced in each 6s slot");
	assert_para_throughput(
		&alice_client,
		20,
		[(ParaId::from(PARA_ID), 50..61)].into_iter().collect(),
	)
	.await?;

	Ok(())
}

async fn build_network_config(async_backing: bool) -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let chain = if async_backing { "async-backing" } else { "sync-backing" };

	// Network setup:
	// - relaychain nodes:
	// 	 - alice   - validator
	// 	 - validator1   - validator
	// 	 - validator2   - validator
	// - parachain nodes
	//   - collator0 - validator
	//   - collator1    - validator
	//   - collator2     - validator
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 3,
								"max_validators_per_core": 1
							},
						}
					}
				}))
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("validator0"))
				.with_node(|node| node.with_name("validator1"))
				.with_node(|node| node.with_name("validator2"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.onboard_as_parachain(false)
				.with_chain(chain)
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator0").validator(true).with_args(vec![
						"--authoring=slot-based".into(),
						("-lparachain=debug,aura=debug").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("collator1").validator(true).with_args(vec![
						"--authoring=slot-based".into(),
						("-lparachain=debug,aura=debug").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("collator2").validator(true).with_args(vec![
						"--authoring=slot-based".into(),
						("-lparachain=debug,aura=debug").into(),
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

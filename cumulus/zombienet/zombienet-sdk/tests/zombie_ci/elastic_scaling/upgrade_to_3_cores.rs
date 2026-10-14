// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Elastic scaling: assign cores, upgrade the runtime, assert three blocks per slot.
//!
//! On JAM (`--features jam`) the cores are assigned by the Coretime chain's broker, which runs on
//! core 0, so the para starts on core 1.

#[cfg(not(feature = "jam"))]
use crate::utils::initialize_network;
#[cfg(not(feature = "jam"))]
use anyhow::anyhow;
#[cfg(feature = "jam")]
use cumulus_jam_zombienet_tests::{
	para::Para,
	spawn::{spawn, SpawnOptions},
};
use cumulus_test_runtime::{
	elastic_scaling::WASM_BINARY as WASM_ELASTIC_SCALING,
	elastic_scaling_12s_slot::WASM_BINARY as WASM_ELASTIC_SCALING_12S_SLOT,
};
use cumulus_zombienet_sdk_helpers::{
	network::{assert_para_throughput, assign_cores, upgrade_runtime},
	wait_for_pvf_prepare,
};
use polkadot_primitives::Id as ParaId;
use rstest::rstest;
#[cfg(not(feature = "jam"))]
use serde_json::json;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;

#[cfg(not(feature = "jam"))]
const FIRST_CORE: u32 = 0;
#[cfg(feature = "jam")]
const FIRST_CORE: u32 = 1;

/// `#[rstest]` cannot infer a runtime for an `async` case body, so `#[tokio::test]` is named
/// explicitly. JAM has no sync backing.
#[rstest]
#[case(true)]
#[cfg_attr(not(feature = "jam"), case(false))]
#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_upgrade_to_3_cores(
	#[case] async_backing: bool,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	#[cfg(not(feature = "jam"))]
	let network = initialize_network(build_network_config(async_backing).await?).await?;
	#[cfg(not(feature = "jam"))]
	let network = &network;
	#[cfg(feature = "jam")]
	let jam = spawn(
		"elastic_scaling_upgrade_to_3_cores",
		&[Para::new(PARA_ID, FIRST_CORE, &["collator0"])],
		SpawnOptions::new(4).with_coretime(),
	)
	.await?;
	#[cfg(feature = "jam")]
	let network = &jam.network;

	assign_cores(network, "validator0", PARA_ID, vec![FIRST_CORE]).await?;
	wait_for_pvf_prepare(network, 1).await?;

	let before_upgrade = if async_backing { 19..21 } else { 9..12 };
	assert_para_throughput(
		network,
		"validator0",
		20,
		[(ParaId::from(PARA_ID), before_upgrade)],
		[],
	)
	.await?;

	assign_cores(network, "validator0", PARA_ID, vec![FIRST_CORE + 1, FIRST_CORE + 2]).await?;

	log::info!("Performing the runtime upgrade to the elastic-scaling flavor");
	let wasm = if async_backing { WASM_ELASTIC_SCALING } else { WASM_ELASTIC_SCALING_12S_SLOT };
	upgrade_runtime(network, "collator0", wasm.expect("Wasm runtime not build")).await?;
	wait_for_pvf_prepare(network, 2).await?;

	log::info!("Ensure elastic scaling works, 3 blocks should be produced in each 6s slot");
	assert_para_throughput(network, "validator0", 20, [(ParaId::from(PARA_ID), 50..61)], []).await
}

#[cfg(not(feature = "jam"))]
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
				.with_validator(|node| node.with_name("validator0"))
				.with_validator(|node| node.with_name("validator1"))
				.with_validator(|node| node.with_name("validator2"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					("-lparachain=debug,xcm=trace").into(),
					("--force-authoring").into(),
				])
				.with_chain(chain)
				.with_collator(|n| n.with_name("collator0"))
				.with_collator(|n| n.with_name("collator1"))
				.with_collator(|n| n.with_name("collator2"))
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

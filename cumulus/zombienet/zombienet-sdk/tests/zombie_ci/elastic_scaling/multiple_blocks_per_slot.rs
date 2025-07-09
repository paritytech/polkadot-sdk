// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use crate::utils::initialize_network;

use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, create_assign_core_call,
	submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2400;

/// This test spawns a parachain network.
/// Initially, one core is assigned. We expect the parachain to produce 1 block per relay.
/// As we increase the number of cores via `assign_core`, we expect the block pace to increase too.
/// **Note:** The runtime in use here has 6s slot duration, so multiple blocks will be produced per
/// slot.
#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_multiple_blocks_per_slot() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node_elastic = network.get_node("collator-1")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();
	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 3..18)].into_iter().collect(),
	)
	.await?;
	assert_finality_lag(&para_node_elastic.wait_client().await?, 5).await?;

	let assign_cores_call = create_assign_core_call(&[(2, PARA_ID), (3, PARA_ID)]);

	let res = submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&relay_client,
		&assign_cores_call,
		&dev::alice(),
		60u64,
	)
	.await;
	assert!(res.is_ok(), "Extrinsic failed to finalize: {:?}", res.unwrap_err());
	log::info!("2 more cores assigned to each parachain");

	assert_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(PARA_ID), 39..46)].into_iter().collect(),
	)
	.await?;
	assert_finality_lag(&para_node_elastic.wait_client().await?, 20).await?;

	let assign_cores_call = create_assign_core_call(&[(4, PARA_ID), (5, PARA_ID), (6, PARA_ID)]);
	// Assign two extra cores to each parachain.
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_cores_call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	log::info!("3 more cores assigned to each parachain");

	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 52..61)].into_iter().collect(),
	)
	.await?;
	assert_finality_lag(&para_node_elastic.wait_client().await?, 30).await?;
	log::info!("Test finished successfully");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=trace").into()])
				.with_default_resources(|resources| {
					// These settings are applicable only for `k8s` provider.
					// Leaving them in case we switch to `k8s` some day.
					resources.with_request_cpu(4).with_request_memory("4G")
				})
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 7,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));
			(1..9).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-multi-block-slot")
				.with_default_args(vec![
					("--authoring").into(),
					("slot-based").into(),
					("-lparachain=trace,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-0"))
				.with_collator(|n| n.with_name("collator-1"))
				.with_collator(|n| n.with_name("collator-2"))
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

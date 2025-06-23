// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::time::Duration;

use crate::utils::{initialize_network, wait_node_is_up, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::{
	create_assign_core_call, submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use serde_json::json;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID_1: u32 = 2100;
const PARA_ID_2: u32 = 2000;

// TODO
#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_slot_based_authoring() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let collator_elastic = network.get_node("collator-elastic")?;
	let collator_single_core = network.get_node("collator-single-core")?;

	log::info!("Checking if alice is up");
	assert!(wait_node_is_up(alice, 60u64).await.is_ok());

	log::info!("Checking if collator-elastic is up");
	assert!(wait_node_is_up(collator_elastic, 60u64).await.is_ok());

	log::info!("Checking if collator-single-core is up");
	assert!(wait_node_is_up(collator_single_core, 60u64).await.is_ok());

	log::info!("Assigning cores for the parachain");
	let assign_cores_call = create_assign_core_call(&[(0, PARA_ID_1), (1, PARA_ID_1)]);

	let relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;
	let res = submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&relay_client,
		&assign_cores_call,
		&dev::alice(),
		60u64,
	)
	.await;
	assert!(res.is_ok(), "Extrinsic failed to finalize: {:?}", res.unwrap_err());
	log::info!("2 more cores assigned to the parachain");

	log::info!("Checking block production");
	assert!(collator_single_core
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 20.0, 225u64)
		.await
		.is_ok());

	assert!(collator_elastic
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 40.0, 225u64)
		.await
		.is_ok());

	// We want to make sure that none of the consensus hook checks fail, even if the chain makes
	// progress. If below log line occurred 1 or more times then test failed.
	log::info!("Ensuring none of the consensus hook checks fail");
	let result = collator_elastic
		.wait_log_line_count_with_timeout(
			"set_validation_data inherent needs to be present in every block",
			false,
			LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
		)
		.await?;
	assert!(result.success());

	let result = collator_single_core
		.wait_log_line_count_with_timeout(
			"set_validation_data inherent needs to be present in every block",
			false,
			LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
		)
		.await?;
	assert!(result.success());

	log::info!("Test finished successfully");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice
	// 	   - validator
	// 	 - validator[0-7]
	// 	   - validator
	// 	   - synchronize only with alice
	// - parachain nodes
	//   - recovery-target
	//     - full node
	//   - collator-elastic
	//     - full node
	//     - collator which is the only one producing blocks
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 4,
								"max_validators_per_core": 1
							},
							"approval_voting_params": {
								"max_approval_coalesce_count": 5
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("alice").with_args(vec![]));

			(0..8).fold(r, |acc, i| {
				acc.with_node(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lruntime=debug,parachain=trace").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_1)
				.with_chain("elastic-scaling")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n|
					n.with_name("collator-elastic")
						.with_args(vec![
							("-laura=trace,runtime=info,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug").into(),
							("--force-authoring").into(),
							("--authoring", "slot-based").into(),
					]))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_2)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n|
					n.with_name("collator-single-core")
						.with_args(vec![
							("-laura=trace,runtime=info,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug").into(),
							("--force-authoring").into(),
							("--authoring", "slot-based").into(),
					]))
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

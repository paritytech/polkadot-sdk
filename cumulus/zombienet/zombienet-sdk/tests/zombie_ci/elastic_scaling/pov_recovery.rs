// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::{sync::Arc, time::Duration};

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::{
	assert_para_is_registered, assert_para_throughput, assign_cores,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder, RegistrationStrategy,
};

const PARA_ID: u32 = 2100;

/// This test checks if parachain node is importing blocks using PoV recovery even
/// after more cores have been assigned for the parachain.
#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_pov_recovery() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network with relay chain only");
	let config = build_network_config().await?;
	let mut network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let collator_elastic = network.get_node("collator-elastic")?;

	log::info!("Checking if alice is up");
	assert!(alice.wait_until_is_up(60u64).await.is_ok());

	log::info!("Checking if collator-elastic is up");
	assert!(collator_elastic.wait_until_is_up(60u64).await.is_ok());

	assign_cores(alice, PARA_ID, vec![0, 1]).await?;

	log::info!("Waiting 20 blocks to register parachain");
	// Wait 20 blocks and register parachain. This part is important for pov-recovery.
	// We need to make sure that the recovering node is able to see all relay-chain
	// notifications containing the candidates to recover.
	assert!(alice
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 20.0, 250u64)
		.await
		.is_ok());

	log::info!("Registering parachain para_id = {PARA_ID}");
	let relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;
	network.register_parachain(PARA_ID).await?;

	log::info!("Ensuring parachain is registered within 30 blocks");
	assert_para_is_registered(&relay_client, ParaId::from(PARA_ID), 30).await?;

	log::info!("Ensuring parachain making progress");
	assert_para_throughput(
		&relay_client,
		20,
		[(ParaId::from(PARA_ID), 40..65)].into_iter().collect(),
	)
	.await?;

	let collator_elastic = network.get_node("collator-elastic")?;

	log::info!("Checking block production");
	assert!(collator_elastic
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 40.0, 225u64)
		.await
		.is_ok());

	// We want to make sure that none of the consensus hook checks fail, even if the chain makes
	// progress. If below log line occurred 1 or more times then test failed.
	log::info!("Ensuring none of the consensus hook checks fail at {}", collator_elastic.name());
	let result = collator_elastic
		.wait_log_line_count_with_timeout(
			"set_validation_data inherent needs to be present in every block",
			false,
			LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
		)
		.await?;

	assert!(result.success(), "Consensus hook failed at {}: {:?}", collator_elastic.name(), result);

	// Wait (up to 10 seconds) until pattern occurs more than 35 times
	let options = LogLineCountOptions {
		predicate: Arc::new(|n| n > 35),
		timeout: Duration::from_secs(10),
		wait_until_timeout_elapses: false,
	};

	let name = "recovery-target";
	log::info!("Ensuring blocks are imported using PoV recovery by {name}");
	let result = network
		.get_node(name)?
		.wait_log_line_count_with_timeout(
			"Importing blocks retrieved using pov_recovery",
			false,
			options,
		)
		.await?;

	assert!(result.success(), "Failed importing blocks using PoV recovery by {name}: {result:?}");

	log::info!("Test finished successfully");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice
	// 	   - validator
	// 	 - validator[0-3]
	// 	   - validator
	// 	   - synchronize only with alice
	// - parachain nodes
	//   - recovery-target
	//     - full node
	//   - collator-elastic
	//     - collator which is the only one producing blocks
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_resources(|resources| {
					// These settings are applicable only for `k8s` provider.
					// Leaving them in case we switch to `k8s` some day.
					resources
						.with_request_cpu(1)
						.with_request_memory("2G")
						.with_limit_cpu(2)
						.with_limit_memory("4G")
				})
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
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

			(0..4).fold(r, |acc, i| {
				acc.with_node(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lruntime=debug,parachain=trace").into(),
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:alice:multiaddr}}").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_chain("elastic-scaling")
				.with_registration_strategy(RegistrationStrategy::Manual)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_resources(|resources| {
					// These settings are applicable only for `k8s` provider.
					// Leaving them in case we switch to `k8s` some day.
					resources
						.with_request_cpu(1)
						.with_request_memory("2G")
						.with_limit_cpu(2)
						.with_limit_memory("4G")
				})
				.with_collator(|n|
					n.with_name("recovery-target")
						.validator(false)
						.with_args(vec![
						("-lparachain::availability=trace,sync=debug,parachain=debug,cumulus-pov-recovery=debug,cumulus-consensus=debug").into(),
						("--disable-block-announcements").into(),
						("--in-peers", "0").into(),
						("--out-peers", "0").into(),
						("--").into(),
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:alice:multiaddr}}").into()
					]))
				.with_collator(|n| n.with_name("collator-elastic")
					.with_args(vec![
						("-laura=trace,runtime=info,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug").into(),
						("--disable-block-announcements").into(),
						("--force-authoring").into(),
						("--authoring", "slot-based").into()
					])
			)
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

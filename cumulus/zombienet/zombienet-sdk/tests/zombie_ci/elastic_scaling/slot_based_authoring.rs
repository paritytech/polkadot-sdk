// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::time::Duration;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use cumulus_zombienet_sdk_helpers::network::{assign_cores, wait_relay_up};
#[cfg(not(feature = "jam"))]
use serde_json::json;
use zombienet_orchestrator::network::node::LogLineCountOptions;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::NetworkConfig;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[cfg(feature = "jam")]
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	genesis_build::{build_jam_genesis, polkavm_env},
	network::{
		base_dir, collator_args, copy_para_specs, path_str, work_dir, ORDINARY_NODE,
		VALIDATORS_PER_CORE,
	},
	para::Para,
};

const PARA_ID_1: u32 = 2100;
const PARA_ID_2: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_slot_based_authoring() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	#[cfg(not(feature = "jam"))]
	let config = build_network_config().await?;
	#[cfg(feature = "jam")]
	let (work_dir, _temp) = work_dir("elastic_scaling_slot_based_authoring")?;
	#[cfg(feature = "jam")]
	let config = {
		const CORES: u16 = 3;
		let binaries = binaries_or_err()?;
		let paras = vec![
			Para::new(PARA_ID_1, 0, &["collator-elastic"]).also_on([1]),
			Para::new(PARA_ID_2, 2, &["collator-single-core"]),
		];
		let genesis = build_jam_genesis(&binaries, &work_dir, &paras, CORES)?;
		let para_specs = copy_para_specs(&work_dir, &genesis.para_specs, &paras)?;
		let base_dir = base_dir(&work_dir)?;
		let jam_node = path_str(&binaries.jam_node)?;
		let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
		let omni_node = path_str(&binaries.omni_node)?;
		let authorizer_blob = path_str(&genesis.authorizer_blob)?;
		let overrides = genesis.overrides.clone();
		let no_overrides = std::collections::HashMap::new();
		let validators = CORES as usize * VALIDATORS_PER_CORE;

		zombienet_sdk::NetworkConfigBuilder::new()
			.with_jamchain(|jam| {
				let jam = jam.with_id("jam").with_default_command(jam_node.as_str());
				let jam = match genspec_node.as_deref() {
					Some(command) if command != jam_node.as_str() => {
						jam.with_chain_spec_command(command)
					},
					_ => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..validators).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env()))
			})
			.with_parachain(|p| {
				p.with_id(paras[0].id)
					.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
					.with_chain_spec_path(para_specs[0].clone())
					.with_default_command(omni_node.as_str())
					.with_collator(|node| {
						node.with_name(paras[0].collators[0].as_str())
							.with_env(polkavm_env())
							.with_args(collator_args(
								&authorizer_blob,
								&no_overrides,
								&paras[0].collators[0],
								true,
							))
					})
			})
			.with_parachain(|p| {
				p.with_id(paras[1].id)
					.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
					.with_chain_spec_path(para_specs[1].clone())
					.with_default_command(omni_node.as_str())
					.with_collator(|node| {
						node.with_name(paras[1].collators[0].as_str())
							.with_env(polkavm_env())
							.with_args(collator_args(
								&authorizer_blob,
								&no_overrides,
								&paras[1].collators[0],
								true,
							))
					})
			})
			.with_global_settings(|g| g.with_base_dir(base_dir))
			.build()
			.map_err(|e| {
				let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
				anyhow!("config errs: {errs}")
			})?
	};
	let network = initialize_network(config).await?;

	let collator_elastic = network.get_node("collator-elastic")?;
	let collator_single_core = network.get_node("collator-single-core")?;

	log::info!("Checking if alice is up");
	wait_relay_up(&network, "alice", 60).await?;

	log::info!("Checking if collator-elastic is up");
	assert!(collator_elastic.wait_until_is_up(60u64).await.is_ok());

	log::info!("Checking if collator-single-core is up");
	assert!(collator_single_core.wait_until_is_up(60u64).await.is_ok());

	assign_cores(&network, "alice", PARA_ID_1, vec![0, 1]).await?;

	for (node, block_cnt) in [(collator_single_core, 20.0), (collator_elastic, 40.0)] {
		log::info!("Checking block production for {}", node.name());
		node.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= block_cnt, 225u64)
			.await
			.unwrap_or_else(|e| {
				panic!("Failed to reach {block_cnt} blocks with node {}: {e}", node.name())
			});
	}

	// We want to make sure that none of the consensus hook checks fail, even if the chain makes
	// progress. If below log line occurred 1 or more times then test failed.
	for node in [collator_elastic, collator_single_core] {
		log::info!("Ensuring none of the consensus hook checks fail at {}", node.name());
		let result = node
			.wait_log_line_count_with_timeout(
				"set_validation_data inherent needs to be present in every block",
				false,
				LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(10)),
			)
			.await?;
		assert!(result.success(), "Consensus hook failed at {}: {:?}", node.name(), result);
	}

	log::info!("Test finished successfully");
	Ok(())
}

#[cfg(not(feature = "jam"))]
async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice
	// 	   - validator
	// 	 - validator[0-4]
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
				// Have to set a `with_validator` outside of the loop below, so that `r` has the right
				// type.
				.with_validator(|node| node.with_name("alice").with_args(vec![]));

			(0..5).fold(r, |acc, i| {
				acc.with_validator(|node| {
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

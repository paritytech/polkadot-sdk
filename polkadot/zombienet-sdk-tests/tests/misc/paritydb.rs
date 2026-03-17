// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Misc ParityDb
//!
//! This test verifies that parachains make progress with paritydb.

use crate::utils::{
	check_metrics, env_or_default, initialize_network, MetricCheckSetup,
	APPROVAL_CHECKING_FINALITY_LAG_METRIC, COL_IMAGE_ENV, INTEGRATION_IMAGE_ENV, NODE_ROLES_METRIC,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::ops::Range;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARAS: [u32; 5] = [2000, 2001, 2002, 2003, 2004];

#[tokio::test(flavor = "multi_thread")]
async fn paritydb_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let validator_nodes = network.relaychain().nodes();
	let relay_node = validator_nodes
		.first()
		.ok_or(anyhow!("Relaychain should have at least one node"))?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Check authority status
	log::info!("Checking validator node roles");
	for validator in &validator_nodes {
		validator
			.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 60u64)
			.await
			.map_err(|e| anyhow!("Validator {} role check failed: {e}", validator.name()))?;
	}
	log::info!("All validators confirmed as authorities");

	// Check that all parachains produce at least 5 blocks within 1 session and 5 blocks (RC)
	log::info!("Checking parachain block production (all paras registered at genesis)");
	let para_throughput: [(ParaId, Range<u32>); 5] = PARAS.map(|id| (ParaId::from(id), 2..6));
	assert_para_throughput(&relay_client, 5, para_throughput).await?;
	log::info!("All parachains producing blocks");

	log::info!("Check lag - approval / dispute conclusion.");
	let metric_checks: Vec<MetricCheckSetup> = vec![
		(APPROVAL_CHECKING_FINALITY_LAG_METRIC, Box::new(|v| v <= 1.0), 0),
		("polkadot_parachain_candidate_disputes_total", Box::new(|v| v == 0.0), 0),
	];

	check_metrics(&validator_nodes, &metric_checks).await?;

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let col_image = env_or_default(COL_IMAGE_ENV, images.cumulus.as_str());

	let mut builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		r.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(polkadot_image.as_str())
			.with_default_args(vec![
				"-lparachain=debug,runtime=debug".into(),
				"--db=paritydb".into(),
			])
			.with_genesis_overrides(json!({
				"patch": {
					"configuration": {
						"config": {
							"needed_approvals": 3,
							"scheduler_params": {
								"max_validators_per_core": 1
							}
						}
					}
				}
			}))
			.with_default_resources(|r| {
				r.with_limit_memory("4G")
					.with_limit_cpu("2")
					.with_request_memory("2G")
					.with_request_cpu("1")
			})
			.with_node_group(|g| g.with_count(5).with_base_node(|n| n.with_name("validator")))
	});

	builder = PARAS.into_iter().fold(builder, |acc, para_id| {
		acc.with_parachain(|p| {
			let pov_size = 10000*(para_id-1999);
			let pvf_complexity = para_id - 1999;

			p.with_id(para_id)
			.cumulus_based(false)
			.with_default_image(col_image.as_str())
			.with_default_command("undying-collator")
			.with_default_args(vec![
				"-lruntime=debug,parachain=trace".into(),
				format!("--pov-size={pov_size}").as_str().into(),
				"--pvf-complexity=1".into(),
			])
			.with_genesis_state_generator(
				format!("undying-collator export-genesis-state --pov-size={pov_size} --pvf-complexity={pvf_complexity}").as_str(),
			)
			.with_collator(|n| n.with_name(&format!("collator-{para_id}")))
		})
	});

	builder = builder.with_global_settings(|global_settings| {
		match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		}
	});

	builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test if parachains make progress with most of approvals being tranch0.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn parachains_max_tranche0_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let mut config_builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		let r = r
			.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(images.polkadot.as_str())
			.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
			.with_default_resources(|r| {
				r.with_limit_cpu("2")
					.with_limit_memory("4G")
					.with_request_cpu("1")
					.with_request_memory("2G")
			})
			.with_genesis_overrides(json!({
				"configuration": {
					"config": {
						"needed_approvals": 7,
						"relay_vrf_modulo_samples": 5,
						"scheduler_params": {
							"max_validators_per_core": 1
						},
					}
				}
			}))
			.with_node(|node| node.with_name("some-validator-0"));

		(1..8)
			.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("some-validator-{i}"))))
	});

	for para_id in 2000..2005 {
		let collator_name = format!("collator-undying-{para_id}");
		config_builder = config_builder.with_parachain(|p| {
			p.with_id(para_id)
				.with_default_command("undying-collator")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.cumulus_based(false)
				.with_collator(|n| {
					n.with_name(&collator_name).with_args(vec![
						("-lparachain=debug").into(),
						format!("--pov-size={}", 1000).as_str().into(),
						// format!("--pov-size={}", 10000 * (para_id - 1999)).as_str().into(),
						format!("--pvf-complexity={}", para_id - 1999).as_str().into(),
					])
				})
		});
	}

	let config = config_builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();

	log::info!("Spawning network");
	let network = spawn_fn(config).await?;

	log::info!("Waiting for network to initialize");
	let mut validators = Vec::new();
	for i in 0..8 {
		let validator_name = format!("some-validator-{}", i);
		let validator = network.get_node(&validator_name)?;
		validators.push(validator);
	}
	let relay_client: OnlineClient<PolkadotConfig> = validators[0].wait_client().await?;

	log::info!("Check authority status");
	for validator in &validators {
		validator.assert("node_roles", 4.0).await?;
	}

	log::info!("Wait for parachains to advance to block 15");
	assert_para_throughput(
		&relay_client,
		15,
		[
			(ParaId::from(2000), 11..16),
			(ParaId::from(2001), 11..16),
			(ParaId::from(2002), 11..16),
			(ParaId::from(2003), 11..16),
			(ParaId::from(2004), 11..16),
		]
		.into_iter()
		.collect(),
	)
	.await?;

	log::info!("Check finality does not lag");
	for validator in validators {
		assert_finality_lag(&validator.wait_client().await?, 3).await?;
	}

	log::info!("Test finished successfully");

	Ok(())
}

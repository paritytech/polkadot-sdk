// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! RPO-0 variants of the `v4_resubmit_*` throughput tests: same V4 stack (V3 descriptors,
//! experimental collator protocol, resubmission) but the `elastic-scaling-v3` runtime with
//! relay-parent-offset 0, isolating the offset axis from the stack axis in the v2-vs-v4
//! comparison. Ranges are wide — measurement, not a gate.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use rstest::rstest;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[rstest]
#[case::single(1, false)]
#[case::three(3, false)]
#[case::single_glutton(1, true)]
#[case::three_glutton(3, true)]
#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_rpo0(
	#[case] num_collators: usize,
	#[case] glutton: bool,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let collator_names: Vec<String> =
		(0..num_collators).map(|i| format!("collator-2900-{i}")).collect();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=debug,parachain::statement-distribution=debug,parachain::prospective-parachains=trace").into(),
					"--experimental-collator-protocol".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							"node_features": {"bits": 8, "data": [0b00011000]},
							"max_relay_parent_session_age": 10,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			let p = p
				.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug,aura::resubmission=trace").into(),
					"--authoring=slot-based".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
				]);
			let p = if glutton {
				p.with_genesis_overrides(json!({
					"patch": {
						"glutton": {
							"compute": "100000000",
							"storage": "500000000",
							"trashDataCount": 5120
						}
					}
				}))
			} else {
				p
			};
			let p = p.with_collator(|n| n.with_name(collator_names[0].as_str()));
			collator_names.iter().skip(1).fold(p, |acc, name| {
				acc.with_collator(|n| n.with_name(name.as_str()))
			})
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node(collator_names[0].as_str())?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assign_cores(&relay_client, 2900, vec![0, 1]).await?;
	log::info!("Para 2900 elastic-scaled to 3 cores (V3 descriptors, rpo 0)");

	assert_para_throughput(&relay_client, 100, [(ParaId::from(2900), 50..311)], []).await?;

	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 30).await?;

	log::info!("V4 resubmit rpo0 ({num_collators} collators, glutton={glutton}) finished");
	Ok(())
}

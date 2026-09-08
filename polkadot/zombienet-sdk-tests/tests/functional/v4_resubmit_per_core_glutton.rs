// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V4 + elastic scaling, **single collator**, with pallet-glutton on the parachain producing
//! non-empty blocks (consuming ~50% of weight budget each block via the `glutton` patch). Same
//! topology as `v4_resubmit_per_core_across_sessions`; only difference is the parachain genesis
//! override that initializes the glutton storage so every block has real work to validate.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_per_core_glutton() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

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
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3-rpo")
				// Make every parablock non-empty: pallet-glutton consumes ~10% of compute and
				// ~50% of proof-size each block via on_idle work. `trashDataCount=5120` seeds
				// the trash-data map so storage proofs have realistic size.
				.with_genesis_overrides(json!({
					"patch": {
						"glutton": {
							"compute": "100000000",
							"storage": "500000000",
							"trashDataCount": 5120
						}
					}
				}))
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug").into(),
					"--authoring=slot-based".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
				])
				.with_collator(|n| n.with_name("collator-2900"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator-2900")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assign_cores(&relay_client, 2900, vec![0, 1]).await?;
	log::info!("Para 2900 elastic-scaled to 3 cores, glutton at ~50% compute/storage");

	assert_para_throughput(&relay_client, 100, [(ParaId::from(2900), 210..310)], []).await?;

	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 15).await?;

	log::info!("V4 per-core glutton test finished successfully");
	Ok(())
}

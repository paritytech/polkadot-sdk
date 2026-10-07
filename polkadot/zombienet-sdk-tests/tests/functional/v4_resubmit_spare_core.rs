// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Spare-core catch-up across relay chain session changes.
//!
//! One slot-based collator authors for a velocity-3 parachain on four cores, with the unincluded
//! segment sized `(5 + RPO) * velocity` and `ReservedCores = 1`. The session-change relay block
//! backs nothing and the next author has not yet received the re-advertised candidates. Without
//! the headroom and the spare core the collator skips two slots per boundary, about 265 backed
//! per 100 counted relay blocks. With them it keeps producing and the spare core drains the
//! backlog: 315 or more backed.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_spare_core() -> Result<(), anyhow::Error> {
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
								"num_cores": 3,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							// V2 (bit 4) and V3 (bit 3) descriptors enabled.
							"node_features": {"bits": 8, "data": [0b00011000]},
							"max_relay_parent_session_age": 10,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..8).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3-headroom")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug,aura::resubmission=trace").into(),
					"--authoring=slot-based".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
					"--network-backend=libp2p".into(),
				])
				.with_collator(|n| n.with_name("collator-2900-0"))
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

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator-2900-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Three coretime cores plus the lease core: four assigned for a velocity of three.
	assign_cores(&relay_client, 2900, vec![0, 1, 2]).await?;
	log::info!("Para 2900 elastic-scaled to 4 cores, one spare");

	assert_para_throughput(&relay_client, 100, [(ParaId::from(2900), 315..334)], []).await?;

	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 30).await?;

	log::info!("V4 spare-core catch-up finished");
	Ok(())
}

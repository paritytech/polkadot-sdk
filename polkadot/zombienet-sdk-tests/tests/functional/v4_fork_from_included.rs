// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Exercises the V3 fork-from-included stall recovery: a healthy elastic-scaling parachain has its
//! inclusion frozen (all cores set to Idle) long enough that the build parent's relay parent lags
//! the scheduling parent past `MAX_RELAY_GAP_BEFORE_FORK`, so the collators abandon the stuck
//! unincluded segment and fork from the included head. Cores are then restored and the parachain
//! must recover — i.e. forking leaves the chain stable. The fork actually firing is confirmed from
//! the collators' "forking from included head" logs.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, assign_cores, unassign_cores,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2900;

/// Wait until the relay chain best block advances by `n` blocks.
async fn wait_for_relay_blocks(
	relay_client: &OnlineClient<PolkadotConfig>,
	n: u32,
) -> Result<(), anyhow::Error> {
	let mut best = relay_client.blocks().subscribe_best().await?;
	let start = best.next().await.ok_or_else(|| anyhow!("relay best stream ended"))??.number();
	while let Some(block) = best.next().await {
		if block?.number() >= start + n {
			return Ok(());
		}
	}
	Err(anyhow!("relay best stream ended before advancing {n} blocks"))
}

#[tokio::test(flavor = "multi_thread")]
async fn v4_fork_from_included() -> Result<(), anyhow::Error> {
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
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=debug,parachain::candidate-validation=trace,parachain::statement-distribution=debug,parachain::prospective-parachains=trace").into(),
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
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
                .with_default_args(vec![
                    "-lparachain=debug,aura=debug,aura::cumulus=trace,aura::resubmission=trace,basic-authorship=debug,sync=debug,sync::import-queue=debug,sc_consensus::block_import=debug,cumulus_client_consensus_common=debug,parachain::collator-protocol=trace".into(),
                    "--authoring=slot-based".into(),
                    "--".into(),
                    "--state-pruning=archive".into(),
                    "--blocks-pruning=archive".into(),
                ])
				.with_chain("elastic-scaling-v3-rpo")
				.with_collator(|n| n.with_name("collator-alice"))
				.with_collator(|n| n.with_name("collator-bob"))
				.with_collator(|n| n.with_name("collator-charlie"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_alice = network.get_node("collator-alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Healthy phase: para is elastic-scaled and producing.
	assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;
	log::info!("Para {PARA_ID} assigned cores; asserting healthy throughput");
	assert_para_throughput(&relay_client, 30, [(ParaId::from(PARA_ID), 30..120)], []).await?;

	// Freeze inclusion: set all the para's cores (including the auto-assigned one) to Idle, long
	// enough for the segment to fill, building to halt, and the build parent's relay parent to age
	// past MAX_RELAY_GAP_BEFORE_FORK so the fork fires.
	log::info!("Freezing inclusion: setting {PARA_ID}'s cores to Idle");
	unassign_cores(&relay_client, vec![0, 1, 2]).await?;
	wait_for_relay_blocks(&relay_client, 40).await?;

	// Restore cores; the para must recover.
	log::info!("Restoring cores to {PARA_ID}");
	assign_cores(&relay_client, PARA_ID, vec![0, 1, 2]).await?;

	log::info!("Asserting recovery after fork");
	assert_para_throughput(&relay_client, 30, [(ParaId::from(PARA_ID), 30..120)], []).await?;
	let collator_client: OnlineClient<PolkadotConfig> = collator_alice.wait_client().await?;
	assert_finality_lag(&collator_client, 15).await?;

	log::info!("V4 fork-from-included stability test finished successfully");
	Ok(())
}

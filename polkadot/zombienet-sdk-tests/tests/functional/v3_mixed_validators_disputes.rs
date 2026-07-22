// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! A V3 candidate is backed by v3-capable validators; a single pre-v3 validator that then takes
//! part in approval voting raises a losing dispute and is disabled.

use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::assert_finality_lag;
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId, ValidatorIndex};
use serde_json::json;
use std::collections::HashMap;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

use crate::utils::assert_candidates_version;

#[tokio::test(flavor = "multi_thread")]
async fn v3_mixed_validators_disputes() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let native = std::env::var("ZOMBIE_PROVIDER").as_deref() == Ok("native");
	let old_image = std::env::var("OLD_POLKADOT_IMAGE").ok();
	let old_command = std::env::var("OLD_POLKADOT_COMMAND").unwrap_or("polkadot".into());

	// V2 (bit 3) and V3 (bit 4) enabled.
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			// westend-local so staking-based disabling takes effect.
			let r = r
				.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime::staking=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// Small groups so validators remain outside the backing group to do
								// real approval checking (approval-voting insta-approves when
								// `n_validators - backing_group_size < needed_approvals`).
								"group_rotation_frequency": 100,
								"max_validators_per_core": 3
							},
							"needed_approvals": 2,
							"node_features": node_features_with_v3
						}
					}
				}))
				// validator-0..5: v3-capable backers.
				.with_validator(|node| {
					node.with_name("validator-0")
						.with_args(vec!["-lparachain=debug,runtime::staking=debug".into()])
						.invulnerable(false)
				});

			let r = (1..6usize).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}"))
						.with_args(vec!["-lparachain=debug,runtime::staking=debug".into()])
						.invulnerable(false)
				})
			});

			// old-validator-6: pre-v3 node; never a backer, but takes part in approval voting.
			(6..7usize).fold(r, |acc, i| {
				acc.with_validator(|node| {
					let node = node
						.with_name(&format!("old-validator-{i}"))
						.with_command(old_command.as_str());
					// Native ignores (and rejects) an image path; only set it for container
					// providers when OLD_POLKADOT_IMAGE is given.
					let node = match (native, old_image.as_deref()) {
						(false, Some(img)) => node.with_image(img),
						_ => node,
					};
					node.invulnerable(false)
				})
			})
		})
		// Para 3000: V3-capable collator ("v3" chain spec + slot-based authoring).
		.with_parachain(|p| {
			p.with_id(3000)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("v3")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-3000"))
		})
		// Keep the network alive on failure for post-mortem logs.
		.with_global_settings(|s| s.with_tear_down_on_failure(false))
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	log::info!("Spawning network");
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-3000")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	log::info!("Waiting for V3 candidates to be backed by the v3 backing group");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(ParaId::from(3000), 5..21)]),
		20,
		None,
	)
	.await?;

	log::info!("Waiting for a dispute to be raised and concluded");
	let parachain_candidate_dispute_metric = "parachain_candidate_disputes_total";
	let concluded_dispute_metric =
		"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}";

	relay_node
		.wait_metric_with_timeout(parachain_candidate_dispute_metric, |d| d >= 1.0, 600_u64)
		.await?;
	relay_node
		.wait_metric_with_timeout(concluded_dispute_metric, |d| d >= 1.0, 200_u64)
		.await?;

	log::info!("Waiting for the pre-v3 disputer to be disabled");
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut disabled_validators: Vec<ValidatorIndex> = Vec::new();
	let mut blocks_checked = 0u32;
	while let Some(block) = best_blocks.next().await {
		let hash = block?.hash();
		disabled_validators = Vec::<ValidatorIndex>::decode(
			&mut &relay_client
				.runtime_api()
				.at(hash)
				.call_raw("ParachainHost_disabled_validators", None)
				.await?[..],
		)?;
		if !disabled_validators.is_empty() {
			break;
		}
		blocks_checked += 1;

		if blocks_checked >= 50 {
			break;
		}
	}

	assert!(
		!disabled_validators.is_empty(),
		"expected the pre-v3 disputer to be disabled after the concluded dispute (checked {blocks_checked} blocks)",
	);
	log::info!("Disabled validators: {:?}", disabled_validators);

	log::info!("Verifying the parachain keeps producing blocks after the dispute");
	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;
	let start_height = para_client.blocks().at_latest().await?.header().number;
	let target_height = (start_height + 8) as f64;
	para_node
		.wait_metric_with_timeout(
			"substrate_block_height{status=\"best\"}",
			|h| h >= target_height,
			120u64,
		)
		.await?;

	log::info!("Waiting for relay-chain finality to recover after the dispute");
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_approval_checking_finality_lag",
			|lag| lag <= 5.0,
			180u64,
		)
		.await?;

	assert_finality_lag(&para_client, 8).await?;

	log::info!("Mixed validators disputes test finished successfully");
	Ok(())
}

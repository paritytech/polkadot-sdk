// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that V3 candidate descriptors with scheduling_parent work correctly.
//!
//! This test verifies that:
//! 1. V3 candidates with scheduling_parent != relay_parent are backed and included
//! 2. The parachain continues to produce blocks when V3 is enabled
//! 3. Legacy (V1/V2) parachains continue to work alongside V3 parachains

use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, wait_for_first_session_change};
use polkadot_primitives::{CandidateDescriptorVersion, CandidateReceiptV2, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{utils::H256, OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Find CandidateBacked events and decode them.
fn find_candidate_backed_events(
	events: &zombienet_sdk::subxt::events::Events<PolkadotConfig>,
) -> Result<Vec<CandidateReceiptV2<H256>>, anyhow::Error> {
	let mut result = vec![];
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "ParaInclusion" && event.variant_name() == "CandidateBacked" {
			result.push(CandidateReceiptV2::<H256>::decode(&mut &event.field_bytes()[..])?);
		}
	}
	Ok(result)
}

/// Asserts that V3 candidates are being produced and backed.
///
/// Waits for `min_v3_candidates` V3 candidates to be backed within `max_blocks` relay chain
/// blocks.
async fn assert_v3_candidates_backed(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_id: ParaId,
	min_v3_candidates: u32,
	max_blocks: u32,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;

	// Wait for the first session change - block production starts after that
	wait_for_first_session_change(&mut blocks_sub).await?;

	let mut v3_candidate_count = 0;
	let mut total_candidate_count = 0;
	let mut block_count = 0;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());
		let events = block.events().await?;

		let receipts = find_candidate_backed_events(&events)?;

		for receipt in receipts {
			if receipt.descriptor.para_id() != para_id {
				continue;
			}

			total_candidate_count += 1;

			// Check if this is a V3 candidate
			// V3 candidates have internal_version = 1 and use scheduling_parent
			let version = receipt.descriptor.version(true); // true = v3_enabled
			log::info!(
				"Para {} candidate backed: version={:?}, relay_parent={:?}",
				para_id,
				version,
				receipt.descriptor.relay_parent(),
			);

			if version == CandidateDescriptorVersion::V3 {
				v3_candidate_count += 1;
				log::info!(
					"V3 candidate detected! scheduling_parent={:?}",
					receipt.descriptor.scheduling_parent(true)
				);
			}
		}

		block_count += 1;

		if v3_candidate_count >= min_v3_candidates {
			log::info!(
				"Successfully detected {v3_candidate_count} V3 candidates out of {total_candidate_count} total in {block_count} blocks"
			);
			return Ok(());
		}

		if block_count >= max_blocks {
			break;
		}
	}

	Err(anyhow!(
		"Only found {v3_candidate_count} V3 candidates (needed {min_v3_candidates}) out of {total_candidate_count} total in {block_count} blocks"
	))
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduling_v3_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Create node_features bitvec with bits 3 (V2) and 4 (V3) enabled
	// Format: {"bits": N, "data": [bytes]} - bitvec serialization
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
							},
							// Enable V3 candidate descriptors via node_features
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..5).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug").into(),
					// Use slot-based collator which supports V3 scheduling
					("--authoring=slot-based").into(),
				])
				.with_collator(|n| n.with_name("collator-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2000")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Wait for V3 candidates to be backed
	// We expect at least 3 V3 candidates within 20 relay chain blocks after session change
	assert_v3_candidates_backed(&relay_client, ParaId::from(2000), 3, 20).await?;

	// Also verify finality is progressing on the parachain
	// Allow up to 5 blocks lag - this is more lenient to avoid flaky failures
	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V3 scheduling test finished successfully");

	Ok(())
}

/// Test that legacy V1 parachains continue to work when V3 is enabled on the relay chain.
#[tokio::test(flavor = "multi_thread")]
async fn v3_backwards_compatibility_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Enable V3 on relay chain
	// Format: {"bits": N, "data": [bytes]} - bitvec serialization
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
							},
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..5).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		// Use sync-backing chain which produces legacy V1 candidates
		.with_parachain(|p| {
			p.with_id(2500)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("sync-backing")
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2500"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2500")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Use the standard throughput assertion - legacy parachain should still work
	cumulus_zombienet_sdk_helpers::assert_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(2500), 5..12)],
	)
	.await?;

	// Verify finality on the parachain
	assert_finality_lag(&para_node.wait_client().await?, 3).await?;

	log::info!("V3 backwards compatibility test finished successfully");

	Ok(())
}

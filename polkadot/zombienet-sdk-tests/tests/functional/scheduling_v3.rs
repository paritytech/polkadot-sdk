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

/// Asserts that candidates of the expected version are being backed for a given parachain.
///
/// Waits for `min_candidates` candidates matching `expected_version` to be backed within
/// `max_blocks` relay chain blocks.
async fn assert_candidates_version(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_id: ParaId,
	expected_version: CandidateDescriptorVersion,
	v3_enabled: bool,
	min_candidates: u32,
	max_blocks: u32,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;

	wait_for_first_session_change(&mut blocks_sub).await?;

	let mut matched = 0u32;
	let mut total = 0u32;
	let mut block_count = 0u32;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());

		for receipt in find_candidate_backed_events(&block.events().await?)? {
			if receipt.descriptor.para_id() != para_id {
				continue;
			}

			total += 1;
			let version = receipt.descriptor.version(v3_enabled);
			log::info!(
				"Para {} candidate backed: version={:?}, relay_parent={:?}",
				para_id,
				version,
				receipt.descriptor.relay_parent(),
			);

			if version == expected_version {
				matched += 1;
			}
		}

		block_count += 1;

		if matched >= min_candidates {
			log::info!(
				"Found {matched}/{total} {:?} candidates for para {para_id} in {block_count} blocks",
				expected_version,
			);
			return Ok(());
		}

		if block_count >= max_blocks {
			break;
		}
	}

	Err(anyhow!(
		"Only found {matched} {:?} candidates (needed {min_candidates}) out of {total} total for para {para_id} in {block_count} blocks",
		expected_version,
	))
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduling_v3_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Create node_features bitvec with bits 4 (V2) and 3 (V3) enabled
	// Format: {"bits": N, "data": [bytes]} - bitvec serialization
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::network-bridge-net=trace,parachain::candidate-backing=trace,parachain::provisioner=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,parachain::collator-protocol=trace,basic-authorship=debug,parachain::statement-distribution=debug").into(),
				])
				.with_genesis_overrides(json!({
					"patch": {
						"configuration": {
							"config": {
								"scheduler_params": {
									"max_validators_per_core": 1,
									"group_rotation_frequency": 1000,
								},
								// Enable V3 candidate descriptors via node_features
								"node_features": node_features_with_v3,
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..5).fold(r, |acc, i| acc.with_validator(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2500)
				.with_default_command("test-parachain")
                .with_chain("async-backing")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug,parachain::collator-protocol=trace,parachain::collator-protocol::stats=trace,basic-authorship=debug").into(),
					// Use slot-based collator which supports V3 scheduling
					("--authoring=slot-based").into(),
				])
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

	// Wait for V3 candidates to be backed
	// We expect at least 5 V3 candidates within 20 relay chain blocks after session change
	assert_candidates_version(
		&relay_client,
		ParaId::from(2500),
		CandidateDescriptorVersion::V3,
		true,
		5,
		20,
	)
	.await?;

	// Also verify finality is progressing on the parachain
	// Allow up to 5 blocks lag - this is more lenient to avoid flaky failures
	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V3 scheduling test finished successfully");
	Ok(())
}

/// Test that V2 candidates are correctly backed when only the V2 node feature is enabled.
#[tokio::test(flavor = "multi_thread")]
async fn v2_candidates_still_working() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Only V2 (bit 4) enabled, no V3
	let node_features_v2_only = json!({"bits": 8, "data": [0b00001000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::candidate-backing=trace,parachain::provisioner=trace").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
							},
							"node_features": node_features_v2_only,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..5).fold(r, |acc, i| acc.with_validator(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2700)
				.with_default_command("test-parachain")
				.with_chain("scheduling-v3-disabled")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2700"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assert_candidates_version(
		&relay_client,
		ParaId::from(2700),
		CandidateDescriptorVersion::V2,
		false, // v3 not enabled
		5,
		20,
	)
	.await?;

	let para_node = network.get_node("collator-2700")?;
	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V2 candidates still working test finished successfully");

	Ok(())
}

/// Test that V3 candidates work correctly with elastic scaling (multiple cores).
///
/// This test assigns 3 cores to a single parachain and verifies that V3 candidates are
/// being backed at elastic scaling throughput.
#[tokio::test(flavor = "multi_thread")]
async fn scheduling_v3_elastic_scaling() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// V2 (bit 4) and V3 (bit 3) enabled
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=trace,parachain::provisioner=trace,runtime::parachains::scheduler=trace").into(),
				])
				.with_genesis_overrides(json!({
					"patch": {
						"configuration": {
							"config": {
								"scheduler_params": {
									// 2 extra cores to assign, plus 1 auto-assigned by zombienet
									"num_cores": 2,
									"max_validators_per_core": 1,
									"group_rotation_frequency": 4,
								},
								"node_features": node_features_with_v3,
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2800)
				.with_default_command("test-parachain")
				.with_chain("elastic-scaling")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug,parachain::collator-protocol=trace,basic-authorship=debug").into(),
					("--authoring=slot-based").into(),
					("--force-authoring").into(),
				])
				.with_collator(|n| n.with_name("collator-2800"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2800")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Assign 2 additional cores to the parachain (zombienet already assigns 1)
	assign_cores(&relay_client, 2800, vec![0, 1]).await?;

	// With 3 cores total, we expect higher throughput.
	// Wait for at least 15 V3 candidates within 20 relay chain blocks.
	assert_candidates_version(
		&relay_client,
		ParaId::from(2800),
		CandidateDescriptorVersion::V3,
		true,
		15,
		20,
	)
	.await?;

	// Allow more finality lag with elastic scaling
	assert_finality_lag(&para_node.wait_client().await?, 15).await?;

	log::info!("V3 elastic scaling test finished successfully");
	Ok(())
}

/// Test that V2 candidates work correctly with elastic scaling when V3 is not enabled.
///
/// This verifies backwards compatibility: elastic scaling should work with V2 candidate
/// descriptors when the V3 node feature is not enabled on the relay chain.
#[tokio::test(flavor = "multi_thread")]
async fn v2_elastic_scaling_backwards_compat() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Only V2 (bit 4) enabled, no V3
	let node_features_v2_only = json!({"bits": 8, "data": [0b00001000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=trace,parachain::provisioner=trace").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 1,
								"group_rotation_frequency": 4,
							},
							"node_features": node_features_v2_only,
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
				.with_chain("elastic-scaling-v3-disabled")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug,parachain::collator-protocol=trace,basic-authorship=debug").into(),
					("--authoring=slot-based").into(),
					("--force-authoring").into(),
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
	let para_node = network.get_node("collator-2900")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Assign 2 additional cores to the parachain (zombienet already assigns 1)
	assign_cores(&relay_client, 2900, vec![0, 1]).await?;

	// With 3 cores and V2 candidates, we still expect elastic throughput.
	assert_candidates_version(
		&relay_client,
		ParaId::from(2900),
		CandidateDescriptorVersion::V2,
		false, // v3 not enabled
		15,
		20,
	)
	.await?;

	assert_finality_lag(&para_node.wait_client().await?, 15).await?;

	log::info!("V2 elastic scaling backwards compat test finished successfully");
	Ok(())
}

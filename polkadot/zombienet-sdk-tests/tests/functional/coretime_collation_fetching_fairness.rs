// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! This test verifies that coretime core assignments respect the configured fairness ratios
//! when multiple parachains share a core. It sets up 2 parachains sharing core 0 with a 3:1
//! ratio (para 2000 gets 43200 parts, para 2001 gets 14400 parts) and verifies the block
//! production follows this ratio.
//!
//! Key difference between collators:
//! - Para 2000: Uses slot-based collator (`--authoring slot-based`) which respects claim queue
//! - Para 2001: Uses default lookahead collator which generates blocks for each relay parent

use crate::utils::{
	env_or_default, initialize_network, register_paras, BLOCK_HEIGHT_METRIC, CUMULUS_IMAGE_ENV,
	INTEGRATION_IMAGE_ENV, NODE_ROLES_METRIC,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use serde_json::json;
use zombienet_sdk::{
	subxt::{
		ext::scale_value::{value, Value},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder, RegistrationStrategy,
};

const PARA_2000: u32 = 2000;
const PARA_2001: u32 = 2001;

// Core assignment parts (3:1 ratio).
const PARA_2000_PARTS: u32 = 43200;
const PARA_2001_PARTS: u32 = 14400;

/// Creates a sudo call to assign a core to parachains with specified parts.
fn create_assign_core_call(core: u32, assignments: Vec<(u32, u32)>) -> Value {
	let assignment_values: Vec<Value> = assignments
		.into_iter()
		.map(|(para_id, parts)| value! { (Task(para_id), parts) })
		.collect();

	value! {
		Coretime(assign_core { core: core, begin: 0u32, assignment: assignment_values, end_hint: None() })
	}
}

/// Test that coretime core assignments respect configured fairness ratios.
///
/// This test:
/// - Spawns 4 validators with max_validators_per_core=4, num_cores=1
/// - Spawns 2 glutton parachains:
///   - Para 2000: slot-based collator (respects claim queue)
///   - Para 2001: lookahead collator (generates blocks for each relay parent)
/// - Registers the parachains via sudo
/// - Assigns core 0 with 3:1 ratio (43200:14400 parts)
/// - Verifies fairness by checking CandidateIncluded events
#[tokio::test(flavor = "multi_thread")]
async fn coretime_collation_fetching_fairness_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();

	// Verify node role is 4 (authority).
	log::info!("Checking validator node role");
	relay_node
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Validator node role check failed: {}", e))?;

	// Register both parachains.
	log::info!("Registering parachains {} and {}", PARA_2000, PARA_2001);
	register_paras(&network, &relay_client, &alice, &[PARA_2000, PARA_2001]).await?;

	// Assign core 0 with 3:1 ratio between para 2000 and 2001.
	log::info!(
		"Assigning core 0 with ratio {}:{} (parts {} and {})",
		3,
		1,
		PARA_2000_PARTS,
		PARA_2001_PARTS
	);
	let assign_core_call = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![create_assign_core_call(
			0,
			vec![(PARA_2000, PARA_2000_PARTS), (PARA_2001, PARA_2001_PARTS)],
		)],
	);
	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&relay_client,
		&assign_core_call,
		&alice,
		600u64,
	)
	.await
	.map_err(|e| anyhow!("Failed to assign core: {}", e))?;
	log::info!("Core 0 assigned with 3:1 ratio");

	log::info!("Verifying collator block heights");
	let collator_2000 = network.get_node("collator-2000")?;
	let collator_2001 = network.get_node("collator-2001")?;
	collator_2000
		.wait_metric_with_timeout(BLOCK_HEIGHT_METRIC, |v| v >= 9.0, 200u64)
		.await?;
	collator_2001
		.wait_metric_with_timeout(BLOCK_HEIGHT_METRIC, |v| v >= 3.0, 10u64)
		.await?;

	// Verify fairness by checking CandidateIncluded events.
	// We expect ~3:1 ratio of blocks for para 2000 vs 2001.
	// Para 2000 (slot-based) should get more blocks since it respects claim queue.
	// Para 2001 (lookahead) gets fewer blocks.
	log::info!("Verifying fairness via CandidateIncluded events");
	verify_fairness(&relay_client).await?;

	log::info!("Test finished successfully");
	Ok(())
}

/// Verifies the fairness of block production by monitoring CandidateIncluded events.
///
/// Waits for a new session to start, then monitors 12 blocks and verifies:
/// - Para 2000 gets >= 6 CandidateIncluded events (slot-based collator)
/// - Para 2001 gets <= 4 CandidateIncluded events (lookahead collator)
async fn verify_fairness(client: &OnlineClient<PolkadotConfig>) -> Result<(), anyhow::Error> {
	use std::collections::HashMap;

	let mut blocks_per_para: HashMap<u32, u32> = HashMap::new();
	let mut block_count = 0u32;
	let mut new_session_started = false;

	log::info!("Waiting for new session to start measuring CandidateIncluded events");

	let mut blocks_sub = client.blocks().subscribe_finalized().await?;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		let events = block.events().await?;

		// Check for NewSession event.
		for event in events.iter() {
			let event = event?;
			if event.pallet_name() == "Session" &&
				event.variant_name() == "NewSession" &&
				!new_session_started
			{
				log::info!("New session started. Measuring CandidateIncluded events.");
				new_session_started = true;
			}
		}

		if !new_session_started {
			continue;
		}

		block_count += 1;

		// Count CandidateIncluded events.
		for event in events.iter() {
			let event = event?;
			if event.pallet_name() == "ParaInclusion" && event.variant_name() == "CandidateIncluded"
			{
				// Decode the event to get the para_id.
				let details = event.field_bytes();
				if details.len() >= 4 {
					let para_id =
						u32::from_le_bytes([details[0], details[1], details[2], details[3]]);
					if para_id == PARA_2000 || para_id == PARA_2001 {
						*blocks_per_para.entry(para_id).or_insert(0) += 1;
						log::info!(
							"CandidateIncluded for {}: block_offset={} block_hash={:?}",
							para_id,
							block_count,
							block.hash()
						);
					}
				}
			}
		}

		if block_count >= 12 {
			break;
		}
	}

	let para_2000_count = *blocks_per_para.get(&PARA_2000).unwrap_or(&0);
	let para_2001_count = *blocks_per_para.get(&PARA_2001).unwrap_or(&0);

	log::info!("Result: {}: {}, {}: {}", PARA_2000, para_2000_count, PARA_2001, para_2001_count);

	// Verify fairness: para 2000 should get >= 6, para 2001 should get <= 4.
	// This assumes para 2000 runs slot-based collator which respects its claim queue
	// and para 2001 runs lookahead which generates blocks for each relay parent.
	if para_2000_count < 6 {
		return Err(anyhow!(
			"Fairness check failed: para {} got {} blocks, expected >= 6",
			PARA_2000,
			para_2000_count
		));
	}
	if para_2001_count > 4 {
		return Err(anyhow!(
			"Fairness check failed: para {} got {} blocks, expected <= 4",
			PARA_2001,
			para_2001_count
		));
	}

	log::info!("Fairness verification passed");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let cumulus_image = env_or_default(CUMULUS_IMAGE_ENV, images.cumulus.as_str());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_default_args(vec![
					"-lparachain=debug,parachain::collator-protocol=trace".into()
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 4,
								"num_cores": 1
							},
							"approval_voting_params": {
								"needed_approvals": 3
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"))
				.with_node(|node| node.with_name("validator-1"))
				.with_node(|node| node.with_name("validator-2"))
				.with_node(|node| node.with_name("validator-3"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_2000)
				.with_registration_strategy(RegistrationStrategy::Manual)
				.with_chain("glutton-westend-local-2000")
				.with_default_command("polkadot-parachain")
				.with_default_image(cumulus_image.as_str())
				.with_default_args(vec![
					"-lparachain=debug,parachain::collator-protocol=trace".into(),
					"--authoring".into(),
					"slot-based".into(),
				])
				.with_genesis_overrides(json!({
					"glutton": {
						"compute": "50000000",
						"storage": "2500000000",
						"trashDataCount": 5120
					}
				}))
				.with_collator(|n| n.with_name("collator-2000"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_2001)
				.with_registration_strategy(RegistrationStrategy::Manual)
				.with_chain("glutton-westend-local-2001")
				.with_default_command("polkadot-parachain")
				.with_default_image(cumulus_image.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_genesis_overrides(json!({
					"glutton": {
						"compute": "50000000",
						"storage": "2500000000",
						"trashDataCount": 5120
					}
				}))
				.with_collator(|n| n.with_name("collator-2001"))
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

	Ok(config)
}

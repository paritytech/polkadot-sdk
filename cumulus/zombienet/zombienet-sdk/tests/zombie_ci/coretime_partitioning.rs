// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that verifies correct claim queue behavior across coretime assignment boundaries.
//!
//! This test ensures that when a core's assignment changes from one set of parachains to another,
//! the scheduler's claim queue correctly reflects the transition at the boundary block.
//!
//! Before the peek-assigner changes, the claim queue would be populated with the old assignment
//! even after crossing the boundary. With peek functionality, the scheduler can look ahead and
//! properly handle assignment transitions.
//!
//! The claim queue has a lookahead depth L. At block N, it contains assignments for [N+1, ..., N+L].
//! With lookahead=5 and boundary at block 21 (A assigned for 0-20, B assigned for 21+):
//! - Block 15: claim queue = [16, 17, 18, 19, 20] = [A, A, A, A, A]
//! - Block 16: claim queue = [17, 18, 19, 20, 21] = [A, A, A, A, B] ← First B appears!
//! - Block 17: claim queue = [18, 19, 20, 21, 22] = [A, A, A, B, B]
//! - Block 18: claim queue = [19, 20, 21, 22, 23] = [A, A, B, B, B]
//! - Block 19: claim queue = [20, 21, 22, 23, 24] = [A, B, B, B, B]
//! - Block 20: claim queue = [21, 22, 23, 24, 25] = [B, B, B, B, B]
//! - Block 21+: claim queue = [22+, ...] = [B, B, B, B, B]

use anyhow::anyhow;
use codec::Decode;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use subxt::{ext::scale_value::value, OnlineClient, PolkadotConfig};

use crate::utils::initialize_network;
use zombienet_sdk::{subxt, subxt_signer::sr25519::dev, NetworkConfig, NetworkConfigBuilder};

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;
const BOUNDARY_BLOCK: u32 = 21;
const LOOKAHEAD: u32 = 5;

#[tokio::test(flavor = "multi_thread")]
async fn coretime_assignment_boundary_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Building network configuration");
	let config = build_network_config().await?;

	log::info!("Spawning network");
	let mut network = initialize_network(config).await?;

	let relay_alice = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;
	let alice = dev::alice();

	log::info!("Registering both parachains");
	network.register_parachain(PARA_A).await?;
	network.register_parachain(PARA_B).await?;

	log::info!("Both parachains registered (not yet onboarded - will be activated via core assignment)");

	// Assign core 0 to para A from block 0 to 20
	log::info!("Assigning core 0 to para {} for blocks 0-20", PARA_A);
	let assign_core_a = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Coretime(assign_core { core: 0, begin: 0, assignment: ((Task(PARA_A), 57600)), end_hint: None() })
		}],
	);
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_core_a, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Assign core 0 to para B from block 21 onwards
	log::info!("Assigning core 0 to para {} for blocks 21+", PARA_B);
	let assign_core_b = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Coretime(assign_core { core: 0, begin: 21, assignment: ((Task(PARA_B), 57600)), end_hint: None() })
		}],
	);
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_core_b, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("Core assignments configured: A for blocks 0-20, B for blocks 21+");
	log::info!("Boundary is at block {}, lookahead is {}", BOUNDARY_BLOCK, LOOKAHEAD);

	// Expected claim queue transitions:
	// At block N, claim queue = [N+1, N+2, N+3, N+4, N+5] (for lookahead=5)
	let expected_transitions: HashMap<u32, Vec<ParaId>> = [
		(15, vec![ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A)]), // [16,17,18,19,20]
		(16, vec![ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_B)]), // [17,18,19,20,21] ← B appears!
		(17, vec![ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [18,19,20,21,22]
		(18, vec![ParaId::from(PARA_A), ParaId::from(PARA_A), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [19,20,21,22,23]
		(19, vec![ParaId::from(PARA_A), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [20,21,22,23,24]
		(20, vec![ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [21,22,23,24,25]
		(21, vec![ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [22,23,24,25,26]
		(25, vec![ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B), ParaId::from(PARA_B)]), // [26,27,28,29,30]
	].into_iter().collect();

	log::info!("Monitoring claim queue transitions around boundary...");

	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut verified_blocks = HashSet::new();

	while let Some(block_result) = blocks_sub.next().await {
		let block = block_result?;
		let block_number = block.number();

		// Query the claim queue for this block
		let claim_queue = BTreeMap::<CoreIndex, VecDeque<ParaId>>::decode(
			&mut &relay_client
				.runtime_api()
				.at(block.hash())
				.call_raw("ParachainHost_claim_queue", None)
				.await?[..],
		)?;

		if let Some(queue) = claim_queue.get(&CoreIndex(0)) {
			let queue_vec: Vec<ParaId> = queue.iter().copied().collect();

			log::info!(
				"Block #{}: Claim queue = {:?} (predicting blocks {}-{})",
				block_number,
				queue_vec,
				block_number + 1,
				block_number + LOOKAHEAD
			);

			// Check if this block is one we want to verify
			if let Some(expected_queue) = expected_transitions.get(&block_number) {
				if &queue_vec == expected_queue {
					log::info!("  ✓ Block {}: Claim queue matches expected {:?}", block_number, expected_queue);
					verified_blocks.insert(block_number);
				} else {
					return Err(anyhow!(
						"FAIL: At block {}, expected claim queue {:?} but got {:?}.\n\
						This indicates the peek-assigner is not working correctly!",
						block_number,
						expected_queue,
						queue_vec
					));
				}
			}
		} else {
			log::debug!("Block #{}: No claim queue entry for core 0", block_number);
		}

		// Check if we've verified all required blocks
		let required_blocks: HashSet<_> = expected_transitions.keys().copied().collect();

		if required_blocks.is_subset(&verified_blocks) {
			log::info!("✓ All claim queue transitions verified successfully!");
			break;
		}

		// Safety: don't wait forever
		if block_number > BOUNDARY_BLOCK + 10 {
			let missing: Vec<_> = required_blocks.difference(&verified_blocks).collect();
			return Err(anyhow!(
				"Failed to verify all expected claim queue transitions. Missing blocks: {:?}\n\
				Verified: {:?}\n\
				This suggests the test didn't observe all required blocks.",
				missing, verified_blocks
			));
		}
	}

	log::info!("Test completed successfully!");
	log::info!("The peek-assigner correctly populates the claim queue by looking ahead at upcoming assignments.");

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let chain_a = format!("glutton-westend-local-{}", PARA_A);
	let chain_b = format!("glutton-westend-local-{}", PARA_B);

	// Network setup:
	// - Relay chain with 4 validators
	// - Two parachains that will be assigned to the same core at different times (partitioning)
	// - Lookahead set to 5 to match production scenario
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lruntime=debug").into(),
					("-lparachain=debug").into(),
					("-lruntime::parachains::scheduler=trace").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 1,
								"num_cores": 1,
								"lookahead": 5
							}
						}
					}
				}))
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
				.with_node(|node| node.with_name("charlie"))
				.with_node(|node| node.with_name("dave"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_chain(chain_a.as_str())
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
				.onboard_as_parachain(false)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-a").validator(true))
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_chain(chain_b.as_str())
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
				.onboard_as_parachain(false)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-b").validator(true))
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

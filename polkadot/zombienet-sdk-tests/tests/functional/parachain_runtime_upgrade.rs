// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// The test sets up a network with 4 validators and 1 collator, waits for the parachain
// to start producing blocks (5 blocks), then performs a runtime upgrade of the parachain
// to a runtime with an increased version and slot duration of 18 seconds, waits for the upgrade
// to complete and verifies that the relay chain is working and finalizing, and the parachain
// is producing blocks (waits for 10 blocks).

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, assert_para_throughput, runtime_upgrade, wait_for_upgrade,
};
use polkadot_primitives::Id as ParaId;
use std::time::Duration;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const WASM_WITH_SLOT_DURATION_18S: &str =
	"/tmp/wasm_binary_slot_duration_18s.rs.compact.compressed.wasm";

#[tokio::test(flavor = "multi_thread")]
async fn parachain_runtime_upgrade_test() -> Result<(), anyhow::Error> {
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
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("validator-0"));

			// Add 4 validators
			(1..4).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator").validator(true))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	// Wait for the parachain to start producing blocks and produce 5 blocks
	log::info!("Waiting for parachain to produce 5 blocks...");
	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 5..20)].into_iter().collect(),
	)
	.await?;

	// Get the current runtime version
	let current_spec_version =
		collator_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version: {current_spec_version}");

	// Use WASM with slot duration 18s
	log::info!("Using runtime WASM: {}", WASM_WITH_SLOT_DURATION_18S);

	// Check if the file exists
	if !std::path::Path::new(WASM_WITH_SLOT_DURATION_18S).exists() {
		return Err(anyhow!(
			"Runtime WASM file not found at: {}. Please ensure the test-parachain artifacts are built with the slot-duration-18s feature.",
			WASM_WITH_SLOT_DURATION_18S
		));
	}

	// Perform runtime upgrade through the parachain collator
	// Important: the upgrade must be performed through the collator, not through the relay node,
	// so that the new runtime can use all necessary host functions
	log::info!("Performing runtime upgrade for parachain {}", PARA_ID);
	runtime_upgrade(&network, &collator_node, PARA_ID, WASM_WITH_SLOT_DURATION_18S).await?;

	let expected_spec_version = current_spec_version + 1;

	// Wait for the upgrade to complete (maximum 250 seconds)
	log::info!("Waiting for parachain runtime upgrade to version {}...", expected_spec_version);
	tokio::time::timeout(
		Duration::from_secs(250),
		wait_for_upgrade(collator_client.clone(), expected_spec_version),
	)
	.await
	.map_err(|_| anyhow!("Timeout waiting for runtime upgrade"))??;

	log::info!("Runtime upgrade completed successfully");

	// Verify that the relay chain is working and finalizing
	log::info!("Checking that relay chain is finalizing blocks...");
	assert_blocks_are_being_finalized(&relay_client).await?;

	// Now with the CurrentSlot migration, the parachain should continue producing blocks
	// after the upgrade, as the migration recalculates CurrentSlot taking into account the new
	// slot duration (18s instead of 6s), preventing a panic in pallet_aura::on_initialize.
	log::info!("Checking that parachain continues producing blocks after upgrade...");

	// Verify that the parachain continues producing blocks after the upgrade
	// The migration should have prevented the panic and allowed the parachain to work normally
	assert_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(PARA_ID), 10..30)].into_iter().collect(),
	)
	.await?;

	log::info!("Test finished - parachain successfully continued producing blocks after upgrade");

	Ok(())
}

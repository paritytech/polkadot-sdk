// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// The test sets up a network with 4 validators and 1 collator, then performs a runtime upgrade
// of the parachain to a runtime with an increased version and slot duration of 18 seconds, waits
// for the upgrade to complete and verifies that the relay chain is working and finalizing, and
// the parachain is producing blocks (waits for 10 blocks).

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_test_runtime::slot_duration_18s::WASM_BINARY_BLOATY as WASM_WITH_SLOT_DURATION_18S;
use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, assert_para_throughput, create_runtime_upgrade_call,
	submit_extrinsic_and_wait_for_finalization_success, wait_for_runtime_upgrade,
};
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn parachain_runtime_upgrade_slot_duration_18s() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	let current_spec_version =
		collator_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version: {current_spec_version}");

	let wasm = WASM_WITH_SLOT_DURATION_18S
		.expect("WASM binary for slot-duration-18s runtime should be available");
	log::info!("Using runtime WASM with slot duration 18s (size: {} bytes)", wasm.len());

	log::info!("Performing runtime upgrade for parachain {}", PARA_ID);
	let call = create_runtime_upgrade_call(&wasm);
	submit_extrinsic_and_wait_for_finalization_success(&collator_client, &call, &dev::alice())
		.await?;

	let expected_spec_version = current_spec_version + 1;

	log::info!("Waiting for parachain runtime upgrade to version {}...", expected_spec_version);
	wait_for_runtime_upgrade(&collator_client).await?;

	let spec_version_after_upgrade =
		collator_client.backend().current_runtime_version().await?.spec_version;
	assert_eq!(
		expected_spec_version, spec_version_after_upgrade,
		"Unexpected runtime spec version"
	);

	log::info!("Runtime upgrade completed successfully");

	log::info!("Verifying that slot duration is 18 seconds after upgrade...");
	let slot_duration = get_slot_duration(&collator_client).await?;
	assert_eq!(
		slot_duration, 18000,
		"Expected slot duration to be 18000 ms (18 seconds), but got {} ms",
		slot_duration
	);
	log::info!("Slot duration verified: {} ms", slot_duration);

	log::info!("Checking that relay chain is finalizing blocks...");
	assert_blocks_are_being_finalized(&relay_client).await?;

	log::info!("Checking that parachain continues producing blocks after upgrade...");

	assert_para_throughput(&relay_client, 15, [(ParaId::from(PARA_ID), 10..30)]).await?;
	log::info!("Test finished - parachain successfully continued producing blocks after upgrade");
	Ok(())
}

async fn get_slot_duration(client: &OnlineClient<PolkadotConfig>) -> Result<u64, anyhow::Error> {
	let best_block = client.blocks().at_latest().await?;
	let block_hash = best_block.hash();

	use zombienet_sdk::subxt::dynamic::Value;
	let result = client
		.runtime_api()
		.at(block_hash)
		.call(zombienet_sdk::subxt::dynamic::runtime_api_call(
			"AuraApi",
			"slot_duration",
			Vec::<Value>::new(),
		))
		.await?;

	let slot_duration: u64 = result.as_type()?;

	log::info!("Slot duration from runtime API: {} ms", slot_duration);
	Ok(slot_duration)
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
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

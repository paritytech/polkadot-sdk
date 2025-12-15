// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachains Upgrade Smoke Test
//!
//! This test verifies that a parachain can be registered, produce blocks,
//! perform a runtime upgrade, and continue producing blocks after the upgrade.

use super::utils::{env_or_default, initialize_network, CUMULUS_IMAGE_ENV, INTEGRATION_IMAGE_ENV};
use anyhow::anyhow;
use cumulus_test_runtime::wasm_spec_version_incremented::WASM_BINARY_BLOATY as WASM_RUNTIME_UPGRADE;
use cumulus_zombienet_sdk_helpers::{
	assert_para_is_registered, assert_para_throughput, create_runtime_upgrade_call,
	submit_extrinsic_and_wait_for_finalization_success, wait_for_runtime_upgrade,
};
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

/// Smoke test that verifies parachain registration, block production, and runtime upgrade.
///
/// - Checks parachain 2000 is registered
/// - Checks parachain 2000 is producing blocks
/// - Performs runtime upgrade with incremented spec version
/// - Checks parachain 2000 continues producing blocks after upgrade
#[tokio::test(flavor = "multi_thread")]
async fn parachains_upgrade_smoke_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	let para_node = network.get_node("collator01")?;
	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;

	log::info!("Checking parachain {} is registered", PARA_ID);
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID), 75).await?;
	log::info!("Parachain {} is registered", PARA_ID);

	// Check parachain produces at least 10 blocks
	log::info!("Checking parachain {} is producing blocks (phase 1)", PARA_ID);
	assert_para_throughput(&alice_client, 30, [(ParaId::from(PARA_ID), 10..100)]).await?;
	log::info!("Parachain {} is producing blocks", PARA_ID);

	// Get current spec version before upgrade
	let current_spec_version = para_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version: {}", current_spec_version);

	log::info!("Performing runtime upgrade");

	let upgrade_wasm = WASM_RUNTIME_UPGRADE.expect("Wasm runtime not built");
	log::info!("Using upgrade runtime ({} bytes)", upgrade_wasm.len());

	let call = create_runtime_upgrade_call(upgrade_wasm);
	submit_extrinsic_and_wait_for_finalization_success(&para_client, &call, &dev::alice()).await?;

	log::info!("Runtime upgrade submitted, waiting for it to be applied");
	wait_for_runtime_upgrade(&para_client).await?;
	log::info!("Runtime upgrade applied");

	// Check parachain continues producing blocks after upgrade
	log::info!("Checking parachain {} is producing blocks (phase 2 - after upgrade)", PARA_ID);
	assert_para_throughput(&alice_client, 10, [(ParaId::from(PARA_ID), 4..50)]).await?;
	log::info!("Parachain {} continues producing blocks after upgrade", PARA_ID);

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let culumus_image = env_or_default(CUMULUS_IMAGE_ENV, images.cumulus.as_str());

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.cumulus_based(true)
				.with_default_command("test-parachain")
				.with_default_image(culumus_image.as_str())
				.with_collator(|n| n.with_name("collator01"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}

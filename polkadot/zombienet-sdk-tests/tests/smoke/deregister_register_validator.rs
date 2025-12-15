// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Deregister/Register Validator Smoke Test
//!
//! This test verifies that validators can be dynamically deregistered and
//! re-registered using the validatorManager pallet. It checks that the
//! validator status metrics update correctly after session boundaries.

use super::utils::{
	create_deregister_validator_call, create_register_validator_call, env_or_default,
	initialize_network, ACTIVE_VALIDATOR_METRIC, INTEGRATION_IMAGE_ENV, PARACHAIN_VALIDATOR_METRIC,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	submit_extrinsic_and_wait_for_finalization_success, wait_for_nth_session_change,
};
use std::str::FromStr;
use zombienet_sdk::{
	subxt::{dynamic::Value, OnlineClient, PolkadotConfig},
	subxt_signer::{
		sr25519::{dev, Keypair},
		SecretUri,
	},
	NetworkConfig, NetworkConfigBuilder,
};

/// Smoke test that verifies validator deregistration and re-registration.
///
/// - Checks dave is in the validator set
/// - Deregisters dave
/// - Waits 2 sessions (authority set changes enacted at current_session + 2)
/// - Checks dave is NOT in the validator set
/// - Registers dave again
/// - Waits 2 sessions
/// - Checks dave is back in the validator set
#[tokio::test(flavor = "multi_thread")]
async fn deregister_register_validator_smoke_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let alice_node = network.get_node("alice")?;
	let dave_node = network.get_node("dave")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice_node.wait_client().await?;

	// Get Dave's stash account (//Dave//stash)
	let dave_stash_uri = SecretUri::from_str("//Dave//stash")?;
	let dave_stash_keypair = Keypair::from_uri(&dave_stash_uri)?;
	let dave_stash_account = Value::from_bytes(dave_stash_keypair.public_key().0);

	// Initial check: dave should be in the validator set
	log::info!("Checking dave is in the validator set");
	dave_node
		.wait_metric_with_timeout(PARACHAIN_VALIDATOR_METRIC, |v| v == 1.0, 240u64)
		.await
		.map_err(|e| anyhow!("Dave is not a parachain validator: {}", e))?;
	dave_node
		.wait_metric_with_timeout(ACTIVE_VALIDATOR_METRIC, |v| v == 1.0, 240u64)
		.await
		.map_err(|e| anyhow!("Dave is not an active validator: {}", e))?;
	log::info!("Dave is in the validator set");

	// Deregister dave
	log::info!("Deregistering dave");
	let deregister_call = create_deregister_validator_call(dave_stash_account.clone());
	submit_extrinsic_and_wait_for_finalization_success(
		&alice_client,
		&deregister_call,
		&dev::alice(),
	)
	.await?;
	log::info!("Deregistration transaction finalized");

	// Wait 2 sessions for the authority set change to be enacted
	log::info!("Waiting for 2 session boundaries");
	let mut blocks_sub = alice_client.blocks().subscribe_finalized().await?;
	wait_for_nth_session_change(&mut blocks_sub, 2).await?;
	log::info!("Session boundaries passed");

	// Check dave is NOT in the validator set
	log::info!("Checking dave is NOT in the validator set");
	dave_node
		.wait_metric_with_timeout(PARACHAIN_VALIDATOR_METRIC, |v| v == 0.0, 180u64)
		.await
		.map_err(|e| anyhow!("Dave is still a parachain validator: {}", e))?;
	dave_node
		.wait_metric_with_timeout(ACTIVE_VALIDATOR_METRIC, |v| v == 0.0, 180u64)
		.await
		.map_err(|e| anyhow!("Dave is still an active validator: {}", e))?;
	log::info!("Dave is NOT in the validator set");

	// Register dave again
	log::info!("Registering dave again");
	let register_call = create_register_validator_call(dave_stash_account);
	submit_extrinsic_and_wait_for_finalization_success(
		&alice_client,
		&register_call,
		&dev::alice(),
	)
	.await?;
	log::info!("Registration transaction finalized");

	// Wait 2 sessions for the authority set change to be enacted
	log::info!("Waiting for 2 session boundaries");
	let mut blocks_sub = alice_client.blocks().subscribe_finalized().await?;
	wait_for_nth_session_change(&mut blocks_sub, 2).await?;
	log::info!("Session boundaries passed");

	// Check dave is back in the validator set
	log::info!("Checking dave is back in the validator set");
	dave_node
		.wait_metric_with_timeout(PARACHAIN_VALIDATOR_METRIC, |v| v == 1.0, 180u64)
		.await
		.map_err(|e| anyhow!("Dave is not a parachain validator: {}", e))?;
	dave_node
		.wait_metric_with_timeout(ACTIVE_VALIDATOR_METRIC, |v| v == 1.0, 180u64)
		.await
		.map_err(|e| anyhow!("Dave is not an active validator: {}", e))?;
	log::info!("Dave is back in the validator set");

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_default_args(vec![("-lruntime=debug,parachain=trace").into()])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
				.with_node(|node| node.with_name("charlie"))
				.with_node(|node| node.with_name("dave"))
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

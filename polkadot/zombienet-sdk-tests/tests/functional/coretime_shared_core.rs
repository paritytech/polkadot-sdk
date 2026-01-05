// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Coretime Shared Core Test
//!
//! This test verifies that multiple parachains can share a single core using coretime
//! assignments. It sets up 4 glutton parachains (2000-2003) that share core 0,
//! and verifies they all produce blocks.

use crate::utils::{
	env_or_default, fetch_genesis_header, fetch_validation_code, initialize_network, COL_IMAGE_ENV,
	INTEGRATION_IMAGE_ENV,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{dynamic::Value, ext::scale_value::value, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_IDS: [u32; 4] = [2000, 2001, 2002, 2003];
const NODE_ROLES_METRIC: &str = "node_roles";

/// Creates a sudo call to assign a core to multiple parachains with equal parts.
fn create_assign_shared_core_call(core: u32, para_ids: &[u32], parts_each: u32) -> Value {
	let mut assignments = vec![];
	for para_id in para_ids {
		assignments.push(value! { (Task(*para_id), parts_each) });
	}

	value! {
		Coretime(assign_core { core: core, begin: 0u32, assignment: assignments, end_hint: None() })
	}
}

/// Test that multiple parachains can share a single core.
///
/// This test:
/// - Spawns 4 validators
/// - Spawns 4 glutton parachains (2000-2003) without auto-onboarding
/// - Registers the parachains via sudo
/// - Assigns core 0 to be shared by all 4 parachains
/// - Verifies each parachain produces blocks
#[tokio::test(flavor = "multi_thread")]
async fn coretime_shared_core_test() -> Result<(), anyhow::Error> {
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

	// Register paras 2 by 2 to speed up the test. registering all at once will exceed the weight
	// limit.
	log::info!("Registering parachains 2000-2001");
	register_paras(&network, &relay_client, &alice, &[2000, 2001]).await?;

	log::info!("Registering parachains 2002-2003");
	register_paras(&network, &relay_client, &alice, &[2002, 2003]).await?;

	// Assign core 0 to be shared by all 4 parachains.
	log::info!("Assigning core 0 to be shared by all parachains");
	let assign_core_call = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![create_assign_shared_core_call(0, &PARA_IDS, 14400)],
	);
	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&relay_client,
		&assign_core_call,
		&alice,
		600u64,
	)
	.await
	.map_err(|e| anyhow!("Failed to assign core: {}", e))?;
	log::info!("Core 0 assigned to all parachains");

	// Wait for each parachain to produce at least 6 blocks.
	// Since they share a core with equal parts, each gets ~1/4 of the block production time.
	// Expect roughly 6 blocks each within 200 relay blocks.
	log::info!("Waiting for parachains to produce blocks");
	assert_para_throughput(
		&relay_client,
		200,
		[
			(ParaId::from(2000u32), 6..100),
			(ParaId::from(2001u32), 6..100),
			(ParaId::from(2002u32), 6..100),
			(ParaId::from(2003u32), 6..100),
		],
	)
	.await?;

	log::info!("Test finished successfully");
	Ok(())
}

/// Registers the given parachains by fetching their genesis header and validation code
/// from the collator nodes, then submitting a sudo batch call.
async fn register_paras<S: zombienet_sdk::subxt::tx::signer::Signer<PolkadotConfig>>(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
	relay_client: &OnlineClient<PolkadotConfig>,
	signer: &S,
	para_ids: &[u32],
) -> Result<(), anyhow::Error> {
	let mut calls = vec![];
	// Get Alice's public key bytes for the account ID.
	let alice_account = Value::from_bytes(dev::alice().public_key().0);

	for para_id in para_ids {
		let collator_name = format!("collator-{para_id}");
		let collator_node = network.get_node(&collator_name)?;
		let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

		let genesis_header = fetch_genesis_header(&collator_client).await?;
		let validation_code = fetch_validation_code(&collator_client).await?;

		log::info!(
			"Para {}: genesis header {} bytes, validation code {} bytes",
			para_id,
			genesis_header.len(),
			validation_code.len()
		);

		let validation_code_value = Value::from_bytes(&validation_code);
		calls.push(value! {
			Paras(add_trusted_validation_code { validation_code: validation_code_value })
		});

		// Add force register call.
		let genesis_head_value = Value::from_bytes(&genesis_header);
		let validation_code_for_register = Value::from_bytes(&validation_code);
		calls.push(value! {
			Registrar(force_register {
				who: alice_account.clone(),
				deposit: 0u128,
				id: *para_id,
				genesis_head: genesis_head_value,
				validation_code: validation_code_for_register
			})
		});
	}

	let sudo_batch = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: calls })
		}],
	);

	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		relay_client,
		&sudo_batch,
		signer,
		600u64,
	)
	.await
	.map_err(|e| anyhow!("Failed to register paras {:?}: {}", para_ids, e))?;

	log::info!("Parachains {:?} registered successfully", para_ids);
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let col_image = env_or_default(COL_IMAGE_ENV, images.cumulus.as_str());

	let mut builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		r.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(polkadot_image.as_str())
			.with_default_args(vec![
				("-lruntime=debug,parachain=debug,parachain::backing=trace,parachain::collator-protocol=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,runtime::inclusion-inherent=trace,runtime::inclusion=trace").into(),
			])
			.with_genesis_overrides(json!({
				"configuration": {
					"config": {
						"scheduler_params": {
							"max_validators_per_core": 1,
							"num_cores": 4
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
	});

	// Add 4 glutton parachains (2000-2003) without auto-onboarding.
	for para_id in PARA_IDS {
		let chain_name = format!("glutton-westend-local-{para_id}");
		let collator_name = format!("collator-{para_id}");
		let col_image = col_image.clone();

		builder = builder.with_parachain(|p| {
			p.with_id(para_id)
				.onboard_as_parachain(false)
				.cumulus_based(true)
				.with_chain(chain_name.as_str())
				.with_default_command("polkadot-parachain")
				.with_default_image(col_image.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_genesis_overrides(json!({
					"glutton": {
						"compute": "50000000",
						"storage": "2500000000",
						"trashDataCount": 5120
					}
				}))
				.with_collator(|n| n.with_name(collator_name.as_str()))
		});
	}

	builder = builder.with_global_settings(|global_settings| {
		match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		}
	});

	builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})
}

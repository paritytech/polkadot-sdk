// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that `CandidateReceiptV3` can be enabled dynamically while the network is running.
//!
//! Starts with only `CandidateReceiptV2` (bit 3) set in `node_features`. After verifying that V2
//! candidates are backed and finalized, enables `CandidateReceiptV3` (bit 4) via a sudo
//! extrinsic. After the next session change (when the config becomes active), verifies that the
//! relay chain continues to accept V2 candidates from the collator.
//!
//! The validator set contains both standard and experimental-collator-protocol validators.

use super::{assert_candidates_version, assert_validator_backed_candidates};
use crate::utils::enable_v3_node_features;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput};
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Test: V3 descriptor enabled dynamically while the network is running.
#[tokio::test(flavor = "multi_thread")]
async fn v3_dynamic_enablement_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4
							}
						}
					}
				}))
				// Standard collator protocol validators (group 0).
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
				// Experimental collator protocol validators (group 1).
				.with_validator(|node| {
					node.with_name("validator-2").with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
				.with_validator(|node| {
					node.with_name("validator-3").with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2900"))
		})
		.with_parachain(|p| {
			p.with_id(2901)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2901"))
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
	let para_node_slot = network.get_node("collator-2901")?;
	let experimental_validator_2 = network.get_node("validator-2")?;
	let experimental_validator_3 = network.get_node("validator-3")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	log::info!("checking V2 candidates with V3 disabled");
	assert_candidates_version(
		&relay_client,
		ParaId::from(2900),
		CandidateDescriptorVersion::V2,
		false, // v3 not enabled
		10,
		20,
	)
	.await?;

	assert_candidates_version(
		&relay_client,
		ParaId::from(2901),
		CandidateDescriptorVersion::V2,
		false,
		10,
		20,
	)
	.await?;

	log::info!("Enabling V3");
	enable_v3_node_features(&relay_client).await?;

	log::info!("checking V2 candidates after V3 enabled");
	assert_candidates_version(
		&relay_client,
		ParaId::from(2900),
		CandidateDescriptorVersion::V2,
		true, // v3 now enabled
		10,
		20,
	)
	.await?;

	assert_candidates_version(
		&relay_client,
		ParaId::from(2901),
		CandidateDescriptorVersion::V2,
		true,
		10,
		20,
	)
	.await?;

	assert_para_throughput(
		&relay_client,
		50,
		[(ParaId::from(2900), 40..51), (ParaId::from(2901), 40..51)],
	)
	.await?;

	assert_validator_backed_candidates(relay_node, 30).await?;
	assert_validator_backed_candidates(experimental_validator_2, 30).await?;
	assert_validator_backed_candidates(experimental_validator_3, 30).await?;

	assert_finality_lag(&para_node.wait_client().await?, 5).await?;
	assert_finality_lag(&para_node_slot.wait_client().await?, 5).await?;

	log::info!("V3 dynamic enablement test finished successfully");

	Ok(())
}

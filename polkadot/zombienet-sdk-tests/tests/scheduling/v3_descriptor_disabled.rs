// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that parachains produce blocks and finalize when the V3 candidate descriptor is disabled.
//!
//! Only `CandidateReceiptV2` (bit 3) is enabled in node_features; `CandidateReceiptV3` (bit 4)
//! is NOT set. The validator set contains both standard and experimental-collator-protocol
//! validators.
//!
//! Verifies that:
//! - The relay chain finalizes blocks after the first session change.
//! - Parachains produce and back V2 candidates with both validator protocol variants.
//! - Parachain finality progresses with acceptable lag.

use super::{assert_candidates_version, assert_validator_backed_candidates};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_finality_lag;
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Test: V3 descriptor disabled, only V2 enabled.
///
/// Enables only `CandidateReceiptV2` (bit 3) in node_features. Verifies that the relay chain
/// accepts V2 candidates, blocks finalize, and the parachain progresses.
#[tokio::test(flavor = "multi_thread")]
async fn v3_disabled_produces_v2_candidates() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Only CandidateReceiptV2 (bit 3) enabled, V3 (bit 4) NOT set.
	// bitvec Lsb0 u8: bit 3 set => 0b00001000 = 8
	let node_features_v2_only = json!({"bits": 8, "data": [0b00001000]});

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
								// 2 validators per backing group: one standard + one experimental.
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4
							},
							"node_features": node_features_v2_only,
						}
					}
				}))
				// Standard collator protocol validators.
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
				// Experimental collator protocol validators.
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
			p.with_id(2600)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2600"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2600")?;
	let experimental_validator_2 = network.get_node("validator-2")?;
	let experimental_validator_3 = network.get_node("validator-3")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// V3 is NOT enabled. Candidates backed by both standard and experimental validators count.
	assert_candidates_version(
		&relay_client,
		ParaId::from(2600),
		CandidateDescriptorVersion::V2,
		false, // v3 not enabled
		15,
		20,
	)
	.await?;

	// Verify that validators from both backing groups signed statements: group 0 (standard
	// protocol) and group 1 (experimental-collator-protocol). Group rotation ensures both groups
	// get assigned to back candidates.
	assert_validator_backed_candidates(relay_node, 30).await?;
	assert_validator_backed_candidates(experimental_validator_2, 30).await?;
	assert_validator_backed_candidates(experimental_validator_3, 30).await?;

	// Verify the parachain is finalizing blocks with acceptable lag.
	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V3 disabled test finished successfully");

	Ok(())
}

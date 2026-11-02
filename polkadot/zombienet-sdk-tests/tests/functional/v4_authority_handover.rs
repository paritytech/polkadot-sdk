// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Authority handover across an aura set change on a V4 parachain.
//!
//! Three collators elastic-scale a parachain onto three cores with per-core resubmission. Mid-run,
//! sudo rotates `pallet_aura::Authorities` left by one, the same post-state flip a session
//! rotation enacts, so every slot's expected author changes while each collator keeps its key.
//! Resubmitted blocks verify against their parent state, so the pre-change blocks can only be
//! resubmitted by the prior era's selected collator: the handover. The test asserts a collator
//! performs it and that throughput survives the boundary.

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, assign_cores,
	submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use sp_crypto_hashing::twox_128;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{self, dynamic::Value, ext::scale_value::value, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2900;
const NUM_COLLATORS: usize = 3;

#[tokio::test(flavor = "multi_thread")]
async fn v4_authority_handover() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let collator_names: Vec<String> =
		(0..NUM_COLLATORS).map(|i| format!("collator-2900-{i}")).collect();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug").into(),
					"--experimental-collator-protocol".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							// V2 (bit 4) and V3 (bit 3) descriptors enabled.
							"node_features": {"bits": 8, "data": [0b00011000]},
							"max_relay_parent_session_age": 2,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			let p = p
				.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,aura::cumulus=debug,aura::resubmission=trace")
						.into(),
					"--authoring=slot-based".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
					"--network-backend=libp2p".into(),
				])
				.with_collator(|n| n.with_name(collator_names[0].as_str()));
			collator_names
				.iter()
				.skip(1)
				.fold(p, |acc, name| acc.with_collator(|n| n.with_name(name.as_str())))
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

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node(collator_names[0].as_str())?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let para_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;
	log::info!("Para {PARA_ID} elastic-scaled to 3 cores (V3 descriptors, resubmission on)");

	// A healthy pre-change window: three cores fed at ~2.6 candidates per relay block (ceiling 3.0,
	// one per assigned core). Half the baseline throughput test's 100-block window.
	assert_para_throughput(&relay_client, 50, [(ParaId::from(PARA_ID), 125..150)], []).await?;
	log::info!("Pre-change throughput confirmed, rotating the aura authority set");

	// Rotate `pallet_aura::Authorities` left by one with sudo. The flip lands in the block's
	// post-state like a session rotation would, aura-ext mirrors it on finalize, and with three
	// distinct keys every slot's expected author changes.
	let authorities_key =
		[twox_128(b"Aura").as_slice(), twox_128(b"Authorities").as_slice()].concat();
	let raw = para_client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(authorities_key.clone())
		.await?
		.ok_or_else(|| anyhow!("aura authorities not found in storage"))?;
	let mut authorities: Vec<[u8; 32]> = Decode::decode(&mut &raw[..])?;
	assert_eq!(authorities.len(), NUM_COLLATORS, "one genesis authority per collator expected");
	authorities.rotate_left(1);

	let items = vec![Value::unnamed_composite([
		Value::from_bytes(authorities_key),
		Value::from_bytes(authorities.encode()),
	])];
	let rotate_call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items })
		}],
	);
	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&para_client,
		&rotate_call,
		&dev::alice(),
		120u64,
	)
	.await
	.map_err(|e| anyhow!("authority rotation failed to finalize: {e:?}"))?;
	log::info!("Authority set rotated, waiting for a collator to perform the handover");

	// The pre-change blocks still in flight can only be resubmitted by the prior era's selected
	// collator, and by the time the rotation is finalized that slot has passed: the handover
	// must already be in some collator's logs, give or take a few seconds of slack.
	let mut handover_seen = false;
	'outer: for _ in 0..12 {
		for name in &collator_names {
			let result = network
				.get_node(name)?
				.wait_log_line_count_with_timeout(
					"*Authority handover: resubmitting the prior era's blocks*",
					true,
					LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(1), false),
				)
				.await?;
			if result.success() {
				log::info!("Collator {name} performed the authority handover");
				handover_seen = true;
				break 'outer;
			}
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
	assert!(handover_seen, "no collator performed the authority handover");

	// The boundary costs the few slots the handover spends resubmitting instead of building, but
	// all three cores must return to steady state within the window.
	assert_para_throughput(&relay_client, 50, [(ParaId::from(PARA_ID), 125..150)], []).await?;

	assert_finality_lag(&para_client, 30).await?;

	log::info!("V4 authority handover test finished");
	Ok(())
}

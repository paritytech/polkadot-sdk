// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V3 candidates require the `CandidateReceiptV3` node feature (bit 4) on the relay AND the v3
//! runtime on the para. The relay side is toggled with a `set_node_feature` extrinsic; the para
//! side via a runtime upgrade to the v3 test runtime.
//!
//! Para 2902 walks all four (relay, para) states:
//!   off/off → V2, on/off → V2, on/on → V3, off/on → V2.
//!
//! Paras 2900 and 2902 run a mix of a v3-capable collator (current image) and a v2-only collator
//! (`OLD_CUMULUS_IMAGE`). On 2900 both emit V2; on 2902's V3 phase only the v3 collator can build,
//! so it carries the para.
//!
//! [`v3_relay_feature_rollback_keeps_para_producing`] is a focused companion: it starts already in
//! the on/on steady state (relay feature on, para on the `v3_rpo_2` runtime — V3 with relay parent
//! offset 2 — from genesis), so there is no runtime upgrade and no live offset change. It rolls
//! back only the relay feature and checks the para keeps producing as V2 with the offset still 2,
//! exercising the offset-2 V2 fallback path in isolation.

use crate::utils::{
	assert_candidates_version, assert_validator_backed_candidates, disable_node_features,
	enable_node_features,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assign_cores, wait_for_nth_session_change, wait_for_runtime_upgrade,
};
use polkadot_primitives::{node_features::FeatureIndex, CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use std::{collections::HashMap, time::Duration};
use zombienet_sdk::{
	subxt::{dynamic::Value, ext::scale_value::value, tx::dynamic, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v3_dynamic_enablement_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let v3_wasm = cumulus_test_runtime::v3::WASM_BINARY
		.ok_or_else(|| anyhow!("cumulus-test-runtime v3 WASM not built (needs WASM build)"))?;
	let old_cumulus_image = std::env::var("OLD_CUMULUS_IMAGE").unwrap_or(images.cumulus.clone());
	let old_cumulus_command =
		std::env::var("OLD_CUMULUS_COMMAND").unwrap_or("test-parachain".into());
	// Encode V2-only node features: bit 3 = CandidateReceiptV2, bit 4 = CandidateReceiptV3.
	// The bitvec byte is LSB-first, so bit index N → 1u8 << N.
	let v2_byte = 1u8 << FeatureIndex::CandidateReceiptV2 as u8; // 1 << 3 = 0b00001000
	let node_features_v2_only = json!({"bits": 8, "data": [v2_byte]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// 2 extra cores beyond each auto-registered para core.
								// Para 2901 uses elastic scaling and is assigned 0 and 1 in addition.
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4
							},
							"node_features": node_features_v2_only
						}
					}
				}))
				// Standard collator protocol validators (groups 0, 1).
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});
			(4..8).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		// Para 2900: basic V2 (single core), stays V2. Mix of a v3-capable and a v2-only collator.
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2900-v3"))
				.with_collator(|n| {
					n.with_name("collator-2900-v2")
						.with_image(old_cumulus_image.as_str())
						.with_command(old_cumulus_command.as_str())
				})
		})
		// Para 2901: V2 elastic scaling (3 cores) — throughput must survive enablement.
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
		// Para 2902: starts V2 ("async-backing"), upgraded to v3 mid-test. Mixed collators.
		.with_parachain(|p| {
			p.with_id(2902)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("async-backing")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2902-v3"))
				.with_collator(|n| {
					n.with_name("collator-2902-v2")
						.with_image(old_cumulus_image.as_str())
						.with_command(old_cumulus_command.as_str())
				})
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2900-v3")?;
	let para_node_slot = network.get_node("collator-2901")?;
	let para_node_v3 = network.get_node("collator-2902-v3")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let para_client_v3: OnlineClient<PolkadotConfig> = para_node_v3.wait_client().await?;

	// Assign 2 extra cores to para 2901 for elastic scaling (3 cores total).
	assign_cores(&relay_client, 2901, vec![0, 1]).await?;

	let para_2900 = ParaId::from(2900);
	let para_2901 = ParaId::from(2901);
	let para_2902 = ParaId::from(2902);

	// State (relay `CandidateReceiptV3` feature, para v3 runtime): off / off → V2.
	log::info!("state off/off (relay off, para off) → V2");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2900, 5..15), (para_2901, 15..40), (para_2902, 5..15)]),
		10,
	)
	.await?;

	// off/off → on/off: enable the relay feature. Para 2902 still runs the V2 runtime
	log::info!("state on/off (relay on, para off) → V2");
	enable_node_features(&relay_client, &[4]).await?;
	let mut sub = relay_client.blocks().subscribe_finalized().await?;
	tokio::time::timeout(Duration::from_secs(300), wait_for_nth_session_change(&mut sub, 1))
		.await
		.map_err(|_| {
			anyhow!("timed out after 300s waiting for 1 session change (on/off state)")
		})??;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2902, 3..20)]),
		10,
	)
	.await?;

	// on/off → on/on: upgrade para 2902 to the v3 runtime. Both sides on → V3.
	// v3 shares its spec_version with V2, so use set_code_without_checks.
	log::info!("state on/on (relay on, para on) → V3");
	let upgrade_call = dynamic(
		"Sudo",
		"sudo_unchecked_weight",
		vec![
			value! { System(set_code_without_checks { code: Value::from_bytes(v3_wasm) }) },
			value! { { ref_time: 1u64, proof_size: 1u64 } },
		],
	);
	para_client_v3
		.tx()
		.sign_and_submit_then_watch_default(&upgrade_call, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_runtime_upgrade(&para_client_v3).await?;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(para_2902, 3..20)]),
		10,
	)
	.await?;

	// on/on → off/on: disable the relay feature while 2902 keeps the v3 runtime. The collator falls
	// back to V2 and the para must keep producing (the case this fix enables). The change applies
	// at session+2 (SESSION_DELAY); wait 2 session changes so the V3 tail has drained before
	// asserting.
	log::info!("state off/on (relay off, para on) → V2 (fallback)");
	disable_node_features(&relay_client, &[4]).await?;
	let mut sub = relay_client.blocks().subscribe_finalized().await?;
	// 480s: generous wall-clock limit so a stalled block stream fails loudly instead of hanging CI.
	tokio::time::timeout(Duration::from_secs(480), wait_for_nth_session_change(&mut sub, 2))
		.await
		.map_err(|_| {
			anyhow!("timed out after 480s waiting for 2 session changes (off/on state)")
		})??;
	// 2902 has fallen back to V2; 2900 (basic) and 2901 (elastic) run the V2 runtime throughout,
	// so all three are V2.
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2902, 3..20), (para_2900, 3..20), (para_2901, 10..50)]),
		10,
	)
	.await?;

	// No disputes throughout the lifecycle.
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_disputes_total",
			|v| v == 0.0,
			30u64,
		)
		.await?;

	assert_validator_backed_candidates(relay_node, 30).await?;
	for i in 4..=7 {
		let node = network.get_node(format!("validator-{i}"))?;
		assert_validator_backed_candidates(node, 30).await?;
	}

	assert_finality_lag(&para_node.wait_client().await?, 6).await?;
	assert_finality_lag(&para_node_slot.wait_client().await?, 15).await?;
	assert_finality_lag(&para_node_v3.wait_client().await?, 15).await?;

	log::info!("V3 dynamic enablement test (relay then para enablement) finished successfully");

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn v3_relay_feature_rollback_keeps_para_producing() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// V2 (bit 3) and V3 (bit 4) enabled from genesis.
	// bit 3 = CandidateReceiptV2, bit 4 = CandidateReceiptV3; both set → 0b00011000.
	let feature_byte = (1u8 << FeatureIndex::CandidateReceiptV2 as u8) |
		(1u8 << FeatureIndex::CandidateReceiptV3 as u8);
	let node_features_with_v3 = json!({"bits": 8, "data": [feature_byte]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 3
							},
							"node_features": node_features_with_v3,
							// The offset-2 relay-parent walk-back is asserted right after a session
							// change, so it must be allowed to cross a session boundary.
							"max_relay_parent_session_age": 2
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..3).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});

			// Experimental collator protocol validators (needed for V3 collation).
			(3..6).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		// Para 2902: V3 with relay parent offset 2 from genesis.
		.with_parachain(|p| {
			p.with_id(2902)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("v3-rpo-2")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2902"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2902")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	let para_2902 = ParaId::from(2902);

	// on/on (relay on, para v3 + offset 2) → V3.
	log::info!("state on/on (relay on, para v3+offset2) → V3");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(para_2902, 5..25)]),
		20,
	)
	.await?;

	// on/on → off/on: roll back only the relay feature. The para keeps the v3 runtime and must keep
	// producing, now as V2 with the offset still 2 (the case the claim-queue-offset bound fix
	// enables). The change applies at session+2 (SESSION_DELAY); wait 2 session changes so the V3
	// tail has drained before asserting.
	log::info!("state off/on (relay off, para v3+offset2) → V2 (fallback)");
	disable_node_features(&relay_client, &[4]).await?;
	let mut sub = relay_client.blocks().subscribe_finalized().await?;
	wait_for_nth_session_change(&mut sub, 2).await?;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2902, 5..15)]),
		15,
	)
	.await?;

	// No disputes throughout.
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_disputes_total",
			|v| v == 0.0,
			30u64,
		)
		.await?;

	assert_validator_backed_candidates(relay_node, 30).await?;

	assert_finality_lag(&para_node.wait_client().await?, 15).await?;

	log::info!("V3 relay feature rollback test finished successfully");

	Ok(())
}

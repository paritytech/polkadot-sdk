// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Enabling V3 while part of the validator set still runs a pre-V3 binary.
//!
//! `westend-local` with 15 validators. Dispute conclusion needs an `n - (n - 1) / 3 = 11` vote
//! supermajority, which is what the two variants pivot on:
//!
//! * `4/15` old: the valid side reaches 11 → disputes conclude valid → the 4 old validators are
//!   disabled (4 is the disabling cap for n = 15) → the storm self-extinguishes. Proves the safety
//!   net holds.
//! * `5/15` old: the valid side maxes out at 10 < 11, so no dispute ever concludes and nobody is
//!   disabled. But 5 invalid votes exceed the byzantine threshold, reverting each disputed block
//!   and clamping GRANDPA to the undisputed chain → finality is degraded. Documents the boundary:
//!   2/3 is the soundness floor, but the operational bar is that ~all validators are upgraded.
//!
//! Two single-collator parachains: A stays plain V2 (canary that V3 gossip does not poison the old
//! validators on an unrelated para), B starts V2 and is upgraded to a v3-descriptor runtime
//! mid-test.

use crate::utils::assert_candidates_version;
use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, wait_for_runtime_upgrade};
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateReceiptV2, Id as ParaId, ValidatorId, ValidatorIndex,
};
use rstest::rstest;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use zombienet_sdk::{
	subxt::{
		dynamic::Value, ext::scale_value::value, tx::dynamic, utils::H256, OnlineClient,
		PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const TOTAL_VALIDATORS: usize = 15;

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;

const GROUP_ROTATION: u32 = 10;

const HEALTHY_BACKING_FLOOR: u32 = 7;

#[rstest]
#[case::four_old(4)]
#[case::five_old(5)]
#[tokio::test(flavor = "multi_thread")]
async fn v3_old_validator_dispute_storm(#[case] num_old: usize) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let num_new = TOTAL_VALIDATORS - num_old;
	assert!(num_new >= 1, "need at least one v3-capable validator to back candidates");

	let images = zombienet_sdk::environment::get_images_from_env();

	let v3_wasm =
		cumulus_test_runtime::v3_spec_version_incremented::WASM_BINARY.ok_or_else(|| {
			anyhow!("cumulus-test-runtime v3_spec_version_incremented WASM not built (needs WASM build)")
		})?;

	let native = std::env::var("ZOMBIE_PROVIDER").as_deref() == Ok("native");
	let old_image = std::env::var("OLD_POLKADOT_IMAGE").ok();
	let old_command = std::env::var("OLD_POLKADOT_COMMAND").unwrap_or("polkadot".into());

	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let old_names: Vec<String> =
		(num_new..TOTAL_VALIDATORS).map(|i| format!("old-validator-{i}")).collect();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime::staking=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 3,
								"group_rotation_frequency": 10
							},
							"minimum_backing_votes": 2,
							"node_features": node_features_with_v3
						}
					}
				}))
				.with_validator(|node| {
					node.with_name("validator-0")
						.with_args(vec!["-lparachain=debug,runtime::staking=debug".into()])
						.invulnerable(false)
				});

			let r = (1..num_new).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}"))
						.with_args(vec!["-lparachain=debug,runtime::staking=debug".into()])
						.invulnerable(false)
				})
			});

			(num_new..TOTAL_VALIDATORS).fold(r, |acc, i| {
				acc.with_validator(|node| {
					let node = node
						.with_name(&format!("old-validator-{i}"))
						.with_command(old_command.as_str())
						.with_args(vec!["-lparachain=debug,runtime::staking=debug".into()]);
					let node = match (native, old_image.as_deref()) {
						(false, Some(img)) => node.with_image(img),
						_ => node,
					};
					node.invulnerable(false)
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-a"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("async-backing")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-b"))
		})
		.with_global_settings(|s| s.with_tear_down_on_failure(false))
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	log::info!("Spawning network: {num_new} v3-capable + {num_old} pre-v3 validators");
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_b_node = network.get_node("collator-b")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let para_b_client: OnlineClient<PolkadotConfig> = para_b_node.wait_client().await?;

	let mut old_pubkeys: HashSet<String> = HashSet::new();
	for name in &old_names {
		let spec = serde_json::to_value(network.get_node(name)?.spec())?;
		let pk = spec
			.pointer("/accounts/accounts/sr/public_key")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow!("no sr public_key in spec for {name}"))?;
		old_pubkeys.insert(normalize_hex(pk));
	}
	assert_eq!(old_pubkeys.len(), num_old, "collected {} old keys", old_pubkeys.len());

	let para_a = ParaId::from(PARA_A);
	let para_b = ParaId::from(PARA_B);

	log::info!("baseline: both paras emit V2");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_a, 7..10), (para_b, 7..10)]),
		8,
		None,
	)
	.await?;
	assert!(
		relay_node
			.assert_with("parachain_candidate_disputes_total", |d| d == 0.0)
			.await?,
		"no disputes expected before the para upgrade"
	);

	log::info!("upgrading para B to the v3 runtime");
	let upgrade_call = dynamic(
		"Sudo",
		"sudo_unchecked_weight",
		vec![
			value! { System(set_code_without_checks { code: Value::from_bytes(v3_wasm) }) },
			value! { { ref_time: 1u64, proof_size: 1u64 } },
		],
	);
	para_b_client
		.tx()
		.sign_and_submit_then_watch_default(&upgrade_call, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_runtime_upgrade(&para_b_client).await?;

	log::info!("waiting for dispute storm to start");
	relay_node
		.wait_metric_with_timeout("parachain_candidate_disputes_total", |d| d >= 1.0, 300u64)
		.await?;

	if num_old == 4 {
		assert_four_old_self_extinguishes(relay_node, &relay_client, &old_pubkeys, num_old).await?;

		log::info!("waiting for dispute storm to self-extinguish");
		let total_disputes = wait_for_dispute_storm_to_settle(relay_node, 240).await?;
		log::info!("storm self-extinguished at {total_disputes} disputes");

		assert_finality_lag(&para_b_client, 6).await?;

		let series = backed_series_over_best_blocks(
			&relay_client,
			GROUP_ROTATION,
			&[(para_b, CandidateDescriptorVersion::V3), (para_a, CandidateDescriptorVersion::V2)],
		)
		.await?;
		let b_backed: u32 = series[&para_b].iter().sum();
		let a_backed: u32 = series[&para_a].iter().sum();
		log::info!("post-recovery: para B V3={b_backed}, para A V2={a_backed} (floor {HEALTHY_BACKING_FLOOR})");
		assert!(
			b_backed >= HEALTHY_BACKING_FLOOR,
			"para B did not recover to a healthy V3 band: {b_backed} in {GROUP_ROTATION} blocks \
			 (floor {HEALTHY_BACKING_FLOOR})",
		);
		assert!(
			a_backed >= HEALTHY_BACKING_FLOOR,
			"canary para A degraded: {a_backed} V2 in {GROUP_ROTATION} blocks (floor \
			 {HEALTHY_BACKING_FLOOR})",
		);
	} else {
		assert_five_old_persists(relay_node, &relay_client).await?;
	}

	log::info!("V3 old-validator dispute storm test ({num_old}/{TOTAL_VALIDATORS} old) finished");
	Ok(())
}

async fn assert_four_old_self_extinguishes(
	relay_node: &zombienet_sdk::NetworkNode,
	relay_client: &OnlineClient<PolkadotConfig>,
	old_pubkeys: &HashSet<String>,
	num_old: usize,
) -> Result<(), anyhow::Error> {
	log::info!("waiting for disputes to conclude valid");
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}",
			|d| d >= 1.0,
			300u64,
		)
		.await?;

	log::info!("waiting for all {num_old} pre-v3 validators to be disabled");
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut disabled_old = 0usize;
	let mut blocks_checked = 0u32;
	while let Some(block) = best_blocks.next().await {
		let hash = block?.hash();
		let disabled = disabled_validators_at(relay_client, hash).await?;
		if !disabled.is_empty() {
			let validators = session_validators_at(relay_client, hash).await?;
			disabled_old = count_disabled_old(&disabled, &validators, old_pubkeys);
			log::info!("disabled {} validators, {disabled_old} of them pre-v3", disabled.len());
			if disabled_old >= num_old {
				break;
			}
		}
		blocks_checked += 1;
		if blocks_checked >= 90 {
			break;
		}
	}
	assert!(
		disabled_old >= num_old,
		"expected all {num_old} pre-v3 validators disabled, got {disabled_old} after \
		 {blocks_checked} blocks",
	);
	Ok(())
}

async fn assert_five_old_persists(
	relay_node: &zombienet_sdk::NetworkNode,
	relay_client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let finalized_metric = "substrate_block_height{status=\"finalized\"}";
	let f0 = read_metric(relay_node, finalized_metric).await?;
	tokio::time::sleep(std::time::Duration::from_secs(60)).await;
	let f1 = read_metric(relay_node, finalized_metric).await?;
	let finalized_progress = f1 - f0;
	log::info!("finalized over 60s: {f0} → {f1} (+{finalized_progress}, healthy ~10)");
	assert!(
		finalized_progress == 0.0,
		"expected finality to be degraded behind the unresolvable dispute, but finalized advanced \
		 {f0} → {f1} (+{finalized_progress}; healthy would be ~10 per 60s)",
	);

	let finalized_hash = relay_client.blocks().at_latest().await?.hash();
	let disabled = disabled_validators_at(relay_client, finalized_hash).await?;
	assert!(
		disabled.is_empty(),
		"no validator should be disabled in the 5/15 case, found {disabled:?}",
	);

	for validity in ["valid", "invalid"] {
		let metric =
			format!("polkadot_parachain_candidate_dispute_concluded{{validity=\"{validity}\"}}");
		assert!(
			relay_node.assert_with(metric, |d| d == 0.0).await?,
			"no dispute should conclude ({validity}) without an 11-vote supermajority",
		);
	}
	Ok(())
}

fn normalize_hex(s: &str) -> String {
	s.trim_start_matches("0x").to_ascii_lowercase()
}

fn count_disabled_old(
	disabled: &[ValidatorIndex],
	validators: &[ValidatorId],
	old_pubkeys: &HashSet<String>,
) -> usize {
	disabled
		.iter()
		.filter(|idx| {
			validators
				.get(idx.0 as usize)
				.map(|id| {
					let hex = id
						.clone()
						.into_inner()
						.0
						.iter()
						.map(|b| format!("{b:02x}"))
						.collect::<String>();
					old_pubkeys.contains(&hex)
				})
				.unwrap_or(false)
		})
		.count()
}

async fn disabled_validators_at(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
) -> Result<Vec<ValidatorIndex>, anyhow::Error> {
	Ok(Vec::<ValidatorIndex>::decode(
		&mut &relay_client
			.runtime_api()
			.at(hash)
			.call_raw("ParachainHost_disabled_validators", None)
			.await?[..],
	)?)
}

async fn session_validators_at(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
) -> Result<Vec<ValidatorId>, anyhow::Error> {
	Ok(Vec::<ValidatorId>::decode(
		&mut &relay_client
			.runtime_api()
			.at(hash)
			.call_raw("ParachainHost_validators", None)
			.await?[..],
	)?)
}

async fn backed_series_over_best_blocks(
	relay_client: &OnlineClient<PolkadotConfig>,
	num_blocks: u32,
	targets: &[(ParaId, CandidateDescriptorVersion)],
) -> Result<HashMap<ParaId, Vec<u32>>, anyhow::Error> {
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut series: HashMap<ParaId, Vec<u32>> = targets.iter().map(|(p, _)| (*p, vec![])).collect();
	let mut blocks_checked = 0u32;
	while let Some(block) = best_blocks.next().await {
		let events = block?.events().await?;
		let mut per_block: HashMap<ParaId, u32> = targets.iter().map(|(p, _)| (*p, 0)).collect();
		for event in events.iter() {
			let event = event?;
			if event.pallet_name() == "ParaInclusion" && event.variant_name() == "CandidateBacked" {
				let receipt = CandidateReceiptV2::<H256>::decode(&mut &event.field_bytes()[..])?;
				let para_id = receipt.descriptor.para_id();
				let version = receipt.descriptor.version();
				if targets.iter().any(|(p, v)| *p == para_id && *v == version) {
					*per_block.get_mut(&para_id).expect("para_id is a target; qed") += 1;
				}
			}
		}
		for (para_id, count) in per_block {
			series.get_mut(&para_id).expect("para_id is a target; qed").push(count);
		}
		blocks_checked += 1;
		if blocks_checked >= num_blocks {
			break;
		}
	}
	Ok(series)
}

async fn read_metric(
	node: &zombienet_sdk::NetworkNode,
	metric: &str,
) -> Result<f64, anyhow::Error> {
	node.reports(metric)
		.await
		.map_err(|e| anyhow!("failed to read metric {metric}: {e}"))
}

async fn wait_for_dispute_storm_to_settle(
	node: &zombienet_sdk::NetworkNode,
	timeout_secs: u64,
) -> Result<f64, anyhow::Error> {
	const STEP_SECS: u64 = 25;
	let metric = "parachain_candidate_disputes_total";
	let mut prev = read_metric(node, metric).await?;
	let mut elapsed = 0u64;
	loop {
		tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
		elapsed += STEP_SECS;
		let cur = read_metric(node, metric).await?;
		log::info!("dispute counter {prev} → {cur} after {elapsed}s");
		if cur - prev == 0.0 {
			return Ok(cur);
		}
		prev = cur;
		if elapsed >= timeout_secs {
			return Err(anyhow!(
				"dispute storm did not settle within {timeout_secs}s (still growing, last {cur})"
			));
		}
	}
}

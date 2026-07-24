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
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput_with, wait_for_runtime_upgrade,
};
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId, ValidatorId, ValidatorIndex};
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
				.with_validator(|node| node.with_name("validator-0").invulnerable(false));

			let r = (1..num_new).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).invulnerable(false)
				})
			});

			(num_new..TOTAL_VALIDATORS).fold(r, |acc, i| {
				acc.with_validator(|node| {
					let node = node
						.with_name(&format!("old-validator-{i}"))
						.with_command(old_command.as_str());
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

		log::info!("waiting for every raised dispute to conclude valid");
		let total_disputes = wait_for_disputes_resolved_valid(relay_node, 240).await?;
		log::info!("storm resolved: all {total_disputes} disputes concluded valid, none invalid");

		assert_finality_lag(&para_b_client, 6).await?;

		const RECOVERY_WINDOW: u32 = 20;
		const PARA_A_FLOOR: u32 = 15;
		const PARA_B_V3_FLOOR: u32 = 8;
		assert_para_throughput_with(
			&relay_client,
			RECOVERY_WINDOW,
			HashMap::from([
				(para_b, PARA_B_V3_FLOOR..RECOVERY_WINDOW + 1),
				(para_a, PARA_A_FLOOR..RECOVERY_WINDOW + 1),
			]),
			|receipt| {
				let para_id = receipt.descriptor.para_id();
				let version = receipt.descriptor.version();
				if para_id == para_b && version != CandidateDescriptorVersion::V3 {
					return Err(anyhow!("para B backed non-V3 post-recovery: {version:?}"));
				}
				if para_id == para_a && version != CandidateDescriptorVersion::V2 {
					return Err(anyhow!("canary para A backed non-V2: {version:?}"));
				}
				Ok(true)
			},
		)
		.await?;
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
	assert_finality_stalls(relay_node, 120).await?;

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
				.is_some_and(|id| old_pubkeys.contains(&hex::encode(id.clone().into_inner().0)))
		})
		.count()
}

/// Call a raw `ParachainHost` runtime API at `hash` and SCALE-decode its result.
async fn runtime_api_decode<T: Decode>(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
	method: &str,
) -> Result<T, anyhow::Error> {
	Ok(T::decode(&mut &relay_client.runtime_api().at(hash).call_raw(method, None).await?[..])?)
}

async fn disabled_validators_at(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
) -> Result<Vec<ValidatorIndex>, anyhow::Error> {
	runtime_api_decode(relay_client, hash, "ParachainHost_disabled_validators").await
}

async fn session_validators_at(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
) -> Result<Vec<ValidatorId>, anyhow::Error> {
	runtime_api_decode(relay_client, hash, "ParachainHost_validators").await
}

async fn read_metric(
	node: &zombienet_sdk::NetworkNode,
	metric: &str,
) -> Result<f64, anyhow::Error> {
	node.reports(metric)
		.await
		.map_err(|e| anyhow!("failed to read metric {metric}: {e}"))
}

async fn assert_finality_stalls(
	node: &zombienet_sdk::NetworkNode,
	timeout_secs: u64,
) -> Result<(), anyhow::Error> {
	const STEP_SECS: u64 = 20;
	let metric = "substrate_block_height{status=\"finalized\"}";
	let mut prev = read_metric(node, metric).await?;
	let mut elapsed = 0u64;
	loop {
		tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
		elapsed += STEP_SECS;
		let cur = read_metric(node, metric).await?;
		log::info!(
			"finalized {prev} → {cur} after {elapsed}s (waiting for the dispute to freeze it)"
		);
		if cur == prev {
			tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
			let after = read_metric(node, metric).await?;
			assert!(
				after == cur,
				"finality resumed after appearing to stall ({cur} → {after}); the unresolved \
				 dispute should keep it frozen in the 5/15 case",
			);
			log::info!("finality frozen at {cur} (degraded as expected)");
			return Ok(());
		}
		prev = cur;
		if elapsed >= timeout_secs {
			return Err(anyhow!(
				"finality kept advancing (last {cur}) for {timeout_secs}s; expected the unresolved \
				 dispute to freeze it in the 5/15 case",
			));
		}
	}
}

/// Wait for the dispute storm to fully resolve: every raised dispute must conclude *valid* (the
/// honest supermajority wins) and none may conclude *invalid*.
async fn wait_for_disputes_resolved_valid(
	node: &zombienet_sdk::NetworkNode,
	timeout_secs: u64,
) -> Result<f64, anyhow::Error> {
	const STEP_SECS: u64 = 20;
	let total_metric = "parachain_candidate_disputes_total";
	let valid_metric = "polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}";
	let invalid_metric = "polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";
	let mut elapsed = 0u64;
	loop {
		let total = read_metric(node, total_metric).await?;
		let valid = read_metric(node, valid_metric).await?;
		// The invalid-conclusion counter is absent until first incremented; treat absent as zero.
		let invalid = read_metric(node, invalid_metric).await.unwrap_or(0.0);
		log::info!(
			"disputes after {elapsed}s: total={total}, concluded_valid={valid}, \
			 concluded_invalid={invalid}"
		);
		assert!(
			invalid == 0.0,
			"a dispute concluded invalid ({invalid}); the honest supermajority must never lose",
		);

		if total >= 1.0 && valid >= total {
			// All raised disputes concluded valid. Confirm no new dispute is still in flight.
			tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
			elapsed += STEP_SECS;
			let total_after = read_metric(node, total_metric).await?;
			let invalid_after = read_metric(node, invalid_metric).await.unwrap_or(0.0);
			assert!(
				invalid_after == 0.0,
				"a dispute concluded invalid ({invalid_after}); the honest supermajority must never lose",
			);
			if total_after <= total {
				return Ok(total_after);
			}
		} else {
			tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
			elapsed += STEP_SECS;
		}

		if elapsed >= timeout_secs {
			return Err(anyhow!(
				"disputes did not all conclude valid within {timeout_secs}s \
				 (total={total}, concluded_valid={valid})"
			));
		}
	}
}

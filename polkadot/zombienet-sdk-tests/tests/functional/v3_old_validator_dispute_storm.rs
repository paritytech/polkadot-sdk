// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Enabling V3 while part of the validator set still runs a pre-V3 binary.
//!
//! The network is `westend-local` with 15 validators. Dispute conclusion needs a
//! `n - (n - 1) / 3 = 11` vote supermajority, which is exactly what the two variants pivot on:
//!
//! * `4/15` old: the valid side reaches `15 - 4 = 11` → disputes conclude valid → the 4 old
//!   validators collect against-valid offences and are disabled (4 is the disabling cap for n = 15)
//!   → with every disputer disabled the storm self-extinguishes. This proves the safety net holds.
//! * `5/15` old: the valid side maxes out at `15 - 5 = 10 < 11` → disputes never conclude, nobody
//!   is slashed or disabled, and the degradation persists. This documents the boundary: 2/3 is the
//!   soundness floor, but the operational bar is that ~all validators are upgraded.
//!
//! Two single-collator parachains are used: A stays plain V2 throughout (a canary proving that V3
//! gossip does not poison the old validators on an unrelated para), and B starts V2 and is upgraded
//! mid-test to a v3-descriptor runtime.

use crate::utils::assert_candidates_version;
use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, wait_for_runtime_upgrade};
use polkadot_primitives::{
	BlockNumber, CandidateDescriptorVersion, CandidateHash, CandidateReceiptV2, DisputeState,
	Id as ParaId, SessionIndex, ValidatorId, ValidatorIndex,
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

/// Total validators in the set. Chosen so the dispute supermajority (`n - (n - 1) / 3`) lands at
/// 11, straddled by the two variants (11 valid voters vs 10).
const TOTAL_VALIDATORS: usize = 15;

const PARA_A: u32 = 2000; // plain V2 canary.
const PARA_B: u32 = 2001; // upgraded from V2 to V3 mid-test.

#[rstest]
// 4 old validators: valid side reaches 11 → disputes conclude → all 4 disabled → storm dies out.
#[case::four_old(4)]
// 5 old validators: valid side stuck at 10 → disputes never conclude → nobody disabled.
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

	// V3 (bit 4) is enabled from genesis alongside V2 (bit 3). Para B still runs a V2 runtime, so
	// it keeps emitting V2 until its mid-test upgrade — the storm is triggered by that upgrade,
	// not by the feature bit.
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let old_names: Vec<String> =
		(num_new..TOTAL_VALIDATORS).map(|i| format!("old-validator-{i}")).collect();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			// westend-local so staking-based disabling can take effect.
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
				// validator-0: v3-capable, used as the query node.
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
		// Para B: starts V2 ("async-backing"), upgraded to the v3 runtime mid-test.
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
		// Keep the network alive on failure for post-mortem logs.
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

	// Collect the pre-V3 validators' public keys so we can tell them apart in the disabled set.
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

	// Baseline: V3 is enabled on the relay from genesis, but para B still runs a V2 runtime, so
	// both paras emit V2 and there are no disputes. This pins the later storm on the para upgrade,
	// not on the feature bit.
	log::info!("baseline: V3 enabled on the relay, both paras still emit V2");
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

	// Para B now emits V3 descriptors. We deliberately do NOT assert this over finalized blocks
	// here: the dispute storm we wait for next only happens on V3 candidates (para A is V2 and the
	// pre-V3 validators validate it fine), so a rising dispute counter is itself proof that B went
	// V3. Counting over finalized blocks would in any case be unreliable now, since the disputes
	// start stalling finality. Para B's V3 band is asserted explicitly in the 4/15 path once
	// finality has recovered.

	// The storm: every V3 candidate a pre-V3 validator approval-checks raises a dispute.
	log::info!("waiting for the dispute storm to start (proves para B emits V3)");
	relay_node
		.wait_metric_with_timeout("parachain_candidate_disputes_total", |d| d >= 1.0, 300u64)
		.await?;

	if num_old == 4 {
		assert_four_old_self_extinguishes(relay_node, &relay_client, &old_pubkeys, num_old).await?;

		// Immediate recovery: disabling takes effect within the session (not at a boundary), so the
		// moment the last disputer is disabled the storm ends and para B resumes. We measure over
		// the *best* chain starting right after disabling — same session, no finality/session
		// wait — which verifies the V3 throughput band and proves the effect is immediate. The
		// window spans two group rotations so a transiently short-handed group (disabled members)
		// can't skew it. Over it: para B holds a healthy V3 band, the canary para A a healthy V2
		// band, and the dispute counter must not keep growing (no disputer left).
		log::info!(
			"verifying immediate recovery on the best chain (V3 band + canary + no new disputes)"
		);
		let disputes_before = read_metric(relay_node, "parachain_candidate_disputes_total").await?;
		let counts = count_backed_over_best_blocks(
			&relay_client,
			20,
			&[(para_b, CandidateDescriptorVersion::V3), (para_a, CandidateDescriptorVersion::V2)],
		)
		.await?;
		let disputes_after = read_metric(relay_node, "parachain_candidate_disputes_total").await?;
		log::info!(
			"over 20 best blocks post-disabling: para B V3={}, para A V2={}, disputes {disputes_before}→{disputes_after}",
			counts[&para_b],
			counts[&para_a],
		);
		assert!(
			counts[&para_b] >= 8,
			"para B did not recover to a healthy V3 band immediately after disabling: {} in 20 best blocks",
			counts[&para_b],
		);
		assert!(
			counts[&para_a] >= 8,
			"canary para A degraded: {} V2 in 20 best blocks",
			counts[&para_a],
		);
		assert!(
			disputes_after - disputes_before <= 3.0,
			"dispute storm should self-extinguish after disabling, but grew {disputes_before}→{disputes_after}",
		);

		log::info!("waiting for relay-chain finality to recover after the storm");
		relay_node
			.wait_metric_with_timeout(
				"polkadot_parachain_approval_checking_finality_lag",
				|lag| lag <= 5.0,
				180u64,
			)
			.await?;
		assert_finality_lag(&para_b_client, 12).await?;
	} else {
		let collator_a = network.get_node("collator-a")?;
		assert_five_old_persists(relay_node, &relay_client, collator_a).await?;
	}

	log::info!("V3 old-validator dispute storm test ({num_old}/{TOTAL_VALIDATORS} old) finished");
	Ok(())
}

/// `4/15`: the valid supermajority (11) concludes the disputes, the 4 pre-V3 validators are
/// disabled, and the storm self-extinguishes.
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

/// `5/15`: the valid side never reaches 11, so disputes never conclude, nobody is disabled, and the
/// degradation persists. The canary para A must stay healthy V2 throughout.
async fn assert_five_old_persists(
	relay_node: &zombienet_sdk::NetworkNode,
	relay_client: &OnlineClient<PolkadotConfig>,
	collator_a: &zombienet_sdk::NetworkNode,
) -> Result<(), anyhow::Error> {
	log::info!("observing that disputes persist unconcluded and nobody is disabled");
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut blocks_checked = 0u32;
	let mut saw_unconcluded = false;
	while let Some(block) = best_blocks.next().await {
		let hash = block?.hash();

		// Nobody may be disabled: the valid side cannot muster the 11-vote supermajority.
		let disabled = disabled_validators_at(relay_client, hash).await?;
		assert!(
			disabled.is_empty(),
			"no validator should be disabled in the 5/15 case, found {disabled:?}",
		);

		// And at least one dispute must remain open with the valid side capped at 10.
		for (_, _, state) in disputes_at(relay_client, hash).await? {
			if state.concluded_at.is_none() {
				saw_unconcluded = true;
				assert!(
					state.validators_for.count_ones() <= 10,
					"valid side should be capped at 10 votes, got {}",
					state.validators_for.count_ones(),
				);
			}
		}

		blocks_checked += 1;
		if blocks_checked >= 20 {
			break;
		}
	}
	assert!(saw_unconcluded, "expected persistent unconcluded disputes in the 5/15 case");

	// No dispute may have concluded on the valid side.
	assert!(
		relay_node
			.assert_with(
				"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}",
				|d| d == 0.0,
			)
			.await?,
		"no dispute should conclude valid with only 10 valid voters",
	);

	// Canary: the unrelated plain-V2 para A keeps making progress — V3 gossip does not poison the
	// pre-v3 validators on paras they can actually validate. Relay finality is stalled here, so we
	// cannot count candidates over finalized blocks; instead we track para A's collator best block
	// height, which advances as its candidates are backed on the (still-progressing) best chain.
	log::info!("asserting canary para A keeps producing despite the stalled relay finality");
	let best_height_metric = "substrate_block_height{status=\"best\"}";
	let start_height = read_metric(collator_a, best_height_metric).await?;
	let target_height = start_height + 8.0;
	collator_a
		.wait_metric_with_timeout(best_height_metric, |h| h >= target_height, 180u64)
		.await
		.map_err(|e| {
			anyhow!(
				"canary para A stalled (best height {start_height} → target {target_height}): {e}"
			)
		})?;

	// The degradation persists; we record the finality lag rather than asserting a bound.
	let lag = read_metric(relay_node, "polkadot_parachain_approval_checking_finality_lag").await?;
	log::info!("finality lag with an unresolvable dispute (5/15 case): {lag}");
	Ok(())
}

/// Normalize a hex public key to lowercase without a `0x` prefix.
fn normalize_hex(s: &str) -> String {
	s.trim_start_matches("0x").to_ascii_lowercase()
}

/// Count how many disabled validator indices belong to a pre-V3 (old) validator.
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

async fn disputes_at(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
) -> Result<Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>, anyhow::Error> {
	Ok(Vec::<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>::decode(
		&mut &relay_client
			.runtime_api()
			.at(hash)
			.call_raw("ParachainHost_disputes", None)
			.await?[..],
	)?)
}

/// Count backed candidates matching the requested `(para, version)` targets over the next
/// `num_blocks` *best* relay blocks. Best (not finalized) blocks keep advancing while relay
/// finality is still catching up, and starting from the current best means the count reflects what
/// happens immediately, not after a session or finality boundary.
async fn count_backed_over_best_blocks(
	relay_client: &OnlineClient<PolkadotConfig>,
	num_blocks: u32,
	targets: &[(ParaId, CandidateDescriptorVersion)],
) -> Result<HashMap<ParaId, u32>, anyhow::Error> {
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut counts: HashMap<ParaId, u32> = targets.iter().map(|(p, _)| (*p, 0)).collect();
	let mut blocks_checked = 0u32;
	while let Some(block) = best_blocks.next().await {
		let events = block?.events().await?;
		for event in events.iter() {
			let event = event?;
			if event.pallet_name() == "ParaInclusion" && event.variant_name() == "CandidateBacked" {
				let receipt = CandidateReceiptV2::<H256>::decode(&mut &event.field_bytes()[..])?;
				let para_id = receipt.descriptor.para_id();
				let version = receipt.descriptor.version();
				if targets.iter().any(|(p, v)| *p == para_id && *v == version) {
					*counts.get_mut(&para_id).expect("para_id is a target; qed") += 1;
				}
			}
		}
		blocks_checked += 1;
		if blocks_checked >= num_blocks {
			break;
		}
	}
	Ok(counts)
}

/// Read a single metric value from a node.
async fn read_metric(
	node: &zombienet_sdk::NetworkNode,
	metric: &str,
) -> Result<f64, anyhow::Error> {
	node.reports(metric)
		.await
		.map_err(|e| anyhow!("failed to read metric {metric}: {e}"))
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Enabling V3 while part of the validator set still runs a pre-V3 binary.
//!
//! `westend-local` with 15 validators. Dispute conclusion needs an `n - (n - 1) / 3 = 11` vote
//! supermajority, which is what the two variants pivot on:
//!
//! * `4/15` old: the valid side reaches 11 → disputes conclude valid → the 4 old validators are
//!   disabled (4 is the disabling cap for n = 15). Proves the safety net holds.
//!
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
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateReceiptV2, CoreIndex, GroupRotationInfo, Id as ParaId,
	ValidatorId, ValidatorIndex,
};
use rstest::rstest;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use zombienet_sdk::{
	subxt::{
		blocks::Block, dynamic::Value, ext::scale_value::value, tx::dynamic, utils::H256,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const TOTAL_VALIDATORS: usize = 15;

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;

/// Relay blocks to measure para B over once a capable group holds its core. Must stay below
/// `group_rotation_frequency` (10) so the whole window falls inside one group assignment.
const PARA_B_WINDOW: u32 = 6;

/// How many relay blocks to spend looking for a capable group before giving up.
const PARA_B_SEARCH_LIMIT: u32 = 150;

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
			.assert_with("polkadot_parachain_candidate_disputes_total", |d| d == 0.0)
			.await?,
		"no disputes expected before the para upgrade"
	);

	// Capture finalized height before the upgrade so assert_finality_stalls (5/15 path) can
	// require real forward progress before declaring a stall.
	let baseline_finalized =
		read_metric(relay_node, "substrate_block_height{status=\"finalized\"}").await?;
	log::info!("baseline relay finalized height (before para-B upgrade): {baseline_finalized}");

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
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_disputes_total",
			|d| d >= 4.0,
			300u64,
		)
		.await?;

	match num_old {
		4 => {
			assert_four_old_disabled(relay_node, &relay_client, &old_pubkeys).await?;

			log::info!("waiting for every dispute raised so far to conclude valid");
			let total_disputes = wait_for_pending_disputes_resolved_valid(relay_node, 300).await?;
			log::info!("all {total_disputes} disputes raised so far concluded valid, none invalid");

			assert_finality_lag(&para_b_client, 6).await?;
			assert_para_throughput_with(
				&relay_client,
				50,
				HashMap::from([(para_b, 20..51), (para_a, 45..51)]),
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
		},
		5 => {
			assert_five_old_persists(relay_node, &relay_client, baseline_finalized).await?;
		},
		other => unreachable!("unexpected num_old: {other}"),
	}

	log::info!("V3 old-validator dispute storm test ({num_old}/{TOTAL_VALIDATORS} old) finished");
	Ok(())
}

async fn assert_four_old_disabled(
	relay_node: &zombienet_sdk::NetworkNode,
	relay_client: &OnlineClient<PolkadotConfig>,
	old_pubkeys: &HashSet<String>,
) -> Result<(), anyhow::Error> {
	const NUM_OLD: usize = 4;
	log::info!("waiting for disputes to conclude valid");
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}",
			|d| d >= 1.0,
			300u64,
		)
		.await?;

	log::info!("waiting for all {NUM_OLD} pre-v3 validators to be disabled");
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
			if disabled_old >= NUM_OLD {
				break;
			}
		}
		blocks_checked += 1;
		if blocks_checked >= 90 {
			break;
		}
	}
	assert!(
		disabled_old >= NUM_OLD,
		"expected all {NUM_OLD} pre-v3 validators disabled, got {disabled_old} after \
		 {blocks_checked} blocks",
	);
	Ok(())
}

async fn assert_five_old_persists(
	relay_node: &zombienet_sdk::NetworkNode,
	relay_client: &OnlineClient<PolkadotConfig>,
	baseline_finalized: f64,
) -> Result<(), anyhow::Error> {
	assert_finality_stalls(relay_node, 120, baseline_finalized).await?;

	// `at_latest()` returns the best imported block, not the finalized head; in the 5/15 case
	// they diverge by design. We check for disabled validators at the best block, which is the
	// correct thing to check here.
	let best_hash = relay_client.blocks().at_latest().await?.hash();
	let disabled = disabled_validators_at(relay_client, best_hash).await?;
	assert!(
		disabled.is_empty(),
		"no validator should be disabled in the 5/15 case (at best block), found {disabled:?}",
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

/// What para B's backing pipeline looks like at one relay block.
struct BackingContext {
	/// Group membership is redrawn here; used to notice a reshuffle mid-window.
	session_start: u32,
	group_index: u32,
	group_size: usize,
	/// Group members that can actually back a V3 candidate: neither pre-v3 nor disabled.
	usable: usize,
	min_backing: usize,
	blocks_left_in_rotation: u32,
}

impl BackingContext {
	fn is_capable(&self) -> bool {
		self.usable >= self.min_backing
	}
}

/// Measure para B's backing rate while a group that can actually back it holds its core.
///
/// A pre-v3 validator cannot validate a V3 candidate at all — `validate_block` panics on the
/// missing V3 extension — so it is dead weight in a backing group, whether or not it has been
/// disabled yet.
///
/// That makes para B's candidate count over a fixed window a *draw*, not a rate. Group membership
/// is redrawn from on-chain randomness at every session change, and the group-to-core assignment
/// rotates every `group_rotation_frequency` blocks, so a 20-block window samples only about two
/// assignments. Observed runs produced 4 and 9 candidates in 20 blocks while para A produced 20 in
/// both — same storm, same disabled validators, just a different shuffle.
///
/// So rather than widening the bound until it asserts nothing, condition on the layout: wait for a
/// capable group with room left in its rotation, then require para B to be backed in essentially
/// every block of the window. This terminates because `num_old` pre-v3 validators can starve at
/// most `num_old / (group_size - min_backing + 1)` groups — 2 of 5 here — so a capable group always
/// comes round.
async fn assert_para_b_backed_under_capable_group(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_b: ParaId,
	old_pubkeys: &HashSet<String>,
) -> Result<(), anyhow::Error> {
	struct Window {
		session_start: u32,
		group_index: u32,
		counted: u32,
		backed: u32,
	}

	log::info!("looking for a v3-capable backing group on para B's core");
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut window: Option<Window> = None;
	let mut searched = 0u32;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		let number = block.number();

		// Session changes redraw group membership and never carry backed candidates.
		if is_session_change(&block).await? {
			if window.take().is_some() {
				log::info!("session change at relay block {number}, restarting the measurement");
			}
			continue;
		}

		let Some(ctx) =
			para_b_backing_context(relay_client, block.hash(), para_b, old_pubkeys).await?
		else {
			log::debug!("relay block {number}: para B has no core assigned");
			continue;
		};

		if window.is_none() {
			searched += 1;
			if searched > PARA_B_SEARCH_LIMIT {
				return Err(anyhow!(
					"no v3-capable group held para B's core for {PARA_B_WINDOW} consecutive blocks \
					 within {PARA_B_SEARCH_LIMIT} relay blocks; at most 2 of 5 groups should be \
					 starved by 4 pre-v3 validators, so this points at a real backing failure",
				));
			}
			if !ctx.is_capable() || ctx.blocks_left_in_rotation < PARA_B_WINDOW {
				log::debug!(
					"relay block {number}: group {} has {}/{} usable backers (need {}), \
					 {} blocks left in rotation - not measuring",
					ctx.group_index,
					ctx.usable,
					ctx.group_size,
					ctx.min_backing,
					ctx.blocks_left_in_rotation,
				);
				continue;
			}
			log::info!(
				"relay block {number}: group {} on para B's core has {}/{} usable backers \
				 (need {}), {} blocks left in rotation - measuring {PARA_B_WINDOW} blocks",
				ctx.group_index,
				ctx.usable,
				ctx.group_size,
				ctx.min_backing,
				ctx.blocks_left_in_rotation,
			);
			window = Some(Window {
				session_start: ctx.session_start,
				group_index: ctx.group_index,
				counted: 0,
				backed: 0,
			});
		}

		let w = window.as_mut().expect("set directly above or on an earlier iteration; qed");

		// The layout must not shift under us, or we are no longer measuring what we selected for.
		if ctx.session_start != w.session_start ||
			ctx.group_index != w.group_index ||
			!ctx.is_capable()
		{
			log::info!(
				"relay block {number}: para B's backing context changed mid-window \
				 (group {} -> {}, usable {}/{}), restarting",
				w.group_index,
				ctx.group_index,
				ctx.usable,
				ctx.group_size,
			);
			window = None;
			continue;
		}

		let backed = para_b_backed_in(&block, para_b).await?;
		w.counted += 1;
		if backed {
			w.backed += 1;
		}
		log::info!(
			"relay block {number}: para B backed={backed} ({}/{} blocks measured)",
			w.backed,
			w.counted,
		);

		if w.counted >= PARA_B_WINDOW {
			let backed = w.backed;
			// One tolerated miss absorbs a single dropped collation; anything more means the V3
			// para is not being backed even when the validators serving it are able to.
			assert!(
				backed + 1 >= PARA_B_WINDOW,
				"para B was backed in only {backed} of {PARA_B_WINDOW} relay blocks while a group \
				 that can back it held its core; under a capable group it should be backed in \
				 essentially every block, dispute storm or not",
			);
			log::info!("para B backed in {backed}/{PARA_B_WINDOW} blocks under a v3-capable group");
			return Ok(());
		}
	}

	Err(anyhow!("finalized block subscription ended while measuring para B"))
}

/// Read the group currently serving para B's core and how much of it can back a V3 candidate.
///
/// Returns `None` if para B has no core in the claim queue at this block.
async fn para_b_backing_context(
	relay_client: &OnlineClient<PolkadotConfig>,
	hash: H256,
	para_b: ParaId,
	old_pubkeys: &HashSet<String>,
) -> Result<Option<BackingContext>, anyhow::Error> {
	let claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>> =
		runtime_api_decode(relay_client, hash, "ParachainHost_claim_queue").await?;
	let Some(core) = claim_queue
		.iter()
		.find(|(_, queue)| queue.contains(&para_b))
		.map(|(core, _)| *core)
	else {
		return Ok(None);
	};

	let (groups, rotation): (Vec<Vec<ValidatorIndex>>, GroupRotationInfo) =
		runtime_api_decode(relay_client, hash, "ParachainHost_validator_groups").await?;
	let min_backing: u32 =
		runtime_api_decode(relay_client, hash, "ParachainHost_minimum_backing_votes").await?;

	// The rotation modulus is the number of *groups*, not the number of occupied cores — see
	// `n_cores` in `polkadot/node/core/backing/src/lib.rs`. There are more groups (5) than paras
	// (2) here, so using the claim queue length would resolve to the wrong group.
	let group_index = rotation.group_for_core(core, groups.len());
	let group = groups.get(group_index.0 as usize).ok_or_else(|| {
		anyhow!("group {} out of range, only {} groups", group_index.0, groups.len())
	})?;

	let validators = session_validators_at(relay_client, hash).await?;
	let disabled = disabled_validators_at(relay_client, hash).await?;
	let usable = group
		.iter()
		.filter(|idx| {
			!disabled.contains(idx) &&
				validators.get(idx.0 as usize).is_some_and(|id| {
					!old_pubkeys.contains(&hex::encode(id.clone().into_inner().0))
				})
		})
		.count();

	Ok(Some(BackingContext {
		session_start: rotation.session_start_block,
		group_index: group_index.0,
		group_size: group.len(),
		usable,
		min_backing: min_backing as usize,
		blocks_left_in_rotation: rotation.next_rotation_at().saturating_sub(rotation.now),
	}))
}

/// Whether `block` backed a candidate for para B, erroring if one of them is not V3.
async fn para_b_backed_in(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	para_b: ParaId,
) -> Result<bool, anyhow::Error> {
	let events = block.events().await?;
	let mut backed = false;
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() != "ParaInclusion" || event.variant_name() != "CandidateBacked" {
			continue;
		}
		let receipt = CandidateReceiptV2::<H256>::decode(&mut &event.field_bytes()[..])?;
		if receipt.descriptor.para_id() != para_b {
			continue;
		}
		let version = receipt.descriptor.version();
		if version != CandidateDescriptorVersion::V3 {
			return Err(anyhow!("para B backed a non-V3 candidate post-recovery: {version:?}"));
		}
		backed = true;
	}
	Ok(backed)
}

/// Returns `true` if `block` contains a session change.
async fn is_session_change(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<bool, anyhow::Error> {
	let events = block.events().await?;
	Ok(events.iter().any(|event| {
		event.as_ref().is_ok_and(|event| {
			event.pallet_name() == "Session" && event.variant_name() == "NewSession"
		})
	}))
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
	baseline_finalized: f64,
) -> Result<(), anyhow::Error> {
	const STEP_SECS: u64 = 20;
	// Require finality to have advanced at least this many blocks from the pre-upgrade baseline
	// before we start watching for a stall. This guards against a vacuous pass if the function
	// is entered before GRANDPA has had a chance to advance (slow pod, subsystem init).
	const ADVANCE_MARGIN: f64 = 3.0;
	let metric = "substrate_block_height{status=\"finalized\"}";
	let mut elapsed = 0u64;

	// wait for finality to advance past baseline + margin, proving GRANDPA was running.
	loop {
		let cur = read_metric(node, metric).await?;
		if cur >= baseline_finalized + ADVANCE_MARGIN {
			log::info!(
				"finality at {cur} ({:.0} blocks past baseline {baseline_finalized}); \
				 now watching for stall",
				cur - baseline_finalized
			);
			break;
		}
		tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
		elapsed += STEP_SECS;
		if elapsed >= timeout_secs {
			return Err(anyhow!(
				"finality at {cur} never advanced {ADVANCE_MARGIN} blocks above baseline \
				 {baseline_finalized} within {timeout_secs}s; expected GRANDPA to make progress \
				 before the dispute storm clamps it",
			));
		}
	}

	// look for the stall — two consecutive equal samples confirm finality is frozen.
	let mut prev = read_metric(node, metric).await?;
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

/// Wait until every dispute raised *up to the moment this is called* has concluded valid, with none
/// concluding invalid.
///
/// Deliberately snapshot-based rather than waiting for the storm to burn out. Disabling only holds
/// for the session it is applied in: each new session re-enables the pre-v3 validators, which then
/// dispute the next batch of V3 candidates. As long as para B keeps authoring V3 candidates the
/// raised counter therefore keeps climbing for the lifetime of the network, so any condition of the
/// form "the raised count stopped growing" is a race against the session length and will flake.
///
/// What actually holds — and what is worth asserting — is that the honest supermajority resolves
/// every round it is given, and never loses one.
async fn wait_for_pending_disputes_resolved_valid(
	node: &zombienet_sdk::NetworkNode,
	timeout_secs: u64,
) -> Result<f64, anyhow::Error> {
	const STEP_SECS: u64 = 10;

	let total_metric = "polkadot_parachain_candidate_disputes_total";
	let valid_metric = "polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}";
	let invalid_metric = "polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";

	let target = read_metric(node, total_metric).await?;
	assert!(target >= 1.0, "expected at least one dispute to have been raised by now");
	log::info!("{target} disputes raised so far; waiting for all of them to conclude valid");

	let mut elapsed = 0u64;
	loop {
		let valid = read_metric(node, valid_metric).await?;
		let invalid = read_metric(node, invalid_metric).await?;
		assert!(
			invalid == 0.0,
			"a dispute concluded invalid ({invalid}); the honest supermajority must never lose",
		);

		if valid >= target {
			return Ok(target);
		}

		log::info!(
			"after {elapsed}s: {valid}/{target} of the snapshotted disputes concluded valid \
			 (raised so far: {})",
			read_metric(node, total_metric).await?,
		);

		tokio::time::sleep(std::time::Duration::from_secs(STEP_SECS)).await;
		elapsed += STEP_SECS;

		if elapsed >= timeout_secs {
			return Err(anyhow!(
				"only {valid} of the {target} disputes raised at snapshot time concluded valid \
				 within {timeout_secs}s",
			));
		}
	}
}

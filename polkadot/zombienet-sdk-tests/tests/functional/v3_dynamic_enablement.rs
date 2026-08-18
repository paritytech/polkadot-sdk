// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V3 candidates require the `CandidateReceiptV3` node feature (bit 4) on the relay AND the v3
//! runtime on the para. The relay side is toggled with a `set_node_feature` extrinsic; the para
//! side via a runtime upgrade to the v3 test runtime.
//!
//! Para 2902 walks up into V3 and then back out, once per rollback direction. The first three steps
//! are the same for both cases:
//!
//!   off/off → V2, on/off → V2, on/on → V3
//!
//! and then the case parameter picks which side gives up V3:
//!
//! * `relay_rollback` — final step off/on → V2. The node feature is disabled while the para keeps
//!   the v3 runtime, i.e. the V2 fallback with the v3 const still on.
//! * `para_rollback` — final step on/off → V2. The para is upgraded to a V3-disabled runtime while
//!   the feature stays on, so the collator is still in V3 mode when the code swaps.

use crate::utils::{
	assert_candidates_version, assert_validator_backed_candidates, disable_node_features,
	enable_node_features,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assign_cores, current_session_index, submit_sudo_runtime_upgrade,
	wait_for_pvf_prepare, wait_for_runtime_upgrade,
};
use polkadot_primitives::{node_features::FeatureIndex, CandidateDescriptorVersion, Id as ParaId};
use rstest::rstest;
use serde_json::json;
use std::{collections::HashMap, ops::Range};
use zombienet_sdk::{
	subxt::{dynamic::Value, ext::scale_value::value, tx::dynamic, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const SINGLE_CORE_CANDIDATES: Range<u32> = 7..11;
const ELASTIC_CANDIDATES: Range<u32> = 17..31;
const V3_CANDIDATES: Range<u32> = 4..11;

/// Relay config changes (node features included) enact `SESSION_DELAY` sessions after the session
/// they are applied in. Mirrors `polkadot_runtime_parachains::shared::SESSION_DELAY`.
const SESSION_DELAY: u32 = 2;

/// Which side gives up V3 in the final step, see the module docs.
#[derive(Copy, Clone, Debug)]
enum Rollback {
	/// Disable the `CandidateReceiptV3` node feature, para keeps the v3 runtime.
	Relay,
	/// Upgrade the para to a V3-disabled runtime, node feature stays on.
	Para,
}

#[rstest]
#[case::relay_rollback(Rollback::Relay, 0)]
#[case::para_rollback(Rollback::Para, 0)]
#[case::para_rollback_rpo_2(Rollback::Para, 2)]
#[case::para_rollback_rpo_4(Rollback::Para, 4)]
#[tokio::test(flavor = "multi_thread")]
async fn v3_dynamic_enablement_test(
	#[case] rollback: Rollback,
	// Para 2902's `relay_parent_offset`, held constant across the whole walk-up and rollback.
	//
	// At offset 0 the inherent's relay-parent-descendant check is inert
	// (`expected_rp_descendants_num == 0` short-circuits it), so a collator whose view of
	// `V3_SCHEDULING_ENABLED` lags the executing runtime only shows up later in the PVF. At
	// offset 2 that check is armed and catches the same disagreement in `set_validation_data`
	// instead, because the collator omits the descendants a v2-mode runtime demands.
	#[case] relay_parent_offset: u32,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Runtime flavours for 2902, picked so the offset never changes — only the v3 const does.
	//
	// offset 0: async-backing -> v3 -> spec_version_incremented (bumped version, so the rollback
	//           can use the version-checked `System::set_code`).
	// offset 2: relay-parent-offset -> v3_rpo_2 -> relay-parent-offset. Those two share a
	//           spec_version, so both swaps need `set_code_without_checks`.
	let (para_chain, v3_wasm, v3_disabled_wasm) = match relay_parent_offset {
		0 => (
			"async-backing",
			cumulus_test_runtime::v3::WASM_BINARY
				.ok_or_else(|| anyhow!("cumulus-test-runtime v3 WASM not built"))?,
			cumulus_test_runtime::spec_version_incremented::WASM_BINARY.ok_or_else(|| {
				anyhow!("cumulus-test-runtime spec_version_incremented WASM not built")
			})?,
		),
		2 => (
			"relay-parent-offset",
			cumulus_test_runtime::v3_rpo_2::WASM_BINARY
				.ok_or_else(|| anyhow!("cumulus-test-runtime v3_rpo_2 WASM not built"))?,
			cumulus_test_runtime::relay_parent_offset::WASM_BINARY.ok_or_else(|| {
				anyhow!("cumulus-test-runtime relay_parent_offset WASM not built")
			})?,
		),
		// offset 4: relay-parent-offset-4 -> v3_rpo_4 -> relay-parent-offset-4. Same shape as
		// offset 2, one step deeper. Shared spec_version, so both swaps are unchecked.
		4 => (
			"relay-parent-offset-4",
			cumulus_test_runtime::v3_rpo_4::WASM_BINARY
				.ok_or_else(|| anyhow!("cumulus-test-runtime v3_rpo_4 WASM not built"))?,
			cumulus_test_runtime::relay_parent_offset_4::WASM_BINARY.ok_or_else(|| {
				anyhow!("cumulus-test-runtime relay_parent_offset_4 WASM not built")
			})?,
		),
		other => return Err(anyhow!("no runtime flavours wired up for offset {other}")),
	};
	// `scheduling_v3.rs` pairs offset 2 with 1 and is green in CI; 2 pushes the scheduling parent
	// out of prospective-parachains' scope and candidates are dropped as
	// `SchedulingParentNotInScope`.
	let max_relay_parent_session_age = if relay_parent_offset == 0 { 2 } else { 1 };

	let scheduling_lookahead = 5.max(relay_parent_offset + 3);

	// 2902's rate falls as its relay parent trails further behind the tip. The widened bound
	// (scaled from `scheduling_v3.rs`'s `v3-rpo-4`, 15..30 over 40 blocks) covers every non-zero
	// offset.
	let para_2902_candidates = || -> Range<u32> {
		match relay_parent_offset {
			2 | 4 => 4..11,
			_ => SINGLE_CORE_CANDIDATES,
		}
	};

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
								"lookahead": scheduling_lookahead,
								// 2 extra cores beyond each auto-registered para core.
								// Para 2901 uses elastic scaling and is assigned 0 and 1 in addition.
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4
							},
							"node_features": node_features_v2_only,
							"max_relay_parent_session_age": max_relay_parent_session_age
						}
					}
				}))
				// Standard collator protocol validators (groups 0, 1).
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});
			(4..10).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		// Para 2900: basic V2 (single core), stays V2. Two collators share the core.
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2900-a"))
				.with_collator(|n| n.with_name("collator-2900-b"))
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
		// Para 2902: starts V2, upgraded to v3 mid-test, then rolled back. Two collators.
		// The chain spec fixes its `relay_parent_offset` for the whole test.
		.with_parachain(|p| {
			p.with_id(2902)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain(para_chain)
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2902-a"))
				.with_collator(|n| n.with_name("collator-2902-b"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2900-a")?;
	let para_node_slot = network.get_node("collator-2901")?;
	let para_node_v3 = network.get_node("collator-2902-a")?;

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
		HashMap::from([
			(para_2900, SINGLE_CORE_CANDIDATES),
			(para_2901, ELASTIC_CANDIDATES),
			(para_2902, para_2902_candidates()),
		]),
		10,
		None,
	)
	.await?;

	// off/off → on/off: enable the relay feature. Para 2902 still runs the V2 runtime
	log::info!("state on/off (relay on, para off) → V2");
	enable_node_features(&relay_client, &[4]).await?;
	let enactment_session = current_session_index(&relay_client).await? + SESSION_DELAY;
	// 2902 is still on the V2 runtime, so it stays V2 once the feature is enacted. Anchoring here
	// also guarantees the feature is active before the on/on step upgrades the para to v3.
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([
			(para_2900, SINGLE_CORE_CANDIDATES),
			(para_2901, ELASTIC_CANDIDATES),
			(para_2902, para_2902_candidates()),
		]),
		10,
		Some(enactment_session),
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
	// At offset 0 2902's genesis code is 2900's, so the network holds 3 distinct validation codes
	// once the v3 one is added; every other offset gives 2902 its own genesis code, making 4.
	wait_for_pvf_prepare(&network, if relay_parent_offset == 0 { 3 } else { 4 }).await?;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(para_2902, V3_CANDIDATES)]),
		10,
		None,
	)
	.await?;

	// on/on → the rollback under test. Both directions land every para on V2 and must keep them
	// producing at the same rate; the difference is the state the collator is in when the switch
	// happens, so each case exercises a different transition.
	match rollback {
		// Relay side: disable the node feature while 2902 keeps the v3 runtime, so it falls back to
		// V2 with the v3 const still on. Session gated, so anchor to the enactment session to skip
		// the V3 tail.
		Rollback::Relay => {
			log::info!("state off/on (relay off, para on) → V2 (fallback)");
			disable_node_features(&relay_client, &[4]).await?;
			let enactment_session = current_session_index(&relay_client).await? + SESSION_DELAY;
			assert_candidates_version(
				&relay_client,
				CandidateDescriptorVersion::V2,
				HashMap::from([
					(para_2900, SINGLE_CORE_CANDIDATES),
					(para_2901, ELASTIC_CANDIDATES),
					(para_2902, para_2902_candidates()),
				]),
				10,
				Some(enactment_session),
			)
			.await?;
		},
		// Para side: upgrade 2902 to a V3-disabled runtime while the node feature stays on, so the
		// collator is still in V3 mode at the moment of the swap — the V3→V2 shape switch, not just
		// a change of claim queue depth. Not session gated (the new runtime applies at the next
		// para block), but anchor one session ahead anyway so V3 collations that were in flight
		// when the code was swapped are not counted as a version violation.
		Rollback::Para => {
			log::info!("state on/off (relay on, para rolled back off v3) → V2");
			if relay_parent_offset == 0 {
				// `spec_version_incremented` bumps the version, so the checked call works.
				submit_sudo_runtime_upgrade(&para_client_v3, v3_disabled_wasm, &dev::alice())
					.await?;
			} else {
				// The offset-2 rollback target shares a spec_version with `v3_rpo_2`, so
				// `System::set_code` would reject it for not increasing the version.
				let rollback_call = dynamic(
					"Sudo",
					"sudo_unchecked_weight",
					vec![
						value! { System(set_code_without_checks { code: Value::from_bytes(v3_disabled_wasm) }) },
						value! { { ref_time: 1u64, proof_size: 1u64 } },
					],
				);
				para_client_v3
					.tx()
					.sign_and_submit_then_watch_default(&rollback_call, &dev::alice())
					.await?
					.wait_for_finalized_success()
					.await?;
			}
			wait_for_runtime_upgrade(&para_client_v3).await?;
			// One more distinct validation code: 2902's V3-disabled runtime. Only at offset 0 —
			// the offset-2 rollback restores the genesis code, whose artifact the validators
			// already hold, so the concluded-prepares metric never advances.
			if relay_parent_offset == 0 {
				wait_for_pvf_prepare(&network, 4).await?;
			}
			// One session is enough here, unlike the relay arm's `SESSION_DELAY`: this is not a
			// session-gated config change, and `wait_for_runtime_upgrade` above has already seen
			// the code-swap block finalized, so every relay parent from the next session on is
			// past it.
			let post_rollback_session = current_session_index(&relay_client).await? + 1;
			assert_candidates_version(
				&relay_client,
				CandidateDescriptorVersion::V2,
				HashMap::from([
					(para_2900, SINGLE_CORE_CANDIDATES),
					(para_2901, ELASTIC_CANDIDATES),
					(para_2902, para_2902_candidates()),
				]),
				10,
				Some(post_rollback_session),
			)
			.await?;
		},
	}

	assert_validator_backed_candidates(relay_node, 30).await?;
	for i in 4..=9 {
		let node = network.get_node(format!("validator-{i}"))?;
		assert_validator_backed_candidates(node, 30).await?;
	}

	assert_finality_lag(&para_node.wait_client().await?, 6).await?;
	assert_finality_lag(&para_node_slot.wait_client().await?, 15).await?;
	// 2902 finalizes further behind once its relay parent trails the tip, so at any non-zero offset
	// allow the 15 every other offset-2 test in this file already uses.
	assert_finality_lag(
		&para_node_v3.wait_client().await?,
		if relay_parent_offset > 0 { 15 } else { 6 },
	)
	.await?;

	log::info!("V3 dynamic enablement test ({rollback:?} rollback) finished successfully");

	Ok(())
}

/// Full V2 -> V3 -> V2 cycle for a parachain at `relay_parent_offset == 2`.
///
/// The relay keeps the `CandidateReceiptV3` feature on throughout, and the offset stays 2 across
/// all three phases, so the only thing that ever changes is the para's `V3_SCHEDULING_ENABLED`
/// const:
///
///   `relay_parent_offset` (v3 off) -> `v3_rpo_2` (v3 on) -> `relay_parent_offset` (v3 off)
///
/// Both flavours share a spec_version, so each swap goes through `set_code_without_checks`, and
/// `wait_for_runtime_upgrade` keys off the `RuntimeEnvironmentUpdated` digest rather than the
/// version, so it still fires.
///
/// Why the offset matters: at offset 0 the inherent's relay-parent-descendant check is inert
/// (`expected_rp_descendants_num == 0` short-circuits it), so a collator whose view of the const
/// lags the executing runtime only shows up later in the PVF. At offset 2 the check is armed and
/// the same disagreement is caught in `set_validation_data` instead — the collator omits the
/// descendants a v2-mode runtime demands. Both transitions are exercised here because the window
/// exists in each direction.
#[tokio::test(flavor = "multi_thread")]
async fn v3_para_full_cycle_with_relay_parent_offset() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// v3 on, offset 2 — the enablement target.
	let v3_rpo2_wasm = cumulus_test_runtime::v3_rpo_2::WASM_BINARY
		.ok_or_else(|| anyhow!("cumulus-test-runtime v3_rpo_2 WASM not built"))?;
	// v3 off, offset 2 — genesis, and the rollback target.
	let v3_disabled_rpo2_wasm = cumulus_test_runtime::relay_parent_offset::WASM_BINARY
		.ok_or_else(|| anyhow!("cumulus-test-runtime relay_parent_offset WASM not built"))?;

	// V2 (bit 3) and V3 (bit 4) on from genesis and never touched, so the relay side is constant.
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
							// Offset 2 is paired with 1 in `scheduling_v3.rs`, which is green in
							// CI. A value of 2 pushes the scheduling parent out of
							// prospective-parachains' scope and every candidate is dropped with
							// `SchedulingParentNotInScope`.
							"max_relay_parent_session_age": 1
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
		// Para 2902: starts v3-disabled at offset 2.
		.with_parachain(|p| {
			p.with_id(2902)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("relay-parent-offset")
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
	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;

	let para_2902 = ParaId::from(2902);

	// Swapping between two runtimes that share a spec_version needs the unchecked call.
	let set_code = |wasm: &'static [u8]| {
		dynamic(
			"Sudo",
			"sudo_unchecked_weight",
			vec![
				value! { System(set_code_without_checks { code: Value::from_bytes(wasm) }) },
				value! { { ref_time: 1u64, proof_size: 1u64 } },
			],
		)
	};

	// Phase 1 — on/off: relay feature on, para v3 off, offset 2. Plain V2 at an offset.
	//
	// Ranges here and below are deliberately wide: this pairing has no CI baseline, and V3
	// throughput in particular is still low and jittery (paritytech/polkadot-sdk#10836, #11903).
	log::info!("phase 1: on/off (relay on, para v3 off, offset 2) → V2");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2902, 10..41)]),
		40,
		None,
	)
	.await?;

	// Phase 2 — on/on: enable v3 on the para, offset unchanged.
	log::info!("phase 2: on/on (relay on, para v3 on, offset 2) → V3");
	para_client
		.tx()
		.sign_and_submit_then_watch_default(&set_code(v3_rpo2_wasm), &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_runtime_upgrade(&para_client).await?;
	// Two distinct validation codes so far: genesis (v3 off) and the v3 one.
	wait_for_pvf_prepare(&network, 2).await?;
	let post_enable_session = current_session_index(&relay_client).await? + 1;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(para_2902, 15..30)]),
		40,
		Some(post_enable_session),
	)
	.await?;

	// Phase 3 — back to on/off: roll the para off v3 again, offset still 2.
	log::info!("phase 3: on/off (relay on, para rolled back off v3, offset 2) → V2");
	para_client
		.tx()
		.sign_and_submit_then_watch_default(&set_code(v3_disabled_rpo2_wasm), &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_runtime_upgrade(&para_client).await?;
	// No `wait_for_pvf_prepare` here: this restores the genesis code, whose artifact the
	// validators already hold, so the concluded-prepares metric never advances past 2.
	let post_rollback_session = current_session_index(&relay_client).await? + 1;
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2902, 10..41)]),
		40,
		Some(post_rollback_session),
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

	assert_finality_lag(&para_client, 15).await?;

	log::info!("V3 para full cycle at relay parent offset 2 finished successfully");

	Ok(())
}

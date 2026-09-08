// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V4 collator-protocol + elastic scaling: first-core-only resubmission across sessions.
//!
//! The collator-side V4 work routes the parachain's unincluded segment exclusively through the
//! first of its assigned cores. The remaining cores ship only freshly built bundles. The
//! resubmission of historical entries on that single dedicated core is what keeps the parachain
//! progressing across relay-chain session boundaries: when the validator set rotates, the unbacked
//! tail of the unincluded segment must be re-advertised to the new backers, or block production
//! stalls.
//!
//! Set-up: rococo-local, 6 validators all on `--experimental-collator-protocol` (so V4 wire
//! advertisements are exercised), one elastic-scaled parachain assigned 3 cores. The test waits
//! for the **third** session change (the first session typically passes with the parachain not
//! yet producing — core assignment + collator warm-up), then asserts throughput and finality
//! across the subsequent window.
//!
//! Negative variant (manual): comment out the first-core `unincluded_segment` clone in
//! `cumulus/client/consensus/aura/src/collators/slot_based/block_builder_task.rs` so no core ships
//! the historical entries, rebuild the test-parachain binary, and re-run this test. The
//! `assert_para_throughput` and `assert_finality_lag` calls should fail — the parachain stalls
//! once it crosses a session boundary with unincluded blocks still pending.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_per_core_across_sessions() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=debug,parachain::statement-distribution=debug,parachain::prospective-parachains=trace").into(),
					"--experimental-collator-protocol".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// 2 extra cores beyond the auto-registered parachain core →
								// 3 cores total once we `assign_cores` cores 0 and 1.
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								// Prospective-parachains scope = (active leaf + `lookahead - 1`
								// ancestors). For elastic-scaling-v3 the unincluded-segment
								// capacity is `velocity-3 * (3 + 0) = 9` blocks, so the scope
								// must cover at least 9 ancestors or any entry anchored to an
								// older relay parent gets `SchedulingParentNotInScope` and is
								// silently dropped by the validator. Default is 1, which only
								// admits candidates anchored to the leaf itself.
								"lookahead": 5
							},
							// Enable V2 (bit 3) + V3 (bit 4) candidate descriptors so the
							// elastic-scaling-v3 runtime receives the V3 `ValidationParamsExtension`
							// its PVF requires (otherwise `validate_v3_scheduling` panics with
							// "V3 scheduling is enabled but no V3 extension present").
							"node_features": {"bits": 8, "data": [0b00011000]},
							// Allow relay parents from up to 2 sessions ago. The V3 resubmission
							// path relies on this to keep an unincluded block's original
							// relay parent valid after a session boundary lands.
							"max_relay_parent_session_age": 10,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3-rpo")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug").into(),
					"--authoring=slot-based".into(),
					// Keep the collator's embedded relay-chain client in archive mode so all
					// relay blocks (and their state) remain available for post-hoc analysis.
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
				])
				.with_collator(|n| n.with_name("collator-2900"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator-2900")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Elastic-scale to 3 cores (auto-registered core + 0 + 1).
	assign_cores(&relay_client, 2900, vec![0, 1]).await?;
	log::info!("Para 2900 elastic-scaled to 3 cores");

	// Count backed candidates over a window long enough to span ~3 sessions of relay-chain
	// activity (sessions are ~10 RC blocks each here). The helper internally waits for the
	// first session change + first backed candidate before counting starts, which absorbs the
	// `assign_cores`/PVF warm-up window; the rest of the window then exercises the
	// resubmit-on-first-core path across validator-set rotations. With 3 cores at the
	// elastic-scaling-v3 throughput target, ~3 backed candidates per relay block.
	assert_para_throughput(&relay_client, 100, [(ParaId::from(2900), 210..310)], []).await?;

	// Finality must keep up — a stalled parachain would lag finality unboundedly.
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 15).await?;

	log::info!("V4 first-core resubmit across sessions test finished successfully");
	Ok(())
}

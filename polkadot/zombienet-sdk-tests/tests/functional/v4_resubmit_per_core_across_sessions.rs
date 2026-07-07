// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V4 collator-protocol + elastic scaling: reserved-capacity resubmission across sessions.
//!
//! The parachain runs the `elastic-scaling-v3-reserved-core` runtime (`MaxCores = 3`) and is
//! assigned 4 cores. The runtime caps *fresh* production at `MaxCores` distinct core selectors per
//! relay slot, so it touches at most 3 cores; the remaining core's backing capacity is left to
//! drain the unincluded segment. The collator splits the backlog across all assigned cores so it
//! keeps progressing across relay-chain session boundaries: when the validator set rotates, the
//! unbacked tail of the unincluded segment is re-advertised to the new backers, or block
//! production stalls.
//!
//! Set-up: rococo-local, 6 validators all on `--experimental-collator-protocol` (so V4 wire
//! advertisements are exercised), one parachain assigned 4 cores. `target_block_rate` is 3 and
//! `MaxCores` is 3, so fresh production is one block per core across 3 cores (no block-bundling);
//! the 4th core is reserved capacity. In steady state 3 cores suffice, but after a session change
//! the blocks built before the validator-set rotation must be re-advertised to the new backers,
//! and the 4th core drains that backlog — that's the behaviour to watch. The test logs Polkadot-JS
//! links and sleeps for an hour for manual inspection before the throughput/finality checks.

use std::time::Duration;

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_first_core_across_sessions() -> Result<(), anyhow::Error> {
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
								// 3 extra cores beyond the auto-registered parachain core →
								// 4 cores total once we `assign_cores` cores 0, 1 and 2.
								"num_cores": 3,
								"max_validators_per_core": 1,
								"group_rotation_frequency": 40,
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
							"max_relay_parent_session_age": 5,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2901)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3-reserved-core")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug").into(),
					"--authoring=slot-based".into(),
					// Keep the collator's embedded relay-chain client in archive mode so all
					// relay blocks (and their state) remain available for post-hoc analysis.
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
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
	let collator_node = network.get_node("collator-2901")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Log Polkadot-JS Apps links for manual inspection: the relay validator and the parachain
	// collator. Open these in a browser to watch the chain during the sleep at the end.
	let pjs_link = |ws: &str| -> String {
		format!(
			"https://polkadot.js.org/apps/?rpc={}#/explorer",
			ws.replace(':', "%3A").replace('/', "%2F"),
		)
	};
	log::info!(
		"validator-0 RPC {} | Polkadot-JS {}",
		relay_node.ws_uri(),
		pjs_link(relay_node.ws_uri())
	);
	log::info!(
		"collator-2901 RPC {} | Polkadot-JS {}",
		collator_node.ws_uri(),
		pjs_link(collator_node.ws_uri()),
	);

	// 4 cores total (auto-registered core + 0 + 1 + 2): 3 for fresh production (MaxCores = 3) and
	// one of reserved capacity to drain resubmissions across session boundaries.
	assign_cores(&relay_client, 2901, vec![0, 1, 2]).await?;
	log::info!("Para 2901 assigned 4 cores (3 fresh + 1 reserved capacity)");

	// Keep the network alive for an hour for manual inspection via the Polkadot-JS links / logs
	// above. Placed BEFORE the assertions so the network stays up even when they would fail.
	tokio::time::sleep(Duration::from_secs(3600)).await;

	// Count backed candidates over a window spanning several sessions. The helper internally waits
	// for the first session change + first backed candidate before counting, which absorbs the
	// `assign_cores`/PVF warm-up. The range is wide for this first calibration run; tighten once
	// the steady-state throughput is known.
	assert_para_throughput(&relay_client, 100, [(ParaId::from(2901), 100..360)], []).await?;

	// Finality must keep up — a stalled parachain would lag finality unboundedly.
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 10).await?;

	log::info!("V4 reserved-capacity resubmit across sessions test finished successfully");

	Ok(())
}

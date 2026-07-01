// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! V4 collator-protocol + resubmission under **glutton load**: each parablock carries a large
//! uncompressable PoV, so the collation-generation segment build
//! (`handle_submit_segment` → `construct_receipt` → erasure×N) runs at production-scale PoV sizes.
//! This test exists to *measure* that build cost, not to assert throughput.
//!
//! Glutton's `blockLength` knob stuffs the block body with high-entropy `twox_256` trash (see
//! `pallet_glutton::gen_value`) — uncompressable, so the PoV does not shrink under zstd. The relay
//! `max_pov_size` is raised to 12 MiB (the hard limit is 16 MiB) to admit the big PoVs.
//! `trashDataCount` is kept above the per-block trash demand so the `bloat` inherent never cycles
//! and repeats entries (repeats would be compressible).
//!
//! Read the cost from the collator logs:
//!   - `Compressed PoV size: …kb` (target `aura::cumulus::collation_task`) — confirms PoV size;
//!     tune `blockLength` toward `1.0` to land near 10 MiB.
//!   - `Built segment receipts … count=… total_pov_bytes=… elapsed=…` (target
//!     `parachain::collation-generation`) — the erasure×N build time at large PoVs. The
//!     per-candidate cost is the `count`-vs-`elapsed` slope (and `count=1` samples give it
//!     directly).
//!
//! Note: a parked bug (non-leading bundle blocks dropped at resubmit time) caps built segments at
//! the per-bundle leaders, so `count` stays small — that does not affect the per-candidate erasure
//! measurement.

use std::time::Duration;

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assign_cores;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_resubmit_three_collators_glutton() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Per-collator args: zombienet-sdk's `with_args` overrides `with_default_args`, so each
	// collator repeats the parachain log filter and `--authoring=slot-based`.
	fn collator_args(keyring_flag: &'static str) -> Vec<zombienet_sdk::Arg> {
		vec![
			"-lparachain=debug,aura=debug,aura::cumulus=trace,basic-authorship=debug,sync=debug,sync::import-queue=debug,sc_consensus::block_import=debug,cumulus_client_consensus_common=debug,parachain::collator-protocol=trace".into(),
			"--authoring=slot-based".into(),
			keyring_flag.into(),
			"--".into(),
			"--state-pruning=archive".into(),
			"--blocks-pruning=archive".into(),
			("--network-backend=libp2p").into(),
		]
	}

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=debug,parachain::statement-distribution=debug,parachain::prospective-parachains=trace").into(),
					"--experimental-collator-protocol".into(),
					("--network-backend=libp2p").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 1,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							"node_features": {"bits": 8, "data": [0b00011000]},
							"max_relay_parent_session_age": 10,
							// Raised from the 5 MiB default to admit ~10 MiB PoVs (hard limit 16 MiB).
							"max_pov_size": 12582912
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..2).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3-rpo")
				// Fill each block body with ~9 MiB of uncompressable trash. `trashDataCount`
				// (12000 × 1 KiB = ~12 MiB pool) stays above the per-block demand
				// (0.9 × 10 MiB / 1 KiB ≈ 9216 entries) so the `bloat` inherent never repeats.
				.with_genesis_overrides(json!({
					"glutton": {
						"compute": "0",
						"storage": "0",
						"blockLength": "0.9",
						"trashDataCount": 12000
					}
				}))
				.with_collator(|n| {
					n.with_name("collator-alice").with_args(collator_args("--alice"))
				})
				.with_collator(|n| {
					n.with_name("collator-bob").with_args(collator_args("--bob"))
				})
				.with_collator(|n| {
					n.with_name("collator-charlie").with_args(collator_args("--charlie"))
				})
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
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assign_cores(&relay_client, 2900, vec![0, 1]).await?;
	log::info!("Para 2900 on multiple cores with glutton large uncompressable PoVs");

	// Run for 10 minutes, then stop. Glutton warms up on its own from genesis.
	tokio::time::sleep(Duration::from_secs(600)).await;

	// Point at the logs for the collector one-liner.
	let base = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.unwrap_or_else(|_| "<this run's zombie-<uuid> temp dir>".to_string());
	log::info!(
		"glutton measurement done after 10 min. Collator logs: \
		 {base}/collator-{{alice,bob,charlie}}/collator-*.log — grep 'Built segment receipts' \
		 (count / total_pov_bytes / elapsed) and 'Compressed PoV size' (per-candidate PoV)."
	);
	Ok(())
}

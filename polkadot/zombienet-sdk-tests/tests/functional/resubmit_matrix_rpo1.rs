// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Exploratory throughput matrix over validator side × candidate-descriptor version.
//!
//! All four cells run the same network: rococo-local with 6 validators, one parachain (2900)
//! elastic-scaled to 3 cores, a single slot-based collator and relay parent offset 1 (lookahead
//! 5, max_validators_per_core 2, group_rotation_frequency 4, libp2p network backend everywhere,
//! 100-block window). Cells vary only
//! the validator-side collator protocol (classic vs `--experimental-collator-protocol`) and the
//! descriptor version the collator emits (V2 = node-feature bit 3 on the plain flavour, V3 =
//! bits 3+4 on the `-v3-` flavour). Ranges are wide — measurement, not a gate.
//!
//! A fifth diagnostic case, exp_v3_rpo2, re-runs the experimental V3 cell at relay parent
//! offset 2 (`elastic-scaling-v3-rpo`, segment capacity V×(4+RPO) = 18) to test whether the depth-9
//! production clipping observed at RPO 1 is offset-bound or constant-headroom.
//!
//! A sixth diagnostic case, experimental_v3_rpo0, runs the experimental V3 cell at relay
//! parent offset 0 (`elastic-scaling-v3`, segment capacity V×(4+RPO) = 12): together with the constant
//! headroom result above, any throughput change vs the RPO-1 cell attributes to the
//! validator-side PP-knowledge staleness fix, not the offset.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use rstest::rstest;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	Arg, NetworkConfigBuilder,
};

#[rstest]
#[case::classic_v2(false, "elastic-scaling-rpo-1", 0b00001000u8)]
#[case::classic_v3(false, "elastic-scaling-v3-rpo-1", 0b00011000u8)]
#[case::experimental_v2(true, "elastic-scaling-rpo-1", 0b00001000u8)]
#[case::experimental_v3(true, "elastic-scaling-v3-rpo-1", 0b00011000u8)]
#[case::exp_v3_rpo2(true, "elastic-scaling-v3-rpo", 0b00011000u8)]
#[case::experimental_v3_rpo0(true, "elastic-scaling-v3", 0b00011000u8)]
#[tokio::test(flavor = "multi_thread")]
async fn resubmit_matrix_rpo1(
	#[case] experimental: bool,
	#[case] chain: &'static str,
	#[case] node_features: u8,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let mut relay_args: Vec<Arg> = vec![
		("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace,parachain::candidate-backing=debug,parachain::statement-distribution=debug,parachain::prospective-parachains=trace").into(),
		"--network-backend=libp2p".into(),
	];
	if experimental {
		relay_args.push("--experimental-collator-protocol".into());
	}

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(relay_args)
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4,
								"lookahead": 5
							},
							// Bit 3 = V2 descriptors; bit 4 adds V3. Set per matrix cell.
							"node_features": {"bits": 8, "data": [node_features]},
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
			let p = p
				.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain(chain)
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,parachain::collator-protocol=trace,aura::cumulus=debug,basic-authorship=debug").into(),
					"--authoring=slot-based".into(),
					"--network-backend=libp2p".into(),
					"--".into(),
					"--state-pruning=archive".into(),
					"--blocks-pruning=archive".into(),
					"--network-backend=libp2p".into(),
				]);
			p.with_collator(|n| n.with_name("collator-2900"))
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
	let collator_node = network.get_node("collator-2900")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assign_cores(&relay_client, 2900, vec![0, 1]).await?;
	log::info!("Para 2900 elastic-scaled to 3 cores (experimental={experimental}, chain={chain})");

	// Same 100-block window as the v4 suite; wide range — measurement, not a gate.
	assert_para_throughput(&relay_client, 100, [(ParaId::from(2900), 50..311)], []).await?;

	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;
	assert_finality_lag(&collator_client, 30).await?;

	log::info!("Matrix cell finished (experimental={experimental}, chain={chain})");
	Ok(())
}

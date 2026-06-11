// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn approved_peer_mixed_collators_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let col_image =
		std::env::var("COL_IMAGE").unwrap_or("docker.io/paritypr/colander:latest".to_string());
	let old_col_image = std::env::var("OLD_COL_IMAGE")
		.unwrap_or("docker.io/paritypr/colander:master-fe011353".to_string());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("undying-collator")
				.with_default_image(col_image.as_str())
				.cumulus_based(false)
				// Recent collator: sends the ApprovedPeer UMPSignal.
				.with_collator(|n| {
					n.with_name("collator-recent").with_image(col_image.as_str()).with_args(vec![
						("-lparachain=debug").into(),
						("--experimental-send-approved-peer").into(),
					])
				})
				// Old collator (v1.17.0-rc5): doesn't send the ApprovedPeer UMPSignal.
				.with_collator(|n| {
					n.with_name("collator-old")
						.with_image(old_col_image.as_str())
						.with_args(vec![("-lparachain=debug").into()])
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

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// The parachain should keep producing blocks at a healthy rate, regardless of which collator
	// (recent or old) authored a given block.
	assert_para_throughput(&relay_client, 15, [(ParaId::from(2000), 10..16)], []).await?;

	// Finality should not be affected by the mixed collator fleet.
	assert_finality_lag(&relay_node.wait_client().await?, 5).await?;

	// Check that no disputes are raised.
	assert!(relay_node
		.wait_metric_with_timeout("polkadot_parachain_candidate_disputes_total", |v| v > 0.0, 30u64)
		.await
		.is_err());

	relay_node
		.assert("polkadot_parachain_approval_checking_finality_lag", 0.0)
		.await?;

	relay_node.assert("polkadot_parachain_disputes_finality_lag", 0.0).await?;

	log::info!("Test finished successfully");

	Ok(())
}

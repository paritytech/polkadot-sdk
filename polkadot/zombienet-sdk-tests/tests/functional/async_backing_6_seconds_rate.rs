// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test we are producing 12-second parachain blocks if using an old collator, pre async-backing.

use anyhow::anyhow;

use crate::helpers::{assert_finalized_block_height, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn async_backing_6_seconds_rate_test() -> Result<(), anyhow::Error> {
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
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
								"lookahead": 2,
								"max_candidate_depth": 3,
								"allowed_ancestry_len": 2,
							},
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..12)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("adder-collator")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.cumulus_based(false)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-adder-2000"))
		})
		.with_parachain(|p| {
			p.with_id(2001)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2001"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node_2001 = network.get_node("collator-2001")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assert_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(2000), 11..16), (ParaId::from(2001), 11..16)]
			.into_iter()
			.collect(),
	)
	.await?;

	// Assert the parachain finalized block height is also on par with the number of backed
	// candidates. We can only do this for the collator based on cumulus.
	assert_finalized_block_height(&para_node_2001.wait_client().await?, 10..16).await?;

	log::info!("Test finished successfully");

	Ok(())
}

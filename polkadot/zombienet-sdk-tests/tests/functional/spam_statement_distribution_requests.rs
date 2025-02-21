// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test if parachains progress when group is getting spammed by statement distribution requests.

use anyhow::anyhow;

use crate::helpers::{assert_finalized_block_height, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn spam_statement_distribution_requests_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"needed_approvals": 2,
							"scheduler_params": {
								"max_validators_per_core": 5
							}
						}
					}
				}))
				.with_node(|node| node.with_name("honest-0"))
				.with_node(|node| node.with_name("honest-1"))
				.with_node(|node| node.with_name("honest-2"))
				.with_node(|node| node.with_name("honest-3"))
				.with_node(|node| {
					node.with_name("malus")
						.with_image(
							std::env::var("MALUS_IMAGE")
								.unwrap_or("docker.io/paritypr/malus".to_string())
								.as_str(),
						)
						.with_command("malus")
						.with_subcommand("spam-statement-requests")
						.with_args(vec![
							"--alice".into(),
							"--spam-factor=1000".into(),
							"-lMALUS=trace,parachain=debug".into(),
						])
				})
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("undying-collator")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-2000"))
		})
		.with_parachain(|p| {
			p.with_id(2001)
				.with_default_command("undying-collator")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-2001"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	Ok(())
}

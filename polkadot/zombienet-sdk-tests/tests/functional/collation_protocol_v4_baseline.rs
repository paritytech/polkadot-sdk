// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn v4_collation_protocol_baseline() -> Result<(), anyhow::Error> {
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
					("-lparachain=debug,parachain::collator-protocol=trace".into()),
					("--experimental-collator-protocol".into()),
					("--network-backend=libp2p").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 4
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));
			let r = (1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});
			r
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					("-lparachain=debug,aura=debug".into()),
					("--network-backend=libp2p").into(),
					("--").into(),
					("--network-backend=libp2p").into(),
				])
				.with_collator(|node| node.with_name("collator-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator-2000")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assert_para_throughput(&relay_client, 10, [(ParaId::from(2000), 5..11)], []).await?;
	let opts = LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(5), false);
	collator_node
		.wait_log_line_count_with_timeout(
			"DistributeSegment for para_id: 2000",
			false,
			opts.clone(),
		)
		.await?
		.success()
		.then_some(())
		.ok_or_else(|| anyhow!("DistributeSegment received not found"))?;
	for i in 0..4 {
		let v = network.get_node(&format!("validator-{i}"))?;
		v.wait_log_line_count_with_timeout(
			"Received an segment advertisement",
			false,
			opts.clone(),
		)
		.await?
		.success()
		.then_some(())
		.ok_or_else(|| anyhow!("Received an segment advertisement not found"))?;
	}
	Ok(())
}

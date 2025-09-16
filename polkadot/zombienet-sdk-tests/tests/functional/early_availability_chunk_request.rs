// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that nodes fetch availability chunks early for scheduled cores and normally for occupied
// core.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn early_availability_chunk_request_test() -> Result<(), anyhow::Error> {
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
				.with_default_args(vec![("-lavailability=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
						 "scheduler_params": {
							 "num_cores": 2,
							 "group_rotation_frequency": 4
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));
			(1..3).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_collator(|n| n.with_name("collator-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;

	// Wait for some parachain blocks to be produced so metrics endpoint is up and candidates have
	// been processed.
	let relay_client = relay_node.wait_client().await?;
	assert_para_throughput(&relay_client, 12, [(ParaId::from(2000), 6..14)].into_iter().collect())
		.await?;

	let early = relay_node.reports("polkadot_parachain_early_candidates_fetched_total").await?;
	let slow = relay_node.reports("polkadot_parachain_slow_candidates_fetched_total").await?;
	let never = relay_node
		.reports("polkadot_parachain_early_candidates_never_onchain_total")
		.await?;

	log::info!("Early candidates fetched: {early}");
	log::info!("Slow candidates fetched: {slow}");
	log::info!("Early candidates never onchain: {never}");

	assert!(early > 0.into(), "Expected early candidates fetched > 0");
	assert!(slow > 0.into(), "Expected slow candidates fetched > 0");

	Ok(())
}

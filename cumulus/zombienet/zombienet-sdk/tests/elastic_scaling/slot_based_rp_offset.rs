// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that parachains that use a single slot-based collator with elastic scaling MVP and with
// elastic scaling with RFC103 can achieve full throughput of 3 candidates per block.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, create_assign_core_call,
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn slot_based_relay_parent_offset_test() -> Result<(), anyhow::Error> {
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
								// Num cores is 4, because 2 extra will be added automatically when registering the paras.
								"num_cores": 4,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2400)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("relay-parent-offset")
				.with_default_args(vec![
					"--authoring=slot-based".into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-rp-offset"))
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
	let para_node_rp_offset = network.get_node("collator-rp-offset")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();

	let assign_cores_call = create_assign_core_call(&[(0, 2400), (1, 2400)]);
	// Assign two extra cores to each parachain.
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_cores_call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("2 more cores assigned to the parachain");

	// Expect a backed candidate count of at least 39 for each parachain in 15 relay chain blocks
	// (2.6 candidates per para per relay chain block).
	// Note that only blocks after the first session change and blocks that don't contain a session
	// change will be counted.
	assert_para_throughput(&relay_client, 15, [(ParaId::from(2400), 39..46)].into_iter().collect())
		.await?;

	// Assert the parachain finalized block height is also on par with the number of backed
	// candidates.
	assert_finality_lag(&para_node_rp_offset.wait_client().await?, 15).await?;

	log::info!("Test finished successfully");

	Ok(())
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that a parachain that uses a collator set which builds both V1 and V2 receipts cannot fully
// utilise elastic scaling but can still make some progress.

use anyhow::anyhow;

use super::{
	helpers::assert_para_throughput,
	rococo,
	rococo::runtime_types::{
		pallet_broker::coretime_interface::CoreAssignment,
		polkadot_runtime_parachains::assigner_coretime::PartsOf57600,
	},
};
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn mixed_receipt_versions_test() -> Result<(), anyhow::Error> {
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
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// Num cores is 2, because 1 extra will be added automatically when registering the paras.
								"num_cores": 2,
								"max_validators_per_core": 1
							},
							"async_backing_params": {
								"max_candidate_depth": 6,
								"allowed_ancestry_len": 2
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..3).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2200)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling")
				.with_default_args(vec![("--experimental-use-slot-based").into()])
				// This collator uses the image from the PR, which will build a v2 receipt.
				.with_collator(|n| n.with_name("collator-elastic"))
				// This collator uses an old image, which will build a v1 receipt.
				// The image is hardcoded to roughly where the stable2407 was branched off.
				.with_collator(|n| {
					n.with_name("old-collator-elastic")
						.with_image("docker.io/paritypr/test-parachain:master-b862b181")
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
	let alice = dev::alice();

	// Assign two extra cores to the parachain.
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(
			&rococo::tx()
				.sudo()
				.sudo(rococo::runtime_types::rococo_runtime::RuntimeCall::Utility(
					rococo::runtime_types::pallet_utility::pallet::Call::batch {
						calls: vec![
							rococo::runtime_types::rococo_runtime::RuntimeCall::Coretime(
								rococo::runtime_types::polkadot_runtime_parachains::coretime::pallet::Call::assign_core {
									core: 0,
									begin: 0,
									assignment: vec![(CoreAssignment::Task(2200), PartsOf57600(57600))],
									end_hint: None
								}
							),
							rococo::runtime_types::rococo_runtime::RuntimeCall::Coretime(
								rococo::runtime_types::polkadot_runtime_parachains::coretime::pallet::Call::assign_core {
									core: 1,
									begin: 0,
									assignment: vec![(CoreAssignment::Task(2200), PartsOf57600(57600))],
									end_hint: None
								}
							),
						],
					},
				)),
			&alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("2 more cores assigned to the parachain");

	// We won't get full throughput as some candidates would be built for the same block and
	// therefore dropped in the runtime.
	// The perfect throughput would have been 45 for a parachain with 3 cores over 15 relay chain
	// blocks.
	assert_para_throughput(&relay_client, 15, [(2200, 25..37)].into_iter().collect()).await?;

	log::info!("Test finished successfully");

	Ok(())
}

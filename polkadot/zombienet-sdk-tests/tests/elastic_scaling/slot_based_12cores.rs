// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that a parachain that uses a single slot-based collator with elastic scaling can use 12
// cores in order to achieve 500ms blocks.

use anyhow::anyhow;

use crate::helpers::{
	assert_finalized_block_height, assert_para_throughput, rococo,
	rococo::runtime_types::{
		pallet_broker::coretime_interface::CoreAssignment,
		polkadot_runtime_parachains::assigner_coretime::PartsOf57600,
	},
};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn slot_based_12cores_test() -> Result<(), anyhow::Error> {
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
								"num_cores": 11,
								"max_validators_per_core": 1
							},
							"async_backing_params": {
								"max_candidate_depth": 24,
								"allowed_ancestry_len": 2
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..12)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2300)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-500ms")
				.with_default_args(vec![
					("--experimental-use-slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-elastic"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-elastic")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();

	// Assign 11 extra cores to the parachain.

	relay_client
		.tx()
		.sign_and_submit_then_watch_default(
			&rococo::tx()
				.sudo()
				.sudo(rococo::runtime_types::rococo_runtime::RuntimeCall::Utility(
					rococo::runtime_types::pallet_utility::pallet::Call::batch {
						calls: (0..11).map(|idx| rococo::runtime_types::rococo_runtime::RuntimeCall::Coretime(
                            rococo::runtime_types::polkadot_runtime_parachains::coretime::pallet::Call::assign_core {
                                core: idx,
                                begin: 0,
                                assignment: vec![(CoreAssignment::Task(2300), PartsOf57600(57600))],
                                end_hint: None
                            }
                        )).collect()
					},
				)),
			&alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("11 more cores assigned to the parachain");

	// Expect a backed candidate count of at least 170 in 15 relay chain blocks
	// (11.33 candidates per para per relay chain block).
	// Note that only blocks after the first session change and blocks that don't contain a session
	// change will be counted.
	assert_para_throughput(
		&relay_client,
		15,
		[(ParaId::from(2300), 170..181)].into_iter().collect(),
	)
	.await?;

	// Assert the parachain finalized block height is also on par with the number of backed
	// candidates.
	assert_finalized_block_height(&para_node.wait_client().await?, 158..181).await?;

	log::info!("Test finished successfully");

	Ok(())
}

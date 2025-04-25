// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::assert_para_throughput;

use polkadot_primitives::Id as ParaId;
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_configuration::types::Arg;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder, RegistrationStrategy};

const PARA_ID: u32 = 2000;
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

#[tokio::test(flavor = "multi_thread")]
async fn pov_recovery() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network with relay chain only");
	let mut network = setup_network_with_relaychain_only().await?;

	let validator_3 = network.get_node("validator-3")?;

	log::info!("Waiting 20 blocks to register parachain");
	// Wait 20 blocks and register parachain. This part is important for pov-recovery.
	// We need to make sure that the recovering node is able to see all relay-chain
	// notifications containing the candidates to recover.
	assert!(validator_3
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 20.0, 250u64)
		.await
		.is_ok());

	log::info!("Registering parachain para_id = {PARA_ID}");
	network.register_parachain(PARA_ID).await?;

	let relay_ferdie = network.get_node("ferdie")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_ferdie.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 2..15)].into_iter().collect(),
	)
	.await?;

	log::info!("Checking block production");
	for (name, timeout_secs) in [
		("bob", 600u64),
		("alice", 600u64),
		("charlie", 600u64),
		("one", 800u64),
		("two", 800u64),
		// Re-enable once we upgraded from smoldot 0.11.0 and https://github.com/paritytech/polkadot-sdk/pull/1631 is merged
		// ("three", 800u64),
		("eve", 800u64),
	] {
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 20.0, timeout_secs)
			.await
			.is_ok());
	}

	log::info!("Ensuring blocks are imported using PoV recovery");
	for name in ["one", "two", "three", "eve", "charlie", "alice"] {
		assert!(network
			.get_node(name)?
			.wait_log_line_count_with_timeout(
				"Importing blocks retrieved using pov_recovery",
				false,
				20,
				10u64,
			)
			.await
			.is_ok());
	}

	Ok(())
}

async fn setup_network_with_relaychain_only() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

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
									"max_validators_per_core": 1,
									"group_rotation_frequency": 100
								}
							}
						}
				}))
				.with_node(|node| node.with_name("ferdie").validator(false));

			(0..13).fold(r, |acc, i| {
				acc.with_node(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain::availability=trace,sync=debug,parachain=debug").into(),
						("--reserved-only").into(),
						("--reserved-nodes", "{{ZOMBIE:ferdie:multiaddr}}").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				// do not register parachain when network is spawned, it will be registered later on
				.with_registration_strategy(RegistrationStrategy::Manual)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				// run 'bob' as a parachain collator who is the only one producing blocks
				// 'alice' and 'charlie' will need to recover the pov blocks through availability recovery
				.with_collator(|c| {
					c.with_name("bob")
						.with_p2p_port(55915)
						.validator(true)
						.with_args(vec!["--disable-block-announcements".into()])
				})
				// run 'alice' as a parachain collator which does not produce blocks
				.with_collator(|c| {
					c.with_name("alice")
						.validator(true)
						.with_args(build_collator_args(vec!["--use-null-consensus".into()]))
				})
				// run 'charlie' as a parachain full node
				.with_collator(|c| {
					c.with_name("charlie").validator(false).with_args(build_collator_args(vec![]))
				})
				// we fail recovery for 'eve' from time to time to test retries
				.with_collator(|c| {
					c.with_name("eve").validator(true).with_args(build_collator_args(vec![
						"--fail-pov-recovery".into(),
						"--use-null-consensus".into(),
					]))
				})
				// run 'one' as a RPC collator which does not produce blocks
				.with_collator(|c| {
					c.with_name("one").validator(true).with_args(build_collator_args(vec![
						"--use-null-consensus".into(),
						("--relay-chain-rpc-url", "{{ZOMBIE:ferdie:ws_uri}}").into(),
					]))
				})
				// run 'two' as a RPC parachain full node
				.with_collator(|c| {
					c.with_name("two").validator(false).with_args(build_collator_args(vec![(
						"--relay-chain-rpc-url",
						"{{ZOMBIE:ferdie:ws_uri}}",
					)
						.into()]))
				})
				// run 'three' with light client
				.with_collator(|c| {
					c.with_name("three")
						.validator(false)
						.with_args(build_collator_args(vec!["--relay-chain-light-client".into()]))
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

	// Spawn network
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	Ok(network)
}

fn build_collator_args(in_args: Vec<Arg>) -> Vec<Arg> {
	let start_args: Vec<Arg> = vec![
		("-lparachain::availability=trace,sync=debug,parachain=debug,cumulus-pov-recovery=debug,cumulus-consensus=debug").into(),
		("--in-peers=0").into(),
		("--out-peers=0").into(),
		("--bootnodes", "{{ZOMBIE:bob:multiaddr}}").into(),
	];

	let remaining_args: Vec<Arg> = vec![
		("--").into(),
		("--reserved-only").into(),
		("--reserved-nodes", "{{ZOMBIE:ferdie:multiaddr}}").into(),
	];

	let args = [start_args, in_args, remaining_args].concat();
	args
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::utils::BEST_BLOCK_METRIC;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use serde_json::json;
use std::path::Path;
use subxt_signer::sr25519::dev;

use polkadot_primitives::Id as ParaId;
use subxt::{dynamic::Value, tx::DynamicPayload, OnlineClient, PolkadotConfig, SubstrateConfig};

use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder, RegistrationStrategy};

const PARA_ID: u32 = 2000;

async fn create_migrate_solo_to_para_call(
	base_dir: &str,
	solo_dir: &str,
) -> Result<DynamicPayload, anyhow::Error> {
	let file_path = Path::new(base_dir).join(solo_dir).join("genesis-state");

	let genesis_state = std::fs::read(file_path)?;

	let call = subxt::dynamic::tx(
		"TestPallet",
		"set_custom_validation_head_data",
		vec![Value::from_bytes(genesis_state)],
	);
	Ok(call)
}

#[tokio::test(flavor = "multi_thread")]
async fn migrate_solo_to_para() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let network = initialize_network().await?;
	let base_dir = network.base_dir().ok_or(anyhow!("failed to get base dir"))?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_throughput(
		&alice_client,
		// 20,
		5,
		[(ParaId::from(PARA_ID), 2..40)].into_iter().collect(),
	)
	.await?;

	let dave = network.get_node("dave")?;

	// alice: parachain 2000 block height is at least 10 within 250 seconds
	log::info!("Ensuring dave reports expected block height");
	assert!(dave
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 10.0, 250u64)
		.await
		.is_ok());

	let eve = network.get_node("eve")?;
	// solo node should not produce blocks
	log::info!("Ensuring eve reports expected block height");
	assert!(eve
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b == 0.0, 20u64)
		.await
		.is_ok());

	log::info!("Migrating solo to para");
	// dave: js-script ./migrate_solo_to_para.js with "dave,2000-1,eve" within 200 seconds
	// after migration solo should start producing blocks and dave should stop
	let call = create_migrate_solo_to_para_call(base_dir, "2000-1").await?;
	let dave_client: OnlineClient<SubstrateConfig> = dave.wait_client().await?;

	let _ = dave_client
		.tx()
		.sign_and_submit_then_watch_default(&call, &dev::alice())
		.await
		.inspect(|_| log::info!("Tx send, waiting for finalization"))?
		.wait_for_finalized_success()
		.await?;

	// solo node should produce blocks now
	log::info!("Ensuring eve reports expected block height");
	assert!(eve
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b == 10.0, 250u64)
		.await
		.is_ok());

	Ok(())
}

async fn initialize_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice   - validator
	// 	 - bob     - validator
	// - parachain nodes
	//   - dave
	//     validator.
	//     run the solo chain (in our case this is also already a parachain, but as it has a different genesis it will not produce any blocks.)
	//   - eve
	//     validator.
	//     run the parachain that will be used to return the header of the solo chain.
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("dave").with_args(vec![("-lparachain=debug").into()])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_registration_strategy(RegistrationStrategy::Manual)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_genesis_overrides(json!({
					"sudo": {
						"key": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty"
					}
				}))
				.with_collator(|n| {
					n.with_name("eve").with_args(vec![("-lparachain=debug").into()]).bootnode(true)
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

	log::info!("parachains config = {:#?}", config.parachains());
	// Spawn network
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	Ok(network)
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::anyhow;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn happy_case() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	log::info!("Spawned");

	let rc_node = network.get_node("alice")?;
	let ah_next_node = network.get_node("charlie")?;

	tokio::time::sleep(Duration::from_secs(30 * 60)).await;
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r
				.with_chain("custom")
				.with_chain_spec_path("rc.json") // TODO: how to autogenerate this?
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=trace").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(2).with_request_memory("2G")
				})
				.with_node(|n| n.with_name("alice"))
				.with_node(|n| n.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(1100)
			.with_chain_spec_path("parachain.json") // TODO: how to autogenerate this?
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					("-lruntime::system=debug,runtime::multiblock-election=debug,runtime::staking=debug,runtime::staking::rc-client=trace").into(),
				])
				.with_collator(|n| n.with_name("charlie"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}

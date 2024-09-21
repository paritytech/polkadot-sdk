// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::env;
use zombienet_sdk::{tx_helper::RuntimeUpgradeOptions, NetworkConfigBuilder};

const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

#[tokio::test(flavor = "multi_thread")]
async fn runtime_upgrade_test() -> Result<(), anyhow::Error> {
	env_logger::init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_node(|node| {
					node.with_name("latest-release")
						// used for getting the chain-spec
						.with_image("parity/polkadot:latest")
				})
				.with_node(|node| node.with_name("bob"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// wait 10 blocks
	let latest_release = network.get_node("latest-release")?;
	assert!(latest_release.wait_metric(BEST_BLOCK_METRIC, |b| b > 10_f64).await.is_ok());

	// get current runtime spec
	let client = network.get_node("latest-release")?.client::<subxt::PolkadotConfig>().await?;
	let current_runtime = client.backend().current_runtime_version().await?;

	// get current best
	latest_release.wait_metric(BEST_BLOCK_METRIC, |x| x > 10_f64).await?;
	let best_block = latest_release.reports(BEST_BLOCK_METRIC).await?;

	// upgrade runtime
	let wasm = env::var("ZOMBIE_WASM_INCREMENTED_PATH").unwrap_or_else(|_| {
        let root_workspace_dir = env!("CARGO_WORKSPACE_ROOT_DIR");
        format!("{root_workspace_dir}/target/testnet/wbuild/rococo-runtime/wasm_binary_spec_version_incremented.rs.compact.compressed.wasm")
    });

	network
		.relaychain()
		.runtime_upgrade(RuntimeUpgradeOptions::new(wasm.as_str().into()))
		.await?;

	// wait 10 more blocks
	latest_release
		.wait_metric(BEST_BLOCK_METRIC, |x| x > best_block + 10_f64)
		.await?;

	let incremented_runtime = client.backend().current_runtime_version().await?;

	assert_eq!(
		incremented_runtime.spec_version,
		current_runtime.spec_version + 1000,
		"version should be incremented"
	);

	Ok(())
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Validator Downgrade Test
//!
//! This test verifies that validators can be downgraded from `stable2603-rc3` to `v1.21.2`
//! while the network continues to operate correctly.
//!
//! Steps:
//! 1. Start a relay chain + parachain with polkadot image `stable2603-rc3`.
//! 2. Assert the network produces parachain blocks.
//! 3. Restart all validators with polkadot image `v1.21.2`.
//! 4. Assert the network still produces parachain blocks.

use crate::utils::{env_or_default, initialize_network, COL_IMAGE_ENV};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{subxt::PolkadotConfig, NetworkConfig, NetworkConfigBuilder, AssetLocation};

const PARA_ID: u32 = 1000;
const POLKADOT_IMAGE_RC: &str = "docker.io/parity/polkadot:stable2603-rc3";
const POLKADOT_IMAGE_DOWNGRADE: &str = "docker.io/parity/polkadot:v1.21.2";
const VALIDATOR_NAMES: &[&str] = &["validator-0", "validator-1", "validator-2"];
const COLLATOR_IMAGE: &str = "docker.io/paritypr/test-parachain:master-99670e97";

/// Downgrade test that verifies validator image downgrade does not break the network.
///
/// - Starts relay chain with `stable2603-rc3` polkadot image and asserts para throughput.
/// - Restarts all validators with `v1.21.2` polkadot image.
/// - Asserts para throughput again to confirm the network is still healthy.
#[tokio::test(flavor = "multi_thread")]
async fn downgrade_validators_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let relay_client: zombienet_sdk::subxt::OnlineClient<PolkadotConfig> =
		network.get_node("validator-0")?.wait_client().await?;

	// Assert network is working before downgrade.
	log::info!("Asserting para throughput before downgrade");
	assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 5..11)]).await?;
	log::info!("Network is operating correctly with {}", POLKADOT_IMAGE_RC);

	// Restart all validators with the downgraded image.
	log::info!("Restarting all validators with image {}", POLKADOT_IMAGE_DOWNGRADE);
	let needed_asstes: Vec<AssetLocation> = vec![
		"https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2512-2/polkadot".into(),
		"https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2512-2/polkadot-execute-worker".into(),
		"https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-stable2512-2/polkadot-prepare-worker".into(),
	];

	for name in VALIDATOR_NAMES {
		let node = network.get_node(*name)?;
		node.restart_with(
			needed_asstes.clone(),
			Some(String::from("polkadot")),
			Some(vec![("-lparachain=debug").into()]),
			None,
		)
		.await?;
		log::info!("Restarted {} with {}", name, POLKADOT_IMAGE_DOWNGRADE);
	}

	// Reconnect to validator-0 after restart.
	let relay_client: zombienet_sdk::subxt::OnlineClient<PolkadotConfig> =
		network.get_node("validator-0")?.wait_client().await?;

	// Assert network is still working after downgrade.
	log::info!("Asserting para throughput after downgrade");
	assert_para_throughput(&relay_client, 20, [(ParaId::from(PARA_ID), 10..25)]).await?;
	log::info!("Network is operating correctly with {}", POLKADOT_IMAGE_DOWNGRADE);

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let col_image = env_or_default(COL_IMAGE_ENV, images.cumulus.as_str());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command(
					"polkadot",
				)
				.with_default_image(POLKADOT_IMAGE_RC)
							.with_default_args(vec![
				("-lparachain=info,parachain::collator-protocol=trace,parachain::network-bridge-rx=trace").into(),
				("--experimental-collator-protocol").into(),
			])

				.with_validator(|node| node.with_name("validator-0"));

			(1..3).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(COLLATOR_IMAGE)
				.with_collator(|n| n.with_name("collator-1000"))
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

	Ok(config)
}

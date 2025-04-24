// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use subxt::{dynamic, dynamic::Value, ext::scale_value::Composite, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

mod tests;

#[subxt::subxt(runtime_metadata_path = "tests/metadata/polkadot-metadata-stripped.scale")]
pub mod polkadot {}

#[subxt::subxt(runtime_metadata_path = "tests/metadata/ah-metadata-stripped.scale")]
pub mod assethub {}

/// Sets `ValidatorCount` in staking pallet to 500.
async fn set_validator_count(
	ah_client: &OnlineClient<PolkadotConfig>,
	validator_count: u32,
) -> Result<(), anyhow::Error> {
	let set_validator_count = dynamic::tx("Staking", "set_validator_count", vec![validator_count]);
	let sudo = dynamic::tx("Sudo", "sudo", vec![set_validator_count.into_value()]);
	let alice = dev::alice();

	ah_client
		.tx()
		.sign_and_submit_then_watch_default(&sudo, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// Sets `Mode` in `StakingNextAhClient` pallet to `Active`.
async fn activate_ah_client(rc_client: &OnlineClient<PolkadotConfig>) -> Result<(), anyhow::Error> {
	let mode_value = Value::variant("Active", Composite::unnamed(vec![]));
	let set_validator_count = dynamic::tx("StakingNextAhClient", "set_mode", vec![mode_value]);
	let sudo = dynamic::tx("Sudo", "sudo", vec![set_validator_count.into_value()]);
	let alice = dev::alice();

	rc_client
		.tx()
		.sign_and_submit_then_watch_default(&sudo, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// Builds a dummy RC and AH networks with the test runtimes from pallet-staking-async
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

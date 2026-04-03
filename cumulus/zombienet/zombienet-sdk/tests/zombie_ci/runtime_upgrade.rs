// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_test_runtime::wasm_spec_version_incremented::WASM_BINARY_BLOATY as WASM_RUNTIME_UPGRADE;
use cumulus_zombienet_sdk_helpers::{
	create_runtime_upgrade_call, submit_extrinsic_and_wait_for_finalization_success,
	wait_for_runtime_upgrade,
};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

// This tests makes sure that it is possible to upgrade parachain's runtime
// and parachain produces blocks after such upgrade.
#[tokio::test(flavor = "multi_thread")]
async fn runtime_upgrade() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let wasm_upgrade = WASM_RUNTIME_UPGRADE.expect("Wasm runtime not built");
	log::info!(
		"WASM_BINARY_BLOATY (upgrade) size: {} bytes ({:.2} KB)",
		wasm_upgrade.len(),
		wasm_upgrade.len() as f64 / 1024.0
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let charlie = network.get_node("charlie")?;
	let charlie_client: OnlineClient<PolkadotConfig> = charlie.wait_client().await?;

	let current_spec_version =
		charlie_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version {current_spec_version}");

	log::info!("Performing runtime upgrade");

	let call = create_runtime_upgrade_call(wasm_upgrade);
	let finalized_hash =
		submit_extrinsic_and_wait_for_finalization_success(&charlie_client, &call, &dev::alice())
			.await?;

	// Inspect the block events to check if the sudo inner dispatch succeeded
	log::info!("Inspecting events in finalized block {finalized_hash:?}");
	let block = charlie_client.blocks().at(finalized_hash).await?;
	log::info!("Extrinsic included in block #{}", block.number());

	let events = block.events().await?;
	let mut found_validation_function_stored = false;
	for event in events.iter() {
		let event = event?;
		let pallet = event.pallet_name();
		let variant = event.variant_name();
		if pallet == "Sudo" || pallet == "ParachainSystem" || pallet == "System" {
			log::info!("Event: {pallet}::{variant}");
		}
		if pallet == "Sudo" && variant == "Sudid" {
			// The Sudid event contains the dispatch result of the inner call.
			// field_bytes contains the SCALE-encoded DispatchResult.
			let bytes = event.field_bytes();
			// DispatchResult::Ok is encoded as 0x00, Err starts with 0x01
			if bytes.first() == Some(&0x00) {
				log::info!("Sudid: inner dispatch SUCCEEDED");
			} else {
				log::error!("Sudid: inner dispatch FAILED, bytes={bytes:?}");
			}
		}
		if pallet == "ParachainSystem" && variant == "ValidationFunctionStored" {
			log::info!("ValidationFunctionStored event found - schedule_code_upgrade succeeded!");
			found_validation_function_stored = true;
		}
	}

	if !found_validation_function_stored {
		log::error!(
			"ValidationFunctionStored event NOT found! The runtime upgrade was NOT scheduled. \
			 Check collator logs for runtime::parachain-system and runtime::sudo targets."
		);
	}

	let dave = network.get_node("dave")?;
	let dave_client: OnlineClient<PolkadotConfig> = dave.wait_client().await?;
	let expected_spec_version = current_spec_version + 1;

	log::info!("Waiting for parachain runtime upgrade to version {}", expected_spec_version);

	let upgrade_result = tokio::time::timeout(
		std::time::Duration::from_secs(300),
		wait_for_runtime_upgrade(&dave_client),
	)
	.await;

	match upgrade_result {
		Ok(Ok(hash)) => log::info!("Runtime upgrade detected at block {hash:?}"),
		Ok(Err(e)) => return Err(anyhow!("wait_for_runtime_upgrade error: {e}")),
		Err(_) => {
			return Err(anyhow!(
				"TIMEOUT: Runtime upgrade not detected within 300s. \
				 ValidationFunctionStored found={found_validation_function_stored}. \
				 Check collator logs for runtime::parachain-system and runtime::sudo targets."
			))
		},
	}

	let spec_version_from_charlie =
		dave_client.backend().current_runtime_version().await?.spec_version;
	assert_eq!(expected_spec_version, spec_version_from_charlie, "Unexpected runtime spec version");

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	// 	 - alice   - validator
	// 	 - bob     - validator
	// - parachain nodes
	//   - charlie - validator
	//   - dave    - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("charlie")
						.validator(true)
						.with_args(vec![
							"-lparachain=debug,runtime=debug".into(),
						])
				})
				.with_collator(|n| {
					n.with_name("dave")
						.validator(false)
						.with_args(vec!["-lruntime=debug".into()])
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

	Ok(config)
}

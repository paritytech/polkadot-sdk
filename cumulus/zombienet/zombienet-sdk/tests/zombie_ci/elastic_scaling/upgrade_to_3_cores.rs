// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// This test ensures that we can upgrade the parachain to elastic-scaling configuration and
// that the parachain produces 3 blocks per relay slot after the upgrade. It covers both
// async-backing and sync-backing parachains.
//
// Pre-refactor, elastic scaling required shipping dedicated WASM blobs (`elastic_scaling`,
// `elastic_scaling_12s_slot`) with `velocity-3` baked in at compile time. After the
// consensus-parameter refactor, velocity is a runtime-configurable
// `pallet_parameters` value. The test therefore:
//   1. Updates `BlockProcessingVelocity` to 3 via `pallet_parameters::set_parameter`.
//   2. Performs a `set_code` upgrade to the `spec_version_incremented` WASM so the existing
//      `spec_version + 1` assertion (and the PVF preparation path) is still exercised.

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_test_runtime::spec_version_incremented::WASM_BINARY as WASM_SPEC_VERSION_INCREMENTED;
use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, assign_cores, submit_extrinsic_and_wait_for_finalization_success,
	submit_sudo_runtime_upgrade, wait_for_pvf_prepare, wait_for_runtime_upgrade,
};
use polkadot_primitives::Id as ParaId;
use rstest::rstest;
use serde_json::json;
use zombienet_sdk::{
	subxt::{
		ext::scale_value::value,
		tx::{dynamic, DynamicPayload},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const TARGET_VELOCITY: u32 = 3;

// Covers both sync and async backing parachains.
#[tokio::test(flavor = "multi_thread")]
#[rstest]
#[case(true)]
#[case(false)]
async fn elastic_scaling_upgrade_to_3_cores(
	#[case] async_backing: bool,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config(async_backing).await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("validator0")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	assign_cores(&alice_client, PARA_ID, vec![0]).await?;

	// Wait for PVF preparation to complete.
	wait_for_pvf_prepare(&network, 1).await?;

	if async_backing {
		log::info!("Ensuring parachain makes progress making 6s blocks");
		assert_para_throughput(&alice_client, 20, [(ParaId::from(PARA_ID), 19..21)], []).await?;
	} else {
		log::info!("Ensuring parachain makes progress making 12s blocks");
		assert_para_throughput(&alice_client, 20, [(ParaId::from(PARA_ID), 9..12)], []).await?;
	}

	assign_cores(&alice_client, PARA_ID, vec![1, 2]).await?;
	let collator0 = network.get_node("collator0")?;
	let collator0_client: OnlineClient<PolkadotConfig> = collator0.wait_client().await?;

	let current_spec_version =
		collator0_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version {current_spec_version}");

	// 1. Bump `BlockProcessingVelocity` to 3 at runtime via `pallet_parameters::set_parameter`.
	//    This used to require swapping the WASM to `elastic_scaling[_12s_slot]`.
	let signer = dev::alice();
	let set_velocity_call = sudo_set_consensus_block_processing_velocity_call(TARGET_VELOCITY);
	log::info!("Submitting sudo set_parameter(BlockProcessingVelocity = {TARGET_VELOCITY})");
	submit_extrinsic_and_wait_for_finalization_success(
		&collator0_client,
		&set_velocity_call,
		&signer,
	)
	.await?;

	// 2. Perform a no-behaviour `set_code` upgrade to the spec-version-incremented WASM so the
	//    existing `spec_version + 1` assertion (and the PVF preparation path) is still
	//    exercised.
	let wasm = WASM_SPEC_VERSION_INCREMENTED
		.expect("WASM binary for spec_version_incremented runtime should be available");

	log::info!("Performing runtime upgrade");
	submit_sudo_runtime_upgrade(&collator0_client, wasm, &signer).await?;

	let collator1 = network.get_node("collator1")?;
	let collator1_client: OnlineClient<PolkadotConfig> = collator1.wait_client().await?;
	let expected_spec_version = current_spec_version + 1;

	log::info!("Waiting for parachain runtime upgrade to version {}", expected_spec_version);
	wait_for_runtime_upgrade(&collator1_client).await?;

	let spec_version_from_collator0 =
		collator0_client.backend().current_runtime_version().await?.spec_version;
	assert_eq!(
		expected_spec_version, spec_version_from_collator0,
		"Unexpected runtime spec version"
	);

	log::info!("Ensure elastic scaling works, 3 blocks should be produced in each 6s slot");
	// Wait for post-upgrade PVF preparation to complete.
	wait_for_pvf_prepare(&network, 2).await?;
	assert_para_throughput(&alice_client, 20, [(ParaId::from(PARA_ID), 50..61)], []).await?;

	Ok(())
}

/// Build a `Sudo::sudo(Parameters::set_parameter(Consensus::BlockProcessingVelocity(Some(v))))`
/// dynamic extrinsic for the `cumulus-test-runtime`.
///
/// Mirrors the enum shape that `#[dynamic_params]` generates in
/// `cumulus/test/runtime/src/lib.rs`:
///   `RuntimeParameters::Consensus(consensus::Parameters::BlockProcessingVelocity(<unit>, Some(v)))`
fn sudo_set_consensus_block_processing_velocity_call(velocity: u32) -> DynamicPayload {
	let runtime_parameter = value!(Consensus(BlockProcessingVelocity({}, Some(velocity))));
	let set_parameter_call = dynamic("Parameters", "set_parameter", vec![runtime_parameter]);
	dynamic("Sudo", "sudo", vec![set_parameter_call.into_value()])
}

async fn build_network_config(async_backing: bool) -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let chain = if async_backing { "async-backing" } else { "sync-backing" };

	// Network setup:
	// - relaychain nodes:
	// 	 - alice   - validator
	// 	 - validator1   - validator
	// 	 - validator2   - validator
	// - parachain nodes
	//   - collator0 - validator
	//   - collator1    - validator
	//   - collator2     - validator
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 3,
								"max_validators_per_core": 1
							},
						}
					}
				}))
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_validator(|node| node.with_name("validator0"))
				.with_validator(|node| node.with_name("validator1"))
				.with_validator(|node| node.with_name("validator2"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--authoring=slot-based".into(),
					"-lparachain=debug,aura=debug".into(),
				])
				.onboard_as_parachain(false)
				.with_chain(chain)
				.with_collator(|n| n.with_name("collator0").validator(true))
				.with_collator(|n| n.with_name("collator1").validator(true))
				.with_collator(|n| n.with_name("collator2").validator(true))
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

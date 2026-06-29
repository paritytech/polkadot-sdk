// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// The test sets up a network with 4 validators and 1 collator, then changes the slot duration
// of the parachain at runtime via `pallet_parameters::set_parameter` (the slot-duration setting
// used to ship as a separate WASM blob — `slot_duration_18s` — that the test installed via
// `set_code`; after the consensus-parameter refactor it is a dynamic runtime parameter). To
// still exercise the runtime-upgrade path (PVF preparation, spec_version change), the test
// also performs a `set_code` upgrade to the `spec_version_incremented` WASM. The test
// verifies that the relay chain is working and finalizing, and the parachain is producing
// blocks with the new slot duration.

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_test_runtime::spec_version_incremented::WASM_BINARY as WASM_SPEC_VERSION_INCREMENTED;
use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, assert_para_throughput,
	submit_extrinsic_and_wait_for_finalization_success, submit_sudo_runtime_upgrade,
	wait_for_pvf_prepare, wait_for_runtime_upgrade,
};
use futures::StreamExt;
use polkadot_primitives::Id as ParaId;
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
const TARGET_SLOT_DURATION_MS: u64 = 18_000;

#[tokio::test(flavor = "multi_thread")]
async fn parachain_runtime_upgrade_slot_duration_18s() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	let initial_slot_duration = get_slot_duration(&collator_client).await?;
	log::info!("Initial slot duration: {initial_slot_duration} ms");

	// 1. Change the slot duration at runtime via `pallet_parameters::set_parameter`. Today the
	//    runtime ships a single WASM blob whose consensus parameters are configurable from
	//    storage; this used to require a dedicated `slot_duration_18s` WASM.
	let alice = dev::alice();
	let set_slot_duration_call =
		sudo_set_consensus_slot_duration_millis_call(TARGET_SLOT_DURATION_MS);
	log::info!("Submitting sudo set_parameter(SlotDurationMillis = {TARGET_SLOT_DURATION_MS})");
	submit_extrinsic_and_wait_for_finalization_success(
		&collator_client,
		&set_slot_duration_call,
		&alice,
	)
	.await?;

	// 2. Perform a no-behaviour `set_code` upgrade to the spec-version-incremented WASM so the
	//    test still exercises the PVF preparation and runtime-upgrade paths that the prior
	//    variant WASM provided.
	let wasm = WASM_SPEC_VERSION_INCREMENTED
		.expect("WASM binary for spec_version_incremented runtime should be available");
	log::info!("Performing runtime upgrade for parachain {PARA_ID}");
	submit_sudo_runtime_upgrade(&collator_client, wasm, &alice).await?;

	let block_hash_of_upgrade = wait_for_runtime_upgrade(&collator_client).await?;

	// Since https://github.com/paritytech/polkadot-sdk/pull/6029 we need to wait for the next
	// finalized block after the upgrade to observe the new slot duration via `AuraApi`.
	log::info!("Waiting for next finalized block for parachain {PARA_ID}...");
	let mut finalized_blocks = collator_client.blocks().subscribe_finalized().await?.take(2);
	while let Some(block) = finalized_blocks.next().await {
		let block = block?;
		let hash = block.hash();
		log::info!("Checking Block #{} ({hash})", block.header().number);
		if block_hash_of_upgrade != hash {
			break;
		} else {
			log::info!("Same block where the upgrade was detected, waiting one more...");
		}
	}

	let slot_duration = get_slot_duration(&collator_client).await?;
	assert_ne!(
		initial_slot_duration, slot_duration,
		"Slot duration should have changed after the parameter update"
	);
	assert_eq!(
		slot_duration, TARGET_SLOT_DURATION_MS,
		"Expected slot duration to be {TARGET_SLOT_DURATION_MS} ms, but got {slot_duration} ms",
	);
	log::info!("Slot duration verified: {slot_duration} ms");

	log::info!("Checking that parachain continues producing blocks after upgrade...");

	// Wait for post-upgrade PVF preparation to complete.
	wait_for_pvf_prepare(&network, 2).await?;

	assert_para_throughput(&relay_client, 15, [(ParaId::from(PARA_ID), 10..30)], []).await?;

	log::info!("Checking that relay chain is finalizing blocks...");
	assert_blocks_are_being_finalized(&relay_client).await?;
	Ok(())
}

/// Build a `Sudo::sudo(Parameters::set_parameter(Consensus::SlotDurationMillis(Some(value))))`
/// dynamic extrinsic for the `cumulus-test-runtime`.
///
/// The shape of the inner `RuntimeParameters` value mirrors the enum that `#[dynamic_params]`
/// generates in `cumulus/test/runtime/src/lib.rs`:
///   `RuntimeParameters::Consensus(consensus::Parameters::SlotDurationMillis(<unit>, Some(v)))`
fn sudo_set_consensus_slot_duration_millis_call(value_ms: u64) -> DynamicPayload {
	let runtime_parameter = value!(Consensus(SlotDurationMillis({}, Some(value_ms))));
	let set_parameter_call = dynamic("Parameters", "set_parameter", vec![runtime_parameter]);
	dynamic("Sudo", "sudo", vec![set_parameter_call.into_value()])
}

async fn get_slot_duration(client: &OnlineClient<PolkadotConfig>) -> Result<u64, anyhow::Error> {
	let best_block = client.blocks().at_latest().await?;
	let block_hash = best_block.hash();

	use zombienet_sdk::subxt::dynamic::Value;
	let result = client
		.runtime_api()
		.at(block_hash)
		.call(zombienet_sdk::subxt::dynamic::runtime_api_call(
			"AuraApi",
			"slot_duration",
			Vec::<Value>::new(),
		))
		.await?;

	result.as_type().map_err(Into::into)
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_validator(|node| node.with_name("validator-0"));

			// Add 4 validators
			(1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator").validator(true))
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

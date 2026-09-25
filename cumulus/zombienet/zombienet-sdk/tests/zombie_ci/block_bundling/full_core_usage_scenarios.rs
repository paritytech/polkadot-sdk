// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::anyhow;
#[cfg(feature = "jam")]
use cumulus_jam_zombienet_tests::{
	para::{Para, TINY_CORES},
	spawn::{spawn, SpawnOptions},
};
use cumulus_primitives_core::relay_chain::MAX_POV_SIZE;
#[cfg(not(feature = "jam"))]
use cumulus_zombienet_sdk_helpers::{assign_cores, ensure_is_last_block_in_core};
use cumulus_zombienet_sdk_helpers::{
	ensure_is_only_block_in_core, submit_extrinsic_and_wait_for_finalization_success, BlockToCheck,
};
#[cfg(feature = "jam")]
use cumulus_zombienet_sdk_helpers::{ensure_uses_full_core, ParaConfig};
use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;
#[cfg(not(feature = "jam"))]
use serde_json::json;
#[cfg(feature = "jam")]
use std::path::PathBuf;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::NetworkConfig;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::{subxt::PolkadotConfig, NetworkConfigBuilder};
use zombienet_sdk::{
	subxt::{ext::scale_value::value, tx::DynamicPayload, OnlineClient},
	subxt_signer::sr25519::dev,
};

const PARA_ID: u32 = 2400;

/// A test that sends transactions using `pallet-utility` `with_weight` through `pallet-sudo`.
///
/// This test starts with 3 cores assigned and sends two transactions:
/// 1. One with 1s ref_time
/// 2. One with a PoV size bigger than what one block alone is allowed to process.
/// Each transaction is sent after the other and waits for finalization.
///
/// On JAM the same para runs from the `block_bundling` runtime flavor, one collator authors it,
/// and every block is alone in its core. The checks then also assert the `UseFullCore` digest.
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_full_core_usage_scenarios() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	#[cfg(not(feature = "jam"))]
	let config = build_network_config().await?;
	#[cfg(not(feature = "jam"))]
	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	#[cfg(feature = "jam")]
	let jam = spawn(
		"block_bundling_full_core_usage_scenarios",
		&[Para::new(PARA_ID, 0, &["collator-1"]).with_runtime(block_bundling_runtime()?)],
		SpawnOptions::new(TINY_CORES),
	)
	.await?;
	#[cfg(feature = "jam")]
	let network = &jam.network;

	// ── Relay path ─────────────────────────────────────────────────────────────────────────
	#[cfg(not(feature = "jam"))]
	{
		let relay_node = network.get_node("validator-0")?;
		let para_node = network.get_node("collator-1")?;

		let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;
		let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
		let alice = dev::alice();

		// Assign cores 0 and 1 to start with 3 cores total (core 2 is assigned by Zombienet)
		assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;

		// Create and send first transaction: 1s ref_time using utility.with_weight
		//
		// While we only should have 500ms available.
		let ref_time_1s = WEIGHT_REF_TIME_PER_SECOND;
		let first_call = create_utility_with_weight_call(ref_time_1s, 0);
		let sudo_first_call = create_sudo_call(first_call);

		log::info!("Testing scenario 1: Sending a transaction with 1s ref time weight usage");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_first_call,
			&alice,
		)
		.await?;

		ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(block_hash)).await?;

		// Create a transaction that uses more than the allowed POV size per block.
		let pov_size = MAX_POV_SIZE / 4 + 512 * 1024;
		let second_call = create_utility_with_weight_call(0, pov_size as u64);
		let sudo_second_call = create_sudo_call(second_call);

		log::info!("Testing scenario 2: Sending a transaction with ~2.5MiB storage weight usage");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_second_call,
			&alice,
		)
		.await?;

		ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(block_hash)).await?;

		let third_call = create_schedule_weight_registration_call();
		let sudo_third_call = create_sudo_call(third_call);

		log::info!("Testing scenario 5: Enabling `on_initialize` to use 1s ref time");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_third_call,
			&alice,
		)
		.await?;

		ensure_is_only_block_in_core(&para_client, BlockToCheck::NextFirstBundleBlock(block_hash))
			.await?;

		let inherent_weight_call = create_set_inherent_weight_consume_call(ref_time_1s, 0);
		let sudo_inherent_weight_call = create_sudo_call(inherent_weight_call);

		log::info!("Testing scenario 4: Enabling an inherent that will use 1s ref time");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_inherent_weight_call,
			&alice,
		)
		.await?;

		// The next block should contain the consume_weight_inherent and consume the 1s ref_time
		ensure_is_only_block_in_core(&para_client, BlockToCheck::NextFirstBundleBlock(block_hash))
			.await?;

		let use_more_weight_than_announced = create_use_more_weight_than_announced_call(true);

		log::info!(
			"Testing scenario 5: Sending a transaction which uses more weight than what \
			it registered and transactions appears in the first block of a core"
		);
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&use_more_weight_than_announced,
			&alice,
		)
		.await?;

		ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(block_hash)).await?;

		let use_more_weight_than_announced = create_use_more_weight_than_announced_call(false);

		// Here we are testing that a transaction that uses more weight than registered makes the
		// block production stop for this core. Even as the block is not the first block in the
		// core.
		log::info!(
			"Testing scenario 6: Sending a transaction which uses more weight than what \
			it registered and transactions appears in the last block of a core"
		);
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&use_more_weight_than_announced,
			&alice,
		)
		.await?;

		ensure_is_last_block_in_core(&para_client, block_hash).await?;
	}

	// ── JAM path ───────────────────────────────────────────────────────────────────────────
	#[cfg(feature = "jam")]
	{
		let para_node = network.get_node("collator-1")?;

		let para_client: OnlineClient<ParaConfig> = para_node.wait_client().await?;
		let alice = dev::alice();

		// Create and send first transaction: 1s ref_time using utility.with_weight
		//
		// While we only should have 500ms available.
		let ref_time_1s = WEIGHT_REF_TIME_PER_SECOND;
		let first_call = create_utility_with_weight_call(ref_time_1s, 0);
		let sudo_first_call = create_sudo_call(first_call);

		log::info!("Testing scenario 1: Sending a transaction with 1s ref time weight usage");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_first_call,
			&alice,
		)
		.await?;

		ensure_full_core_block(&para_client, BlockToCheck::Exact(block_hash)).await?;

		// Create a transaction that uses more than the allowed POV size per block.
		let pov_size = MAX_POV_SIZE / 4 + 512 * 1024;
		let second_call = create_utility_with_weight_call(0, pov_size as u64);
		let sudo_second_call = create_sudo_call(second_call);

		log::info!("Testing scenario 2: Sending a transaction with ~2.5MiB storage weight usage");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_second_call,
			&alice,
		)
		.await?;

		ensure_full_core_block(&para_client, BlockToCheck::Exact(block_hash)).await?;

		let third_call = create_schedule_weight_registration_call();
		let sudo_third_call = create_sudo_call(third_call);

		log::info!("Testing scenario 5: Enabling `on_initialize` to use 1s ref time");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_third_call,
			&alice,
		)
		.await?;

		ensure_full_core_block(&para_client, BlockToCheck::NextFirstBundleBlock(block_hash))
			.await?;

		let inherent_weight_call = create_set_inherent_weight_consume_call(ref_time_1s, 0);
		let sudo_inherent_weight_call = create_sudo_call(inherent_weight_call);

		log::info!("Testing scenario 4: Enabling an inherent that will use 1s ref time");
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_inherent_weight_call,
			&alice,
		)
		.await?;

		// The next block should contain the consume_weight_inherent and consume the 1s ref_time
		ensure_full_core_block(&para_client, BlockToCheck::NextFirstBundleBlock(block_hash))
			.await?;

		let use_more_weight_than_announced = create_use_more_weight_than_announced_call(true);

		log::info!(
			"Testing scenario 5: Sending a transaction which uses more weight than what \
			it registered and transactions appears in the first block of a core"
		);
		let block_hash = submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&use_more_weight_than_announced,
			&alice,
		)
		.await?;

		ensure_full_core_block(&para_client, BlockToCheck::Exact(block_hash)).await?;

		// Scenario 6 (`use_more_weight_than_announced(false)`) is relay-only: on JAM every block
		// is the first and only block of its core, so the extension never admits that transaction.
	}

	Ok(())
}

/// Checks that `block_to_check` is the only block in its core and, on JAM, that the runtime also
/// escalated it with the `UseFullCore` digest.
#[cfg(feature = "jam")]
async fn ensure_full_core_block(
	para_client: &OnlineClient<ParaConfig>,
	block_to_check: BlockToCheck,
) -> Result<(), anyhow::Error> {
	let block_hash = ensure_is_only_block_in_core(para_client, block_to_check).await?;
	ensure_uses_full_core(para_client, block_hash).await
}

/// Creates a `pallet-utility` `with_weight` call
fn create_utility_with_weight_call(ref_time: u64, proof_size: u64) -> DynamicPayload {
	// Create a simple remark call as the inner call
	let remark_data = vec![0u8; proof_size as usize]; // Fill with dummy data for PoV size
	let inner_call =
		zombienet_sdk::subxt::tx::dynamic("System", "remark", vec![value!(remark_data)]);

	// Create the weight struct
	let weight = value!({
		ref_time: ref_time,
		proof_size: proof_size
	});

	// Create the utility.with_weight call
	zombienet_sdk::subxt::tx::dynamic(
		"Utility",
		"with_weight",
		vec![inner_call.into_value(), weight],
	)
}

/// Creates a `pallet-sudo` `sudo` call wrapping the inner call
fn create_sudo_call(inner_call: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("Sudo", "sudo", vec![inner_call.into_value()])
}

/// Creates a `test-pallet` `schedule_weight_registration` call
fn create_schedule_weight_registration_call() -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"TestPallet",
		"schedule_weight_registration",
		vec![] as Vec<zombienet_sdk::subxt::ext::scale_value::Value>,
	)
}

/// Creates a `test-pallet` `use_more_weight_than_announced` call
fn create_use_more_weight_than_announced_call(must_be_first_block_in_core: bool) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"TestPallet",
		"use_more_weight_than_announced",
		vec![value![must_be_first_block_in_core]]
			as Vec<zombienet_sdk::subxt::ext::scale_value::Value>,
	)
}

/// Creates a `test-pallet` `set_inherent_weight_consume` call
fn create_set_inherent_weight_consume_call(ref_time: u64, proof_size: u64) -> DynamicPayload {
	let weight = value!({
		ref_time: ref_time,
		proof_size: proof_size
	});

	zombienet_sdk::subxt::tx::dynamic("TestPallet", "set_inherent_weight_consume", vec![weight])
}

/// Writes the `block_bundling` runtime flavor to a file the JAM harness freezes into the para's
/// validation code.
///
/// The relay path selects this flavor with `--chain block-bundling`; JAM takes the blob itself.
/// The harness copies the file into the run's work dir before genesis reads it.
#[cfg(feature = "jam")]
fn block_bundling_runtime() -> Result<PathBuf, anyhow::Error> {
	let bytes = cumulus_test_runtime::block_bundling::WASM_BINARY
		.ok_or_else(|| anyhow!("the `block_bundling` runtime flavor was not built"))?;
	// A per-process dir so two checkouts running the suite at once cannot overwrite each other's
	// blob; the harness only keeps the file name, which is what ends up in the work dir.
	let dir = std::env::temp_dir().join(format!("cumulus-jam-full-core-{}", std::process::id()));
	std::fs::create_dir_all(&dir).map_err(|e| anyhow!("creating {}: {e}", dir.display()))?;
	let path =
		dir.join(format!("{}.polkavm", cumulus_test_runtime::block_bundling::WASM_FILE_NAME));
	std::fs::write(&path, bytes)
		.map_err(|e| anyhow!("writing the `block_bundling` runtime to {}: {e}", path.display()))?;
	Ok(path)
}

#[cfg(not(feature = "jam"))]
async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=trace").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(4).with_request_memory("4G")
				})
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 3,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));
			(1..9).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("block-bundling")
				.with_default_args(vec![
					("--authoring").into(),
					("slot-based").into(),
					("-lparachain=debug,aura=trace,runtime=trace").into(),
				])
				.with_collator(|n| n.with_name("collator-0"))
				.with_collator(|n| n.with_name("collator-1"))
				.with_collator(|n| n.with_name("collator-2"))
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

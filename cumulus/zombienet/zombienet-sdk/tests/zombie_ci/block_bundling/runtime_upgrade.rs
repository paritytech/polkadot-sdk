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
	para::{Para, PARACHAIN_SERVICE_ID},
	spawn::{spawn, SpawnOptions},
};
use cumulus_primitives_core::relay_chain::MAX_POV_SIZE;
use cumulus_test_runtime::block_bundling::WASM_BINARY;
use cumulus_zombienet_sdk_helpers::{
	ensure_is_only_block_in_core, inflate_runtime_wasm, network::assign_cores,
	wait_for_runtime_upgrade, BlockToCheck, ParaConfig,
};
#[cfg(feature = "jam")]
use cumulus_zombienet_sdk_helpers::{
	jam_rpc::provide_validation_code, network::schedule_code_upgrade,
};
#[cfg(not(feature = "jam"))]
use cumulus_zombienet_sdk_helpers::{
	submit_extrinsic_and_wait_for_finalization_success,
	submit_unsigned_extrinsic_and_wait_for_finalization_success,
};
#[cfg(not(feature = "jam"))]
use serde_json::json;
#[cfg(not(feature = "jam"))]
use sp_crypto_hashing::blake2_256;
use zombienet_sdk::subxt::OnlineClient;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::{
	subxt::{
		ext::scale_value::{value, Value},
		tx::DynamicPayload,
		utils::H256,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2400;
/// 4 blocks per core and each gets 1/4 of the [`MAX_POV_SIZE`], so the runtime needs to be bigger
/// than this to trigger the logic of getting one full core.
const MIN_RUNTIME_SIZE_BYTES: usize = MAX_POV_SIZE as usize / 4 + 50 * 1024;

/// A test that performs runtime upgrade using the `authorize_upgrade` and
/// `apply_authorized_upgrade` logic.
///
/// This test starts with 3 cores assigned and performs two transactions:
/// 1. First calls `authorize_upgrade` to authorize the new runtime code hash
/// 2. Then calls `apply_authorized_upgrade` with the actual runtime code
/// The runtime code is validated to be at least 2.5MiB in size, and both transactions
/// are validated to be the only block in their respective cores.
///
/// On JAM the code never travels on chain: `schedule_code_upgrade_hash` arms the upgrade by hash
/// and the code is handed to JAM out of band.
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_runtime_upgrade() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let compressed_wasm =
		WASM_BINARY.ok_or_else(|| anyhow!("WASM runtime binary not available"))?;

	// Decompress and inflate with a custom wasm section containing pseudo-random data until
	// the compressed size exceeds `MIN_RUNTIME_SIZE_BYTES`.
	let runtime_wasm = inflate_runtime_wasm(compressed_wasm, MIN_RUNTIME_SIZE_BYTES)?;

	log::info!("Runtime size validation passed: {} bytes", runtime_wasm.len());

	#[cfg(not(feature = "jam"))]
	let config = build_network_config().await?;
	#[cfg(not(feature = "jam"))]
	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	#[cfg(not(feature = "jam"))]
	let network = &network;
	// The para holds its three cores from genesis, there is no relay chain to assign them.
	#[cfg(feature = "jam")]
	let jam = spawn(
		"block_bundling_runtime_upgrade",
		&[Para::new(PARA_ID, 0, &["collator-0", "collator-1", "collator-2"])
			.also_on([1, 2])
			.with_runtime_code("block_bundling", WASM_BINARY)?],
		SpawnOptions::new(3),
	)
	.await?;
	#[cfg(feature = "jam")]
	let network = &jam.network;

	let para_node = network.get_node("collator-1")?;

	let para_client: OnlineClient<ParaConfig> = para_node.wait_client().await?;

	// Assign cores 0 and 1 to start with 3 cores total (core 2 is assigned by Zombienet)
	assign_cores(network, "validator-0", PARA_ID, vec![0, 1]).await?;

	log::info!("3 cores total assigned to the parachain");

	#[cfg(not(feature = "jam"))]
	let block_hash = {
		let alice = dev::alice();

		// Step 1: Authorize the runtime upgrade
		let code_hash = blake2_256(&runtime_wasm);
		let authorize_call = create_authorize_upgrade_call(code_hash.into());
		let sudo_authorize_call = create_sudo_call(authorize_call);

		log::info!("Sending authorize_upgrade transaction");
		submit_extrinsic_and_wait_for_finalization_success(
			&para_client,
			&sudo_authorize_call,
			&alice,
		)
		.await?;
		log::info!("Authorize upgrade transaction finalized");

		// Step 2: Apply the authorized upgrade with the actual runtime code
		let apply_call = create_apply_authorized_upgrade_call(runtime_wasm.clone());

		log::info!(
			"Sending apply_authorized_upgrade transaction with runtime size: {} bytes",
			runtime_wasm.len()
		);

		let block_hash =
			submit_unsigned_extrinsic_and_wait_for_finalization_success(&para_client, &apply_call)
				.await?;
		log::info!("Apply authorized upgrade transaction finalized in block: {:?}", block_hash);
		block_hash
	};

	// On JAM `set_code` refuses the code bytes: the upgrade is armed by hash and the code is
	// handed to JAM out of band. The para is finalized only once JAM accumulates its head, which
	// the service holds back until it has the code, so the arming block is awaited as best block.
	#[cfg(feature = "jam")]
	let block_hash = {
		// Step 1: Schedule the runtime upgrade by hash
		log::info!("Sending schedule_code_upgrade_hash transaction");
		let block_hash = schedule_code_upgrade(&para_client, &runtime_wasm).await?;
		log::info!("schedule_code_upgrade_hash included in block: {:?}", block_hash);

		// Step 2: Provide the actual runtime code to JAM
		log::info!("Providing the runtime code to JAM, runtime size: {} bytes", runtime_wasm.len());
		provide_validation_code(&jam.jam_rpc, PARACHAIN_SERVICE_ID, &runtime_wasm).await?;
		block_hash
	};

	ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(block_hash)).await?;

	let upgrade_block = wait_for_runtime_upgrade(&para_client).await?;

	ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(upgrade_block)).await?;

	Ok(())
}

/// Creates a `System::authorize_upgrade` call
#[cfg(not(feature = "jam"))]
fn create_authorize_upgrade_call(code_hash: H256) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"System",
		"authorize_upgrade",
		vec![Value::from_bytes(code_hash)],
	)
}

/// Creates a `System::apply_authorized_upgrade` call
#[cfg(not(feature = "jam"))]
fn create_apply_authorized_upgrade_call(code: Vec<u8>) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("System", "apply_authorized_upgrade", vec![value!(code)])
}

/// Creates a `pallet-sudo` `sudo` call wrapping the inner call
#[cfg(not(feature = "jam"))]
fn create_sudo_call(inner_call: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("Sudo", "sudo", vec![inner_call.into_value()])
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
					("-lparachain=debug,aura=trace,basic-authorship=trace,runtime=trace,txpool=trace")
						.into(),
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

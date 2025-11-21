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
use cumulus_primitives_core::relay_chain::MAX_POV_SIZE;
use cumulus_test_runtime::wasm_spec_version_incremented::WASM_BINARY_BLOATY as WASM_RUNTIME_UPGRADE;
use cumulus_zombienet_sdk_helpers::{
	assign_cores, ensure_is_only_block_in_core, submit_extrinsic_and_wait_for_finalization_success,
	submit_unsigned_extrinsic_and_wait_for_finalization_success, wait_for_runtime_upgrade,
	BlockToCheck,
};
use serde_json::json;
use sp_core::blake2_256;
use zombienet_sdk::{
	subxt::{
		ext::scale_value::{value, Value},
		tx::DynamicPayload,
		utils::H256,
		OnlineClient, PolkadotConfig,
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
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_runtime_upgrade() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Validate runtime size requirement
	let runtime_wasm =
		WASM_RUNTIME_UPGRADE.ok_or_else(|| anyhow!("WASM runtime upgrade binary not available"))?;

	if runtime_wasm.len() <= MIN_RUNTIME_SIZE_BYTES {
		return Err(anyhow!(
			"Runtime size {} bytes is below minimum required {} bytes (2.5MiB)",
			runtime_wasm.len(),
			MIN_RUNTIME_SIZE_BYTES
		));
	}

	log::info!("Runtime size validation passed: {} bytes", runtime_wasm.len());

	let config = build_network_config().await?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-1")?;

	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;
	let alice = dev::alice();

	// Assign cores 0 and 1 to start with 3 cores total (core 2 is assigned by Zombienet)
	assign_cores(&relay_node, PARA_ID, vec![0, 1]).await?;

	log::info!("3 cores total assigned to the parachain");

	// Step 1: Authorize the runtime upgrade
	let code_hash = blake2_256(runtime_wasm);
	let authorize_call = create_authorize_upgrade_call(code_hash.into());
	let sudo_authorize_call = create_sudo_call(authorize_call);

	log::info!("Sending authorize_upgrade transaction");
	submit_extrinsic_and_wait_for_finalization_success(&para_client, &sudo_authorize_call, &alice)
		.await?;
	log::info!("Authorize upgrade transaction finalized");

	// Step 2: Apply the authorized upgrade with the actual runtime code
	let apply_call = create_apply_authorized_upgrade_call(runtime_wasm.to_vec());

	log::info!(
		"Sending apply_authorized_upgrade transaction with runtime size: {} bytes",
		runtime_wasm.len()
	);

	let block_hash =
		submit_unsigned_extrinsic_and_wait_for_finalization_success(&para_client, &apply_call)
			.await?;
	log::info!("Apply authorized upgrade transaction finalized in block: {:?}", block_hash);

	ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(block_hash)).await?;

	let upgrade_block = wait_for_runtime_upgrade(&para_client).await?;

	ensure_is_only_block_in_core(&para_client, BlockToCheck::Exact(upgrade_block)).await?;

	Ok(())
}

/// Creates a `System::authorize_upgrade` call
fn create_authorize_upgrade_call(code_hash: H256) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"System",
		"authorize_upgrade",
		vec![Value::from_bytes(code_hash)],
	)
}

/// Creates a `System::apply_authorized_upgrade` call
fn create_apply_authorized_upgrade_call(code: Vec<u8>) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("System", "apply_authorized_upgrade", vec![value!(code)])
}

/// Creates a `pallet-sudo` `sudo` call wrapping the inner call
fn create_sudo_call(inner_call: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("Sudo", "sudo", vec![inner_call.into_value()])
}

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
				.with_node(|node| node.with_name("validator-0"));
			(1..9).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("block-bundling")
				.with_default_args(vec![
					("--authoring").into(),
					("slot-based").into(),
					("-lparachain=debug,aura=trace,basic-authorship=trace,runtime=trace,txpool=trace").into(),
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

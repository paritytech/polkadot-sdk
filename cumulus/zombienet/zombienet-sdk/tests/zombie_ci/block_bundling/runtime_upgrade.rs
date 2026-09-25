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
#[cfg(not(feature = "jam"))]
use cumulus_test_runtime::block_bundling::WASM_BINARY;
use cumulus_zombienet_sdk_helpers::inflate_runtime_wasm;
#[cfg(feature = "jam")]
use cumulus_zombienet_sdk_helpers::{
	submit_extrinsic_and_wait_for_best_block_success, wait_for_runtime_upgrade_on_best,
};
#[cfg(not(feature = "jam"))]
use cumulus_zombienet_sdk_helpers::{
	submit_extrinsic_and_wait_for_finalization_success, wait_for_runtime_upgrade,
};
use sp_crypto_hashing::blake2_256;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::NetworkConfig;
use zombienet_sdk::{
	subxt::{ext::scale_value::value, tx::DynamicPayload, OnlineClient},
	subxt_signer::sr25519::dev,
};

#[cfg(not(feature = "jam"))]
use {
	cumulus_primitives_core::relay_chain::MAX_POV_SIZE,
	cumulus_zombienet_sdk_helpers::submit_unsigned_extrinsic_and_wait_for_finalization_success,
	cumulus_zombienet_sdk_helpers::{assign_cores, ensure_is_only_block_in_core, BlockToCheck},
	serde_json::json,
	zombienet_sdk::subxt::{ext::scale_value::Value, utils::H256},
	zombienet_sdk::{subxt::PolkadotConfig, NetworkConfigBuilder},
};

#[cfg(feature = "jam")]
use {
	cumulus_jam_zombienet_tests::{
		env::binaries_or_err,
		para::{Para, PARACHAIN_SERVICE_ID, TINY_CORES},
		spawn::{spawn, SpawnOptions},
	},
	cumulus_zombienet_sdk_helpers::{
		find_event_and_decode_fields, jam::DigestItem, network::assert_para_throughput, ParaConfig,
	},
	polkadot_primitives::Id as ParaId,
	std::{collections::HashMap, time::Duration},
	zombienet_sdk::subxt::ext::scale_value::Value,
};

const PARA_ID: u32 = 2400;

/// The collator the test drives and reads blocks from.
#[cfg(not(feature = "jam"))]
const PARA_NODE: &str = "collator-1";

/// One collator, and the test talks to it.
///
/// A JAM collator can only build on a head it holds locally, and recovering a work package another
/// collator produced is not implemented yet — with several collators all but the one that happens
/// to author first stall on `accumulated head is not known locally` and never finalize anything.
/// Single-collator is what every JAM test that passes today runs.
#[cfg(feature = "jam")]
const PARA_NODE: &str = "collator-0";

/// On the relay path: 4 blocks per core each getting 1/4 of [`MAX_POV_SIZE`], so the runtime
/// must exceed this size to trigger the full-core block-bundling logic.
#[cfg(not(feature = "jam"))]
const MIN_RUNTIME_SIZE_BYTES: usize = MAX_POV_SIZE as usize / 4 + 50 * 1024;

/// On the JAM path: 4 blocks per core each getting 1/4 of the JAM PoV budget.
/// Derived from the JAM collator's `MAX_POV_SIZE` (12 MiB, `builder_task.rs:106`) / 4 + headroom.
/// No relay import used: the relay and JAM budgets are independent constants.
#[cfg(feature = "jam")]
const MIN_RUNTIME_SIZE_BYTES: usize = 12 * 1024 * 1024 / 4 + 50 * 1024;

/// Best para blocks observed before providing the preimage (S2 negative window).
/// A tiny JAM network runs at one 6-second slot per block; 5 blocks ≈ 30 seconds — long enough
/// to confirm the upgrade does not self-activate while the service cannot resolve the code.
#[cfg(feature = "jam")]
const NEGATIVE_WINDOW_BLOCKS: u32 = 5;

/// Runtime upgrade via the offchain code-upgrade flow.
///
/// Relay path: 3 cores, checked `authorize_upgrade` + unsigned `apply_authorized_upgrade`,
/// `ensure_is_only_block_in_core` guards.
///
/// JAM path: `schedule_code_upgrade_hash` arms the upgrade with a 32-byte hash — the blob never
/// travels on chain — an S2 negative window asserts no premature switch before the manual
/// `provide_validation_code`, then `wait_for_runtime_upgrade_on_best`.
/// No `spec_version` assertion: the blob is padded, not re-versioned, so the digest is the signal.
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_runtime_upgrade() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// On WASM: decompress, bump spec_version, re-compress until the compressed size reaches
	// MIN_RUNTIME_SIZE_BYTES. On PolkaVM: pad with PADDING_SECTION chunks (no version bump —
	// the JAM path schedules by hash; padding changes the blake2b-256 hash, which is
	// what the JAM code-upgrade lifecycle keys on).
	#[cfg(not(feature = "jam"))]
	let source_blob = WASM_BINARY
		.ok_or_else(|| anyhow!("WASM runtime binary not available"))?
		.to_vec();

	// The JAM para runs the PolkaVM blob at `RUNTIME_WASM`, so that is what it must upgrade to:
	// a padded copy of the code already running. `cumulus-test-runtime` has no JAM build yet, and
	// swapping a template-runtime chain onto a different runtime would brick it.
	#[cfg(feature = "jam")]
	let source_blob = {
		let path = binaries_or_err()?.runtime_wasm;
		std::fs::read(&path)
			.map_err(|e| anyhow!("reading RUNTIME_WASM at {}: {e}", path.display()))?
	};

	let runtime_wasm = inflate_runtime_wasm(&source_blob, MIN_RUNTIME_SIZE_BYTES)?;
	log::info!("Runtime size: {} bytes", runtime_wasm.len());

	#[cfg(not(feature = "jam"))]
	let code_hash = blake2_256(&runtime_wasm);

	#[cfg(not(feature = "jam"))]
	let config = build_network_config().await?;

	#[cfg(feature = "jam")]
	let jam = spawn(
		"block_bundling_runtime_upgrade",
		&[Para::new(PARA_ID, 0, &[PARA_NODE])],
		SpawnOptions {
			cores: TINY_CORES,
			ordinary_rpc_port: None,
			collators: HashMap::new(),
			ready_timeout: Duration::from_secs(120),
		},
	)
	.await?;
	#[cfg(not(feature = "jam"))]
	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	#[cfg(not(feature = "jam"))]
	let network = &network;
	#[cfg(feature = "jam")]
	let network = &jam.network;

	let para_node = network.get_node(PARA_NODE)?;
	let alice = dev::alice();

	// ── Relay path ─────────────────────────────────────────────────────────────────────────
	#[cfg(not(feature = "jam"))]
	{
		let relay_node = network.get_node("validator-0")?;
		let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
		let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;

		// Assign cores 0 and 1 to start with 3 cores total (core 2 is assigned by Zombienet).
		assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;
		log::info!("3 cores total assigned to the parachain");

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

		let apply_call = create_apply_authorized_upgrade_call(runtime_wasm.clone());
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
	}

	// ── JAM path ───────────────────────────────────────────────────────────────────────────
	#[cfg(feature = "jam")]
	{
		let para_client: OnlineClient<ParaConfig> = para_node.wait_client().await?;

		// The JAM ordinary node RPC, for the out-of-band preimage submission.
		let jam_rpc = &jam.jam_rpc;

		// Step 1: arm the upgrade by hash. `schedule_code_upgrade_hash` takes the 32-byte blake2b
		// hash and the length, never the code bytes: a multi-MB extrinsic payload traps the
		// PolkaVM runtime, and the code itself lives offchain in JAM. The runtime records
		// `PendingCodeHash`; refine emits `RequestCodeUpgrade` and the service solicits the
		// preimage.
		//
		// The padded target hashes differently from the code already active — the service
		// treats a request for the active code as a no-op — so this is what arms the lifecycle.
		let code_hash = blake2_256(&runtime_wasm);
		let code_len = runtime_wasm.len() as u32;
		let schedule_call = create_schedule_code_upgrade_hash_call(code_hash, code_len);
		let sudo_schedule_call = create_sudo_unchecked_weight_call(schedule_call);
		log::info!("Sending sudo(schedule_code_upgrade_hash) for {code_len} bytes");
		// Inclusion in a best block, not finalization: the JAM collator only finalizes a para head
		// once JAM accumulates it, and JAM's accumulated head stays put here, so waiting for
		// finalization would wait forever.
		let authorize_block = submit_extrinsic_and_wait_for_best_block_success(
			&para_client,
			&sudo_schedule_call,
			&alice,
		)
		.await?;

		// `sudo` reports the INNER dispatch only through `Sudid`; the extrinsic itself succeeds
		// either way. Without this an arming that never happened surfaces much later as an
		// opaque "Transaction is invalid".
		let events = para_client.blocks().at(authorize_block).await?.events().await?;
		let sudid: Vec<Result<(), sp_runtime::DispatchError>> =
			find_event_and_decode_fields(&events, "Sudo", "Sudid")?;
		match sudid.first() {
			Some(Ok(())) => log::info!("schedule_code_upgrade_hash included (code scheduled)"),
			Some(Err(e)) => return Err(anyhow!("sudo(schedule_code_upgrade_hash) failed: {e:?}")),
			None => return Err(anyhow!("no Sudid event in block {authorize_block:?}")),
		}

		// Step 2 — S2 (negative window): observe NEGATIVE_WINDOW_BLOCKS para blocks and assert
		// NONE carry RuntimeEnvironmentUpdated. The service refuses an announcement whose code it
		// cannot resolve (`announce_code_upgrade`), so `ParaInfo.announced_upgrade` stays unset and
		// the runtime keeps announcing; the `:code` marker is written only once the service
		// reflects the announcement. Catching N blocks in this window simultaneously proves the
		// para is still producing under the old code.
		//
		// Best blocks, not finalized: the digest is deposited into the block HEADER, so it is
		// observable on every authored block, while the JAM collator's `finalized` tracks JAM's
		// accumulated para head and never moves here. 6 seconds per JAM slot ×
		// NEGATIVE_WINDOW_BLOCKS ≈ 30 s of deliberate negative coverage.
		log::info!(
			"S2 negative window: observing {NEGATIVE_WINDOW_BLOCKS} best para blocks, \
			 asserting no RuntimeEnvironmentUpdated"
		);
		{
			let mut blocks_sub = para_client.blocks().subscribe_best().await?;
			for i in 0..NEGATIVE_WINDOW_BLOCKS {
				let block = blocks_sub.next().await.ok_or_else(|| {
					anyhow!("para best block stream ended during the S2 negative window")
				})??;
				anyhow::ensure!(
					!block
						.header()
						.digest
						.logs
						.iter()
						.any(|d| matches!(d, DigestItem::RuntimeEnvironmentUpdated)),
					"S2 violated: RuntimeEnvironmentUpdated at block {:?} \
					 before preimage was provided to JAM",
					block.hash()
				);
				log::info!(
					"S2: block {}/{NEGATIVE_WINDOW_BLOCKS} {:?} — no premature upgrade",
					i + 1,
					block.hash()
				);
			}
		}
		log::info!("S2 passed: {NEGATIVE_WINDOW_BLOCKS} best blocks, no premature upgrade");

		// Step 3: Provide the validation code to JAM — the deliberate out-of-band operator step.
		// `RequestCodeUpgrade(Announcement)` was emitted by refine; only once JAM holds this
		// preimage does accumulate store it in `ParaInfo.announced_upgrade`, which is what the
		// runtime waits on before switching (service design §5.2). No node or collator code may
		// submit the preimage.
		log::info!("Providing validation code to JAM service {PARACHAIN_SERVICE_ID}");
		cumulus_jam_zombienet_tests::rpc::provide_validation_code(
			jam_rpc,
			PARACHAIN_SERVICE_ID,
			&runtime_wasm,
		)
		.await?;
		log::info!("Preimage provided and confirmed at a finalized JAM anchor");

		// Step 4: Wait for the first para best block whose header digest carries
		// RuntimeEnvironmentUpdated, deposited by the runtime once the service reflects the
		// announced upgrade. Best blocks, not finalized: see the S2 note above.
		let _upgrade_block = wait_for_runtime_upgrade_on_best(&para_client).await?;
		log::info!("RuntimeEnvironmentUpdated seen — runtime upgrade complete");

		// Step 5: Assert the para keeps producing blocks after the upgrade.
		// `ensure_is_only_block_in_core` is not used on JAM: that helper reads
		// `CumulusDigestItem::BlockBundleInfo` which JAM para headers do not carry.
		let current_best = para_node.reports("block_height{status=\"best\"}").await? as u32;
		let target = current_best + 5;
		log::info!("Asserting para reaches block {target} after upgrade");
		assert_para_throughput(
			network,
			PARA_NODE,
			5,
			[(ParaId::from(PARA_ID), 5..100)],
			[(ParaId::from(PARA_ID), (para_client, target..target + 10))],
		)
		.await?;
	}

	Ok(())
}

/// Creates a `System::authorize_upgrade` call (checked; relay path only).
#[cfg(not(feature = "jam"))]
fn create_authorize_upgrade_call(code_hash: H256) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"System",
		"authorize_upgrade",
		vec![Value::from_bytes(code_hash)],
	)
}

/// Creates a `ParachainSystem::schedule_code_upgrade_hash` call (JAM path only).
///
/// Takes the 32-byte blake2b hash and the byte length, never the code: a multi-MB extrinsic
/// payload traps the PolkaVM runtime, and the code lives offchain in JAM.
#[cfg(feature = "jam")]
fn create_schedule_code_upgrade_hash_call(code_hash: [u8; 32], code_len: u32) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"ParachainSystem",
		"schedule_code_upgrade_hash",
		vec![Value::from_bytes(code_hash), value!(code_len)],
	)
}

/// Creates a `System::apply_authorized_upgrade` call.
#[cfg(not(feature = "jam"))]
fn create_apply_authorized_upgrade_call(code: Vec<u8>) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("System", "apply_authorized_upgrade", vec![value!(code)])
}

/// Creates a `pallet_sudo::sudo` call wrapping the inner call.
#[cfg(not(feature = "jam"))]
fn create_sudo_call(inner_call: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("Sudo", "sudo", vec![inner_call.into_value()])
}

/// Wraps `inner_call` in `Sudo::sudo_unchecked_weight` with a nominal declared weight.
///
/// Plain `Sudo::sudo` derives its dispatch weight from the inner call, which for a runtime upgrade
/// is charged before the block-length and weight checks have any slack left. The relay helper
/// `create_runtime_upgrade_call` declares `{1, 1}` here for the same reason; the real cost is
/// still accounted post-dispatch.
#[cfg(feature = "jam")]
fn create_sudo_unchecked_weight_call(inner_call: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo_unchecked_weight",
		vec![inner_call.into_value(), value! { { ref_time: 1u64, proof_size: 1u64 } }],
	)
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

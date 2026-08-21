// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end zombienet test for the dynamic `read_relay_chain_state` mechanism.
//!
//! # What is being validated
//!
//! A Cumulus parachain reads relay-chain state *dynamically during block execution* via the
//! `read_relay_chain_state` host function. This is now a **core** part of `set_validation_data`:
//! `cumulus-pallet-parachain-system` reads the relay host configuration, messaging state and
//! upgrade signals through it on **every** block, instead of from a fixed proof carried in the
//! inherent. Each read is recorded into a minimal storage proof, carried in the PoV through the
//! additional-data channel, committed by a `DigestItem::AdditionalData` header digest, and —
//! crucially — **re-verified against the authenticated `relay_parent_storage_root` inside the
//! PVF** when the relay chain validates the candidate.
//!
//! This test proves the whole loop on a *real network* of actual node binaries (no mocks):
//!
//!   relay chain (rococo-local, 3 validators)
//!        │  authenticates each candidate's relay_parent_storage_root
//!        ▼
//!   parachain collator (test-parachain, EMBEDDED relay full node)
//!        │  every block: runtime reads relay state via read_relay_chain_state while authoring;
//!        │  reads are served from the collator's live relay state and recorded as a proof;
//!        │  proof rides the PoV; block header commits its hash
//!        ▼
//!   relay validators run validate_block (the PVF)
//!        │  re-read the same relay keys from the proof, verify vs relay_parent_storage_root,
//!        │  re-execute → state root must match → candidate is backed + included + finalized
//!        ▼
//!   the parachain keeps producing + finalizing blocks
//!
//! # Why the assertions mean what they mean
//!
//! * **Every** parachain block now reads relay state, so a candidate can only be backed, included
//!   and finalized if the relay validators ran `validate_block` and that PVF accepted the recorded
//!   relay-read proof against the `relay_parent_storage_root` the relay chain itself authenticated.
//!   If the read/verify were unsound the candidate would be rejected and the parachain would stall.
//!   So **sustained parachain throughput + finalization is itself the proof** that the dynamic
//!   read/verify path is sound end to end.
//!
//! * The test additionally proves the **generic block-import** path: a non-authoring
//!   full node must receive the `additional_data` proof over the sync protocol and re-execute each
//!   block, serving `read_relay_chain_state` from the synced proof. If the generic import path
//!   could not serve the reads, import would panic and the full node's finalized head would never
//!   advance.
//!
//! # Non-obvious implementation note
//!
//! `read_relay_chain_state` is only served by the **in-process** relay-chain interface (a collator
//! that embeds a full relay node). The RPC/minimal relay interface cannot serve dynamic reads yet,
//! so the collator here runs with **no** `--relay-chain-rpc-url` (the default embedded-relay mode).
//! Because every parachain header now carries a custom `DigestItem::AdditionalData` that subxt's
//! `PolkadotConfig` decoder does not know, we drive the *parachain* purely over raw RPC (reading
//! only the JSON `number` field of headers, never SCALE-decoding them). The *relay* chain uses
//! standard headers, so subxt is used there as normal.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use std::time::{Duration, Instant};
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

use crate::utils::initialize_network;

const PARA_ID: u32 = 2000;

/// How long to wait for the parachain's finalized head to advance past the target. Parachain blocks
/// are finalized by relay GRANDPA a little after inclusion; this is a generous margin.
const FINALIZATION_TIMEOUT: Duration = Duration::from_secs(180);

/// The parachain finalized block number the polling helpers wait to see. Reaching it means several
/// relay-read candidates were validated by the PVF and finalized by the relay chain.
const TARGET_FINALIZED_NUMBER: u64 = 6;

/// Manual/interactive variant: spawns the exact same network as the automated test, prints the
/// endpoints, and holds the network open for hands-on inspection.
///
/// Run it explicitly (it's `#[ignore]`d so it never runs in a normal `cargo test`):
///
/// ```text
/// ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests \
///   --features zombie-ci -- --ignored --nocapture --test-threads 1 relay_chain_read_manual
/// ```
///
/// Every parachain block reads relay state via `read_relay_chain_state` as part of
/// `set_validation_data`, so there is nothing to submit. Watch the relay chain's `paraInclusion`
/// events to confirm the parachain's candidates are backed/included/finalized, and watch the
/// parachain's finalized block number advance — both only happen if the PVF accepted the recorded
/// relay reads.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "manual: spawns the network and holds it open for hands-on inspection"]
async fn relay_chain_read_manual() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let collator = network.get_node("collator01")?;
	let relay = network.get_node("alice")?;

	log::info!("================ MANUAL VERIFICATION ================");
	log::info!("collator RPC: {}", collator.ws_uri());
	log::info!("relay/alice RPC: {}", relay.ws_uri());
	log::info!("Every parachain block reads relay state via read_relay_chain_state (no submission");
	log::info!("needed). Confirm the relay's paraInclusion events back/include/finalize the");
	log::info!("parachain's candidates, and that the parachain's finalized block number advances.");
	log::info!("Holding the network open for 1 hour (Ctrl-C to stop early).");
	log::info!("=====================================================");

	tokio::time::sleep(Duration::from_secs(3600)).await;
	Ok(())
}

/// End-to-end test covering **both** verification paths in one network:
///
/// * **PVF path** — every parachain block reads relay state via `read_relay_chain_state`, so a
///   candidate can only be backed on the relay chain if the relay validators ran `validate_block`
///   and that PVF accepted the recorded relay-read proof against the authenticated
///   `relay_parent_storage_root`. Sustained backed throughput (`assert_para_throughput`) is itself
///   the proof that the dynamic read/verify path is sound.
/// * **Generic block-import path** — a non-authoring full node `full01` syncs from the collator,
///   receives the `additional_data` (relay-read proof) over the sync protocol, and must re-execute
///   each block on its generic import path, serving `read_relay_chain_state` from the synced proof.
///   We assert the full node's **finalized** head advances, which is only possible if it
///   successfully synced *and* imported the relay-read blocks. If the generic import path could not
///   serve the reads, import would panic and the finalized head would never advance.
#[tokio::test(flavor = "multi_thread")]
async fn relay_chain_read_syncs_and_imports_on_full_node() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network: rococo-local (alice,bob,charlie) + collator01 + follower full01");
	let config = build_network_config_inner(true).await?;
	let network = initialize_network(config).await?;

	let relay_alice = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;
	assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 2..20)], []).await?;

	// Key assertion: the FULL NODE's finalized head advances. It only can if `full01` synced the
	// `additional_data` proof and re-executed each relay-read block on its generic import path
	// (serving `read_relay_chain_state` from the synced proof).
	let full_node = network.get_node("full01")?;
	let full_rpc: RpcClient = full_node.rpc().await?;
	log::info!("Waiting for full01 to sync + generic-import relay-read blocks");
	let reached = wait_for_para_finalized_number(&full_rpc, TARGET_FINALIZED_NUMBER).await?;
	log::info!(
		"PASS: full node synced + generic-imported relay-read blocks (finalized head #{reached}); \
		 the generic import path served read_relay_chain_state from the synced proof"
	);

	Ok(())
}

/// Poll the parachain's **finalized** head number over raw RPC until it reaches `target`, or
/// [`FINALIZATION_TIMEOUT`] elapses. Reads only the JSON `number` field, so no SCALE decode of the
/// custom `AdditionalData` digest is needed.
async fn wait_for_para_finalized_number(
	para_rpc: &RpcClient,
	target: u64,
) -> Result<u64, anyhow::Error> {
	let deadline = Instant::now() + FINALIZATION_TIMEOUT;
	let mut last_logged = 0u64;
	loop {
		let finalized_head: String =
			para_rpc.request("chain_getFinalizedHead", rpc_params![]).await?;
		let number = match para_rpc
			.request::<serde_json::Value>("chain_getHeader", rpc_params![finalized_head.clone()])
			.await
		{
			Ok(header) => header
				.get("number")
				.and_then(|n| n.as_str())
				.and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
				.unwrap_or(0),
			Err(_) => 0,
		};

		if number != last_logged {
			log::info!("parachain finalized block number is now {number}");
			last_logged = number;
		}
		if number >= target {
			return Ok(number);
		}

		if Instant::now() >= deadline {
			return Err(anyhow!(
				"parachain finalized head did not reach #{target} within {FINALIZATION_TIMEOUT:?} \
				 — a relay-read block never finalized (would indicate the PVF or generic import \
				 rejected the recorded relay reads)"
			));
		}
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	build_network_config_inner(false).await
}

/// Build the network. When `with_full_node` is set, adds a non-authoring parachain full node
/// `full01` that syncs from the collator — used to exercise the sync + **generic block-import**
/// path (distinct from the PVF path): a remote node must receive the `additional_data` over sync
/// and re-execute each block, serving `read_relay_chain_state` from the synced proof.
async fn build_network_config_inner(with_full_node: bool) -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();

	// Network:
	// - relay chain (rococo-local): validators alice, bob, charlie — they back, approve and
	//   finalize the parachain's candidates (running validate_block / the PVF). Three validators
	//   (rather than two) keep backing/inclusion stable across session changes and give the
	//   collator's embedded relay node enough peers to reliably advertise collations.
	// - parachain: a collator `collator01` with an EMBEDDED relay full node (no
	//   `--relay-chain-rpc-url`), which is the only mode that can serve `read_relay_chain_state`;
	//   plus, optionally, a follower full node `full01`.
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
				.with_validator(|node| node.with_name("charlie"))
		})
		.with_parachain(|p| {
			let p = p
				.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,cumulus-collator=debug").into()])
				.with_collator(|n| n.with_name("collator01").validator(true));
			if with_full_node {
				p.with_collator(|n| n.with_name("full01").validator(false))
			} else {
				p
			}
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

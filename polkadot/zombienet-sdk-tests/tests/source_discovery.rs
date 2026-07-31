// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Cross-parachain source discovery E2E.
//!
//! Spawns a rococo-local relay + two penpal parachains. Each penpal is told, via
//! the governance-gated `SourceDiscovery::set_source_genesis` (sudo → root), how
//! to reach the *other* parachain's collators. The node's discovery worker
//! (`cumulus-client-source-discovery`) then resolves that source's peers over the
//! relay-chain DHT (RFC-0008 `/paranode`). The test asserts, from the collators'
//! logs, that discovery resolved at least one peer per source and hit no protocol
//! error — no messaging/fetch is involved.
//!
//! Calls are constructed dynamically (subxt runtime-metadata API) to avoid a
//! static-codegen dependency on a specific subxt version.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as RelayParaId;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{dynamic::Value, tx::dynamic, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;

/// Records how the penpal reachable at `client` should reach `source`'s
/// collators — its 32-byte genesis hash — via sudo
/// (`SourceDiscovery::set_source_genesis`, root-gated).
async fn set_source_genesis(
	client: &OnlineClient<PolkadotConfig>,
	source: u32,
	genesis: [u8; 32],
) -> Result<(), anyhow::Error> {
	// info = Some((genesis, None)); source = ParaId (a `u32` newtype). Built with
	// explicit scale-value constructors (a plain `None` in the `value!` macro is
	// mis-read as Rust's `Option::None`).
	let genesis = Value::from_bytes(genesis);
	let none = Value::unnamed_variant("None", []);
	let info = Value::unnamed_variant("Some", [Value::unnamed_composite([genesis, none])]);
	let pallet_call = Value::named_variant(
		"set_source_genesis",
		[("source", Value::u128(source as u128)), ("info", info)],
	);
	let runtime_call = Value::unnamed_variant("SourceDiscovery", [pallet_call]);
	let call = dynamic("Sudo", "sudo", vec![runtime_call]);
	client
		.tx()
		.sign_and_submit_then_watch_default(&call, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn source_discovery_penpal() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
				.with_validator(|node| node.with_name("charlie"))
				.with_validator(|node| node.with_name("dave"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2000")
				.with_collator(|n| {
					// `--spec-msg-serve`: advertise under the capability key that source-discovery
					// resolves against (so B finds A, and vice-versa).
					n.with_name("penpal-a").with_args(vec![
						("-lsource-discovery=trace,bootnodes=trace").into(),
						"--spec-msg-serve".into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2001")
				.with_collator(|n| {
					n.with_name("penpal-b").with_args(vec![
						("-lsource-discovery=trace,bootnodes=trace").into(),
						"--spec-msg-serve".into(),
					])
				})
		})
		.build()
		.map_err(|e| {
			anyhow!(
				"config errs: {}",
				e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
			)
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_client: OnlineClient<PolkadotConfig> =
		network.get_node("alice")?.wait_client().await?;
	let a_client: OnlineClient<PolkadotConfig> =
		network.get_node("penpal-a")?.wait_client().await?;
	let b_client: OnlineClient<PolkadotConfig> =
		network.get_node("penpal-b")?.wait_client().await?;

	log::info!("Waiting for both penpals to produce blocks");
	assert_para_throughput(
		&relay_client,
		15,
		[(RelayParaId::from(PARA_A), 2..40), (RelayParaId::from(PARA_B), 2..40)],
		[],
	)
	.await?;

	// Each penpal learns how to reach the other's collators — governance-gated,
	// on-chain. The discovery worker picks it up within ~1 block.
	let genesis_a = a_client.genesis_hash();
	let genesis_b = b_client.genesis_hash();
	log::info!("Setting source genesis both directions (B<-A, A<-B)");
	set_source_genesis(&b_client, PARA_A, genesis_a.0).await?;
	set_source_genesis(&a_client, PARA_B, genesis_b.0).await?;

	// Assert, from the collator logs, that discovery resolved ≥1 peer per source
	// and hit no `/paranode` protocol error. The second arg is `is_glob`: pass
	// `false` so these patterns are treated as (unanchored) regexes, not globs.
	log::info!("Asserting discovery resolved peers on both collators");
	let penpal_a = network.get_node("penpal-a")?;
	let penpal_b = network.get_node("penpal-b")?;
	let found = LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(600), false);
	assert!(
		penpal_a
			.wait_log_line_count_with_timeout(
				"Discovered source peers.*count=[1-9]",
				false,
				found.clone(),
			)
			.await?
			.success(),
		"penpal-a did not discover source 2001's peers",
	);
	assert!(
		penpal_b
			.wait_log_line_count_with_timeout("Discovered source peers.*count=[1-9]", false, found)
			.await?
			.success(),
		"penpal-b did not discover source 2000's peers",
	);

	let absent = LogLineCountOptions::new(|n| n == 0, Duration::from_secs(5), false);
	assert!(
		penpal_a
			.wait_log_line_count_with_timeout("/paranode doesn't exist", false, absent)
			.await?
			.success(),
		"penpal-a saw a `/paranode doesn't exist` error",
	);

	log::info!("Source discovery E2E passed");
	Ok(())
}

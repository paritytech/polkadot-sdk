// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Capability-scoped cross-parachain source discovery E2E (RFC-0008 capability key).
//!
//! Proves the fix for the *serving-subset-missed* failure: a receiver's capability-scoped DHT query
//! (`get_providers(para_id ++ b"spec-msg/v1" ++ randomness)`) resolves **only** collators that
//! advertise the capability (`--spec-msg-serve`), filtering out non-serving collators of the same
//! source parachain — so it can never be diluted by them (the closest-K problem).
//!
//! Topology:
//! ```
//! relay (rococo-local)
//! ├── penpal-A = 2000  (source)
//! │     ├── penpal-a1  → --spec-msg-serve : advertises under the capability key   [serving]
//! │     └── penpal-a2  → plain RFC-0008 bootnode only                             [non-serving]
//! └── penpal-B = 2001  (receiver)
//!       └── penpal-b   → source-discovery resolves 2000 under the capability
//! ```
//! `a2` is a genuine distractor: up and advertising the *plain* key (a plain query would find it) —
//! the test asserts the *capability* query never does.

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

/// Tell the penpal at `client` how to reach `source`'s collators (its genesis hash), via sudo →
/// `SourceDiscovery::set_source_genesis` (root-gated). Same helper as `source_discovery.rs`.
async fn set_source_genesis(
	client: &OnlineClient<PolkadotConfig>,
	source: u32,
	genesis: [u8; 32],
) -> Result<(), anyhow::Error> {
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
async fn source_capability_discovery_filters_non_servers() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let logs = "-lsource-discovery=trace,bootnodes=trace";
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
		// Source para 2000: two collators — a1 serves the capability, a2 does not.
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2000")
				.with_collator(|n| {
					n.with_name("penpal-a1").with_args(vec![logs.into(), "--spec-msg-serve".into()])
				})
				.with_collator(|n| n.with_name("penpal-a2").with_args(vec![logs.into()]))
		})
		// Receiver para 2001: discovers 2000 under the capability.
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2001")
				.with_collator(|n| n.with_name("penpal-b").with_args(vec![logs.into()]))
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
	let a1_client: OnlineClient<PolkadotConfig> =
		network.get_node("penpal-a1")?.wait_client().await?;
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

	// B learns how to reach source 2000's collators (governance-gated, on-chain).
	let genesis_a = a1_client.genesis_hash();
	log::info!("Setting B's source genesis for 2000");
	set_source_genesis(&b_client, PARA_A, genesis_a.0).await?;

	let penpal_a1 = network.get_node("penpal-a1")?;
	let penpal_a2 = network.get_node("penpal-a2")?;
	let penpal_b = network.get_node("penpal-b")?;
	let found = LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(600), false);
	let absent = LogLineCountOptions::new(|n| n == 0, Duration::from_secs(30), false);

	// (1) a1 is the serving node: it started the capability advertisement.
	log::info!("Asserting a1 advertises the capability");
	assert!(
		penpal_a1
			.wait_log_line_count_with_timeout(
				"Starting DHT capability advertisement.*spec-msg/v1",
				false,
				found.clone(),
			)
			.await?
			.success(),
		"penpal-a1 did not start capability advertisement",
	);

	// (2) a2 is a genuine distractor: up and advertising the *plain* key (a plain query would find
	//     it) — but it is NOT serving the capability.
	assert!(
		penpal_a2
			.wait_log_line_count_with_timeout(
				"advertisement of bootnode for .*epoch key",
				false,
				found.clone(),
			)
			.await?
			.success(),
		"penpal-a2 did not advertise the plain bootnode key (distractor not up?)",
	);
	assert!(
		penpal_a2
			.wait_log_line_count_with_timeout(
				"Starting DHT capability advertisement",
				false,
				absent.clone(),
			)
			.await?
			.success(),
		"penpal-a2 unexpectedly advertised the capability",
	);

	// (3) B's capability query resolves EXACTLY ONE peer for source 2000 — a1.
	log::info!("Asserting B discovered exactly the serving peer (a1)");
	assert!(
		penpal_b
			.wait_log_line_count_with_timeout(
				"Discovered source peers.*source=2000.*count=1",
				false,
				found,
			)
			.await?
			.success(),
		"penpal-b did not discover the serving peer under the capability key",
	);

	// (4) THE FILTER: B never sees count=2 for source 2000 — it never resolves the non-serving a2.
	//     A capability query can only return capability advertisers, so a2 (plain-only) is
	// invisible;     if this fired, discovery queried the wrong (plain) key.
	assert!(
		penpal_b
			.wait_log_line_count_with_timeout(
				"Discovered source peers.*source=2000.*count=2",
				false,
				absent.clone(),
			)
			.await?
			.success(),
		"penpal-b resolved the non-serving collator under the capability key (filter broken)",
	);

	// (5) And no `/paranode` protocol error on the discovery path.
	assert!(
		penpal_b
			.wait_log_line_count_with_timeout("paranode.*protocol.*error", false, absent)
			.await?
			.success(),
		"penpal-b hit a /paranode protocol error",
	);

	Ok(())
}

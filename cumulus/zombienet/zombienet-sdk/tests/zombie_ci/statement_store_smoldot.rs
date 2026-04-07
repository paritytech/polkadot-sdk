// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! # Statement Store — Smoldot Light Client E2E Tests
//!
//! End-to-end tests that verify the statement store protocol (V1 gossip and V2 topic affinity)
//! works correctly when a **smoldot light client** participates in the network alongside
//! full nodes.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────┐  statement/2   ┌───────────────┐
//!  │  full node    │◄──────────────►│  smoldot      │
//!  │  (charlie)    │   gossip       │  (Node.js WS) │
//!  └──────────────┘                └───────┬───────┘
//!         ▲                                │
//!         │ submit / subscribe             │ subscribe / submit
//!         │ via subxt RPC                  │ via subxt RPC
//!         └──────────── test code ─────────┘
//! ```
//!
//! Each test:
//! 1. Spawns a fresh zombienet network (westend-local relay chain + People-Westend parachain with
//!    `--enable-statement-store`).
//! 2. Starts a **smoldot JS/WASM process** (`smoldot-rpc-proxy.mjs`) that acts as a WebSocket
//!    JSON-RPC proxy. Each inbound WebSocket connection gets its own smoldot chain instances with
//!    the statement protocol enabled (including V2 bloom-filter topic affinity).
//! 3. Extracts chain specs from the zombienet base directory, injects bootnode multiaddresses from
//!    the running nodes, fixes `para_id` and `relay_chain` fields for smoldot compatibility, and
//!    writes modified specs for the smoldot process to consume.
//! 4. Connects to smoldot via standard `subxt::RpcClient` (identical to connecting to a full node)
//!    and exercises the statement JSON-RPC APIs.
//!
//! ## Statement JSON-RPC APIs tested
//!
//! - `statement_submit(encoded)` — submit a SCALE-encoded statement for broadcast
//! - `statement_subscribeStatement(filter)` — subscribe to statements matching a topic filter
//! - `statement_unsubscribeStatement(subscription)` — unsubscribe (via drop)
//! - `statement_statement` server notification — delivers matched statements to subscribers
//!
//! ## TopicFilter variants tested
//!
//! - `Any` — matches all statements regardless of topics
//! - `MatchAll(topics)` — statement must contain ALL specified topics
//! - `MatchAny(topics)` — statement must contain at least ONE specified topic
//!
//! ## Protocol versions
//!
//! The parachain nodes negotiate **statement/2** (V2) with smoldot, which enables:
//! - Bloom-filter-based topic affinity advertisements
//! - Initial sync: when affinity changes, the full node re-sends all matching statements from its
//!   store
//!
//! The relay chain nodes negotiate statement/1 (V1) which is broadcast-only — this is
//! expected since the relay chain validators do not have the statement store enabled.
//!
//! ## Prerequisites
//!
//! - `polkadot` and `polkadot-parachain` release binaries on `PATH`
//! - `node` (v18+) on `PATH`
//! - `SMOLDOT_JS_PATH` env var pointing to a pre-built smoldot `wasm-node/javascript` directory
//!   (run `npm run build` there first)
//! - `ZOMBIE_PROVIDER=native` (set by `run_smoldot_tests.sh` or manually)

use anyhow::anyhow;
use log::info;
use sp_core::{Bytes, Encode};
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};
use zombienet_sdk::{
	LocalFileSystem, Network,
	subxt::{
		backend::rpc::RpcClient,
		ext::subxt_rpcs::{client::RpcSubscription, rpc_params},
	},
};

use crate::zombie_ci::statement_store::common::spawn_network_with_injected_allowances;
use sc_statement_store::test_utils::get_keypair;

/// Path to the smoldot wasm-node/javascript directory.
/// Must be set via SMOLDOT_JS_PATH env var. The directory must contain a pre-built
/// `dist/mjs/index-nodejs.js` (run `npm run build` in the smoldot wasm-node/javascript dir).
fn smoldot_js_path() -> String {
	std::env::var("SMOLDOT_JS_PATH")
		.expect("SMOLDOT_JS_PATH env var must be set to the smoldot wasm-node/javascript directory")
}

/// Paths to chain spec files written for smoldot consumption.
struct ChainSpecPaths {
	relay: String,
	para: String,
}

/// Injects bootnode multiaddresses into a chain spec JSON string.
fn inject_bootnodes(spec_json: &str, multiaddrs: &[&str]) -> Result<String, anyhow::Error> {
	let mut spec: serde_json::Value = serde_json::from_str(spec_json)?;
	let bootnodes: Vec<String> = multiaddrs.iter().map(|s| s.to_string()).collect();
	spec["bootNodes"] = serde_json::json!(bootnodes);
	Ok(serde_json::to_string(&spec)?)
}

/// Reads chain specs from the zombienet base dir, injects bootnode multiaddresses
/// from the running nodes, and writes modified specs to disk for smoldot.
fn prepare_chain_specs(
	network: &Network<LocalFileSystem>,
	base_dir: &str,
) -> Result<ChainSpecPaths, anyhow::Error> {
	let validator_0 = network.get_node("validator-0")?;
	let validator_1 = network.get_node("validator-1")?;
	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	// Read relay chain spec
	let relay_spec_path = format!("{base_dir}/westend-local.json");
	let relay_spec_json = std::fs::read_to_string(&relay_spec_path)
		.map_err(|e| anyhow!("Failed to read relay spec at {relay_spec_path}: {e}"))?;

	// Read parachain spec (written by create_chain_spec_with_allowances)
	let para_spec_path = format!("{base_dir}/people-westend-custom.json");
	let para_spec_json = std::fs::read_to_string(&para_spec_path)
		.map_err(|e| anyhow!("Failed to read para spec at {para_spec_path}: {e}"))?;

	// Inject bootnode multiaddresses
	let relay_spec =
		inject_bootnodes(&relay_spec_json, &[validator_0.multiaddr(), validator_1.multiaddr()])?;
	let mut para_spec =
		inject_bootnodes(&para_spec_json, &[charlie.multiaddr(), dave.multiaddr()])?;

	// Fix parachain spec for smoldot:
	// - Set para_id (smoldot requires it)
	// - Match relay_chain to the relay spec's actual id
	{
		let relay_parsed: serde_json::Value = serde_json::from_str(&relay_spec)?;
		let relay_id = relay_parsed["id"].as_str().unwrap_or("westend_local_testnet");

		let mut spec: serde_json::Value = serde_json::from_str(&para_spec)?;
		spec["para_id"] = serde_json::json!(2400);
		spec["relay_chain"] = serde_json::json!(relay_id);
		para_spec = serde_json::to_string(&spec)?;
	}

	// Write modified specs
	let smoldot_relay_path = format!("{base_dir}/smoldot-relay-spec.json");
	let smoldot_para_path = format!("{base_dir}/smoldot-para-spec.json");
	std::fs::write(&smoldot_relay_path, &relay_spec)?;
	std::fs::write(&smoldot_para_path, &para_spec)?;

	info!("Wrote smoldot chain specs: relay={smoldot_relay_path}, para={smoldot_para_path}");

	Ok(ChainSpecPaths { relay: smoldot_relay_path, para: smoldot_para_path })
}

/// Starts a smoldot JS process as a WebSocket RPC proxy on the given port.
/// Waits for the "SMOLDOT_READY" signal on stdout before returning.
async fn start_smoldot_proxy(
	specs: &ChainSpecPaths,
	port: u16,
) -> Result<tokio::process::Child, anyhow::Error> {
	let script_path =
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/zombie_ci/smoldot-rpc-proxy.mjs");

	info!("Starting smoldot proxy on port {port} with script {}", script_path.display());

	let mut child = tokio::process::Command::new("node")
		.arg("--max-old-space-size=8192")
		.arg(&script_path)
		.env("RELAY_CHAIN_SPEC", &specs.relay)
		.env("PARA_CHAIN_SPEC", &specs.para)
		.env("SMOLDOT_PORT", port.to_string())
		.env("SMOLDOT_JS_PATH", smoldot_js_path())
		// Propagate memory limit to worker threads
		.env("NODE_OPTIONS", "--max-old-space-size=8192")
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.map_err(|e| anyhow!("Failed to spawn smoldot process: {e}"))?;

	// Read stdout looking for the readiness signal
	let stdout = child.stdout.take().ok_or(anyhow!("No stdout from smoldot process"))?;
	let mut reader = BufReader::new(stdout).lines();

	tokio::time::timeout(Duration::from_secs(60), async {
		while let Some(line) = reader.next_line().await? {
			info!("[smoldot-{port}] {line}");
			if line.contains("SMOLDOT_READY") {
				return Ok::<(), anyhow::Error>(());
			}
		}
		Err(anyhow!("Smoldot process exited before becoming ready"))
	})
	.await
	.map_err(|_| anyhow!("Timeout waiting for smoldot to become ready on port {port}"))??;

	// Spawn a background task to drain remaining stdout so the process doesn't block
	tokio::spawn(async move {
		while let Ok(Some(line)) = reader.next_line().await {
			info!("[smoldot-stdout] {line}");
		}
	});

	// Also drain stderr in the background
	if let Some(stderr) = child.stderr.take() {
		let mut stderr_reader = BufReader::new(stderr).lines();
		tokio::spawn(async move {
			while let Ok(Some(line)) = stderr_reader.next_line().await {
				info!("[smoldot-stderr] {line}");
			}
		});
	}

	info!("Smoldot proxy ready on port {port}");
	Ok(child)
}

/// Connects to smoldot's WebSocket RPC endpoint, retrying until it has at least
/// one peer (indicating it synced with the network).
/// Keeps a persistent connection to avoid re-triggering expensive addChain on each retry.
async fn connect_smoldot_with_peers(
	url: &str,
	timeout_duration: Duration,
) -> Result<RpcClient, anyhow::Error> {
	let deadline = tokio::time::Instant::now() + timeout_duration;

	// First, wait until smoldot accepts connections
	let rpc = loop {
		if tokio::time::Instant::now() > deadline {
			return Err(anyhow!("Smoldot at {url} did not accept connections within timeout"));
		}
		match RpcClient::from_insecure_url(url).await {
			Ok(rpc) => break rpc,
			Err(e) => {
				info!("Cannot connect to smoldot at {url}: {e}, retrying...");
				tokio::time::sleep(Duration::from_secs(2)).await;
			},
		}
	};

	info!("Connected to smoldot at {url}, waiting for peers...");

	// Now poll system_health on the same connection until peers > 0
	loop {
		if tokio::time::Instant::now() > deadline {
			return Err(anyhow!("Smoldot at {url} did not get peers within timeout"));
		}

		match tokio::time::timeout(
			Duration::from_secs(30),
			rpc.request::<serde_json::Value>("system_health", rpc_params![]),
		)
		.await
		{
			Ok(Ok(health)) => {
				let peers = health.get("peers").and_then(|p| p.as_u64()).unwrap_or(0);
				let is_syncing = health.get("isSyncing").and_then(|s| s.as_bool()).unwrap_or(true);
				info!("Smoldot at {url}: peers={peers}, syncing={is_syncing}");
				if peers > 0 {
					info!("Smoldot at {url} has {peers} peers, ready");
					return Ok(rpc);
				}
			},
			Ok(Err(e)) => {
				info!("Smoldot health check error: {e}, retrying...");
			},
			Err(_) => {
				info!("Smoldot health check timed out (30s), retrying...");
			},
		}

		tokio::time::sleep(Duration::from_secs(5)).await;
	}
}

/// Creates a signed statement with a single topic and data.
fn create_test_statement(topic: Topic, data: Vec<u8>) -> Bytes {
	create_test_statement_multi(&[topic], data)
}

/// Creates a signed statement with multiple topics and data.
fn create_test_statement_multi(topics: &[Topic], data: Vec<u8>) -> Bytes {
	let mut statement = sp_statement_store::Statement::new();
	statement.set_plain_data(data);
	for (i, topic) in topics.iter().enumerate().take(4) {
		statement.set_topic(i, *topic);
	}
	statement.set_expiry_from_parts(u32::MAX, 0);
	let keypair = get_keypair(0);
	statement.sign_sr25519_private(&keypair);
	statement.encode().into()
}

/// Waits for a statement event on a subscription, with timeout.
/// Skips empty batches and returns the first non-empty batch.
async fn receive_statement(
	subscription: &mut RpcSubscription<StatementEvent>,
	timeout_secs: u64,
) -> Result<Bytes, anyhow::Error> {
	loop {
		let event = tokio::time::timeout(Duration::from_secs(timeout_secs), subscription.next())
			.await
			.map_err(|_| anyhow!("Timeout waiting for statement ({timeout_secs}s)"))?
			.ok_or(anyhow!("Subscription stream ended"))?
			.map_err(|e| anyhow!("Subscription error: {e}"))?;

		match event {
			StatementEvent::NewStatements { mut statements, .. } => {
				if statements.is_empty() {
					continue;
				}
				return Ok(statements.remove(0));
			},
		}
	}
}

/// Like receive_statement, but returns None on timeout instead of erroring.
async fn try_receive_statement(
	subscription: &mut RpcSubscription<StatementEvent>,
	timeout_secs: u64,
) -> Option<Bytes> {
	loop {
		match tokio::time::timeout(Duration::from_secs(timeout_secs), subscription.next()).await {
			Ok(Some(Ok(StatementEvent::NewStatements { mut statements, .. }))) => {
				if statements.is_empty() {
					continue;
				}
				return Some(statements.remove(0));
			},
			_ => return None,
		}
	}
}

/// Collects all statements from a subscription until no new ones arrive for `idle_timeout_secs`.
/// Returns deduplicated set of received statement bytes.
async fn collect_statements(
	subscription: &mut RpcSubscription<StatementEvent>,
	idle_timeout_secs: u64,
) -> Vec<Bytes> {
	let mut all = Vec::new();
	loop {
		match tokio::time::timeout(Duration::from_secs(idle_timeout_secs), subscription.next())
			.await
		{
			Ok(Some(Ok(StatementEvent::NewStatements { statements, .. }))) => {
				all.extend(statements);
			},
			_ => break,
		}
	}
	// Deduplicate (smoldot may receive same statement from multiple peers)
	all.sort();
	all.dedup();
	all
}

/// Shared setup: spawns network + smoldot on given port.
/// Returns (network, smoldot process, smoldot RPC client).
async fn setup_smoldot_test(
	port: u16,
) -> Result<(Network<LocalFileSystem>, tokio::process::Child, RpcClient), anyhow::Error> {
	// Use a fresh unique base dir to avoid stale chain data from previous runs
	// causing smoldot warp sync failures (justification targeting unknown blocks).
	let fresh_dir =
		std::env::temp_dir().join(format!("zombienet-smoldot-{}-{}", std::process::id(), port));
	// Clean any stale data
	let _ = std::fs::remove_dir_all(&fresh_dir);
	std::env::set_var("ZOMBIENET_SDK_BASE_DIR", &fresh_dir);

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;
	let base_dir = network.base_dir().ok_or(anyhow!("no base dir"))?;
	let specs = prepare_chain_specs(&network, base_dir)?;
	let smoldot = start_smoldot_proxy(&specs, port).await?;
	let rpc =
		connect_smoldot_with_peers(&format!("ws://127.0.0.1:{port}"), Duration::from_secs(180))
			.await?;
	Ok((network, smoldot, rpc))
}

fn init_logger() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
}

// ---------------------------------------------------------------------------
// Test 1: Full node submits → smoldot receives
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_fullnode_to_smoldot() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	info!("=== Test: fullnode → smoldot ===");

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;
	let base_dir = network.base_dir().ok_or(anyhow!("no base dir"))?;
	let specs = prepare_chain_specs(&network, base_dir)?;

	let mut smoldot = start_smoldot_proxy(&specs, 19944).await?;
	let smoldot_rpc =
		connect_smoldot_with_peers("ws://127.0.0.1:19944", Duration::from_secs(180)).await?;

	// Subscribe to statements on smoldot
	let topic: Topic = [0u8; 32].into();
	let mut subscription = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed to statements on smoldot");

	// Submit statement to full node (charlie)
	let charlie_rpc = network.get_node("charlie")?.rpc().await?;
	let statement_bytes = create_test_statement(topic, vec![1, 2, 3]);
	let _: SubmitResult = charlie_rpc
		.request("statement_submit", rpc_params![statement_bytes.clone()])
		.await?;
	info!("Statement submitted to charlie");

	// Verify received on smoldot
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, statement_bytes, "Received statement should match submitted");
	info!("Statement received on smoldot - PASSED");

	// Verify no more statements arrive
	assert!(
		tokio::time::timeout(Duration::from_secs(20), subscription.next())
			.await
			.is_err(),
		"Should not receive more statements"
	);

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 2: Smoldot submits → full node receives
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_smoldot_submit() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	info!("=== Test: smoldot → fullnode ===");

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;
	let base_dir = network.base_dir().ok_or(anyhow!("no base dir"))?;
	let specs = prepare_chain_specs(&network, base_dir)?;

	let mut smoldot = start_smoldot_proxy(&specs, 19945).await?;
	let smoldot_rpc =
		connect_smoldot_with_peers("ws://127.0.0.1:19945", Duration::from_secs(180)).await?;

	// Subscribe on full node (dave)
	let dave_rpc = network.get_node("dave")?.rpc().await?;
	let topic: Topic = [0u8; 32].into();
	let mut subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed to statements on dave (full node)");

	// Submit through smoldot
	let statement_bytes = create_test_statement(topic, vec![4, 5, 6]);
	let _: SubmitResult = smoldot_rpc
		.request("statement_submit", rpc_params![statement_bytes.clone()])
		.await?;
	info!("Statement submitted through smoldot");

	// Verify received on full node
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, statement_bytes, "Received statement should match submitted");
	info!("Statement received on dave (full node) - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 3: Smoldot-to-smoldot (long running, multiple rounds)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_smoldot_to_smoldot() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	info!("=== Test: smoldot → smoldot (long running) ===");

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;
	let base_dir = network.base_dir().ok_or(anyhow!("no base dir"))?;
	let specs = prepare_chain_specs(&network, base_dir)?;

	// Start two independent smoldot instances
	let mut smoldot_a = start_smoldot_proxy(&specs, 19946).await?;
	let mut smoldot_b = start_smoldot_proxy(&specs, 19947).await?;

	let rpc_a =
		connect_smoldot_with_peers("ws://127.0.0.1:19946", Duration::from_secs(180)).await?;
	let rpc_b =
		connect_smoldot_with_peers("ws://127.0.0.1:19947", Duration::from_secs(180)).await?;

	let topic: Topic = [0u8; 32].into();
	let num_rounds = 5;

	for round in 0..num_rounds {
		info!("=== Round {}/{num_rounds} ===", round + 1);

		// Subscribe on smoldot-B
		let mut subscription = rpc_b
			.subscribe::<StatementEvent>(
				"statement_subscribeStatement",
				rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
				"statement_unsubscribeStatement",
			)
			.await?;

		// Submit from smoldot-A
		let data = vec![round as u8; 3];
		let statement_bytes = create_test_statement(topic, data);
		let _: SubmitResult =
			rpc_a.request("statement_submit", rpc_params![statement_bytes.clone()]).await?;
		info!("Round {}: statement submitted from smoldot-A", round + 1);

		// Verify received on smoldot-B
		let received = receive_statement(&mut subscription, 60).await?;
		assert_eq!(received, statement_bytes, "Round {}: statement mismatch", round + 1);
		info!("Round {}/{num_rounds}: PASSED", round + 1);

		// Small delay between rounds
		tokio::time::sleep(Duration::from_secs(2)).await;
	}

	smoldot_a.kill().await?;
	smoldot_b.kill().await?;
	info!("All {num_rounds} rounds passed for smoldot-to-smoldot");
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 4: TopicFilter::Any — receive all statements regardless of topic
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_topic_filter_any() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: TopicFilter::Any ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19948).await?;

	// Subscribe with "Any" filter on smoldot — should receive ALL statements
	let mut subscription = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::Any],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed with TopicFilter::Any");

	// Submit statements with different topics
	let charlie_rpc = network.get_node("charlie")?.rpc().await?;
	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();

	let stmt_a = create_test_statement(topic_a, vec![10]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_a.clone()]).await?;
	info!("Submitted statement with topic A");

	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt_a, "Should receive statement with topic A");
	info!("Received statement A via Any filter");

	let stmt_b = create_test_statement(topic_b, vec![20]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_b.clone()]).await?;
	info!("Submitted statement with topic B");

	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt_b, "Should receive statement with topic B");
	info!("Received statement B via Any filter - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 5: TopicFilter::MatchAny — receive if any topic matches
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_topic_filter_match_any() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: TopicFilter::MatchAny ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19949).await?;

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();
	let topic_c: Topic = [0xCCu8; 32].into();

	// Subscribe with MatchAny([topic_a, topic_b]) — should receive statements with A or B
	let mut subscription = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAny(
				vec![topic_a, topic_b].try_into().expect("Two topics")
			)],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed with MatchAny([topic_a, topic_b])");

	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	// Statement with topic_a → should be received
	let stmt_a = create_test_statement(topic_a, vec![10]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_a.clone()]).await?;
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt_a, "Should receive statement with topic A");
	info!("MatchAny: received topic A - OK");

	// Statement with topic_b → should be received
	let stmt_b = create_test_statement(topic_b, vec![20]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_b.clone()]).await?;
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt_b, "Should receive statement with topic B");
	info!("MatchAny: received topic B - OK");

	// Statement with topic_c → should NOT be received
	let stmt_c = create_test_statement(topic_c, vec![30]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_c.clone()]).await?;
	let not_received = try_receive_statement(&mut subscription, 15).await;
	assert!(not_received.is_none(), "Should NOT receive statement with topic C");
	info!("MatchAny: correctly filtered out topic C - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 6: MatchAll with multiple topics on a single statement
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_multi_topic_match_all() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: MatchAll with multiple topics ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19950).await?;

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();

	// Subscribe requiring BOTH topic_a AND topic_b
	let mut subscription = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(
				vec![topic_a, topic_b].try_into().expect("Two topics")
			)],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed with MatchAll([topic_a, topic_b])");

	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	// Statement with only topic_a → should NOT match MatchAll([a,b])
	let stmt_a_only = create_test_statement(topic_a, vec![10]);
	let _: SubmitResult = charlie_rpc
		.request("statement_submit", rpc_params![stmt_a_only.clone()])
		.await?;
	let not_received = try_receive_statement(&mut subscription, 15).await;
	assert!(not_received.is_none(), "Should NOT receive statement with only topic A");
	info!("MatchAll: correctly filtered single-topic statement - OK");

	// Statement with both topic_a and topic_b → should match
	let stmt_both = create_test_statement_multi(&[topic_a, topic_b], vec![99]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_both.clone()]).await?;
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt_both, "Should receive multi-topic statement");
	info!("MatchAll: received multi-topic statement - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 7: Topic filtering correctness — negative test
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_topic_filtering_negative() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: topic filtering negative ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19951).await?;

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();

	// Subscribe only to topic_a
	let mut sub_a = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic_a].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed to topic A only");

	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	// Send statement with topic_b → should NOT be received on sub_a
	let stmt_b = create_test_statement(topic_b, vec![20]);
	let _: SubmitResult = charlie_rpc.request("statement_submit", rpc_params![stmt_b]).await?;
	let not_received = try_receive_statement(&mut sub_a, 15).await;
	assert!(not_received.is_none(), "Sub A should NOT receive topic B statement");
	info!("Correctly filtered out non-matching topic");

	// Send statement with topic_a → should be received
	let stmt_a = create_test_statement(topic_a, vec![10]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_a.clone()]).await?;
	let received = receive_statement(&mut sub_a, 60).await?;
	assert_eq!(received, stmt_a);
	info!("Correctly received matching topic - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 8: statement_submit invalid data
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_submit_invalid() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: submit invalid statement ===");

	let (_network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19952).await?;

	// Submit garbage data — should return an error/invalid result.
	// Deserialize as raw JSON because smoldot's InvalidReason is a free-form string
	// that doesn't match polkadot-sdk's typed InvalidReason enum.
	let garbage: Bytes = vec![0xDE, 0xAD, 0xBE, 0xEF].into();
	let result: serde_json::Value =
		smoldot_rpc.request("statement_submit", rpc_params![garbage]).await?;

	info!("Submit invalid result: {result}");
	let status = result["status"].as_str().unwrap_or("");
	assert!(
		status == "invalid" || status == "internalError",
		"Expected 'invalid' or 'internalError' status for garbage data, got: {result}"
	);
	info!("Got expected error status '{status}' for invalid statement - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 9: statement_unsubscribeStatement — explicit unsubscribe
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_unsubscribe() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: explicit unsubscribe ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19953).await?;

	let topic: Topic = [0u8; 32].into();

	// Subscribe
	let mut subscription = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::Any],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscribed with Any filter");

	// Verify subscription works by receiving a statement
	let charlie_rpc = network.get_node("charlie")?.rpc().await?;
	let stmt1 = create_test_statement(topic, vec![1]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt1.clone()]).await?;
	let received = receive_statement(&mut subscription, 60).await?;
	assert_eq!(received, stmt1);
	info!("Received statement before unsubscribe - OK");

	// Unsubscribe (drop the subscription, which triggers unsubscribe via the unsub method)
	drop(subscription);
	info!("Unsubscribed");

	// Small delay to let unsubscribe propagate
	tokio::time::sleep(Duration::from_secs(2)).await;

	// Create a new subscription to verify the old one is gone
	let mut subscription2 = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::Any],
			"statement_unsubscribeStatement",
		)
		.await?;

	// Submit another statement
	let stmt2 = create_test_statement(topic, vec![2]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt2.clone()]).await?;

	// New subscription should receive it
	let received = receive_statement(&mut subscription2, 60).await?;
	assert_eq!(received, stmt2);
	info!("New subscription works after old was dropped - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 10: Multiple concurrent subscriptions with different filters
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_multiple_subscriptions() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: multiple concurrent subscriptions ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19954).await?;

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();

	// Create two subscriptions with different filters on the same connection
	let mut sub_a = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic_a].try_into().expect("topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscription A: MatchAll([topic_a])");

	let mut sub_b = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic_b].try_into().expect("topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Subscription B: MatchAll([topic_b])");

	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	// Send statement with topic_a → sub_a should receive, sub_b should not
	let stmt_a = create_test_statement(topic_a, vec![10]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_a.clone()]).await?;

	let recv_a = receive_statement(&mut sub_a, 60).await?;
	assert_eq!(recv_a, stmt_a, "Sub A should receive topic A statement");
	info!("Sub A received topic A - OK");

	let recv_b = try_receive_statement(&mut sub_b, 10).await;
	assert!(recv_b.is_none(), "Sub B should NOT receive topic A statement");
	info!("Sub B correctly filtered topic A - OK");

	// Send statement with topic_b → sub_b should receive, sub_a should not
	let stmt_b = create_test_statement(topic_b, vec![20]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_b.clone()]).await?;

	let recv_b = receive_statement(&mut sub_b, 60).await?;
	assert_eq!(recv_b, stmt_b, "Sub B should receive topic B statement");
	info!("Sub B received topic B - OK");

	let recv_a = try_receive_statement(&mut sub_a, 10).await;
	assert!(recv_a.is_none(), "Sub A should NOT receive topic B statement");
	info!("Sub A correctly filtered topic B - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 11: Resubscribe after unsubscribe — the original statement should be
// re-delivered to the new subscription via initial sync (no new submission).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_resubscribe_receives_again() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: resubscribe receives original statement again ===");

	let (network, mut smoldot, smoldot_rpc) = setup_smoldot_test(19960).await?;

	let topic: Topic = [0x11u8; 32].into();
	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	// --- Round 1: subscribe, submit, receive, unsubscribe ---
	let mut sub1 = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Round 1: subscribed");

	let stmt = create_test_statement(topic, vec![1]);
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt.clone()]).await?;

	let received = receive_statement(&mut sub1, 60).await?;
	assert_eq!(received, stmt);
	info!("Round 1: received statement - OK");

	// Unsubscribe
	drop(sub1);
	info!("Round 1: unsubscribed");
	tokio::time::sleep(Duration::from_secs(3)).await;

	// --- Round 2: resubscribe on same topic — the original statement should be
	// re-delivered via initial sync without submitting anything new ---
	let mut sub2 = smoldot_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("Round 2: resubscribed on same topic (no new submission)");

	let received = receive_statement(&mut sub2, 30).await?;
	assert_eq!(received, stmt);
	info!("Round 2: received original statement via initial sync - PASSED");

	smoldot.kill().await?;
	Ok(())
}

// ---------------------------------------------------------------------------
// Test 12: All filter variants in one test — submit 4 statements with different
// topics, verify Any / MatchAll / MatchAny each receive the correct subset.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_initial_sync_all_filters() -> Result<(), anyhow::Error> {
	init_logger();
	info!("=== Test: all filter variants with 4 statements ===");

	// 1. Spawn the network and start smoldot (fresh base dir to avoid stale data)
	let fresh_dir =
		std::env::temp_dir().join(format!("zombienet-smoldot-{}-19956", std::process::id()));
	let _ = std::fs::remove_dir_all(&fresh_dir);
	std::env::set_var("ZOMBIENET_SDK_BASE_DIR", &fresh_dir);

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;
	let base_dir = network.base_dir().ok_or(anyhow!("no base dir"))?;
	let specs = prepare_chain_specs(&network, base_dir)?;
	let mut smoldot = start_smoldot_proxy(&specs, 19956).await?;
	let url = "ws://127.0.0.1:19956";

	// 2. Connect three independent RPC clients (each gets its own chain instance)
	let rpc_any = connect_smoldot_with_peers(url, Duration::from_secs(180)).await?;
	let rpc_match_all = connect_smoldot_with_peers(url, Duration::from_secs(180)).await?;
	let rpc_match_any = connect_smoldot_with_peers(url, Duration::from_secs(180)).await?;
	info!("Three smoldot RPC clients connected");

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();
	let topic_c: Topic = [0xCCu8; 32].into();

	// 3. Subscribe with different filters BEFORE submitting statements Any → should receive all 4
	//    statements
	let mut sub_any = rpc_any
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::Any],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("sub_any: subscribed with Any");

	//    MatchAll([A,B]) → only stmt_ab (has both topics)
	let mut sub_match_all = rpc_match_all
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(
				vec![topic_a, topic_b].try_into().expect("two topics")
			)],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("sub_match_all: subscribed with MatchAll([A,B])");

	//    MatchAny([A,B]) → stmt_a, stmt_b, stmt_ab (any of A or B)
	let mut sub_match_any = rpc_match_any
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAny(
				vec![topic_a, topic_b].try_into().expect("two topics")
			)],
			"statement_unsubscribeStatement",
		)
		.await?;
	info!("sub_match_any: subscribed with MatchAny([A,B])");

	// 4. Small delay so bloom filters propagate to peers before submitting
	tokio::time::sleep(Duration::from_secs(3)).await;

	// 5. Submit four statements to charlie (full node)
	let charlie_rpc = network.get_node("charlie")?.rpc().await?;

	let stmt_a = create_test_statement(topic_a, vec![1]);
	let stmt_b = create_test_statement(topic_b, vec![2]);
	let stmt_ab = create_test_statement_multi(&[topic_a, topic_b], vec![3]);
	let stmt_c = create_test_statement(topic_c, vec![4]);

	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_a.clone()]).await?;
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_b.clone()]).await?;
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_ab.clone()]).await?;
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![stmt_c.clone()]).await?;
	info!("Submitted 4 statements to charlie");

	// 6. Collect statements from each subscription (15s idle timeout per sub)
	let any_stmts = collect_statements(&mut sub_any, 15).await;
	info!("sub_any collected {} statements", any_stmts.len());

	let match_all_stmts = collect_statements(&mut sub_match_all, 15).await;
	info!("sub_match_all collected {} statements", match_all_stmts.len());

	let match_any_stmts = collect_statements(&mut sub_match_any, 15).await;
	info!("sub_match_any collected {} statements", match_any_stmts.len());

	// 7. Verify: Any should have all 4
	for (label, expected) in
		[("stmt_a", &stmt_a), ("stmt_b", &stmt_b), ("stmt_ab", &stmt_ab), ("stmt_c", &stmt_c)]
	{
		assert!(
			any_stmts.contains(expected),
			"sub_any should contain {label}, got {} statements",
			any_stmts.len()
		);
	}
	info!("sub_any: contains all 4 statements - OK");

	// 8. Verify: MatchAll([A,B]) should have only stmt_ab
	assert!(match_all_stmts.contains(&stmt_ab), "sub_match_all should contain stmt_ab");
	assert!(
		!match_all_stmts.contains(&stmt_a),
		"sub_match_all should NOT contain stmt_a (single topic)"
	);
	assert!(
		!match_all_stmts.contains(&stmt_b),
		"sub_match_all should NOT contain stmt_b (single topic)"
	);
	assert!(
		!match_all_stmts.contains(&stmt_c),
		"sub_match_all should NOT contain stmt_c (wrong topic)"
	);
	info!("sub_match_all: correctly contains only stmt_ab - OK");

	// 9. Verify: MatchAny([A,B]) should have stmt_a, stmt_b, stmt_ab but NOT stmt_c
	for (label, expected) in [("stmt_a", &stmt_a), ("stmt_b", &stmt_b), ("stmt_ab", &stmt_ab)] {
		assert!(match_any_stmts.contains(expected), "sub_match_any should contain {label}");
	}
	assert!(
		!match_any_stmts.contains(&stmt_c),
		"sub_match_any should NOT contain stmt_c (topic C not in filter)"
	);
	info!("sub_match_any: correctly contains stmt_a, stmt_b, stmt_ab but not stmt_c - OK");

	info!("All filter variants with 4 statements - PASSED");
	smoldot.kill().await?;
	Ok(())
}

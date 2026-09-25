// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Soak of the v2 DHT statement path (gate on) under continuous load.
//!
//! Sizing is the point: `nodes > replication_factor > gossip_target + 1` (12 > 8 > 4) —
//! defects capping the replica set at `gossip_target + 1` copies are invisible below that.
//!
//! Every wave of load asserts that the subscriber received every ring statement, and that each
//! probe and three ring samples are stored on their `replication_factor` XOR-closest nodes (ring
//! ones also on the subscriber, whose subscription grants affinity) and nowhere else. The soak
//! ends with a scan of every node's log for statement errors. `STATEMENT_V2_SOAK_NODES` (default
//! 12) and `STATEMENT_V2_SOAK_SECS` (default 900) size the run.
//!
//! Runs on demand only: a dispatch of .github/workflows/zombienet_statement-store-soak.yml, or
//! locally — against the cluster given a kubeconfig:
//!
//! ```text
//! ZOMBIE_PROVIDER=k8s \
//! POLKADOT_IMAGE=docker.io/parity/polkadot:<tag> \
//! CUMULUS_IMAGE=docker.io/paritypr/polkadot-parachain-debug:<tag> \
//! NEXTEST_RETRIES=0 \
//! cargo nextest run -p cumulus-zombienet-sdk-tests --features zombie-ci --no-capture \
//!     -- statement_store_v2_dht_soak
//! ```
//!
//! or natively, node binaries in `PATH`, without the k8s variables. `NEXTEST_RETRIES=0`
//! overrides the repo default of 5: retrying a failed network soak wholesale is never useful.

use super::common::{
	assert_statements_match, base_dir, collator_args_v2, create_chain_spec_with_allowances,
	launch_network_with_commands, submit_at_rate, submit_statement, subscribe_topic,
	subscribe_topic_filter, wait_for_first_block, Load,
};
use anyhow::anyhow;
use codec::Encode;
use log::info;
use sp_crypto_hashing::blake2_256;
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use std::{
	collections::HashSet,
	path::Path,
	time::{Duration, Instant},
};
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
	LocalFileSystem, Network, NetworkNode,
};

const SOAK_SECS_ENV: &str = "STATEMENT_V2_SOAK_SECS";
const DEFAULT_SOAK_SECS: u64 = 900;
/// How many statement-store nodes to spawn: the 4 authoring collators plus soak-* full nodes for
/// the rest.
const NODES_ENV: &str = "STATEMENT_V2_SOAK_NODES";
const DEFAULT_NODES: usize = 12;
const STATEMENT_SET_PEER_LIMIT: usize = 100;
const CONNECTED_PEER_MARGIN: usize = 5;
const STATEMENT_TTL: Duration = Duration::from_secs(900);
const AUTHORING_COLLATORS: [&str; 4] = ["alice", "bob", "charlie", "dave"];
const REPLICATION_FACTOR: usize = 8;
const GOSSIP_TARGET: u32 = 3;
const PARTICIPANTS: u32 = 2_000;
const LOG_FILTER: &str = "info,statement-store=info,statement-gossip=debug";
const RING_RATE_PER_SECOND: usize = 20;
const RING_SECS: u64 = 45;
const PROBES_PER_WAVE: usize = 8;
const DELIVERY_TIMEOUT_SECS: u64 = 120;
const PLACEMENT_TIMEOUT_SECS: u64 = 90;
const CONNECTED_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_connected_peers";
const ELIGIBLE_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_eligible_peers";

fn env_parsed<T: std::str::FromStr>(name: &str) -> Result<Option<T>, anyhow::Error>
where
	T::Err: std::fmt::Display,
{
	std::env::var(name)
		.ok()
		.filter(|v| !v.is_empty())
		.map(|v| v.parse::<T>().map_err(|e| anyhow!("{name}: {e}")))
		.transpose()
}

fn xor_distance(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
	let mut distance = [0u8; 32];
	for ((distance, a), b) in distance.iter_mut().zip(a).zip(b) {
		*distance = a ^ b;
	}
	distance
}

/// Node indices by the XOR distance of their peer key to `topic`, closest first. Equal distances
/// would mean equal keys, so the order needs no tie-break.
fn ranked_by_distance(peer_keys: &[[u8; 32]], topic: Topic) -> Vec<usize> {
	let mut order: Vec<usize> = (0..peer_keys.len()).collect();
	order.sort_by_cached_key(|&idx| xor_distance(*topic, peer_keys[idx]));
	order
}

fn soak_topic(kind: &[u8], wave: u64, idx: u64) -> Topic {
	let mut preimage = Vec::with_capacity(kind.len() + 16);
	preimage.extend_from_slice(kind);
	preimage.extend_from_slice(&wave.to_le_bytes());
	preimage.extend_from_slice(&idx.to_le_bytes());
	blake2_256(&preimage).into()
}

/// A statement node and its open RPC connection.
struct NodeHandle<'a> {
	node: &'a NetworkNode,
	rpc: RpcClient,
}

impl NodeHandle<'_> {
	fn name(&self) -> &str {
		self.node.name()
	}
}

/// Salts the genesis with a junk storage key: the statement protocol name embeds the genesis
/// hash, so same-genesis networks on one cluster discover each other and cross-connect. The
/// runtime ignores the unknown key.
fn salt_chain_spec(path: &Path) -> Result<(), anyhow::Error> {
	let mut spec: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
	let top = spec
		.pointer_mut("/genesis/raw/top")
		.and_then(|top| top.as_object_mut())
		.ok_or_else(|| anyhow!("chain spec without genesis.raw.top"))?;
	let salt = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
	top.insert(
		format!("0x{}", hex::encode(b"soak-salt")),
		serde_json::json!(format!("0x{salt:032x}")),
	);
	std::fs::write(path, serde_json::to_string(&spec)?)?;
	Ok(())
}

/// Spawns the statement nodes on the v2 DHT path: the 4 authoring collators and one full node per
/// `full_node_names` entry
async fn launch_soak_network(
	full_node_names: &[String],
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let chain_spec_path = create_chain_spec_with_allowances(PARTICIPANTS, &base_dir()?)?;
	salt_chain_spec(&chain_spec_path)?;
	let args = collator_args_v2(PARTICIPANTS, LOG_FILTER, REPLICATION_FACTOR as u32, GOSSIP_TARGET);
	let collators: Vec<(&str, Option<&str>)> =
		AUTHORING_COLLATORS.iter().map(|&name| (name, None)).collect();
	let full_nodes: Vec<&str> = full_node_names.iter().map(String::as_str).collect();
	launch_network_with_commands(
		&collators,
		&full_nodes,
		&chain_spec_path,
		args,
		&[("STATEMENT_STORE_V2_DHT_ENABLED", "1")],
	)
	.await
}

/// Reads each node's peer id and maps it into the XOR topic space: `blake2_256` of the peer id
/// bytes, the key the node ranks replicas by.
async fn collect_peer_keys(nodes: &[NodeHandle<'_>]) -> Result<Vec<[u8; 32]>, anyhow::Error> {
	let mut keys = Vec::with_capacity(nodes.len());
	for handle in nodes {
		let peer_id: String = handle.rpc.request("system_localPeerId", rpc_params![]).await?;
		let id = bs58::decode(&peer_id)
			.into_vec()
			.map_err(|e| anyhow!("{}: cannot decode peer id {peer_id}: {e}", handle.name()))?;
		keys.push(blake2_256(&id));
	}
	Ok(keys)
}

/// Reads a node's persistent store from a fresh subscription's replay.
async fn store_snapshot(rpc: &RpcClient) -> Result<HashSet<Vec<u8>>, anyhow::Error> {
	let mut subscription = subscribe_topic_filter(rpc, TopicFilter::Any).await?;
	let mut snapshot = HashSet::new();
	loop {
		let event = tokio::time::timeout(Duration::from_secs(30), subscription.next())
			.await
			.map_err(|_| anyhow!("store replay stalled"))?
			.ok_or_else(|| anyhow!("subscription ended during store replay"))?
			.map_err(|e| anyhow!("subscription error during store replay: {e}"))?;
		let StatementEvent::NewStatements { statements, remaining } = event;
		let Some(remaining) = remaining else { continue };
		snapshot.extend(statements.iter().map(|bytes| bytes.to_vec()));
		if remaining == 0 {
			return Ok(snapshot);
		}
	}
}

struct WaveReport {
	ring_statements: usize,
	submit_time: Duration,
	verify_time: Duration,
}

async fn run_wave(
	wave: u64,
	nodes: &[NodeHandle<'_>],
	peer_keys: &[[u8; 32]],
	load: &mut Load,
) -> Result<WaveReport, anyhow::Error> {
	let ring_topic = soak_topic(b"soak-ring", wave, 0);
	let subscriber_idx = wave as usize % nodes.len();
	let subscriber = &nodes[subscriber_idx];
	let mut ring_subscription = subscribe_topic(&subscriber.rpc, ring_topic).await?;

	// Placement probes, each on its own topic, submitted via the XOR-farthest node: the entry
	// point with the longest route into the replica set.
	let mut probes = Vec::with_capacity(PROBES_PER_WAVE);
	for idx in 0..PROBES_PER_WAVE {
		let topic = soak_topic(b"soak-probe", wave, idx as u64);
		let order = ranked_by_distance(peer_keys, topic);
		let farthest = *order.last().expect("nodes are never empty");
		let statement = load.next_statement(wave, topic);
		let handle = &nodes[farthest];
		let result = submit_statement(&handle.rpc, &statement).await?;
		assert_eq!(result, SubmitResult::New, "wave {wave}: probe rejected on {}", handle.name());
		probes.push((statement.encode(), order));
	}

	// Ring load: 900 statements on the wave topic at 20/s, round-robin over every node so both
	// entry paths are exercised: a replica persists, a non-replica forwards and holds transiently.
	let targets: Vec<_> = nodes.iter().map(|handle| (handle.name(), &handle.rpc)).collect();
	let started = Instant::now();
	let expected =
		submit_at_rate(load, wave, ring_topic, RING_RATE_PER_SECOND, RING_SECS, &targets).await?;
	let submit_time = started.elapsed();

	// Delivery: the subscriber receives every ring statement of the wave.
	let verify_started = Instant::now();
	assert_statements_match(
		&mut ring_subscription,
		&expected,
		DELIVERY_TIMEOUT_SECS,
		subscriber.name(),
	)
	.await?;
	let verify_time = verify_started.elapsed();

	// Every placement expectation of the wave: each probe on its own replica set, plus three
	// ring samples (first, middle, last — the spread catches a replica that stops persisting
	// mid-wave) on the ring holders: the ring topic's replicas and the subscriber, whose
	// subscription grants explicit affinity.
	let ring_order = ranked_by_distance(peer_keys, ring_topic);
	let mut ring_holders: Vec<usize> = ring_order[..REPLICATION_FACTOR].to_vec();
	if !ring_holders.contains(&subscriber_idx) {
		ring_holders.push(subscriber_idx);
	}
	let mut expectations: Vec<(String, Vec<u8>, Vec<usize>)> = probes
		.iter()
		.enumerate()
		.map(|(idx, (encoded, order))| {
			(format!("probe {idx}"), encoded.clone(), order[..REPLICATION_FACTOR].to_vec())
		})
		.collect();
	for idx in [0, expected.len() / 2, expected.len() - 1] {
		expectations.push(("ring".to_string(), expected[idx].clone(), ring_holders.clone()));
	}

	// Placement converges from both sides: a replica may still be receiving the statement, and
	// a non-replica that submitted one holds it until propagation drains it. Poll until every
	// expectation matches, and report whatever still disagrees at the deadline.
	let deadline = Instant::now() + Duration::from_secs(PLACEMENT_TIMEOUT_SECS);
	loop {
		let snapshots =
			futures::future::try_join_all(nodes.iter().map(|handle| store_snapshot(&handle.rpc)))
				.await?;
		let disagreement = expectations.iter().find_map(|(context, blob, holders)| {
			if let Some(&idx) = holders.iter().find(|&&holder| !snapshots[holder].contains(blob)) {
				return Some(format!(
					"wave {wave} {context}: {} did not store the statement",
					nodes[idx].name()
				));
			}
			(0..nodes.len())
				.find(|idx| !holders.contains(idx) && snapshots[*idx].contains(blob))
				.map(|idx| {
					format!(
						"wave {wave} {context}: {} stores a statement it is not a replica for",
						nodes[idx].name()
					)
				})
		});
		match disagreement {
			None => break,
			Some(problem) if Instant::now() >= deadline => {
				return Err(anyhow!("{problem}, {PLACEMENT_TIMEOUT_SECS}s after the wave"))
			},
			Some(_) => tokio::time::sleep(Duration::from_secs(2)).await,
		}
	}

	Ok(WaveReport { ring_statements: expected.len(), submit_time, verify_time })
}

// .github/zombienet-tests/zombienet_statement_store_soak_tests.yml holds the size at 40 until
// paritytech/litep2p#665 is fixed.
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_v2_dht_soak() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let soak_secs = env_parsed::<u64>(SOAK_SECS_ENV)?.unwrap_or(DEFAULT_SOAK_SECS);
	let statement_node_count = env_parsed::<usize>(NODES_ENV)?.unwrap_or(DEFAULT_NODES);
	assert!(
		statement_node_count > REPLICATION_FACTOR && REPLICATION_FACTOR as u32 > GOSSIP_TARGET + 1
	);

	let full_node_names: Vec<String> = (0..statement_node_count - AUTHORING_COLLATORS.len())
		.map(|i| format!("soak-{i}"))
		.collect();
	let network = launch_soak_network(&full_node_names).await?;

	let mut nodes = Vec::with_capacity(statement_node_count);
	for name in AUTHORING_COLLATORS.iter().map(|n| n.to_string()).chain(full_node_names) {
		let node = network.get_node(name.as_str())?;
		let rpc = node.rpc().await?;
		nodes.push(NodeHandle { node, rpc });
	}

	info!("Waiting for the parachain to produce blocks...");
	wait_for_first_block(&[nodes[0].node], 300).await?;

	// The replica oracle ranks the eligible peers, so it is only valid once every node has all of
	// them, and the load needs the connections that follow from it.
	let topology_timeout = 300 + 5 * statement_node_count as u64;
	let eligible_floor = (statement_node_count - 1) as f64;
	let connected_floor =
		(statement_node_count - 1).min(STATEMENT_SET_PEER_LIMIT - CONNECTED_PEER_MARGIN) as f64;
	for handle in &nodes {
		handle
			.node
			.wait_metric_with_timeout(
				ELIGIBLE_PEERS_METRIC,
				|eligible| eligible >= eligible_floor,
				topology_timeout,
			)
			.await?;
		handle
			.node
			.wait_metric_with_timeout(
				CONNECTED_PEERS_METRIC,
				|connected| connected >= connected_floor,
				topology_timeout,
			)
			.await?;
	}

	let peer_keys = collect_peer_keys(&nodes).await?;

	let mut load = Load::expiring(PARTICIPANTS, STATEMENT_TTL);
	let soak_started = Instant::now();
	let mut wave: u64 = 0;
	let mut total_statements = 0usize;
	loop {
		let report = run_wave(wave, &nodes, &peer_keys, &mut load).await?;
		total_statements += report.ring_statements + PROBES_PER_WAVE;
		info!(
			"Wave {wave}: {} ring statements submitted in {:.1}s, delivered in {:.1}s, \
			 placement verified on {statement_node_count} nodes",
			report.ring_statements,
			report.submit_time.as_secs_f64(),
			report.verify_time.as_secs_f64(),
		);
		wave += 1;
		if soak_started.elapsed() >= Duration::from_secs(soak_secs) {
			break;
		}
	}

	// No statement errors on any node; the count-zero predicate only passes after a full log
	// scan, so the timeout is minimal.
	let options = LogLineCountOptions::new(|n| n == 0, Duration::from_secs(1), false);
	for handle in &nodes {
		let result = handle
			.node
			.wait_log_line_count_with_timeout(".*ERROR.*statement.*", false, options.clone())
			.await?;
		assert!(result.success(), "{} logged statement errors during the soak", handle.name());
	}

	info!(
		"Soak done: {wave} waves, {total_statements} statements in {:.0}s, no statement errors",
		soak_started.elapsed().as_secs_f64(),
	);
	Ok(())
}

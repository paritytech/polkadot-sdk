// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Soak of the v2 DHT statement path (gate on) under continuous load.
//!
//! Sizing is the point: `nodes > replication_factor > gossip_target + 1` (12 > 8 > 4) —
//! defects capping the replica set at `gossip_target + 1` copies are invisible below that.
//!
//! Every wave of load asserts: the subscriber received every statement; each statement is
//! stored on all `replication_factor` XOR-closest nodes and on no other; no statement errors
//! in the logs. `STATEMENT_V2_SOAK_NODES` (default 12; above 51 the statement graph goes
//! sparse) and `STATEMENT_V2_SOAK_SECS` (default 900) size the run.
//!
//! Runs on demand only: the `GHA-statement-store` PR label, a workflow dispatch, or locally —
//! against the cluster given a kubeconfig:
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
	format_build_errors, submit_statement, subscribe_topic, subscribe_topic_filter,
	wait_for_first_block,
};
use anyhow::anyhow;
use codec::Encode;
use log::info;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::sr25519;
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
	LocalFileSystem, Network, NetworkConfigBuilder, NetworkNode,
};

const SOAK_SECS_ENV: &str = "STATEMENT_V2_SOAK_SECS";
const DEFAULT_SOAK_SECS: u64 = 900;
/// How many statement-store nodes to spawn: the 4 authoring collators plus soak-* full nodes for
/// the rest.
const NODES_ENV: &str = "STATEMENT_V2_SOAK_NODES";
const DEFAULT_NODES: usize = 12;
/// Statement peer-set slots hardcoded in `sc-network-statement`; `nodes - 1` at or below it
/// guarantees the full mesh.
const STATEMENT_SET_PEER_LIMIT: usize = 50;

/// Genesis Aura authorities of the People chain spec; they author the parachain blocks.
const AUTHORING_COLLATORS: [&str; 4] = ["alice", "bob", "charlie", "dave"];

const REPLICATION_FACTOR: usize = 8;
const GOSSIP_TARGET: u32 = 3;
const PARTICIPANTS: u32 = 2_000;
const LOG_FILTER: &str = "info,statement-store=info,statement-gossip=debug";

const RING_RATE_PER_SECOND: usize = 20;
const RING_SECS: u64 = 45;
const SUBMIT_TICK: Duration = Duration::from_millis(100);
const PROBES_PER_WAVE: usize = 8;
const PAYLOAD_SIZE: usize = 128;
const DELIVERY_TIMEOUT_SECS: u64 = 120;
const PLACEMENT_TIMEOUT_SECS: u64 = 90;

const CONNECTED_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_connected_peers";
const KNOWN_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_known_peers";

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

/// The position a statement node occupies in the XOR topic space; indexed like `nodes`.
struct StatementPeer {
	/// `blake2_256` of the peer id bytes.
	key: [u8; 32],
	/// Decoded peer id bytes, the tie-break of the distance ordering.
	id: Vec<u8>,
}

fn xor_distance(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
	let mut distance = [0u8; 32];
	for ((distance, a), b) in distance.iter_mut().zip(a).zip(b) {
		*distance = a ^ b;
	}
	distance
}

fn ranked_by_distance(peers: &[StatementPeer], topic: Topic) -> Vec<usize> {
	let mut order: Vec<usize> = (0..peers.len()).collect();
	order.sort_by(|&a, &b| {
		(xor_distance(*topic, peers[a].key), &peers[a].id)
			.cmp(&(xor_distance(*topic, peers[b].key), &peers[b].id))
	});
	order
}

fn soak_topic(kind: &[u8], wave: u64, idx: u64) -> Topic {
	let mut preimage = Vec::with_capacity(kind.len() + 16);
	preimage.extend_from_slice(kind);
	preimage.extend_from_slice(&wave.to_le_bytes());
	preimage.extend_from_slice(&idx.to_le_bytes());
	blake2_256(&preimage).into()
}

struct Load {
	keypairs: Vec<sr25519::Pair>,
	seq: u32,
}

impl Load {
	fn new() -> Self {
		let keypairs = (0..PARTICIPANTS).map(get_keypair).collect();
		Self { keypairs, seq: 0 }
	}

	fn next_statement(&mut self, wave: u64, topic: Topic) -> sp_statement_store::Statement {
		let keypair = &self.keypairs[self.seq as usize % self.keypairs.len()];
		self.seq += 1;
		let mut payload = vec![0u8; PAYLOAD_SIZE];
		payload[..8].copy_from_slice(&wave.to_le_bytes());
		payload[8..12].copy_from_slice(&self.seq.to_le_bytes());
		create_test_statement(keypair, &[topic], None, payload, u32::MAX, self.seq)
	}
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

/// Spawns 2 relay validators plus the statement nodes: the 4 authoring collators and one full
/// node per `full_node_names` entry, every one with the statement store on the v2 DHT path.
async fn launch_soak_network(
	full_node_names: &[String],
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let base_dir = base_dir()?;
	let chain_spec_path = create_chain_spec_with_allowances(PARTICIPANTS, &base_dir)?;
	salt_chain_spec(&chain_spec_path)?;
	let args = collator_args_v2(PARTICIPANTS, LOG_FILTER, REPLICATION_FACTOR as u32, GOSSIP_TARGET);
	let env = [("STATEMENT_STORE_V2_DHT_ENABLED", "1")];

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			let mut p = p
				.with_id(1004)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(args.clone())
				.with_collator(|n| n.with_name(AUTHORING_COLLATORS[0]).with_env(env.to_vec()));
			for name in &AUTHORING_COLLATORS[1..] {
				p = p.with_collator(|n| n.with_name(*name).with_env(env.to_vec()));
			}
			for name in full_node_names {
				let name = name.as_str();
				p = p.with_fullnode(|n| {
					n.with_name(name).with_args(args.clone()).with_env(env.to_vec())
				});
			}
			p
		})
		.with_global_settings(|global_settings| {
			global_settings
				.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
				.with_tear_down_on_failure(false)
		})
		.build()
		.map_err(format_build_errors)?;

	let network = crate::utils::initialize_network(config).await?;
	// Spawn time grows with the network, so the readiness window scales with it.
	let up_timeout = 60 + 10 * (AUTHORING_COLLATORS.len() + full_node_names.len() + 2) as u64;
	assert!(network.wait_until_is_up(up_timeout).await.is_ok());
	Ok(network)
}

/// Reads each node's peer id and maps it into the XOR topic space.
async fn collect_statement_peers(
	nodes: &[NodeHandle<'_>],
) -> Result<Vec<StatementPeer>, anyhow::Error> {
	let mut peers = Vec::with_capacity(nodes.len());
	for handle in nodes {
		let peer_id: String = handle.rpc.request("system_localPeerId", rpc_params![]).await?;
		let id = bs58::decode(&peer_id)
			.into_vec()
			.map_err(|e| anyhow!("{}: cannot decode peer id {peer_id}: {e}", handle.name()))?;
		peers.push(StatementPeer { key: blake2_256(&id), id });
	}
	Ok(peers)
}

/// Reads a node's persistent store from a fresh subscription's replay. The topicless filter
/// grants no affinity and prompts no serving; only replay frames count (`remaining: Some(_)`,
/// ending with `Some(0)` even on an empty store) — live frames (`None`) are ignored.
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
	peers: &[StatementPeer],
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
		let order = ranked_by_distance(peers, topic);
		let farthest = *order.last().expect("nodes are never empty");
		let statement = load.next_statement(wave, topic);
		let handle = &nodes[farthest];
		let result = submit_statement(&handle.rpc, &statement).await?;
		assert_eq!(result, SubmitResult::New, "wave {wave}: probe rejected on {}", handle.name());
		probes.push((statement.encode(), order));
	}

	// Ring load: 20/s (2 per 100 ms tick) for 45 s → 900 statements per wave, all on the wave
	// topic, submitted round-robin over every node (~75 per node). Any node accepts a
	// submission; a non-replica only holds it transiently and forwards, the topic's
	// `REPLICATION_FACTOR` replicas persist it — so both entry paths are exercised
	let total = RING_RATE_PER_SECOND * RING_SECS as usize;
	let per_tick = RING_RATE_PER_SECOND * SUBMIT_TICK.as_millis() as usize / 1000;
	let mut expected = Vec::with_capacity(total);
	let mut ticker = tokio::time::interval(SUBMIT_TICK);
	let started = Instant::now();
	while expected.len() < total {
		ticker.tick().await;
		for _ in 0..per_tick.min(total - expected.len()) {
			let statement = load.next_statement(wave, ring_topic);
			let handle = &nodes[expected.len() % nodes.len()];
			let result = submit_statement(&handle.rpc, &statement).await?;
			assert_eq!(
				result,
				SubmitResult::New,
				"wave {wave}: ring statement rejected on {}",
				handle.name()
			);
			expected.push(statement.encode());
		}
	}
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
	let ring_order = ranked_by_distance(peers, ring_topic);
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

	// Positive direction: poll store snapshots until every expectation sits on all of its
	// holders — forwarding is asynchronous.
	let deadline = Instant::now() + Duration::from_secs(PLACEMENT_TIMEOUT_SECS);
	let mut snapshots: Vec<HashSet<Vec<u8>>>;
	loop {
		let mut collected = Vec::with_capacity(nodes.len());
		for handle in nodes {
			collected.push(store_snapshot(&handle.rpc).await?);
		}
		snapshots = collected;
		let missing = expectations.iter().find_map(|(context, blob, holders)| {
			holders
				.iter()
				.find(|&&holder| !snapshots[holder].contains(blob))
				.map(|&h| (context, h))
		});
		match missing {
			None => break,
			Some((context, holder)) if Instant::now() >= deadline => {
				return Err(anyhow!(
					"wave {wave} {context}: {} did not store the statement within \
					 {PLACEMENT_TIMEOUT_SECS}s",
					nodes[holder].name()
				))
			},
			Some(_) => tokio::time::sleep(Duration::from_secs(2)).await,
		}
	}

	// Negative direction, judged from the same converged snapshots: nothing sits on a node
	// outside its holder set.
	for (context, blob, holders) in &expectations {
		for (idx, handle) in nodes.iter().enumerate() {
			if !holders.contains(&idx) && snapshots[idx].contains(blob) {
				return Err(anyhow!(
					"wave {wave} {context}: {} stores a statement it is not a replica for",
					handle.name()
				));
			}
		}
	}

	Ok(WaveReport { ring_statements: expected.len(), submit_time, verify_time })
}

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

	// The replica oracle is only valid once discovery is complete on every node.
	let topology_timeout = 300 + 5 * statement_node_count as u64;
	info!("Waiting for every node to discover the other {}", statement_node_count - 1);
	let known_floor = (statement_node_count - 1) as f64;
	for handle in &nodes {
		handle
			.node
			.wait_metric_with_timeout(
				KNOWN_PEERS_METRIC,
				|known| known >= known_floor,
				topology_timeout,
			)
			.await?;
	}

	// Within the slot limit the full mesh is guaranteed and asserted; above it the graph is
	// sparse by construction and the floor only guarantees a live, routable graph.
	let full_mesh = statement_node_count - 1 <= STATEMENT_SET_PEER_LIMIT;
	let connected_floor =
		if full_mesh { statement_node_count - 1 } else { STATEMENT_SET_PEER_LIMIT } as f64;
	info!(
		"Topology regime: {}, waiting for >= {connected_floor} substreams per node",
		if full_mesh { "full mesh" } else { "sparse (peer-limited)" },
	);
	for handle in &nodes {
		handle
			.node
			.wait_metric_with_timeout(
				CONNECTED_PEERS_METRIC,
				|connected| connected >= connected_floor,
				topology_timeout,
			)
			.await?;
	}

	let peers = collect_statement_peers(&nodes).await?;

	let mut load = Load::new();
	let soak_started = Instant::now();
	let mut wave: u64 = 0;
	let mut total_statements = 0usize;
	loop {
		let report = run_wave(wave, &nodes, &peers, &mut load).await?;
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

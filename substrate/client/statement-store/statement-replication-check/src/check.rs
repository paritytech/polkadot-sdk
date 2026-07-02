// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! CLI tool that verifies statement-store v2 DHT replication on a live network.
//!
//! Connects to every node of the network over RPC, learns each node's libp2p peer id,
//! submits statements with random topics, and independently recomputes the expected
//! replica set (the K peers closest to the topic by XOR distance, mirroring
//! `PeersTopology::is_dht_affine`). It then polls every node's local store through
//! `statement_subscribeStatement` replay until the expected replicas hold the statement
//! or a timeout expires, and reports recall, extra holders, and convergence times.
//!
//! Designed to run as a Kubernetes Job inside a zombienet-spawned network, after
//! `setup-allowances` has provisioned the signing accounts:
//!
//! ```bash
//! replication-check \
//!   --rpc-endpoints ws://node1:9944,ws://node2:9944,... \
//!   --replication-factor 8 \
//!   --num-statements 20
//! ```

use anyhow::{anyhow, Context};
use clap::{Parser, ValueEnum};
use codec::Encode;
use futures::future::join_all;
use jsonrpsee::{
	core::client::{ClientT, Subscription, SubscriptionClientT},
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use log::{info, warn};
use rand::Rng;
use sc_network_types::PeerId;
use sc_statement_store::test_utils::get_keypair;
use sp_core::{bounded_vec::BoundedVec, Bytes, ConstU32};
use sp_crypto_hashing::blake2_256;
use sp_statement_store::{Statement, StatementEvent, SubmitResult, Topic, TopicFilter};
use std::{
	collections::{HashMap, HashSet},
	str::FromStr,
	time::{Duration, Instant},
};
use tokio::time::timeout;

/// `TopicFilter::MatchAny` is bounded to 128 topics, and each probe statement carries a
/// unique topic the poller filters on.
const MAX_STATEMENTS: u32 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum SubmitMode {
	/// Submit each statement via the node farthest from its topic: a guaranteed
	/// non-replica (when the network is larger than K), so the statement must
	/// traverse DHT steering to reach its replica set.
	Farthest,
	/// Submit each statement via a uniformly random node.
	Random,
}

#[derive(Parser, Debug)]
#[command(name = "replication-check")]
#[command(about = "Verify statement-store v2 DHT replication to the K closest peers", long_about = None)]
struct Args {
	/// Comma-separated list of RPC WebSocket endpoints, one per network node
	/// (e.g., `ws://node1:9944,ws://node2:9944`). The check treats this set as the
	/// whole network when computing expected replica sets.
	#[arg(long, value_delimiter = ',', required = true)]
	rpc_endpoints: Vec<String>,

	/// The replication factor K the nodes run with (`--statement-replication-factor`).
	#[arg(long, required = true)]
	replication_factor: usize,

	/// Number of probe statements to submit (max 128).
	#[arg(long, default_value = "20")]
	num_statements: u32,

	/// Size of each statement's plain data in bytes.
	#[arg(long, default_value = "512")]
	data_size: usize,

	/// Which node submits each statement.
	#[arg(long, value_enum, default_value_t = SubmitMode::Farthest)]
	submit_mode: SubmitMode,

	/// Delay between store-polling rounds in milliseconds.
	#[arg(long, default_value = "2000")]
	poll_interval_ms: u64,

	/// Total time budget for the expected replicas to converge, in milliseconds.
	#[arg(long, default_value = "120000")]
	convergence_timeout_ms: u64,

	/// Per-node timeout for a single subscription-replay probe, in milliseconds.
	#[arg(long, default_value = "10000")]
	replay_timeout_ms: u64,

	/// Minimum aggregate recall (stored replicas / expected replicas, averaged over
	/// statements) for the check to pass.
	#[arg(long, default_value_t = 1.0)]
	min_recall: f64,

	/// Statement expiry time in milliseconds.
	#[arg(long, default_value_t = 600_000)]
	statement_expiry_ms: u64,

	/// Number of deterministic accounts provisioned by `setup-allowances`; statement `i`
	/// is signed by account `i % num_accounts`.
	#[arg(long, default_value = "100")]
	num_accounts: u32,
}

/// One network node under test.
struct Node {
	endpoint: String,
	peer: PeerId,
	client: WsClient,
}

impl Node {
	/// Short label for logs: the host part of the endpoint plus the peer id.
	fn label(&self) -> String {
		let host = self
			.endpoint
			.trim_start_matches("ws://")
			.trim_start_matches("wss://")
			.split([':', '/'])
			.next()
			.unwrap_or(&self.endpoint);
		format!("{host} ({})", self.peer)
	}
}

/// One probe statement and its precomputed expectations.
struct Probe {
	idx: u32,
	expected: HashSet<PeerId>,
	submitter: usize,
	submitted_at: Instant,
}

/// A peer's 32-byte DHT key, `blake2_256(peer_id.to_bytes())`, matching
/// `PeersTopology::peer_key`.
fn peer_key(peer: &PeerId) -> [u8; 32] {
	blake2_256(&peer.to_bytes())
}

/// XOR distance between two points in the 32-byte key space.
fn xor_distance(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
	let mut distance = [0u8; 32];
	for ((out, a), b) in distance.iter_mut().zip(a).zip(b) {
		*out = *a ^ *b;
	}
	distance
}

/// All peers sorted by `(xor_distance to topic_key, peer_id)`, closest first. The first K
/// entries are the expected replica set; the last entry is the farthest peer. Mirrors the
/// ordering of `PeersTopology::is_dht_affine`.
fn ranked_peers(peers: &[PeerId], topic_key: &[u8; 32]) -> Vec<PeerId> {
	let mut ranked = peers.to_vec();
	ranked.sort_by(|a, b| {
		xor_distance(&peer_key(a), topic_key)
			.cmp(&xor_distance(&peer_key(b), topic_key))
			.then_with(|| a.cmp(b))
	});
	ranked
}

async fn connect(endpoint: &str) -> Result<WsClient, anyhow::Error> {
	WsClientBuilder::default()
		.build(endpoint)
		.await
		.with_context(|| format!("Failed to connect to {endpoint}"))
}

/// The libp2p peer id a node reports over `system_localPeerId`. The statement topology
/// keys peers by this identity, so the check uses the value the node itself advertises.
async fn local_peer_id(client: &WsClient) -> Result<PeerId, anyhow::Error> {
	let encoded: String = client.request("system_localPeerId", rpc_params![]).await?;
	PeerId::from_str(&encoded).map_err(|e| anyhow!("invalid peer id {encoded:?}: {e:?}"))
}

/// One replay probe of a node's local store: subscribes with a filter matching every probe
/// topic, drains the replay of currently stored statements, and returns the probe indices
/// found. The replay is done once a batch is empty or reports nothing more to come; a
/// quiet period of `replay_timeout` is treated the same way.
async fn probe_node(
	client: &WsClient,
	filter: &TopicFilter,
	encoded_to_idx: &HashMap<Vec<u8>, u32>,
	replay_timeout: Duration,
) -> Result<HashSet<u32>, anyhow::Error> {
	let mut subscription: Subscription<StatementEvent> = client
		.subscribe(
			"statement_subscribeStatement",
			rpc_params![filter],
			"statement_unsubscribeStatement",
		)
		.await
		.map_err(|e| anyhow!("failed to open subscription: {e}"))?;

	let mut found = HashSet::new();
	loop {
		match timeout(replay_timeout, subscription.next()).await {
			Ok(Some(Ok(StatementEvent::NewStatements { statements, remaining }))) => {
				for statement in &statements {
					if let Some(&idx) = encoded_to_idx.get(&statement.0) {
						found.insert(idx);
					}
				}
				if statements.is_empty() || remaining == Some(0) {
					break;
				}
			},
			Ok(Some(Err(e))) => return Err(anyhow!("subscription stream error: {e}")),
			Ok(None) => break,
			Err(_) => break,
		}
	}

	Ok(found)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
	if sorted.is_empty() {
		return f64::NAN;
	}
	let rank = (p * (sorted.len() - 1) as f64).round() as usize;
	sorted[rank.min(sorted.len() - 1)]
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let args = Args::parse();
	let k = args.replication_factor;

	if args.num_statements == 0 || args.num_statements > MAX_STATEMENTS {
		return Err(anyhow!("--num-statements must be in 1..={MAX_STATEMENTS}"));
	}
	if k == 0 {
		return Err(anyhow!("--replication-factor must be at least 1"));
	}
	if args.num_accounts == 0 {
		return Err(anyhow!("--num-accounts must be at least 1"));
	}

	info!(
		"Starting replication check: nodes={} k={} statements={} data_size={}B mode={:?}",
		args.rpc_endpoints.len(),
		k,
		args.num_statements,
		args.data_size,
		args.submit_mode,
	);

	// Connect to every node and learn its peer id.
	let clients = join_all(args.rpc_endpoints.iter().map(|e| connect(e))).await;
	let mut nodes = Vec::with_capacity(clients.len());
	for (endpoint, client) in args.rpc_endpoints.iter().zip(clients) {
		let client = client?;
		let peer = local_peer_id(&client)
			.await
			.with_context(|| format!("system_localPeerId failed on {endpoint}"))?;
		nodes.push(Node { endpoint: endpoint.clone(), peer, client });
	}

	let peers: Vec<PeerId> = nodes.iter().map(|n| n.peer).collect();
	let peer_to_node: HashMap<PeerId, usize> =
		peers.iter().enumerate().map(|(i, p)| (*p, i)).collect();
	if peer_to_node.len() != nodes.len() {
		return Err(anyhow!("duplicate peer ids reported; endpoint list likely has duplicates"));
	}
	if k > nodes.len() {
		return Err(anyhow!("--replication-factor {k} exceeds the network size {}", nodes.len()));
	}
	if k == nodes.len() && args.submit_mode == SubmitMode::Farthest {
		warn!("K equals the network size: every node is a replica, farthest submitter included");
	}
	for node in &nodes {
		info!("Node: {}", node.label());
	}

	let run_id: u64 = rand::thread_rng().gen();
	let expiry_timestamp = (std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("System clock before UNIX_EPOCH") +
		Duration::from_millis(args.statement_expiry_ms))
	.as_secs() as u32;

	// Build, sign and submit the probe statements.
	let mut probes = Vec::with_capacity(args.num_statements as usize);
	let mut topics = Vec::with_capacity(args.num_statements as usize);
	let mut encoded_to_idx = HashMap::new();
	for idx in 0..args.num_statements {
		let topic_key = blake2_256(format!("{run_id}-{idx}").as_bytes());
		let ranked = ranked_peers(&peers, &topic_key);
		let expected: HashSet<PeerId> = ranked.iter().take(k).copied().collect();
		let submitter_peer = match args.submit_mode {
			SubmitMode::Farthest => *ranked.last().expect("network is non-empty"),
			SubmitMode::Random => ranked[rand::thread_rng().gen_range(0..ranked.len())],
		};
		let submitter = peer_to_node[&submitter_peer];

		let keypair = get_keypair(idx % args.num_accounts);
		let mut statement = Statement::new();
		statement.set_channel(blake2_256(&idx.to_le_bytes()));
		statement.set_expiry_from_parts(expiry_timestamp, idx + 1);
		statement.set_topic(0, topic_key.into());
		statement.set_plain_data(vec![0u8; args.data_size]);
		statement.sign_sr25519_private(&keypair);
		let encoded = statement.encode();

		let result: SubmitResult = nodes[submitter]
			.client
			.request("statement_submit", rpc_params![Bytes::from(encoded.clone())])
			.await
			.with_context(|| format!("statement_submit failed on {}", nodes[submitter].label()))?;
		if !matches!(result, SubmitResult::New) {
			return Err(anyhow!(
				"statement {idx} not accepted by {}: {result:?}",
				nodes[submitter].label()
			));
		}

		info!(
			"Submitted statement {idx} via {} (topic {}), expecting {} replicas",
			nodes[submitter].label(),
			hex(&topic_key),
			expected.len(),
		);
		topics.push(Topic::from(topic_key));
		encoded_to_idx.insert(encoded, idx);
		probes.push(Probe { idx, expected, submitter, submitted_at: Instant::now() });
	}

	let bounded_topics: BoundedVec<Topic, ConstU32<128>> = topics
		.try_into()
		.map_err(|_| anyhow!("num_statements is bounded to {MAX_STATEMENTS}; qed"))?;
	let filter = TopicFilter::MatchAny(bounded_topics);
	let replay_timeout = Duration::from_millis(args.replay_timeout_ms);
	let deadline = Instant::now() + Duration::from_millis(args.convergence_timeout_ms);

	// Poll every node until the expected replicas hold every statement or time runs out.
	// `seen[node][probe]` is the time from that probe's submission to its first sighting.
	let mut seen: Vec<HashMap<u32, Duration>> = vec![HashMap::new(); nodes.len()];
	let mut poll_round = 0u32;
	loop {
		poll_round += 1;
		let results = join_all(
			nodes
				.iter()
				.map(|n| probe_node(&n.client, &filter, &encoded_to_idx, replay_timeout)),
		)
		.await;
		for (node_idx, result) in results.into_iter().enumerate() {
			match result {
				Ok(found) => {
					for idx in found {
						seen[node_idx]
							.entry(idx)
							.or_insert_with(|| probes[idx as usize].submitted_at.elapsed());
					}
				},
				Err(e) => warn!("Probe of {} failed: {e}", nodes[node_idx].label()),
			}
		}

		let missing_total: usize = probes
			.iter()
			.map(|p| {
				p.expected
					.iter()
					.filter(|peer| !seen[peer_to_node[peer]].contains_key(&p.idx))
					.count()
			})
			.sum();
		info!("Poll round {poll_round}: {missing_total} expected replica slots still missing");
		if missing_total == 0 {
			break;
		}
		if Instant::now() >= deadline {
			warn!("Convergence timeout reached with {missing_total} replica slots missing");
			break;
		}
		tokio::time::sleep(Duration::from_millis(args.poll_interval_ms)).await;
	}

	// Report.
	let mut recalls = Vec::with_capacity(probes.len());
	let mut convergence_secs = Vec::new();
	let mut extras_total = 0usize;
	for probe in &probes {
		let stored: Vec<&PeerId> = probe
			.expected
			.iter()
			.filter(|peer| seen[peer_to_node[peer]].contains_key(&probe.idx))
			.collect();
		let missing: Vec<String> = probe
			.expected
			.iter()
			.filter(|peer| !seen[peer_to_node[peer]].contains_key(&probe.idx))
			.map(|peer| nodes[peer_to_node[peer]].label())
			.collect();
		let extras: Vec<String> = nodes
			.iter()
			.enumerate()
			.filter(|(i, n)| {
				seen[*i].contains_key(&probe.idx) &&
					!probe.expected.contains(&n.peer) &&
					*i != probe.submitter
			})
			.map(|(_, n)| n.label())
			.collect();
		let recall = stored.len() as f64 / probe.expected.len() as f64;
		recalls.push(recall);
		extras_total += extras.len();

		if missing.is_empty() {
			let convergence = probe
				.expected
				.iter()
				.map(|peer| seen[peer_to_node[peer]][&probe.idx])
				.max()
				.unwrap_or_default();
			convergence_secs.push(convergence.as_secs_f64());
			info!(
				"Statement {}: replicated {}/{} in {:.1}s, extra holders: {}",
				probe.idx,
				stored.len(),
				probe.expected.len(),
				convergence.as_secs_f64(),
				extras.len(),
			);
		} else {
			warn!(
				"Statement {}: replicated {}/{} — missing: [{}], extra holders: {}",
				probe.idx,
				stored.len(),
				probe.expected.len(),
				missing.join(", "),
				extras.len(),
			);
		}
	}

	let overall_recall = recalls.iter().sum::<f64>() / recalls.len() as f64;
	convergence_secs.sort_by(|a, b| a.partial_cmp(b).expect("no NaNs collected; qed"));
	info!(
		"Replication check finished: recall={:.1}% ({}/{} fully replicated) extras={} \
		 convergence p50={:.1}s p95={:.1}s max={:.1}s",
		overall_recall * 100.0,
		convergence_secs.len(),
		probes.len(),
		extras_total,
		percentile(&convergence_secs, 0.5),
		percentile(&convergence_secs, 0.95),
		percentile(&convergence_secs, 1.0),
	);

	if overall_recall < args.min_recall {
		return Err(anyhow!(
			"recall {:.3} below required minimum {:.3}",
			overall_recall,
			args.min_recall
		));
	}
	Ok(())
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

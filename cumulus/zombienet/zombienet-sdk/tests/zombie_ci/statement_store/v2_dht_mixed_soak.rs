// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Soak of a mixed network: one binary on every node, the v2 DHT path on most of them and off on
//! `STATEMENT_V2_SOAK_DHT_OFF_NODES` full nodes, the `dht-off-*` ones. Every rollout of the gate
//! passes through this state, and a node of a release before the DHT path acts like a DHT-off one:
//! it speaks statement/2 but never advertises a filter
//!
//! The two kinds meet on sync links only, as a DHT-off node opens statement substreams to its sync
//! peers alone. A DHT node serves a DHT-off peer silent past the filter grace as a node that wants
//! everything: it pushes the peer every statement, sends it a match-all filter so the peer pushes
//! everything back, and keeps it out of the DHT index. A DHT node without a link to the DHT-off
//! node cannot tell it apart and keeps it in its index. So for each DHT-off node among a topic's K
//! closest, one DHT node past them holds a replica only if it is linked to the DHT-off nodes ahead,
//! and the soak does not check that node's store
//!
//! Every wave checks delivery in all four directions, entry kind to subscriber kind:
//! - ring load on the wave topic, submitted round-robin through every node, with one subscriber of
//!   each kind;
//! - probes, one statement each, on pairs picked to stress one path: a DHT entry whose topic has
//!   only DHT replicas and a DHT-off subscriber; a plain DHT-to-DHT pair; a DHT subscriber unlinked
//!   from the topic's nearest node, a DHT-off one; a DHT-off entry and a DHT subscriber linked to
//!   none of the DHT-off nodes the entry reaches; DHT-off entry and subscriber in different groups
//!   of linked DHT-off nodes. Links come from each DHT-off node's `system_peers`
//!
//! The DHT nodes stay selective: a statement entered through a DHT node sits on the DHT nodes among
//! its K closest and on DHT subscribers, nowhere else. Restarts alternate between a DHT-off and a
//! DHT full node: the node goes offline, a backlog on its subscribed topic enters through both
//! kinds, and after the restart the node must catch up on it. A fresh DHT-off node and then a fresh
//! DHT node join once: each must catch up on a backlog entered before it joined, and the DHT joiner
//! must take over the statements of the topics whose replica set it entered
//!
//! Misses do not stop the run. The soak reports every direction (delivered, late through a
//! reconnect's initial sync for the probes, missed) and fails at the end when a statement was
//! missed, a DHT placement did not converge, the topology fell short or a node logged a statement
//! error
//!
//! Sized by `STATEMENT_V2_SOAK_NODES` (default 16), `STATEMENT_V2_SOAK_DHT_OFF_NODES` (default 4)
//! and `STATEMENT_V2_SOAK_SECS` (default 900)
//!
//! The rollout soak walks a network through turning the DHT path on: only the collators start on
//! it, then full nodes restart onto it, with their keys and databases, until half and then 90% of
//! the nodes run it, and one goes back off it. A wave checks delivery and placement after each
//! step, so the other nodes must see the switched nodes for what they now run

use super::{
	common::{
		base_dir, collator_args, collator_args_v2, submit_at_rate, submit_statement,
		subscribe_topic, wait_for_first_block, Load,
	},
	v2_dht_soak::{
		bring_online, bring_online_with, collect_peer_keys, env_parsed, is_dropped_connection,
		launch_soak_network, nodes_with_statement_errors, placement_mismatch, ranked_by_distance,
		soak_topic, store_snapshot, take_offline, ExpectedPlacement, NodeHandle,
		AUTHORING_COLLATORS, CONNECTED_PEERS_METRIC, DEFAULT_SOAK_SECS, DELIVERY_TIMEOUT_SECS,
		ELIGIBLE_PEERS_METRIC, GOSSIP_TARGET, LOG_FILTER, MAX_RECONNECTS, NODES_ENV, PARTICIPANTS,
		PLACEMENT_TIMEOUT_SECS, PRE_JOIN_TOPICS, REPLICATION_FACTOR, RING_RATE_PER_SECOND,
		RING_SECS, SOAK_SECS_ENV, STATEMENT_TTL,
	},
};
use anyhow::{anyhow, bail, ensure, Context};
use codec::Encode;
use futures::FutureExt;
use log::{info, warn};
use sp_crypto_hashing::blake2_256;
use sp_statement_store::{Statement, StatementEvent, SubmitResult, Topic};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	panic::AssertUnwindSafe,
	time::{Duration, Instant},
};
use zombienet_sdk::{
	subxt::ext::subxt_rpcs::{client::RpcSubscription, rpc_params},
	AddCollatorOptions, LocalFileSystem, Network,
};

const DHT_OFF_NODES_ENV: &str = "STATEMENT_V2_SOAK_DHT_OFF_NODES";
const DEFAULT_DHT_OFF_NODES: usize = 4;
const DEFAULT_NODES: usize = 16;
const PEERS_CONNECTED_METRIC: &str = "substrate_sync_statement_peers_connected{kind=\"full\"}";
const DESIRED_UNCONNECTED_METRIC: &str = "substrate_sync_statement_v2dht_desired_unconnected_peers";
const BYTES_SENT_METRIC: &str = "substrate_sync_statement_bytes_sent_total";
const BYTES_RECEIVED_METRIC: &str = "substrate_sync_statement_bytes_received_total";
const LATE_AFTER: Duration = Duration::from_secs(10);
const RESTART_EVERY_WAVES: u64 = 4;
const MANDATORY_RESTARTS: u64 = 2;
const JOINS: u64 = 2;
const ROLLOUT_DHT_PERCENT: [usize; 2] = [50, 90];
const BACKLOG_RATE_PER_SECOND: usize = 4;
const BACKLOG_SECS: u64 = 2;
const BACKLOG_SETTLE: Duration = Duration::from_secs(15);
const RESTART_OUTAGE_TIMEOUT_SECS: u64 = 600;
const RESTART_ONLINE_TIMEOUT_SECS: u64 = 240;
const CATCH_UP_TIMEOUT_SECS: u64 = 180;
const TOPIC_SEARCH_LIMIT: u64 = 100_000;

const PROBE_ONLY_DHT_REPLICAS: &str = "probe dht→dht-off, every replica a DHT node";
const PROBE_PLAIN: &str = "probe dht→dht";
const PROBE_NEAREST_DHT_OFF: &str = "probe dht→dht, nearest node DHT-off and unlinked";
const PROBE_UNLINKED_DHT: &str = "probe dht-off→dht, subscriber unlinked from the entry's group";
const PROBE_OTHER_GROUP: &str = "probe dht-off→dht-off, subscriber outside the entry's group";
const JOIN_TAKEOVER: &str = "join dht, statements of the replica topics it entered";

fn farthest_dht(order: &[usize], dht_off: &[bool]) -> usize {
	*order
		.iter()
		.rev()
		.find(|&&idx| !dht_off[idx])
		.expect("the network has DHT nodes; qed")
}

fn node_args(dht: bool) -> Vec<zombienet_sdk::Arg> {
	if dht {
		collator_args_v2(PARTICIPANTS, LOG_FILTER, REPLICATION_FACTOR as u32, GOSSIP_TARGET)
	} else {
		collator_args(PARTICIPANTS, LOG_FILTER)
	}
}

fn dht_off_options() -> AddCollatorOptions {
	AddCollatorOptions { args: node_args(false), ..Default::default() }
}

fn kind(dht_off: &[bool], idx: usize) -> &'static str {
	if dht_off[idx] {
		"dht-off"
	} else {
		"dht"
	}
}

/// Whether a statement reached its subscriber; `Late` is told apart for the probes only
#[derive(Debug, PartialEq)]
enum Delivery {
	Delivered,
	Late,
	Missed,
}

impl Delivery {
	fn from_arrival(submitted_at: Instant, arrived_at: Option<&Instant>) -> Self {
		match arrived_at {
			Some(arrived_at)
				if arrived_at.saturating_duration_since(submitted_at) <= LATE_AFTER =>
			{
				Self::Delivered
			},
			Some(_) => Self::Late,
			None => Self::Missed,
		}
	}
}

#[derive(Default)]
struct DeliveryCounts {
	sent: usize,
	delivered: usize,
	late: usize,
	missed: usize,
	not_applicable: usize,
}

#[derive(Default)]
struct Report {
	/// Delivery counts by category, one per report line: a ring direction such as `ring
	/// dht→dht-off`, a probe path, a restart or a join with its entry kind
	counts: BTreeMap<String, DeliveryCounts>,
	findings: Vec<String>,
	progress: Vec<String>,
}

impl Report {
	fn record(&mut self, category: &str, delivery: Delivery) {
		let counts = self.counts.entry(category.to_string()).or_default();
		counts.sent += 1;
		match delivery {
			Delivery::Delivered => counts.delivered += 1,
			Delivery::Late => counts.late += 1,
			Delivery::Missed => counts.missed += 1,
		}
	}

	fn not_applicable(&mut self, category: &str) {
		self.counts.entry(category.to_string()).or_default().not_applicable += 1;
	}

	fn finding(&mut self, finding: String) {
		warn!("Finding: {finding}");
		self.findings.push(finding);
	}

	fn failures(&self) -> usize {
		self.findings.len() + self.counts.values().map(|counts| counts.missed).sum::<usize>()
	}

	fn progress(&mut self, line: String) {
		info!("{line}");
		self.progress.push(line);
	}

	/// Logs the report and saves it to `logs/` under the zombienet base directory, which CI
	/// uploads: the job log can lose the test's output when the runner's exec stream drops
	fn log(&self, waves: u64, restarts: u64, joins: u64) {
		let mut lines =
			vec![format!("Mixed soak report: {waves} waves, {restarts} restarts, {joins} joins")];
		for (category, counts) in &self.counts {
			lines.push(format!(
				"  {category}: sent {}, delivered {}, late {}, missed {}, not applicable {}",
				counts.sent, counts.delivered, counts.late, counts.missed, counts.not_applicable,
			));
		}
		lines.extend(self.findings.iter().map(|finding| format!("  finding: {finding}")));
		for line in &lines {
			info!("{line}");
		}
		lines.push("Progress:".to_string());
		lines.extend(self.progress.iter().map(|line| format!("  {line}")));
		let saved = base_dir().and_then(|dir| {
			let path = dir.join("logs").join("mixed-soak-report.txt");
			std::fs::create_dir_all(dir.join("logs"))?;
			std::fs::write(&path, lines.join("\n"))?;
			Ok(path)
		});
		match saved {
			Ok(path) => info!("Report saved to {}", path.display()),
			Err(err) => warn!("Report not saved: {err:#}"),
		}
	}
}

/// Reads `subscription` until it delivered `count` distinct statements or `deadline` passed, and
/// returns when each statement arrived
async fn collect_arrivals(
	subscription: &mut RpcSubscription<StatementEvent>,
	count: usize,
	deadline: Instant,
) -> Result<HashMap<Vec<u8>, Instant>, anyhow::Error> {
	let mut arrivals = HashMap::new();
	while arrivals.len() < count {
		let remaining = deadline.saturating_duration_since(Instant::now());
		let Ok(item) = tokio::time::timeout(remaining, subscription.next()).await else { break };
		let event = item
			.ok_or_else(|| anyhow!("subscription ended"))?
			.map_err(|e| anyhow!("subscription error: {e}"))?;
		let StatementEvent::NewStatements { statements, .. } = event;
		let now = Instant::now();
		for statement in statements {
			arrivals.entry(statement.to_vec()).or_insert(now);
		}
	}
	Ok(arrivals)
}

async fn collect_peer_ids(nodes: &[NodeHandle]) -> Result<Vec<String>, anyhow::Error> {
	let mut peer_ids = Vec::with_capacity(nodes.len());
	for handle in nodes {
		peer_ids.push(handle.rpc.request("system_localPeerId", rpc_params![]).await?);
	}
	Ok(peer_ids)
}

/// Statement peers of every DHT-off node by node index, empty for DHT nodes. A DHT-off node opens
/// statement substreams to its sync peers alone, so its `system_peers` are its statement peers
async fn dht_off_links(
	nodes: &[NodeHandle],
	dht_off: &[bool],
	peer_ids: &[String],
) -> Result<Vec<HashSet<usize>>, anyhow::Error> {
	let mut links = vec![HashSet::new(); nodes.len()];
	for (idx, handle) in nodes.iter().enumerate().filter(|(idx, _)| dht_off[*idx]) {
		let peers: serde_json::Value = handle.rpc.request("system_peers", rpc_params![]).await?;
		let peers = peers
			.as_array()
			.ok_or_else(|| anyhow!("{}: system_peers is not a list", handle.name()))?;
		for peer in peers {
			let peer_id = peer["peerId"]
				.as_str()
				.ok_or_else(|| anyhow!("{}: system_peers entry without peerId", handle.name()))?;
			if let Some(other) = peer_ids.iter().position(|id| id == peer_id) {
				links[idx].insert(other);
			}
		}
	}
	Ok(links)
}

fn linked(links: &[HashSet<usize>], a: usize, b: usize) -> bool {
	links[a].contains(&b) || links[b].contains(&a)
}

/// DHT-off nodes reachable from `entry` over links between DHT-off nodes, `entry` included
fn dht_off_group(links: &[HashSet<usize>], dht_off: &[bool], entry: usize) -> HashSet<usize> {
	let mut group = HashSet::from([entry]);
	let mut frontier = vec![entry];
	while let Some(node) = frontier.pop() {
		for &peer in &links[node] {
			if dht_off[peer] && group.insert(peer) {
				frontier.push(peer);
			}
		}
	}
	group
}

/// Names whom `idx` exchanges statements with across the kinds: the peers of a DHT-off node, or
/// the DHT-off nodes linked to a DHT node
fn describe_links(
	nodes: &[NodeHandle],
	dht_off: &[bool],
	links: &[HashSet<usize>],
	idx: usize,
) -> String {
	let names: Vec<&str> = (0..nodes.len())
		.filter(|&other| {
			other != idx && (dht_off[idx] || dht_off[other]) && linked(links, idx, other)
		})
		.map(|other| nodes[other].name())
		.collect();
	format!("{} is linked to {names:?}", nodes[idx].name())
}

/// Waits for the topology the replica oracle needs: every DHT node knows all the other DHT nodes as
/// eligible, and the DHT-off ones it has no link to may count as well. A DHT node whose connected
/// and wanted statement peers stay under the K-peer floor or a DHT-off node without a statement
/// peer is a finding, not a stop
async fn wait_for_mixed_topology(
	nodes: &[NodeHandle],
	dht_off: &[bool],
	at_start: bool,
	report: &mut Report,
) -> Result<(), anyhow::Error> {
	let deadline = Instant::now() + Duration::from_secs(300 + 5 * nodes.len() as u64);
	let dht_peers = (dht_off.iter().filter(|&&off| !off).count() - 1) as f64;
	let connected_floor = dht_peers.min(REPLICATION_FACTOR as f64);
	loop {
		let mut unaware = Vec::new();
		let mut short = Vec::new();
		for (idx, handle) in nodes.iter().enumerate() {
			if dht_off[idx] {
				if handle.node.reports(PEERS_CONNECTED_METRIC).await? < 1.0 {
					short.push(format!("{} has no statement peer", handle.name()));
				}
				continue;
			}
			let eligible = handle.node.reports(ELIGIBLE_PEERS_METRIC).await?;
			let connected = handle.node.reports(CONNECTED_PEERS_METRIC).await?;
			let unconnected = handle.node.reports(DESIRED_UNCONNECTED_METRIC).await?;
			if eligible < dht_peers {
				unaware.push(format!(
					"{} knows {eligible} of the {dht_peers} other DHT nodes as eligible",
					handle.name()
				));
			}
			if connected + unconnected < connected_floor {
				short.push(format!(
					"{} has {connected} statement peers and wants {unconnected} more, under the \
					 floor of {connected_floor}",
					handle.name()
				));
			}
		}
		if unaware.is_empty() && short.is_empty() {
			return Ok(());
		}
		if Instant::now() >= deadline {
			ensure!(
				!at_start || unaware.is_empty(),
				"DHT nodes miss eligible peers, so the replica oracle does not hold: {unaware:?}"
			);
			for problem in unaware.into_iter().chain(short) {
				report.finding(format!("topology: {problem}"));
			}
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	}
}

struct ProbeStatement {
	/// The delivery path under test, which also names the probe's line in the report
	path: &'static str,
	topic: Topic,
	/// Index in `nodes` of the node the statement is submitted to, its entry into the network
	entry: usize,
	/// Index in `nodes` of the node subscribed to `topic`, which must receive the statement
	subscriber: usize,
	/// The statement itself on `topic`, signed by one of the load's accounts
	statement: Statement,
}

/// Probes of the wave. `busy` are the nodes already subscribed in the wave: a DHT node's steering
/// covers all its topics with as few peers as it can, so only a subscriber of one topic is sure to
/// pick the topic's nearest node
fn pick_probes(
	wave: u64,
	peer_keys: &[[u8; 32]],
	dht_off: &[bool],
	links: &[HashSet<usize>],
	busy: &[usize],
	load: &mut Load,
	report: &mut Report,
) -> Vec<ProbeStatement> {
	let dht_nodes: Vec<usize> = (0..dht_off.len()).filter(|&idx| !dht_off[idx]).collect();
	let dht_off_nodes: Vec<usize> = (0..dht_off.len()).filter(|&idx| dht_off[idx]).collect();
	// Rotates the choice over the waves; the offset keeps the probes of one wave apart
	let pick = |candidates: &[usize], offset: u64| {
		(!candidates.is_empty()).then(|| candidates[(wave + offset) as usize % candidates.len()])
	};
	let mut busy = busy.to_vec();
	let mut probes = Vec::new();

	let only_dht_replicas = (0..TOPIC_SEARCH_LIMIT).find_map(|candidate| {
		let topic = soak_topic(b"mixed-p1", wave, candidate);
		let order = ranked_by_distance(peer_keys, topic);
		order[..REPLICATION_FACTOR]
			.iter()
			.all(|&idx| !dht_off[idx])
			.then_some((topic, order))
	});
	match (only_dht_replicas, pick(&dht_off_nodes, 0)) {
		// The closest node is a replica: it forwards only to the other replicas, and they only
		// among themselves
		(Some((topic, order)), Some(subscriber)) => probes.push(ProbeStatement {
			path: PROBE_ONLY_DHT_REPLICAS,
			topic,
			entry: order[0],
			subscriber,
			statement: load.next_statement(wave, topic),
		}),
		_ => report.not_applicable(PROBE_ONLY_DHT_REPLICAS),
	}

	let topic = soak_topic(b"mixed-p2", wave, 0);
	let entry = farthest_dht(&ranked_by_distance(peer_keys, topic), dht_off);
	let subscribers: Vec<usize> = dht_nodes.iter().copied().filter(|&idx| idx != entry).collect();
	match pick(&subscribers, 1) {
		Some(subscriber) => {
			busy.push(subscriber);
			let statement = load.next_statement(wave, topic);
			probes.push(ProbeStatement { path: PROBE_PLAIN, topic, entry, subscriber, statement });
		},
		None => report.not_applicable(PROBE_PLAIN),
	}

	if let Some(entry) = pick(&dht_off_nodes, 1) {
		let group = dht_off_group(links, dht_off, entry);
		let unlinked: Vec<usize> = dht_nodes
			.iter()
			.copied()
			.filter(|&idx| !group.iter().any(|&member| linked(links, idx, member)))
			.collect();
		match pick(&unlinked, 3) {
			Some(subscriber) => {
				busy.push(subscriber);
				let topic = soak_topic(b"mixed-p3", wave, 0);
				probes.push(ProbeStatement {
					path: PROBE_UNLINKED_DHT,
					topic,
					entry,
					subscriber,
					statement: load.next_statement(wave, topic),
				});
			},
			None => report.not_applicable(PROBE_UNLINKED_DHT),
		}
	}

	if let Some(entry) = pick(&dht_off_nodes, 2) {
		let group = dht_off_group(links, dht_off, entry);
		let outside: Vec<usize> =
			dht_off_nodes.iter().copied().filter(|idx| !group.contains(idx)).collect();
		match pick(&outside, 4) {
			Some(subscriber) => {
				let topic = soak_topic(b"mixed-p4", wave, 0);
				probes.push(ProbeStatement {
					path: PROBE_OTHER_GROUP,
					topic,
					entry,
					subscriber,
					statement: load.next_statement(wave, topic),
				});
			},
			None => report.not_applicable(PROBE_OTHER_GROUP),
		}
	}

	// The subscriber's steering keeps one peer for the topic, its nearest known node: here a
	// DHT-off node that refuses the subscriber, as they share no sync link
	let nearest_dht_off = (0..TOPIC_SEARCH_LIMIT).find_map(|candidate| {
		let topic = soak_topic(b"mixed-p2-hard", wave, candidate);
		let order = ranked_by_distance(peer_keys, topic);
		let nearest = order[0];
		if !dht_off[nearest] {
			return None;
		}
		let entry = farthest_dht(&order, dht_off);
		let subscribers: Vec<usize> = dht_nodes
			.iter()
			.copied()
			.filter(|&idx| {
				idx != entry &&
					!busy.contains(&idx) &&
					!order[..REPLICATION_FACTOR].contains(&idx) &&
					!linked(links, idx, nearest)
			})
			.collect();
		pick(&subscribers, 2).map(|subscriber| (topic, entry, subscriber))
	});
	match nearest_dht_off {
		Some((topic, entry, subscriber)) => probes.push(ProbeStatement {
			path: PROBE_NEAREST_DHT_OFF,
			topic,
			entry,
			subscriber,
			statement: load.next_statement(wave, topic),
		}),
		None => report.not_applicable(PROBE_NEAREST_DHT_OFF),
	}

	probes
}

/// Polls the stores of the DHT nodes until `expectations` match or the placement timeout passes,
/// and returns the remaining mismatch. An unchecked node of an expectation passes either way
async fn dht_placement(
	wave: u64,
	nodes: &[NodeHandle],
	dht_off: &[bool],
	expectations: &[(ExpectedPlacement, Vec<usize>)],
) -> Result<Option<String>, anyhow::Error> {
	let names: Vec<&str> = nodes.iter().map(NodeHandle::name).collect();
	let phase = format!("wave {wave}");
	let deadline = Instant::now() + Duration::from_secs(PLACEMENT_TIMEOUT_SECS);
	loop {
		let snapshots = futures::future::try_join_all(
			nodes.iter().enumerate().filter(|(idx, _)| !dht_off[*idx]).map(
				|(idx, handle)| async move {
					Ok::<_, anyhow::Error>((idx, store_snapshot(&handle.rpc).await?))
				},
			),
		)
		.await?;
		let resolved: Vec<ExpectedPlacement> = expectations
			.iter()
			.map(|(placement, unchecked)| {
				let mut placement = placement.clone();
				placement.holder_node_indices.extend(unchecked.iter().filter(|&&idx| {
					snapshots.iter().any(|(node, store)| {
						*node == idx && store.contains(&placement.encoded_statement)
					})
				}));
				placement
			})
			.collect();
		let problem = placement_mismatch(&phase, &resolved, &snapshots, &names);
		if problem.is_none() || Instant::now() >= deadline {
			return Ok(problem);
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	}
}

/// Where `statement` must sit among the DHT nodes: on the DHT nodes among the K closest of its
/// topic's `order`, and on `subscriber` when that is a DHT node that got it. Also returns the DHT
/// nodes left unchecked: one past the K closest for each DHT-off node among them, as it holds a
/// replica only if it is linked to the DHT-off nodes ahead. DHT-off stores are not checked, they
/// keep whatever reaches them
fn expected_dht_placement(
	statement: &[u8],
	order: &[usize],
	dht_off: &[bool],
	subscriber: Option<usize>,
) -> (ExpectedPlacement, Vec<usize>) {
	let (closest, rest) = order.split_at(REPLICATION_FACTOR);
	let mut holders: Vec<usize> = closest.iter().copied().filter(|&idx| !dht_off[idx]).collect();
	let mut unchecked: Vec<usize> = rest
		.iter()
		.copied()
		.filter(|&idx| !dht_off[idx])
		.take(REPLICATION_FACTOR - holders.len())
		.collect();
	if let Some(subscriber) = subscriber.filter(|&idx| !dht_off[idx] && !holders.contains(&idx)) {
		unchecked.retain(|&idx| idx != subscriber);
		holders.push(subscriber);
	}
	let placement = ExpectedPlacement {
		hash: hex::encode(blake2_256(statement)),
		encoded_statement: statement.to_vec(),
		holder_node_indices: holders,
	};
	(placement, unchecked)
}

async fn run_wave(
	wave: u64,
	nodes: &[NodeHandle],
	dht_off: &[bool],
	peer_keys: &[[u8; 32]],
	peer_ids: &[String],
	load: &mut Load,
	report: &mut Report,
) -> Result<(), anyhow::Error> {
	// 1. Pick a ring subscriber of each kind and the probes; probes that need unlinked nodes are
	//    picked from a fresh snapshot of the links
	let links = dht_off_links(nodes, dht_off, peer_ids).await?;
	let ring_topic = soak_topic(b"mixed-ring", wave, 0);
	let dht_nodes: Vec<usize> = (0..nodes.len()).filter(|&idx| !dht_off[idx]).collect();
	let dht_off_nodes: Vec<usize> = (0..nodes.len()).filter(|&idx| dht_off[idx]).collect();
	let dht_ring_subscriber = dht_nodes[wave as usize % dht_nodes.len()];
	let ring_subscribers =
		[dht_ring_subscriber, dht_off_nodes[wave as usize % dht_off_nodes.len()]];
	let probes = pick_probes(wave, peer_keys, dht_off, &links, &ring_subscribers, load, report);
	for probe in &probes {
		info!(
			"Wave {wave} {}: {} → {}, topic={}",
			probe.path,
			nodes[probe.entry].name(),
			nodes[probe.subscriber].name(),
			hex::encode(*probe.topic),
		);
	}

	// 2. Subscribe before anything is sent
	let mut ring_subscriptions = Vec::with_capacity(ring_subscribers.len());
	for &subscriber in &ring_subscribers {
		ring_subscriptions.push(subscribe_topic(&nodes[subscriber].rpc, ring_topic).await?);
	}
	let mut probe_subscriptions = Vec::with_capacity(probes.len());
	for probe in &probes {
		probe_subscriptions.push(subscribe_topic(&nodes[probe.subscriber].rpc, probe.topic).await?);
	}

	// 3. Submit the probes and the ring while reading the subscribers, until every statement
	// arrived or the deadline passed
	let ring_count = RING_RATE_PER_SECOND * RING_SECS as usize;
	let deadline = Instant::now() + Duration::from_secs(RING_SECS + DELIVERY_TIMEOUT_SECS + 30);
	let submit = async {
		let mut probes_submitted_at = Vec::with_capacity(probes.len());
		for probe in &probes {
			let entry = &nodes[probe.entry];
			let result = submit_statement(&entry.rpc, &probe.statement).await?;
			ensure!(
				result == SubmitResult::New,
				"wave {wave}: {} rejected a probe: {result:?}",
				entry.name()
			);
			probes_submitted_at.push(Instant::now());
		}
		let targets: Vec<_> = nodes.iter().map(|handle| (handle.name(), &handle.rpc)).collect();
		let ring =
			submit_at_rate(load, wave, ring_topic, RING_RATE_PER_SECOND, RING_SECS, &targets)
				.await?;
		Ok::<_, anyhow::Error>((probes_submitted_at, ring))
	};
	let read_ring_subscribers = futures::future::try_join_all(
		ring_subscriptions
			.iter_mut()
			.map(|subscription| collect_arrivals(subscription, ring_count, deadline)),
	);
	let read_probe_subscribers = futures::future::try_join_all(
		probe_subscriptions
			.iter_mut()
			.map(|subscription| collect_arrivals(subscription, 1, deadline)),
	);
	let ((probes_submitted_at, ring), ring_arrivals, probe_arrivals) =
		tokio::try_join!(submit, read_ring_subscribers, read_probe_subscribers)?;

	// 4. Check every ring statement at both ring subscribers and record it under its direction:
	//    `ring dht-off→dht` counts statements sent through a DHT-off node to the DHT subscriber
	let mut summary = Vec::new();
	for (&subscriber, arrivals) in ring_subscribers.iter().zip(&ring_arrivals) {
		// Misses by the node they were sent through, for the warning below
		let mut missed_by_entry: BTreeMap<&str, usize> = BTreeMap::new();
		for (idx, statement) in ring.iter().enumerate() {
			// `submit_at_rate` sends statement `i` through node `i % nodes.len()`
			let entry = idx % nodes.len();
			if entry == subscriber {
				continue;
			}
			let delivery = if arrivals.contains_key(statement) {
				Delivery::Delivered
			} else {
				*missed_by_entry.entry(nodes[entry].name()).or_default() += 1;
				Delivery::Missed
			};
			let category = format!("ring {}→{}", kind(dht_off, entry), kind(dht_off, subscriber));
			report.record(&category, delivery);
		}
		if !missed_by_entry.is_empty() {
			warn!(
				"Wave {wave} ring: {} missed statements by entry {missed_by_entry:?}; {}",
				nodes[subscriber].name(),
				describe_links(nodes, dht_off, &links, subscriber),
			);
		}
		summary.push(format!(
			"{} got {}/{ring_count} ring",
			nodes[subscriber].name(),
			arrivals.len(),
		));
	}

	// 5. Record the probes and expect those entered through DHT nodes on their DHT holders
	let mut expectations = Vec::new();
	let mut probes_delivered = 0;
	for ((probe, submitted_at), arrivals) in
		probes.iter().zip(&probes_submitted_at).zip(&probe_arrivals)
	{
		let statement = probe.statement.encode();
		let delivery = Delivery::from_arrival(*submitted_at, arrivals.get(&statement));
		if delivery == Delivery::Delivered {
			probes_delivered += 1;
		} else {
			warn!(
				"Wave {wave} {}: {} → {} {delivery:?}; {}",
				probe.path,
				nodes[probe.entry].name(),
				nodes[probe.subscriber].name(),
				describe_links(nodes, dht_off, &links, probe.subscriber),
			);
		}
		report.record(probe.path, delivery);
		if !dht_off[probe.entry] {
			let subscriber = arrivals.contains_key(&statement).then_some(probe.subscriber);
			let order = ranked_by_distance(peer_keys, probe.topic);
			expectations.push(expected_dht_placement(&statement, &order, dht_off, subscriber));
		}
	}
	summary.push(format!("{probes_delivered}/{} probes delivered", probes.len()));
	report.progress(format!("Wave {wave}: {}", summary.join(", ")));

	// 6. Check the placement on the DHT nodes. Three ring statements entered through DHT nodes, the
	// first, middle and last, join the probes: a replica that stops storing mid-wave shows on one
	// of them
	let ring_order = ranked_by_distance(peer_keys, ring_topic);
	let dht_ring_arrivals = &ring_arrivals[0];
	let dht_entered: Vec<&Vec<u8>> = ring
		.iter()
		.enumerate()
		.filter(|(idx, _)| !dht_off[idx % nodes.len()])
		.map(|(_, statement)| statement)
		.collect();
	let samples = [dht_entered.first(), dht_entered.get(dht_entered.len() / 2), dht_entered.last()];
	for statement in samples.into_iter().flatten() {
		let subscriber = dht_ring_arrivals.contains_key(*statement).then_some(dht_ring_subscriber);
		expectations.push(expected_dht_placement(statement, &ring_order, dht_off, subscriber));
	}
	if let Some(problem) = dht_placement(wave, nodes, dht_off, &expectations).await? {
		report.finding(format!("placement: {problem}, {PLACEMENT_TIMEOUT_SECS}s after the wave"));
	}
	drop(ring_subscriptions);
	drop(probe_subscriptions);
	Ok(())
}

/// Up to two backlog entries of each kind, `excluded` aside, interleaved and rotated by `rotation`
fn backlog_entries(dht_off: &[bool], excluded: Option<usize>, rotation: u64) -> Vec<usize> {
	let of_kind = |is_dht_off: bool| -> Vec<usize> {
		let pool: Vec<usize> = (0..dht_off.len())
			.filter(|&idx| Some(idx) != excluded && dht_off[idx] == is_dht_off)
			.collect();
		(0..pool.len().min(2))
			.map(|n| pool[(rotation as usize + n) % pool.len()])
			.collect()
	};
	let (dht_entries, dht_off_entries) = (of_kind(false), of_kind(true));
	(0..2)
		.flat_map(|n| dht_entries.get(n).into_iter().chain(dht_off_entries.get(n)))
		.copied()
		.collect()
}

/// Records whether `receiver` caught up on each `backlog` statement, submitted round-robin through
/// `entries`, under `step`, and returns how many it missed
fn record_catch_up(
	report: &mut Report,
	step: &str,
	receiver: usize,
	backlog: &[Vec<u8>],
	entries: &[usize],
	arrivals: &HashMap<Vec<u8>, Instant>,
	dht_off: &[bool],
) -> usize {
	let mut missed = 0;
	for (idx, statement) in backlog.iter().enumerate() {
		let entry = entries[idx % entries.len()];
		let delivery = if arrivals.contains_key(statement) {
			Delivery::Delivered
		} else {
			missed += 1;
			Delivery::Missed
		};
		let category = format!("{step} {}←{}", kind(dht_off, receiver), kind(dht_off, entry));
		report.record(&category, delivery);
	}
	missed
}

/// Takes a full node offline, submits a backlog on its subscribed topic through nodes of both
/// kinds, restarts it and checks it catches up on the backlog
async fn run_restart(
	restart: u64,
	nodes: &mut [NodeHandle],
	dht_off: &[bool],
	load: &mut Load,
	report: &mut Report,
) -> Result<(), anyhow::Error> {
	let target_is_dht_off = restart.is_multiple_of(2);
	let candidates: Vec<usize> = (AUTHORING_COLLATORS.len()..nodes.len())
		.filter(|&idx| dht_off[idx] == target_is_dht_off)
		.collect();
	let target_idx = candidates[(restart / 2) as usize % candidates.len()];
	let target = nodes[target_idx].node.clone();
	let topic = soak_topic(b"mixed-restart", restart, 0);
	let entries = backlog_entries(dht_off, Some(target_idx), restart);
	info!(
		"Restart {restart}: {} ({}) goes offline, backlog entries {:?}",
		target.name(),
		kind(dht_off, target_idx),
		entries.iter().map(|&idx| nodes[idx].name()).collect::<Vec<_>>(),
	);

	// Subscribed before the outage, like a client of the node
	let pre_outage_subscription = subscribe_topic(&nodes[target_idx].rpc, topic).await?;
	let targets: Vec<_> = entries.iter().map(|&idx| (nodes[idx].name(), &nodes[idx].rpc)).collect();
	let outage_result = tokio::time::timeout(
		Duration::from_secs(RESTART_OUTAGE_TIMEOUT_SECS),
		AssertUnwindSafe(async {
			take_offline(&target).await?;
			// The node is down once its RPC stops answering: from then on the backlog cannot reach
			// it
			while target.is_responsive().await {
				tokio::time::sleep(Duration::from_secs(1)).await;
			}
			let batch = submit_at_rate(
				load,
				restart,
				topic,
				BACKLOG_RATE_PER_SECOND,
				BACKLOG_SECS,
				&targets,
			)
			.await?;
			tokio::time::sleep(BACKLOG_SETTLE).await;
			Ok::<_, anyhow::Error>(batch)
		})
		.catch_unwind(),
	)
	.await;

	let online_result = tokio::time::timeout(
		Duration::from_secs(RESTART_ONLINE_TIMEOUT_SECS),
		bring_online(&target),
	)
	.await
	.context("restarted node up deadline")
	.and_then(|result| result);
	let batch = match outage_result {
		Err(_) => bail!("restart {restart} outage exceeded {RESTART_OUTAGE_TIMEOUT_SECS}s"),
		Ok(Err(panic)) => std::panic::resume_unwind(panic),
		Ok(Ok(result)) => result?,
	};
	nodes[target_idx].rpc = online_result?;
	drop(pre_outage_subscription);

	let mut subscription = subscribe_topic(&nodes[target_idx].rpc, topic).await?;
	let arrivals = collect_arrivals(
		&mut subscription,
		batch.len(),
		Instant::now() + Duration::from_secs(CATCH_UP_TIMEOUT_SECS),
	)
	.await?;
	drop(subscription);
	let missed =
		record_catch_up(report, "restart", target_idx, &batch, &entries, &arrivals, dht_off);
	report.progress(format!(
		"Restart {restart}: {} caught up on {}/{} backlog statements",
		target.name(),
		batch.len() - missed,
		batch.len(),
	));
	wait_for_mixed_topology(nodes, dht_off, false, report).await
}

/// Adds a new node a DHT-off one on the first join and a DHT one on the second and checks that
/// it gets the statements sent before it joined:
/// - those on the topic it subscribes to;
/// - for a DHT node, also those on the topics it now stores as one of their K closest nodes
async fn run_join(
	join: u64,
	network: &mut Network<LocalFileSystem>,
	nodes: &mut Vec<NodeHandle>,
	dht_off: &mut Vec<bool>,
	load: &mut Load,
	report: &mut Report,
) -> Result<(), anyhow::Error> {
	let joiner_is_dht_off = join.is_multiple_of(2);
	let name = if joiner_is_dht_off { "dht-off-joiner" } else { "dht-joiner" };
	let backlog_topic = soak_topic(b"mixed-join", join, 0);
	let entries = backlog_entries(dht_off, None, join);
	let targets: Vec<_> = entries.iter().map(|&idx| (nodes[idx].name(), &nodes[idx].rpc)).collect();
	let backlog =
		submit_at_rate(load, join, backlog_topic, BACKLOG_RATE_PER_SECOND, BACKLOG_SECS, &targets)
			.await?;
	// One statement per topic, through the topic's farthest DHT node, for the DHT joiner to take
	// over where it enters the replica set
	let peer_keys = collect_peer_keys(nodes).await?;
	let mut pre_join = Vec::with_capacity(PRE_JOIN_TOPICS as usize);
	for idx in 0..PRE_JOIN_TOPICS {
		let topic = soak_topic(b"mixed-pre-join", join, idx);
		let entry = &nodes[farthest_dht(&ranked_by_distance(&peer_keys, topic), dht_off)];
		let statement = load.next_statement(join, topic);
		let result = submit_statement(&entry.rpc, &statement).await?;
		ensure!(
			result == SubmitResult::New,
			"{} rejected a pre-join statement: {result:?}",
			entry.name()
		);
		pre_join.push((topic, statement.encode()));
	}
	tokio::time::sleep(BACKLOG_SETTLE).await;

	info!("Join {join}: {name} joins");
	// A DHT joiner keeps the network's args, the v2 DHT flag among them
	let options = if joiner_is_dht_off { dht_off_options() } else { AddCollatorOptions::default() };
	network.add_collator(name, options, 1004).await?;
	let node = network.get_node(name)?.clone();
	let rpc = node.rpc().await?;
	nodes.push(NodeHandle { node, rpc });
	dht_off.push(joiner_is_dht_off);
	let joiner = nodes.len() - 1;

	let mut subscription = subscribe_topic(&nodes[joiner].rpc, backlog_topic).await?;
	let arrivals = collect_arrivals(
		&mut subscription,
		backlog.len(),
		Instant::now() + Duration::from_secs(CATCH_UP_TIMEOUT_SECS),
	)
	.await?;
	drop(subscription);
	let missed = record_catch_up(report, "join", joiner, &backlog, &entries, &arrivals, dht_off);
	report.progress(format!(
		"Join {join}: {name} caught up on {}/{} backlog statements",
		backlog.len() - missed,
		backlog.len(),
	));
	// The takeover check and the next wave's oracle rank the joiner among the eligible peers
	wait_for_mixed_topology(nodes, dht_off, false, report).await?;
	if joiner_is_dht_off {
		return Ok(());
	}

	let peer_keys = collect_peer_keys(nodes).await?;
	let entered: Vec<&Vec<u8>> = pre_join
		.iter()
		.filter(|(topic, _)| {
			ranked_by_distance(&peer_keys, *topic)[..REPLICATION_FACTOR].contains(&joiner)
		})
		.map(|(_, statement)| statement)
		.collect();
	if entered.is_empty() {
		report.not_applicable(JOIN_TAKEOVER);
		return Ok(());
	}
	let deadline = Instant::now() + Duration::from_secs(PLACEMENT_TIMEOUT_SECS);
	let store = loop {
		let store = store_snapshot(&nodes[joiner].rpc).await?;
		if entered.iter().all(|statement| store.contains(*statement)) || Instant::now() >= deadline
		{
			break store;
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	};
	for statement in entered {
		let delivery =
			if store.contains(statement) { Delivery::Delivered } else { Delivery::Missed };
		report.record(JOIN_TAKEOVER, delivery);
	}
	Ok(())
}

async fn log_traffic(nodes: &[NodeHandle], dht_off: &[bool]) -> Result<(), anyhow::Error> {
	const MIB: f64 = 1024.0 * 1024.0;
	for (label, is_dht_off) in [("DHT", false), ("DHT-off", true)] {
		let (mut sent, mut received, mut count) = (0.0, 0.0, 0.0);
		for (handle, _) in nodes.iter().zip(dht_off).filter(|(_, &off)| off == is_dht_off) {
			sent += handle.node.reports(BYTES_SENT_METRIC).await?;
			received += handle.node.reports(BYTES_RECEIVED_METRIC).await?;
			count += 1.0;
		}
		info!(
			"Statement traffic per {label} node since its last start: sent {:.1} MiB, received \
			 {:.1} MiB",
			sent / count / MIB,
			received / count / MIB,
		);
	}
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_v2_dht_mixed_soak() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	// 1. Read the soak duration and the node counts from the env
	let soak_secs = env_parsed::<u64>(SOAK_SECS_ENV)?.unwrap_or(DEFAULT_SOAK_SECS);
	let statement_node_count = env_parsed::<usize>(NODES_ENV)?.unwrap_or(DEFAULT_NODES);
	let dht_off_count = env_parsed::<usize>(DHT_OFF_NODES_ENV)?.unwrap_or(DEFAULT_DHT_OFF_NODES);
	let full_node_count = statement_node_count.saturating_sub(AUTHORING_COLLATORS.len());
	assert!(
		(1..=full_node_count).contains(&dht_off_count) &&
			statement_node_count - dht_off_count > REPLICATION_FACTOR
	);

	// 2. Start the collators and the DHT nodes, then add the DHT-off nodes: the same binary, the
	// args without the v2 DHT ones
	let dht_node_names: Vec<String> =
		(0..full_node_count - dht_off_count).map(|i| format!("soak-{i}")).collect();
	let mut network = launch_soak_network(&dht_node_names).await?;
	let dht_off_node_names: Vec<String> =
		(0..dht_off_count).map(|i| format!("dht-off-{i}")).collect();
	for name in &dht_off_node_names {
		network.add_collator(name.as_str(), dht_off_options(), 1004).await?;
	}

	let mut nodes = Vec::with_capacity(statement_node_count);
	for name in AUTHORING_COLLATORS
		.iter()
		.map(|n| n.to_string())
		.chain(dht_node_names)
		.chain(dht_off_node_names)
	{
		let node = network.get_node(name.as_str())?.clone();
		let rpc = node.rpc().await?;
		nodes.push(NodeHandle { node, rpc });
	}
	// The DHT-off nodes come last
	let mut dht_off: Vec<bool> =
		(0..nodes.len()).map(|idx| idx >= nodes.len() - dht_off_count).collect();

	// 3. Wait for blocks and until every DHT node sees all the other nodes
	info!("Waiting for the parachain to produce blocks...");
	wait_for_first_block(&[&nodes[0].node], 300).await?;

	let mut report = Report::default();
	wait_for_mixed_topology(&nodes, &dht_off, true, &mut report).await?;
	let mut peer_keys = collect_peer_keys(&nodes).await?;
	let mut peer_ids = collect_peer_ids(&nodes).await?;
	info!(
		"Mixed network up: {} DHT nodes, {dht_off_count} DHT-off nodes",
		statement_node_count - dht_off_count
	);

	// 4. Run waves until the time is up, with a restart or a join between waves
	let mut load = Load::expiring(PARTICIPANTS, STATEMENT_TTL);
	let soak_started = Instant::now();
	let (mut wave, mut restarts, mut joins, mut reconnects) = (0u64, 0u64, 0u64, 0usize);
	let soak = async {
		loop {
			match run_wave(wave, &nodes, &dht_off, &peer_keys, &peer_ids, &mut load, &mut report)
				.await
			{
				Ok(()) => {},
				Err(err) if is_dropped_connection(&err) && reconnects < MAX_RECONNECTS => {
					reconnects += 1;
					warn!("Wave {wave}: reopening the RPC connections after a dropped one: {err}");
					for handle in nodes.iter_mut() {
						handle.rpc = handle.node.rpc().await?;
					}
				},
				Err(err) => return Err(err),
			}
			wave += 1;
			let lifecycle_done = restarts >= MANDATORY_RESTARTS && joins >= JOINS;
			if lifecycle_done && soak_started.elapsed() >= Duration::from_secs(soak_secs) {
				break;
			}
			// One step after each of the first waves: the two restarts, then the two joins;
			// afterwards a restart every `RESTART_EVERY_WAVES` waves
			if restarts < MANDATORY_RESTARTS ||
				(lifecycle_done && wave.is_multiple_of(RESTART_EVERY_WAVES))
			{
				run_restart(restarts, &mut nodes, &dht_off, &mut load, &mut report).await?;
				restarts += 1;
			} else if joins < JOINS {
				run_join(joins, &mut network, &mut nodes, &mut dht_off, &mut load, &mut report)
					.await?;
				joins += 1;
				// The joiner shifts replica sets and links
				peer_keys = collect_peer_keys(&nodes).await?;
				peer_ids = collect_peer_ids(&nodes).await?;
			}
		}
		// 5. Check the logs for statement errors and print the traffic
		let failing = nodes_with_statement_errors(&nodes).await?;
		if !failing.is_empty() {
			report.finding(format!("{failing:?} logged statement errors during the soak"));
		}
		log_traffic(&nodes, &dht_off).await
	}
	.await;
	// 6. Print the report, also after an error, and fail on any miss or finding
	report.log(wave, restarts, joins);
	soak?;
	ensure!(
		report.failures() == 0,
		"mixed soak found {} problems, listed in the report above",
		report.failures()
	);
	info!(
		"Soak done: {wave} waves, {restarts} restarts, {joins} joins in {:.0}s, every statement \
		 delivered",
		soak_started.elapsed().as_secs_f64(),
	);
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_v2_dht_rollout_soak() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	// 1. Start the collators on the DHT path and every full node off it, as before a rollout
	let statement_node_count = env_parsed::<usize>(NODES_ENV)?.unwrap_or(DEFAULT_NODES);
	let full_node_names: Vec<String> = (0..statement_node_count - AUTHORING_COLLATORS.len())
		.map(|i| format!("rollout-{i}"))
		.collect();
	let mut network = launch_soak_network(&[]).await?;
	for name in &full_node_names {
		network.add_collator(name.as_str(), dht_off_options(), 1004).await?;
	}
	let mut nodes = Vec::with_capacity(statement_node_count);
	for name in AUTHORING_COLLATORS.iter().map(|n| n.to_string()).chain(full_node_names) {
		let node = network.get_node(name.as_str())?.clone();
		let rpc = node.rpc().await?;
		nodes.push(NodeHandle { node, rpc });
	}
	let mut dht_off: Vec<bool> =
		(0..nodes.len()).map(|idx| idx >= AUTHORING_COLLATORS.len()).collect();

	info!("Waiting for the parachain to produce blocks...");
	wait_for_first_block(&[&nodes[0].node], 300).await?;
	let mut report = Report::default();
	wait_for_mixed_topology(&nodes, &dht_off, true, &mut report).await?;
	let peer_keys = collect_peer_keys(&nodes).await?;
	let peer_ids = collect_peer_ids(&nodes).await?;

	// 2. A wave before the rollout and one after each step. The full nodes switch in order
	let mut load = Load::expiring(PARTICIPANTS, STATEMENT_TTL);
	let (mut wave, mut switches) = (0u64, 0u64);
	let rollout = async {
		run_wave(wave, &nodes, &dht_off, &peer_keys, &peer_ids, &mut load, &mut report).await?;
		wave += 1;
		for percent in ROLLOUT_DHT_PERCENT {
			let target = nodes.len() * percent / 100;
			while dht_off.iter().filter(|&&off| !off).count() < target {
				let idx =
					dht_off.iter().position(|&off| off).expect("below 100% a node is off; qed");
				nodes[idx].rpc = bring_online_with(&nodes[idx].node, Some(node_args(true))).await?;
				dht_off[idx] = false;
				switches += 1;
			}
			report.progress(format!("Rollout: {target} of {} nodes on the DHT path", nodes.len()));
			wait_for_mixed_topology(&nodes, &dht_off, false, &mut report).await?;
			run_wave(wave, &nodes, &dht_off, &peer_keys, &peer_ids, &mut load, &mut report).await?;
			wave += 1;
		}

		// 3. The last node switched goes back off the DHT path
		let idx = dht_off.iter().rposition(|&off| !off).expect("the collators run it; qed");
		nodes[idx].rpc = bring_online_with(&nodes[idx].node, Some(node_args(false))).await?;
		dht_off[idx] = true;
		switches += 1;
		report.progress(format!("Rollback: {} goes back off the DHT path", nodes[idx].name()));
		wait_for_mixed_topology(&nodes, &dht_off, false, &mut report).await?;
		run_wave(wave, &nodes, &dht_off, &peer_keys, &peer_ids, &mut load, &mut report).await?;
		wave += 1;

		// 4. Check the logs for statement errors and print the traffic
		let failing = nodes_with_statement_errors(&nodes).await?;
		if !failing.is_empty() {
			report.finding(format!("{failing:?} logged statement errors during the soak"));
		}
		log_traffic(&nodes, &dht_off).await
	}
	.await;
	// 5. Print the report, also after an error, and fail on any miss or finding
	report.log(wave, switches, 0);
	rollout?;
	ensure!(
		report.failures() == 0,
		"rollout soak found {} problems, listed in the report above",
		report.failures()
	);
	info!("Rollout soak done: {wave} waves, {switches} switches, every statement delivered");
	Ok(())
}

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
//! also stops and restarts one full node between two successful ordinary waves. During the outage,
//! every cohort statement must reach an online subscriber and stay on its original online holders.
//! After restart, the node must recover every replica and subscription statement.
//! A new full node then joins under load. Fresh probes must land on exactly the new top-K, both
//! on shifted topics, whose replica set the joiner entered, and on control topics. The joiner must
//! also receive every statement placed before the join on its shifted topics. Displaced replicas
//! keep their pre-join copies until expiry, so those are not checked.
//! The soak ends with a scan of every node's log for statement errors.
//! `STATEMENT_V2_SOAK_NODES` (default 12) and `STATEMENT_V2_SOAK_SECS` (default
//! 900) size the initial network and the run; both lifecycle steps and an ordinary wave after
//! each are mandatory even with a zero duration.
//! The lifecycle requires a full statement mesh.
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
use anyhow::{anyhow, ensure, Context};
use codec::Encode;
use futures::FutureExt;
use log::{info, warn};
use sp_crypto_hashing::blake2_256;
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use std::{
	collections::HashSet,
	panic::AssertUnwindSafe,
	path::Path,
	time::{Duration, Instant},
};
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
	AddCollatorOptions, AssetLocation, LocalFileSystem, Network, NetworkNode,
};

const SOAK_SECS_ENV: &str = "STATEMENT_V2_SOAK_SECS";
const DEFAULT_SOAK_SECS: u64 = 900;
/// How many statement-store nodes to spawn: the 4 authoring collators plus soak-* full nodes for
/// the rest.
const NODES_ENV: &str = "STATEMENT_V2_SOAK_NODES";
const DEFAULT_NODES: usize = 12;
const CONNECTED_PEER_FLOOR: usize = 50;
const STATEMENT_TTL: Duration = Duration::from_secs(900);
const MAX_RECONNECTS: usize = 5;
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
const JOINER_NAME: &str = "soak-joiner";
/// Round tag of the join step's statements: waves count from 0, the outage step uses `u64::MAX`.
const JOIN_ROUND: u64 = u64::MAX - 1;
/// At the CI size of 40 nodes a topic shifts to the joiner with odds 8/41, so 64 topics leave
/// none shifted with odds below 1e-6.
const PRE_JOIN_TOPICS: u64 = 64;
const OUTAGE_BATCHES: usize = 3;
const OUTAGE_RATE_PER_SECOND: usize = 4;
const OUTAGE_DELIVERY_TIMEOUT_SECS: u64 = 30;
const LIFECYCLE_WAIT_SECS: u64 = 240;
/// Above the sum of the outage step's inner deadlines, so the inner diagnostic fires first.
const OUTAGE_TIMEOUT_SECS: u64 =
	2 * LIFECYCLE_WAIT_SECS + 3 * OUTAGE_BATCHES as u64 * OUTAGE_DELIVERY_TIMEOUT_SECS + 60;
const STABLE_PLACEMENT_SECS: u64 = 35;
const QUIET_METRICS: [&str; 4] = [
	"substrate_sync_initial_sync_peers_active",
	"substrate_sync_initial_sync_in_flight_bytes",
	"substrate_sync_propagation_in_flight_bytes",
	"substrate_sync_pending_statement_validations",
];

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

fn is_dropped_connection(err: &anyhow::Error) -> bool {
	err.chain().any(|cause| cause.to_string().contains("restart required"))
}

/// A statement node and its open RPC connection.
#[derive(Clone)]
struct NodeHandle {
	node: NetworkNode,
	rpc: RpcClient,
}

impl NodeHandle {
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
async fn collect_peer_keys(nodes: &[NodeHandle]) -> Result<Vec<[u8; 32]>, anyhow::Error> {
	let mut keys = Vec::with_capacity(nodes.len());
	for handle in nodes {
		let peer_id: String = handle.rpc.request("system_localPeerId", rpc_params![]).await?;
		let id = bs58::decode(&peer_id)
			.into_vec()
			.map_err(|e| anyhow!("{}: cannot decode peer id {peer_id}: {e}", handle.name()))?;
		keys.push(blake2_256(&id));
	}
	ensure!(keys.iter().collect::<HashSet<_>>().len() == keys.len(), "duplicate peer keys");
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

#[derive(Clone)]
struct ExpectedPlacement {
	hash: String,
	encoded_statement: Vec<u8>,
	holder_node_indices: Vec<usize>,
}

fn placement_mismatch(
	phase: &str,
	expected_placements: &[ExpectedPlacement],
	snapshots: &[(usize, HashSet<Vec<u8>>)],
	node_names: &[&str],
) -> Option<String> {
	for placement in expected_placements {
		let mut missing_nodes = Vec::new();
		let mut extra_nodes = Vec::new();
		for (node_idx, snapshot) in snapshots {
			match (
				placement.holder_node_indices.contains(node_idx),
				snapshot.contains(&placement.encoded_statement),
			) {
				(true, false) => missing_nodes.push(node_names[*node_idx]),
				(false, true) => extra_nodes.push(node_names[*node_idx]),
				_ => {},
			}
		}
		if !missing_nodes.is_empty() || !extra_nodes.is_empty() {
			return Some(format!(
				"{phase} {}: missing={missing_nodes:?}, extra={extra_nodes:?}",
				placement.hash,
			));
		}
	}
	None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutageNodeRole {
	Replica,
	Subscriber,
	NonAffine,
}

impl std::fmt::Display for OutageNodeRole {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Replica => "replica",
			Self::Subscriber => "subscriber",
			Self::NonAffine => "non-affine",
		})
	}
}

struct OutageTopic {
	outage_node_role: OutageNodeRole,
	topic: Topic,
	holder_node_indices: Vec<usize>,
	submitter_node_idx: usize,
	receiver_node_idx: usize,
}

fn outage_topics(peer_keys: &[[u8; 32]]) -> Result<(usize, Vec<OutageTopic>), anyhow::Error> {
	// A fixed peer need not have every XOR rank. Choose the non-affine topic and outage node
	// together. Peer keys follow the soak's node order: authoring collators, then full nodes.
	let (non_affine_topic, outage_node_idx) = (0..100_000)
		.find_map(|candidate| {
			let topic = soak_topic(b"soak-outage", 0, candidate);
			let outage_node_idx = ranked_by_distance(peer_keys, topic)[REPLICATION_FACTOR];
			(outage_node_idx >= AUTHORING_COLLATORS.len()).then_some((topic, outage_node_idx))
		})
		.ok_or_else(|| anyhow!("no non-affine full outage node in 100000 bounded candidates"))?;

	let mut topics = Vec::new();
	for outage_node_role in
		[OutageNodeRole::Replica, OutageNodeRole::Subscriber, OutageNodeRole::NonAffine]
	{
		let (topic, order) = (0..100_000)
			.find_map(|candidate| {
				let topic = soak_topic(b"soak-outage", 0, candidate);
				let order = ranked_by_distance(peer_keys, topic);
				let suitable = match outage_node_role {
					OutageNodeRole::Replica => {
						order[..REPLICATION_FACTOR].contains(&outage_node_idx)
					},
					OutageNodeRole::Subscriber => {
						topic != non_affine_topic &&
							!order[..REPLICATION_FACTOR].contains(&outage_node_idx)
					},
					OutageNodeRole::NonAffine => topic == non_affine_topic,
				};
				suitable.then_some((topic, order))
			})
			.ok_or_else(|| {
				anyhow!(
					"no {outage_node_role} topic for node index {outage_node_idx} \
					 in 100000 bounded candidates"
				)
			})?;

		let receiver_node_idx = *order[..REPLICATION_FACTOR]
			.iter()
			.find(|&&idx| idx != outage_node_idx)
			.expect("K > 1 provides an online replica; qed");

		let submitter_node_idx = if outage_node_role == OutageNodeRole::NonAffine {
			// A farther explicit submitter makes the outage node a routing target on reconnect.
			order[REPLICATION_FACTOR + 1]
		} else {
			*order[..REPLICATION_FACTOR]
				.iter()
				.find(|&&idx| idx != outage_node_idx && idx != receiver_node_idx)
				.expect("K > 2 provides a submitter distinct from the delivery receiver; qed")
		};
		let mut holder_node_indices = order[..REPLICATION_FACTOR].to_vec();
		match outage_node_role {
			OutageNodeRole::Subscriber => holder_node_indices.push(outage_node_idx),
			OutageNodeRole::NonAffine => holder_node_indices.push(submitter_node_idx),
			OutageNodeRole::Replica => {},
		}
		topics.push(OutageTopic {
			outage_node_role,
			topic,
			holder_node_indices,
			submitter_node_idx,
			receiver_node_idx,
		});
	}

	Ok((outage_node_idx, topics))
}

async fn topology_mismatch(
	nodes: &[NodeHandle],
	offline_node_idx: Option<usize>,
) -> Result<Option<String>, anyhow::Error> {
	let expected_eligible = nodes.len() - 1;
	let expected_connected = expected_eligible - usize::from(offline_node_idx.is_some());
	let observations = futures::future::try_join_all(
		nodes.iter().enumerate().filter(|(idx, _)| Some(*idx) != offline_node_idx).map(
			|(_, handle)| async move {
				let eligible = handle.node.reports(ELIGIBLE_PEERS_METRIC).await?;
				let actual_connected = handle.node.reports(CONNECTED_PEERS_METRIC).await?;
				let health: serde_json::Value =
					handle.rpc.request("system_health", rpc_params![]).await?;
				let syncing = health["isSyncing"]
					.as_bool()
					.ok_or_else(|| anyhow!("{}: missing system_health.isSyncing", handle.name()))?;
				if eligible != expected_eligible as f64 ||
					actual_connected != expected_connected as f64 ||
					syncing
				{
					return Ok(Some(format!(
						"{}: eligible={eligible}, connected={actual_connected}, syncing={syncing}; \
						 expected eligible={expected_eligible}, connected={expected_connected}, \
						 syncing=false",
						handle.name(),
					)));
				}
				Ok::<_, anyhow::Error>(None)
			},
		),
	)
	.await?;
	Ok(observations.into_iter().flatten().next())
}

async fn wait_for_topology(
	phase: &str,
	nodes: &[NodeHandle],
	offline_node_idx: Option<usize>,
) -> Result<(), anyhow::Error> {
	let mut last = String::from("no observation");
	tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
		loop {
			match topology_mismatch(nodes, offline_node_idx).await? {
				None => return Ok::<_, anyhow::Error>(()),
				Some(problem) => last = problem,
			}
			tokio::time::sleep(Duration::from_secs(2)).await;
		}
	})
	.await
	.map_err(|_| anyhow!("{phase}: topology deadline: {last}"))??;
	info!("Lifecycle {phase}: full eligible topology, expected open substreams, no major sync");
	Ok(())
}

async fn wait_for_stable_placement(
	phase: &str,
	nodes: &[NodeHandle],
	expected_placements: &[ExpectedPlacement],
	offline_node_idx: Option<usize>,
) -> Result<(), anyhow::Error> {
	let node_names: Vec<_> = nodes.iter().map(NodeHandle::name).collect();
	let online_nodes: Vec<_> = nodes
		.iter()
		.enumerate()
		.filter(|(idx, _)| Some(*idx) != offline_node_idx)
		.collect();
	let mut last = String::from("no complete snapshot");
	tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
		let mut stable_since: Option<Instant> = None;
		loop {
			let snapshots =
				futures::future::try_join_all(online_nodes.iter().map(
					|&(idx, handle)| async move {
						let snapshot = tokio::time::timeout(
							Duration::from_secs(30),
							store_snapshot(&handle.rpc),
						)
						.await
						.with_context(|| format!("{phase}: {} snapshot deadline", handle.name()))?
						.with_context(|| format!("{phase}: {} snapshot failed", handle.name()))?;
						Ok::<_, anyhow::Error>((idx, snapshot))
					},
				))
				.await?;
			let mut problem =
				placement_mismatch(phase, expected_placements, &snapshots, &node_names);
			if problem.is_none() {
				problem = topology_mismatch(nodes, offline_node_idx).await?;
			}
			if problem.is_none() {
				for &(_, handle) in &online_nodes {
					for metric in QUIET_METRICS {
						let value = handle.node.reports(metric).await?;
						if value != 0.0 {
							problem = Some(format!("{}: {metric}={value}", handle.name()));
						}
					}
				}
			}
			if let Some(problem) = problem {
				last = problem;
				stable_since = None;
			} else {
				let since = stable_since.get_or_insert_with(Instant::now);
				if since.elapsed() >= Duration::from_secs(STABLE_PLACEMENT_SECS) {
					return Ok::<_, anyhow::Error>(());
				}
				last = String::from("waiting for stable placement and quiet queues");
			}
			tokio::time::sleep(Duration::from_secs(2)).await;
		}
	})
	.await
	.map_err(|_| anyhow!("{phase}: placement deadline: {last}"))??;
	info!(
		"Lifecycle {phase}: all {} cohort hashes exactly placed, \
		 stable for {STABLE_PLACEMENT_SECS}s with quiet queues",
		expected_placements.len(),
	);
	Ok(())
}

fn record_expected_placements(
	phase: &str,
	topic: &OutageTopic,
	encoded_statements: &[Vec<u8>],
	expected_placements: &mut Vec<ExpectedPlacement>,
) {
	for encoded_statement in encoded_statements {
		let hash = hex::encode(blake2_256(encoded_statement));
		info!("Lifecycle {phase} topic {}: hash={hash}", topic.outage_node_role);
		expected_placements.push(ExpectedPlacement {
			hash,
			encoded_statement: encoded_statement.clone(),
			holder_node_indices: topic.holder_node_indices.clone(),
		});
	}
}

async fn admissions_by_reason(node: &NetworkNode) -> Result<String, anyhow::Error> {
	let mut parts = Vec::new();
	for reason in ["dht", "explicit", "both", "transient", "persistent"] {
		let metric =
			format!("substrate_sub_statement_store_submitted_statements{{reason=\"{reason}\"}}");
		let value = node.reports(metric).await?;
		parts.push(format!("{reason}={value}"));
	}
	Ok(parts.join(" "))
}

/// Full-node outage and recovery:
/// 1. Pick replica/subscriber/non-affine topics; check baseline delivery and placement.
/// 2. Stop the node; wait for its statement substreams to close.
/// 3. Submit fresh statements; check delivery and placement on original online holders.
/// 4. Restart even if the outage fails, panics or times out.
/// 5. Check unchanged PeerId, subscription backlog and replica/subscriber placement.
async fn run_replica_outage(
	nodes: &mut [NodeHandle],
	peer_keys: &[[u8; 32]],
) -> Result<usize, anyhow::Error> {
	let (outage_node_idx, topics) = outage_topics(peer_keys)?;
	let subscriber_topic = topics
		.iter()
		.find(|topic| topic.outage_node_role == OutageNodeRole::Subscriber)
		.expect("outage topics include a subscriber; qed")
		.topic;
	let outage_node = nodes[outage_node_idx].node.clone();
	let mut expected_placements = Vec::new();
	let mut expected_subscription_backlog = Vec::new();
	let mut load = Load::new(PARTICIPANTS);
	let mut receiver_subscriptions = Vec::new();
	let mut submitter_subscriptions = Vec::new();

	wait_for_topology("baseline-ready", nodes, None).await?;
	let original_peer_id: String =
		nodes[outage_node_idx].rpc.request("system_localPeerId", rpc_params![]).await?;
	let mut pre_outage_subscription =
		subscribe_topic(&nodes[outage_node_idx].rpc, subscriber_topic).await?;
	for topic in &topics {
		info!(
			"Lifecycle topic {}: topic={}, holders={:?}, submitter={}, receiver={}",
			topic.outage_node_role,
			hex::encode(*topic.topic),
			topic
				.holder_node_indices
				.iter()
				.map(|&idx| nodes[idx].name())
				.collect::<Vec<_>>(),
			nodes[topic.submitter_node_idx].name(),
			nodes[topic.receiver_node_idx].name()
		);
		receiver_subscriptions
			.push(subscribe_topic(&nodes[topic.receiver_node_idx].rpc, topic.topic).await?);
		if topic.outage_node_role != OutageNodeRole::Replica {
			submitter_subscriptions
				.push(subscribe_topic(&nodes[topic.submitter_node_idx].rpc, topic.topic).await?);
		}
		let submitter = &nodes[topic.submitter_node_idx];
		let encoded_statements = submit_at_rate(
			&mut load,
			u64::MAX,
			topic.topic,
			1,
			1,
			&[(submitter.name(), &submitter.rpc)],
		)
		.await?;
		assert_statements_match(
			receiver_subscriptions.last_mut().expect("just subscribed; qed"),
			&encoded_statements,
			OUTAGE_DELIVERY_TIMEOUT_SECS,
			nodes[topic.receiver_node_idx].name(),
		)
		.await?;

		if topic.outage_node_role == OutageNodeRole::Subscriber {
			assert_statements_match(
				&mut pre_outage_subscription,
				&encoded_statements,
				OUTAGE_DELIVERY_TIMEOUT_SECS,
				outage_node.name(),
			)
			.await?;
			expected_subscription_backlog.extend(encoded_statements.clone());
		}

		record_expected_placements(
			"baseline",
			topic,
			&encoded_statements,
			&mut expected_placements,
		);
	}
	wait_for_stable_placement("baseline", nodes, &expected_placements, None).await?;

	// Both providers upload these assets as executables; k8s requires a /scripts command even
	// when restoring. restart_with regenerates the original node arguments for each invocation.
	let offline_program = "soak-replica-offline.sh";
	let online_program = "soak-replica-online.sh";
	let dir = base_dir()?;
	let offline_path = dir.join(offline_program);
	let online_path = dir.join(online_program);
	std::fs::write(&offline_path, "#!/bin/sh\nexec sleep 3600\n")?;
	std::fs::write(&online_path, "#!/bin/sh\nexec polkadot-parachain \"$@\"\n")?;

	let outage_result = tokio::time::timeout(
		Duration::from_secs(OUTAGE_TIMEOUT_SECS),
		AssertUnwindSafe(async {
			info!("Lifecycle stop: {} peer={original_peer_id}", outage_node.name());
			outage_node
				.restart_with(
					vec![AssetLocation::FilePath(offline_path)],
					Some(offline_program.into()),
					None,
					None,
				)
				.await?;

			// Pause only the idle replacement so the SDK monitor treats the node as offline.
			outage_node.pause().await?;
			wait_for_topology("disconnected", nodes, Some(outage_node_idx)).await?;

			// Send data during the outage
			for batch in 0..OUTAGE_BATCHES {
				for (topic_idx, topic) in topics.iter().enumerate() {
					// Mint fresh hashes only after the statement-substream disconnect barrier.
					let started = Instant::now();
					let encoded_statements = tokio::time::timeout(
						Duration::from_secs(OUTAGE_DELIVERY_TIMEOUT_SECS),
						async {
							let submitter = &nodes[topic.submitter_node_idx];
							let encoded_statements = submit_at_rate(
								&mut load,
								u64::MAX,
								topic.topic,
								OUTAGE_RATE_PER_SECOND,
								1,
								&[(submitter.name(), &submitter.rpc)],
							)
							.await?;
							assert_statements_match(
								&mut receiver_subscriptions[topic_idx],
								&encoded_statements,
								OUTAGE_DELIVERY_TIMEOUT_SECS,
								nodes[topic.receiver_node_idx].name(),
							)
							.await?;
							Ok::<_, anyhow::Error>(encoded_statements)
						},
					)
					.await
					.with_context(|| {
						format!(
							"outage topic {} batch {batch}: delivery deadline",
							topic.outage_node_role
						)
					})??;
					info!(
						"Lifecycle outage topic {} batch {batch}: all {} hashes delivered \
						 to {} in {:.1}s while outage node stopped",
						topic.outage_node_role,
						encoded_statements.len(),
						nodes[topic.receiver_node_idx].name(),
						started.elapsed().as_secs_f64(),
					);
					if topic.outage_node_role == OutageNodeRole::Subscriber {
						expected_subscription_backlog.extend(encoded_statements.clone());
					}
					record_expected_placements(
						"outage",
						topic,
						&encoded_statements,
						&mut expected_placements,
					);
				}
			}
			// The replica topic keeps its original K-1 online holders, without a replacement.
			wait_for_stable_placement("outage", nodes, &expected_placements, Some(outage_node_idx))
				.await?;
			Ok::<_, anyhow::Error>(())
		})
		.catch_unwind(),
	)
	.await;

	let restart_rpc_result =
		tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
			outage_node
				.restart_with(
					vec![AssetLocation::FilePath(online_path)],
					Some(online_program.into()),
					None,
					None,
				)
				.await?;
			outage_node.wait_until_is_up(120u64).await?;
			loop {
				match outage_node.rpc().await {
					Ok(rpc) => break Ok::<_, anyhow::Error>(rpc),
					Err(error) => warn!("Lifecycle RPC not ready after restart: {error:#}"),
				}
				tokio::time::sleep(Duration::from_secs(1)).await;
			}
		})
		.await
		.context("outage node restart/up deadline")
		.and_then(|result| result);
	if let Err(error) = &restart_rpc_result {
		warn!("Lifecycle recovery failed: {error:#}; original node is not confirmed running");
	}

	match outage_result {
		Err(_) => return Err(anyhow!("replica outage exceeded {OUTAGE_TIMEOUT_SECS}s")),
		Ok(Err(panic)) => std::panic::resume_unwind(panic),
		Ok(Ok(result)) => result?,
	}

	nodes[outage_node_idx].rpc = restart_rpc_result?;
	drop(pre_outage_subscription);

	tokio::time::timeout(Duration::from_secs(OUTAGE_TIMEOUT_SECS), async {
		let restarted_peer_id: String =
			nodes[outage_node_idx].rpc.request("system_localPeerId", rpc_params![]).await?;
		ensure!(
			restarted_peer_id == original_peer_id,
			"outage node PeerId changed: {original_peer_id} -> {restarted_peer_id}"
		);
		let eligible = outage_node.reports(ELIGIBLE_PEERS_METRIC).await?;
		info!(
			"Lifecycle restarted: unchanged PeerId {restarted_peer_id}, same base directory {}, \
				 {eligible} eligible peers known at the first RPC",
			outage_node.base_dir().display()
		);
		let mut outage_node_subscription =
			subscribe_topic(&nodes[outage_node_idx].rpc, subscriber_topic).await?;
		assert_statements_match(
			&mut outage_node_subscription,
			&expected_subscription_backlog,
			DELIVERY_TIMEOUT_SECS,
			outage_node.name(),
		)
		.await?;
		// Non-affine placements are not asserted after the restart: a cold topology can grant
		// permanent DHT retention.
		let recovery_placements: Vec<ExpectedPlacement> = expected_placements
			.iter()
			.filter(|placement| placement.holder_node_indices.contains(&outage_node_idx))
			.cloned()
			.collect();
		let placement_result =
			wait_for_stable_placement("recovered", nodes, &recovery_placements, None).await;
		let admissions = admissions_by_reason(&outage_node)
			.await
			.unwrap_or_else(|error| format!("unavailable: {error:#}"));
		placement_result.with_context(|| {
			format!("{} admissions by reason since restart: {admissions}", outage_node.name())
		})?;
		info!(
			"Lifecycle recovered: {} holds every required replica/subscription statement; \
				 admissions by reason: {admissions}",
			outage_node.name()
		);
		Ok::<_, anyhow::Error>(())
	})
	.await
	.map_err(|_| anyhow!("replica recovery exceeded {OUTAGE_TIMEOUT_SECS}s"))??;
	drop(submitter_subscriptions);
	Ok(expected_placements.len())
}

struct WaveReport {
	ring_statements: usize,
	submit_time: Duration,
	verify_time: Duration,
}

/// Submits a statement on `topic` through its XOR-farthest node, the longest route into the
/// replica set, and returns its expected placement on the topic's K closest nodes
async fn submit_via_farthest(
	nodes: &[NodeHandle],
	load: &mut Load,
	topic: Topic,
	order: &[usize],
) -> Result<ExpectedPlacement, anyhow::Error> {
	let submitter = &nodes[*order.last().expect("nodes are never empty")];
	let statement = load.next_statement(JOIN_ROUND, topic);
	let result = submit_statement(&submitter.rpc, &statement).await?;
	ensure!(
		result == SubmitResult::New,
		"{} rejected a join statement: {result:?}",
		submitter.name()
	);
	let encoded_statement = statement.encode();
	Ok(ExpectedPlacement {
		hash: hex::encode(blake2_256(&encoded_statement)),
		encoded_statement,
		holder_node_indices: order[..REPLICATION_FACTOR].to_vec(),
	})
}

/// Full-node join under load:
/// 1. Place pre-join statements on their K closest nodes, one topic each.
/// 2. Add the joiner while ring load is delivered, until every node has it in its topology.
/// 3. Check fresh probes land on exactly the new top-K, on shifted and control topics.
/// 4. Check the joiner received the pre-join statements of its shifted topics.
async fn run_node_join(
	network: &mut Network<LocalFileSystem>,
	nodes: &mut Vec<NodeHandle>,
	peer_keys: &mut Vec<[u8; 32]>,
	load: &mut Load,
) -> Result<usize, anyhow::Error> {
	let pre_join_topics: Vec<Topic> =
		(0..PRE_JOIN_TOPICS).map(|idx| soak_topic(b"soak-pre-join", 0, idx)).collect();
	let mut pre_join_placements = Vec::with_capacity(pre_join_topics.len());
	for &topic in &pre_join_topics {
		let order = ranked_by_distance(peer_keys, topic);
		pre_join_placements.push(submit_via_farthest(nodes, load, topic, &order).await?);
	}
	wait_for_stable_placement("pre-join", nodes, &pre_join_placements, None).await?;

	let ring_topic = soak_topic(b"soak-join-load", 0, 0);
	let mut subscription = subscribe_topic(&nodes[0].rpc, ring_topic).await?;
	let mut load_targets: Vec<_> =
		nodes.iter().map(|handle| (handle.name(), &handle.rpc)).collect();
	let (load_started_tx, load_started_rx) = tokio::sync::oneshot::channel();
	let (topology_ready_tx, mut topology_ready_rx) = tokio::sync::oneshot::channel();
	let mut nodes_after_join = nodes.clone();
	let join_node = async {
		load_started_rx.await?;
		info!("Lifecycle join: adding {JOINER_NAME} to {} statement nodes", nodes.len());
		// The joiner inherits the parachain's default args, the soak's v2 DHT flags among them
		let joiner_options = AddCollatorOptions {
			env: vec![("STATEMENT_STORE_V2_DHT_ENABLED", "1").into()],
			..Default::default()
		};
		network.add_collator(JOINER_NAME, joiner_options, 1004).await?;
		let node = network.get_node(JOINER_NAME)?.clone();
		let rpc = node.rpc().await?;
		nodes_after_join.push(NodeHandle { node, rpc });
		wait_for_topology("joined", &nodes_after_join, None).await?;
		let _ = topology_ready_tx.send(());
		Ok::<_, anyhow::Error>(())
	};
	// Submit statements and verify delivery until the topology converges after the join
	let join_load = async {
		let mut load_started_tx = Some(load_started_tx);
		let mut delivered_count = 0;
		loop {
			let expected = submit_at_rate(
				load,
				JOIN_ROUND,
				ring_topic,
				RING_RATE_PER_SECOND,
				1,
				&load_targets,
			)
			.await?;
			if let Some(load_started_tx) = load_started_tx.take() {
				let _ = load_started_tx.send(());
			}
			assert_statements_match(
				&mut subscription,
				&expected,
				DELIVERY_TIMEOUT_SECS,
				nodes[0].name(),
			)
			.await?;
			delivered_count += expected.len();
			load_targets.rotate_left(expected.len() % nodes.len());
			if topology_ready_rx.try_recv().is_ok() {
				return Ok::<_, anyhow::Error>(delivered_count);
			}
		}
	};
	let ((), ring_statement_count) = tokio::try_join!(join_node, join_load)?;
	*nodes = nodes_after_join;
	*peer_keys = collect_peer_keys(nodes).await?;
	drop(subscription);

	// Fresh statements must land on exactly the new top-K: on shifted topics the joiner displaces
	// the previous rank-K replica, control topics keep their replica set. Both groups exist: the
	// joiner is the closest peer to its own key, and the network is larger than K
	let joiner_idx = nodes.len() - 1;
	let shifted = |order: &[usize]| order[..REPLICATION_FACTOR].contains(&joiner_idx);
	let candidates = (0..).map(|idx| {
		let topic = soak_topic(b"soak-join-probe", 0, idx);
		(topic, ranked_by_distance(peer_keys, topic))
	});
	let probes: Vec<_> = candidates
		.clone()
		.filter(|(_, order)| shifted(order))
		.take(PROBES_PER_WAVE)
		.chain(candidates.filter(|(_, order)| !shifted(order)).take(PROBES_PER_WAVE))
		.collect();
	let mut probe_placements = Vec::with_capacity(probes.len());
	for (topic, order) in &probes {
		info!(
			"Lifecycle join {} topic={}: replicas={:?}",
			if shifted(order) { "shifted" } else { "control" },
			hex::encode(**topic),
			order[..REPLICATION_FACTOR]
				.iter()
				.map(|&idx| nodes[idx].name())
				.collect::<Vec<_>>(),
		);
		probe_placements.push(submit_via_farthest(nodes, load, *topic, order).await?);
	}
	wait_for_stable_placement("join", nodes, &probe_placements, None).await?;

	// The stable placement drained every initial sync, so one snapshot is final. The joiner gets
	// the pre-join statements of its shifted topics from their other replicas
	let joiner = &nodes[joiner_idx];
	let snapshot = store_snapshot(&joiner.rpc).await?;
	let mut received = 0;
	for (&topic, placement) in pre_join_topics.iter().zip(&pre_join_placements) {
		if shifted(&ranked_by_distance(peer_keys, topic)) {
			ensure!(
				snapshot.contains(&placement.encoded_statement),
				"{} lacks pre-join statement {} of a shifted topic",
				joiner.name(),
				placement.hash,
			);
			received += 1;
		}
	}
	ensure!(received > 0, "none of the {PRE_JOIN_TOPICS} pre-join topics shifted to the joiner");
	info!(
		"Lifecycle join: {} received all {received} pre-join statements of its shifted topics",
		joiner.name(),
	);
	Ok(pre_join_placements.len() + ring_statement_count + probe_placements.len())
}

async fn run_wave(
	wave: u64,
	nodes: &[NodeHandle],
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
	enum Phase {
		ReplicaOutage,
		NodeJoin,
		SteadyState,
	}

	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let soak_secs = env_parsed::<u64>(SOAK_SECS_ENV)?.unwrap_or(DEFAULT_SOAK_SECS);
	let statement_node_count = env_parsed::<usize>(NODES_ENV)?.unwrap_or(DEFAULT_NODES);
	// The outage step's non-affine submitter is XOR rank K + 1.
	assert!(
		statement_node_count > REPLICATION_FACTOR + 1 &&
			REPLICATION_FACTOR as u32 > GOSSIP_TARGET + 1
	);

	let full_node_names: Vec<String> = (0..statement_node_count - AUTHORING_COLLATORS.len())
		.map(|i| format!("soak-{i}"))
		.collect();
	let mut network = launch_soak_network(&full_node_names).await?;

	let mut nodes = Vec::with_capacity(statement_node_count);
	for name in AUTHORING_COLLATORS.iter().map(|n| n.to_string()).chain(full_node_names) {
		let node = network.get_node(name.as_str())?.clone();
		let rpc = node.rpc().await?;
		nodes.push(NodeHandle { node, rpc });
	}

	info!("Waiting for the parachain to produce blocks...");
	wait_for_first_block(&[&nodes[0].node], 300).await?;

	// The replica oracle ranks the eligible peers, so it is only valid once every node has all of
	// them, and the load needs the connections that follow from it.
	let topology_timeout = 300 + 5 * statement_node_count as u64;
	let eligible_floor = (statement_node_count - 1) as f64;
	let connected_floor = (statement_node_count - 1).min(CONNECTED_PEER_FLOOR) as f64;
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

	let mut peer_keys = collect_peer_keys(&nodes).await?;

	let mut load = Load::expiring(PARTICIPANTS, STATEMENT_TTL);
	let soak_started = Instant::now();
	let mut wave: u64 = 0;
	let mut total_statements = 0usize;
	let mut reconnects = 0usize;
	let mut phase = Phase::ReplicaOutage;
	loop {
		let report = match run_wave(wave, &nodes, &peer_keys, &mut load).await {
			Ok(report) => report,
			Err(err) if is_dropped_connection(&err) && reconnects < MAX_RECONNECTS => {
				reconnects += 1;
				warn!("Wave {wave}: reopening the RPC connections after a dropped one: {err}");
				for handle in nodes.iter_mut() {
					handle.rpc = handle.node.rpc().await?;
				}
				wave += 1;
				continue;
			},
			Err(err) => return Err(err),
		};
		total_statements += report.ring_statements + PROBES_PER_WAVE;
		info!(
			"Wave {wave}: {} ring statements submitted in {:.1}s, delivered in {:.1}s, \
			 placement verified on {} nodes",
			report.ring_statements,
			report.submit_time.as_secs_f64(),
			report.verify_time.as_secs_f64(),
			nodes.len(),
		);
		wave += 1;
		match phase {
			Phase::ReplicaOutage => {
				total_statements += run_replica_outage(&mut nodes, &peer_keys).await?;
				phase = Phase::NodeJoin;
			},
			Phase::NodeJoin => {
				total_statements +=
					run_node_join(&mut network, &mut nodes, &mut peer_keys, &mut load).await?;
				phase = Phase::SteadyState;
			},
			Phase::SteadyState if soak_started.elapsed() >= Duration::from_secs(soak_secs) => break,
			Phase::SteadyState => {},
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

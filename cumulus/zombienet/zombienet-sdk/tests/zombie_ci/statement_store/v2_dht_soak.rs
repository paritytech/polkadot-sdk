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
//! The soak ends with a scan of every node's log for statement errors, including the victim's
//! pre-outage log. `STATEMENT_V2_SOAK_NODES` (default 12) and `STATEMENT_V2_SOAK_SECS` (default
//! 900) size the run; the lifecycle and post-recovery wave are mandatory even with a zero duration.
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
	AssetLocation, LocalFileSystem, Network, NetworkNode,
};

const SOAK_SECS_ENV: &str = "STATEMENT_V2_SOAK_SECS";
const DEFAULT_SOAK_SECS: u64 = 900;
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
const OUTAGE_BATCHES: usize = 3;
const OUTAGE_RATE: usize = 4;
const OUTAGE_DELIVERY_SECS: u64 = 30;
const LIFECYCLE_WAIT_SECS: u64 = 240;
const OUTAGE_TIMEOUT_SECS: u64 = 600;
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
	ensure!(keys.iter().collect::<HashSet<_>>().len() == keys.len(), "duplicate peer keys");
	Ok(keys)
}

/// Reads every active stored body, including transient records, through a completed replay.
/// `Any` contributes no explicit topics and therefore does not change the placement oracle.
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
struct Placement {
	hash: String,
	encoded_statement: Vec<u8>,
	holders: Vec<usize>,
}

fn placement_disagreement(
	phase: &str,
	expected: &[Placement],
	snapshots: &[(usize, HashSet<Vec<u8>>)],
	names: &[&str],
) -> Option<String> {
	for placement in expected {
		let mut missing = Vec::new();
		let mut extra = Vec::new();
		for (idx, snapshot) in snapshots {
			match (placement.holders.contains(idx), snapshot.contains(&placement.encoded_statement))
			{
				(true, false) => missing.push(names[*idx]),
				(false, true) => extra.push(names[*idx]),
				_ => {},
			}
		}
		if !missing.is_empty() || !extra.is_empty() {
			return Some(format!(
				"{phase} {}: missing={missing:?}, extra={extra:?}",
				placement.hash,
			));
		}
	}
	None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutageRole {
	Replica,
	Subscriber,
	NonAffine,
}

impl std::fmt::Display for OutageRole {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Replica => "replica",
			Self::Subscriber => "subscriber",
			Self::NonAffine => "non-affine",
		})
	}
}

struct OutageTopic {
	role: OutageRole,
	topic: Topic,
	holders: Vec<usize>,
	donor: usize,
	witness: usize,
}

fn outage_topics(peer_keys: &[[u8; 32]]) -> Result<(usize, Vec<OutageTopic>), anyhow::Error> {
	// A fixed peer need not have every XOR rank. Choose the non-affine topic and victim together.
	// Peer keys follow the soak's node order: authoring collators, then full nodes.
	let (non_affine_topic, victim) = (0..100_000)
		.find_map(|candidate| {
			let topic = soak_topic(b"soak-outage", 0, candidate);
			let victim = ranked_by_distance(peer_keys, topic)[REPLICATION_FACTOR];
			(victim >= AUTHORING_COLLATORS.len()).then_some((topic, victim))
		})
		.ok_or_else(|| anyhow!("no non-affine full-node victim in 100000 bounded candidates"))?;

	let mut topics = Vec::new();
	for role in [OutageRole::Replica, OutageRole::Subscriber, OutageRole::NonAffine] {
		let (topic, order) = (0..100_000)
			.find_map(|candidate| {
				let topic = soak_topic(b"soak-outage", 0, candidate);
				let order = ranked_by_distance(peer_keys, topic);
				let suitable = match role {
					OutageRole::Replica => {
						topic != non_affine_topic && order[..REPLICATION_FACTOR].contains(&victim)
					},
					OutageRole::Subscriber => {
						topic != non_affine_topic && !order[..REPLICATION_FACTOR].contains(&victim)
					},
					OutageRole::NonAffine => topic == non_affine_topic,
				};
				(suitable && topics.iter().all(|t: &OutageTopic| t.topic != topic))
					.then_some((topic, order))
			})
			.ok_or_else(|| {
				anyhow!("no {role} topic for node index {victim} in 100000 bounded candidates")
			})?;

		let witness = *order[..REPLICATION_FACTOR]
			.iter()
			.find(|&&idx| idx != victim)
			.expect("K > 1 provides an online replica; qed");

		let donor = if role == OutageRole::NonAffine {
			// A farther explicit donor makes the non-replica victim a routing target on reconnect.
			order[REPLICATION_FACTOR + 1]
		} else {
			*order[..REPLICATION_FACTOR]
				.iter()
				.find(|&&idx| idx != victim && idx != witness)
				.expect("K > 2 provides a donor distinct from the delivery witness; qed")
		};
		let mut holders = order[..REPLICATION_FACTOR].to_vec();
		match role {
			OutageRole::Subscriber => holders.push(victim),
			OutageRole::NonAffine => holders.push(donor),
			OutageRole::Replica => {},
		}
		topics.push(OutageTopic { role, topic, holders, donor, witness });
	}

	Ok((victim, topics))
}

/// Require a full statement mesh, excluding the stopped node.
/// After stopping the node, N-2 open statement substreams on *every* survivor is the barrier;
/// eligibility stays N-1, so none of the oracle's original replica slots is replaced.
async fn topology_disagreement(
	nodes: &[NodeHandle<'_>],
	offline: Option<usize>,
) -> Result<Option<String>, anyhow::Error> {
	let connected = nodes.len() - 1 - usize::from(offline.is_some());
	let observations = futures::future::try_join_all(
		nodes.iter().enumerate().filter(|(idx, _)| Some(*idx) != offline).map(
			|(_, handle)| async move {
				let eligible = handle.node.reports(ELIGIBLE_PEERS_METRIC).await?;
				let actual_connected = handle.node.reports(CONNECTED_PEERS_METRIC).await?;
				let health: serde_json::Value =
					handle.rpc.request("system_health", rpc_params![]).await?;
				let syncing = health["isSyncing"]
					.as_bool()
					.ok_or_else(|| anyhow!("{}: missing system_health.isSyncing", handle.name()))?;
				Ok::<_, anyhow::Error>(
					(eligible != (nodes.len() - 1) as f64 ||
						actual_connected != connected as f64 ||
						syncing)
						.then(|| {
							format!(
							"{}: eligible={eligible}, connected={actual_connected}, syncing={syncing}; \
							 expected eligible={}, connected={connected}, syncing=false",
							handle.name(), nodes.len() - 1,
						)
						}),
				)
			},
		),
	)
	.await?;
	Ok(observations.into_iter().flatten().next())
}

async fn wait_lifecycle_topology(
	phase: &str,
	nodes: &[NodeHandle<'_>],
	offline: Option<usize>,
) -> Result<(), anyhow::Error> {
	let mut last = String::from("no observation");
	tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
		loop {
			match topology_disagreement(nodes, offline).await? {
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

/// Require exact placement throughout a quiet interval longer than the maintenance period.
/// Store snapshots prove the retention outcome without polling each node's entire log.
async fn wait_cohort_placement(
	phase: &str,
	nodes: &[NodeHandle<'_>],
	expected: &[Placement],
	offline: Option<usize>,
) -> Result<(), anyhow::Error> {
	let names: Vec<_> = nodes.iter().map(NodeHandle::name).collect();
	let active: Vec<_> =
		nodes.iter().enumerate().filter(|(idx, _)| Some(*idx) != offline).collect();
	let mut last = String::from("no complete snapshot");
	tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
		let mut stable_since: Option<Instant> = None;
		loop {
			let snapshots =
				futures::future::try_join_all(active.iter().map(|&(idx, handle)| async move {
					let snapshot =
						tokio::time::timeout(Duration::from_secs(30), store_snapshot(&handle.rpc))
							.await
							.with_context(|| {
								format!("{phase}: {} snapshot deadline", handle.name())
							})?
							.with_context(|| {
								format!("{phase}: {} snapshot failed", handle.name())
							})?;
					Ok::<_, anyhow::Error>((idx, snapshot))
				}))
				.await?;
			let mut problem = placement_disagreement(phase, expected, &snapshots, &names);
			if problem.is_none() {
				problem = topology_disagreement(nodes, offline).await?;
			}
			if problem.is_none() {
				for &(_, handle) in &active {
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
		expected.len(),
	);
	Ok(())
}

fn record_cohort(
	phase: &str,
	topic: &OutageTopic,
	encoded_statements: &[Vec<u8>],
	cohort: &mut Vec<Placement>,
) {
	for encoded_statement in encoded_statements {
		let hash = hex::encode(blake2_256(encoded_statement));
		info!("Lifecycle {phase} topic {}: hash={hash}", topic.role);
		cohort.push(Placement {
			hash,
			encoded_statement: encoded_statement.clone(),
			holders: topic.holders.clone(),
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

async fn victim_unavailable(victim: &NodeHandle<'_>) -> Result<(), anyhow::Error> {
	let probe = victim.rpc.request::<String>("system_localPeerId", rpc_params![]);
	match tokio::time::timeout(Duration::from_secs(3), probe).await {
		Ok(Ok(peer)) => Err(anyhow!("stopped victim still answers RPC as {peer}")),
		Ok(Err(error)) => {
			info!("Lifecycle outage: expected victim RPC failure: {error}");
			Ok(())
		},
		Err(_) => {
			info!("Lifecycle outage: expected victim RPC timeout (substreams separately checked)");
			Ok(())
		},
	}
}

async fn archive_victim_log(victim: &NetworkNode, phase: &str) -> Result<(), anyhow::Error> {
	let logs = tokio::time::timeout(Duration::from_secs(30), victim.logs())
		.await
		.context("victim log deadline")??;
	let log_dir = base_dir()?.join("logs");
	std::fs::create_dir_all(&log_dir)?;
	let path = log_dir.join(format!("{}.{phase}.log", victim.name()));
	std::fs::write(&path, &logs)?;
	info!("Lifecycle {phase}: preserved victim log at {}", path.display());
	ensure!(
		!logs.lines().any(|line| line.contains("ERROR") && line.contains("statement")),
		"{}: statement errors in {}",
		victim.name(),
		path.display(),
	);
	Ok(())
}

async fn run_replica_outage(
	nodes: &mut [NodeHandle<'_>],
	peer_keys: &[[u8; 32]],
) -> Result<usize, anyhow::Error> {
	let (victim_idx, topics) = outage_topics(peer_keys)?;
	let subscriber_topic = topics
		.iter()
		.find(|topic| topic.role == OutageRole::Subscriber)
		.expect("outage topics include a subscriber; qed")
		.topic;
	let victim = nodes[victim_idx].node;
	let mut cohort = Vec::new();
	let mut subscribed_statements = Vec::new();
	let mut load = Load::new(PARTICIPANTS);
	let mut witnesses = Vec::new();
	let mut donor_subscriptions = Vec::new();

	let (original_peer, old_subscriber_subscription) =
		tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
			wait_lifecycle_topology("baseline-ready", nodes, None).await?;
			let original_peer: String =
				nodes[victim_idx].rpc.request("system_localPeerId", rpc_params![]).await?;
			let mut subscriber_subscription =
				subscribe_topic(&nodes[victim_idx].rpc, subscriber_topic).await?;
			for topic in &topics {
				info!(
					"Lifecycle topic {}: topic={}, holders={:?}, donor={}, witness={}",
					topic.role,
					hex::encode(*topic.topic),
					topic.holders.iter().map(|&idx| nodes[idx].name()).collect::<Vec<_>>(),
					nodes[topic.donor].name(),
					nodes[topic.witness].name()
				);
				witnesses.push(subscribe_topic(&nodes[topic.witness].rpc, topic.topic).await?);
				if topic.role != OutageRole::Replica {
					donor_subscriptions
						.push(subscribe_topic(&nodes[topic.donor].rpc, topic.topic).await?);
				}
				let donor = &nodes[topic.donor];
				let blobs = submit_at_rate(
					&mut load,
					u64::MAX,
					topic.topic,
					1,
					1,
					&[(donor.name(), &donor.rpc)],
				)
				.await?;
				assert_statements_match(
					witnesses.last_mut().expect("just subscribed; qed"),
					&blobs,
					OUTAGE_DELIVERY_SECS,
					nodes[topic.witness].name(),
				)
				.await?;
				if topic.role == OutageRole::Subscriber {
					assert_statements_match(
						&mut subscriber_subscription,
						&blobs,
						OUTAGE_DELIVERY_SECS,
						victim.name(),
					)
					.await?;
					subscribed_statements.extend(blobs.clone());
				}
				record_cohort("baseline", topic, &blobs, &mut cohort);
			}
			wait_cohort_placement("baseline", nodes, &cohort, None).await?;
			Ok::<_, anyhow::Error>((original_peer, subscriber_subscription))
		})
		.await
		.context("lifecycle baseline deadline")??;

	// Both providers upload these assets as executables; k8s requires a /scripts command even
	// when restoring. restart_with regenerates the original node arguments for each invocation.
	let offline_program = "soak-replica-offline.sh";
	let online_program = "soak-replica-online.sh";
	let dir = base_dir()?;
	let offline_path = dir.join(offline_program);
	let online_path = dir.join(online_program);
	std::fs::write(&offline_path, "#!/bin/sh\nexec sleep 3600\n")?;
	std::fs::write(&online_path, "#!/bin/sh\nexec polkadot-parachain \"$@\"\n")?;

	// Keep this outside run_wave's reconnect-and-skip catch. Even helper assertions/panics or a
	// timeout must attempt recovery before returning the original failure. Replacing the node
	// closes its TCP connections; SIGSTOP alone does not guarantee a statement disconnect.
	let outage = tokio::time::timeout(
		Duration::from_secs(OUTAGE_TIMEOUT_SECS),
		AssertUnwindSafe(async {
			archive_victim_log(victim, "before-outage").await?;
			info!("Lifecycle stop: {} peer={original_peer}", victim.name());
			victim
				.restart_with(
					vec![AssetLocation::FilePath(offline_path)],
					Some(offline_program.into()),
					None,
					None,
				)
				.await?;

			// Pause only the idle replacement so the SDK monitor treats the node as offline.
			victim.pause().await?;
			wait_lifecycle_topology("disconnected", nodes, Some(victim_idx)).await?;
			victim_unavailable(&nodes[victim_idx]).await?;

			for batch in 0..OUTAGE_BATCHES {
				for (idx, topic) in topics.iter().enumerate() {
					// Mint fresh hashes only after the statement-substream disconnect barrier.
					let started = Instant::now();
					let blobs =
						tokio::time::timeout(Duration::from_secs(OUTAGE_DELIVERY_SECS), async {
							let donor = &nodes[topic.donor];
							let blobs = submit_at_rate(
								&mut load,
								u64::MAX,
								topic.topic,
								OUTAGE_RATE,
								1,
								&[(donor.name(), &donor.rpc)],
							)
							.await?;
							assert_statements_match(
								&mut witnesses[idx],
								&blobs,
								OUTAGE_DELIVERY_SECS,
								nodes[topic.witness].name(),
							)
							.await?;
							Ok::<_, anyhow::Error>(blobs)
						})
						.await
						.with_context(|| {
							format!("outage topic {} batch {batch}: delivery deadline", topic.role)
						})??;
					info!(
						"Lifecycle outage topic {} batch {batch}: all {} hashes delivered \
						 to {} in {:.1}s while victim stopped",
						topic.role,
						blobs.len(),
						nodes[topic.witness].name(),
						started.elapsed().as_secs_f64(),
					);
					if topic.role == OutageRole::Subscriber {
						subscribed_statements.extend(blobs.clone());
					}
					record_cohort("outage", topic, &blobs, &mut cohort);
				}
			}
			// The replica topic keeps its original K-1 online holders, without a replacement.
			wait_cohort_placement("outage", nodes, &cohort, Some(victim_idx)).await?;
			victim_unavailable(&nodes[victim_idx]).await?;
			Ok::<_, anyhow::Error>(())
		})
		.catch_unwind(),
	)
	.await;

	let restart = tokio::time::timeout(Duration::from_secs(LIFECYCLE_WAIT_SECS), async {
		victim
			.restart_with(
				vec![AssetLocation::FilePath(online_path)],
				Some(online_program.into()),
				None,
				None,
			)
			.await?;
		victim.wait_until_is_up(120u64).await?;
		loop {
			match victim.rpc().await {
				Ok(rpc) => break Ok::<_, anyhow::Error>(rpc),
				Err(error) => warn!("Lifecycle RPC not ready after restart: {error:#}"),
			}
			tokio::time::sleep(Duration::from_secs(1)).await;
		}
	})
	.await
	.context("victim restart/up deadline")
	.and_then(|result| result);
	if let Err(error) = &restart {
		warn!("Lifecycle recovery failed: {error:#}; original node is not confirmed running");
	}
	match outage {
		Err(_) => return Err(anyhow!("replica outage exceeded {OUTAGE_TIMEOUT_SECS}s")),
		Ok(Err(panic)) => std::panic::resume_unwind(panic),
		Ok(Ok(result)) => result?,
	}
	nodes[victim_idx].rpc = restart?;
	drop(old_subscriber_subscription);

	let recovered = tokio::time::timeout(
		Duration::from_secs(OUTAGE_TIMEOUT_SECS),
		AssertUnwindSafe(async {
			let peer: String =
				nodes[victim_idx].rpc.request("system_localPeerId", rpc_params![]).await?;
			ensure!(peer == original_peer, "victim PeerId changed: {original_peer} -> {peer}");
			let eligible = victim.reports(ELIGIBLE_PEERS_METRIC).await?;
			info!(
				"Lifecycle restarted: unchanged PeerId {peer}, same base directory {}, \
				 {eligible} eligible peers known at the first RPC",
				victim.base_dir().display()
			);
			let mut subscriber_subscription =
				subscribe_topic(&nodes[victim_idx].rpc, subscriber_topic).await?;
			assert_statements_match(
				&mut subscriber_subscription,
				&subscribed_statements,
				DELIVERY_TIMEOUT_SECS,
				victim.name(),
			)
			.await?;
			wait_lifecycle_topology("recovered-ready", nodes, None).await?;
			// Defer non-affine recovery placement: a cold topology can grant permanent DHT
			// retention.
			let expected: Vec<Placement> = cohort
				.iter()
				.filter(|placement| placement.holders.contains(&victim_idx))
				.cloned()
				.collect();
			let placement = wait_cohort_placement("recovered", nodes, &expected, None).await;
			let admissions = admissions_by_reason(victim).await?;
			placement.with_context(|| {
				format!("{} admissions by reason since restart: {admissions}", victim.name())
			})?;
			info!(
				"Lifecycle recovered: {} holds every required replica/subscription statement; \
				 admissions by reason: {admissions}",
				victim.name()
			);
			Ok::<_, anyhow::Error>(())
		})
		.catch_unwind(),
	)
	.await;
	let after_logs = archive_victim_log(victim, "after-restart").await;
	if let Err(error) = &after_logs {
		warn!("Lifecycle post-restart log check failed: {error:#}");
	}
	match recovered {
		Err(_) => return Err(anyhow!("replica recovery exceeded {OUTAGE_TIMEOUT_SECS}s")),
		Ok(Err(panic)) => std::panic::resume_unwind(panic),
		Ok(Ok(result)) => result?,
	}
	after_logs?;
	drop(donor_subscriptions);
	Ok(cohort.len())
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

	let peer_keys = collect_peer_keys(&nodes).await?;

	let mut load = Load::expiring(PARTICIPANTS, STATEMENT_TTL);
	let soak_started = Instant::now();
	let mut wave: u64 = 0;
	let mut total_statements = 0usize;
	let mut reconnects = 0usize;
	let mut outage_completed = false;
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
			 placement verified on {statement_node_count} nodes",
			report.ring_statements,
			report.submit_time.as_secs_f64(),
			report.verify_time.as_secs_f64(),
		);
		wave += 1;
		if !outage_completed {
			// Mandatory exactly once after the first successful wave, outside the ordinary
			// dropped-RPC skip path. Do not check elapsed time until a later wave succeeds.
			total_statements += run_replica_outage(&mut nodes, &peer_keys).await?;
			outage_completed = true;
		} else if soak_started.elapsed() >= Duration::from_secs(soak_secs) {
			break;
		}
	}

	// No statement errors on any node
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

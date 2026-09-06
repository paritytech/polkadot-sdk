// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the v2 DHT-affinity statement routing.
//!
//! v2 routes a statement to the `K` nodes with affinity to its topic instead of flooding every
//! peer. It is enabled per node by the `STATEMENT_STORE_V2_DHT_ENABLED` environment variable (off
//! by default); the replication factor `K` and gossip target are set via CLI flags.

use super::common::{
	expect_statement_delivered, spawn_network_with_injected_allowances_v2, stores_locally,
	submit_statement, subscribe_topic,
};
use codec::Encode;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::{sr25519, Bytes};
use sp_statement_store::{SubmitResult, Topic};
use std::time::Duration;

const TEST_GOSSIP_TARGET: u32 = 3;
// Statement peers with an open notification substream, exported per node by the v2 DHT path.
const CONNECTED_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_connected_peers";
// Statement-store peers known to a node's topology, exported per node by the v2 DHT path.
const KNOWN_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_known_peers";

/// Scans `[i; 32]` topics and returns the first `count` to which the node behind `non_affine` is
/// not a DHT replica. With `K=1` over two nodes the other node is then the replica.
///
/// A non-affine node keeps none of its own submissions (it holds them transiently, which the query
/// API never surfaces), so we submit a throwaway statement on each candidate and pick the topics
/// the node does not report storing.
async fn select_topics_non_affine_to(
	non_affine: &zombienet_sdk::subxt::backend::rpc::RpcClient,
	keypair: &sr25519::Pair,
	count: usize,
) -> Result<Vec<Topic>, anyhow::Error> {
	const CANDIDATES: u8 = 24;

	let mut found = Vec::with_capacity(count);
	for i in 0..CANDIDATES {
		let topic: Topic = [i; 32].into();
		let probe = create_test_statement(
			keypair,
			&[topic],
			None,
			vec![i, 0xfe],
			u32::MAX,
			5000 + i as u32,
		);
		let expected: Bytes = probe.encode().into();
		assert_eq!(submit_statement(non_affine, &probe).await?, SubmitResult::New);

		// The node decides retention synchronously on submit: a topic it is a replica for is kept
		// (persistent, visible to the query API), one it is not is held transiently (never
		// surfaced). So a topic the node does not report storing is one it is not a DHT replica
		// for.
		if !stores_locally(non_affine, topic, &expected).await? {
			found.push(topic);
			if found.len() == count {
				return Ok(found);
			}
		}
	}

	Err(anyhow::anyhow!(
		"found only {} of {count} topics the node is not a DHT replica for",
		found.len()
	))
}

/// Polls [`stores_locally`] until the node reports storing `expected`, once a second up to
/// `attempts` times. Use for statements that arrive by forwarding: forwarding is asynchronous, so a
/// single probe could miss a not-yet-delivered statement.
async fn wait_until_stored(
	rpc: &zombienet_sdk::subxt::backend::rpc::RpcClient,
	topic: Topic,
	expected: &Bytes,
	attempts: u32,
) -> Result<(), anyhow::Error> {
	for _ in 0..attempts {
		if stores_locally(rpc, topic, expected).await? {
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	Err(anyhow::anyhow!("statement not stored after {attempts} probes"))
}

/// A node applies the affinity rule to its own RPC submissions, and the responsible node stores
/// them regardless of where they originate.
///
/// Two nodes, `K=1`. We pick `topic_a` and `topic_b` to which node_1 is not a DHT replica, so
/// node_2 is. node_1 subscribes to `topic_a` only, then submits one statement on each topic. node_1
/// keeps `topic_a` (its subscription grants explicit affinity) but drops `topic_b` (no affinity).
/// node_2, the DHT replica for both, stores both — reached by forwarding from node_1.
///
/// We first wait for node_1 to open its statement substream to node_2: affinity is computed over
/// the peers a node has learned, so until then node_1 knows no peer, judges itself the closest to
/// every topic, and every affinity decision would be wrong.
#[tokio::test(flavor = "multi_thread")]
async fn local_submission_retention_works() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let names = ["charlie", "dave"];
	let replication_factor: usize = 1;
	let network = spawn_network_with_injected_allowances_v2(
		&names,
		names.len() as u32,
		replication_factor as u32,
		TEST_GOSSIP_TARGET,
	)
	.await?;

	let node_1 = network.get_node(names[0])?;
	let node_2 = network.get_node(names[1])?;
	let rpc_1 = node_1.rpc().await?;
	let rpc_2 = node_2.rpc().await?;
	let keypair = get_keypair(0);

	// Wait until node_1 has an open statement substream to node_2
	node_1
		.wait_metric_with_timeout(CONNECTED_PEERS_METRIC, |peers| peers >= 1.0, 120u64)
		.await?;

	// Pick two topics node_1 is not a DHT replica for; with K=1 over two nodes, node_2 is the
	// replica for both.
	let topics = select_topics_non_affine_to(&rpc_1, &keypair, 2).await?;
	let topic_a = topics[0];
	let topic_b = topics[1];

	// Control: with no affinity for `topic_a` yet, node_1 drops its own submission.
	let probe_a =
		create_test_statement(&keypair, &[topic_a], None, vec![0xa0, 9, 9, 9], u32::MAX, 1000);
	let probe_a_encoded: Bytes = probe_a.encode().into();
	assert_eq!(submit_statement(&rpc_1, &probe_a).await?, SubmitResult::New);
	assert!(
		!stores_locally(&rpc_1, topic_a, &probe_a_encoded).await?,
		"node_1 accepted but must not store its own topic_a submission while non-affine",
	);

	// Subscribing grants node_1 explicit affinity for `topic_a` only; `topic_b` stays
	// affinity-free.
	let mut sub_a = subscribe_topic(&rpc_1, topic_a).await?;

	let statement_a =
		create_test_statement(&keypair, &[topic_a], None, vec![0xaa, 1, 2, 3], u32::MAX, 1001);
	let statement_b =
		create_test_statement(&keypair, &[topic_b], None, vec![0xbb, 1, 2, 3], u32::MAX, 1002);
	let expected_a: Bytes = statement_a.encode().into();
	let expected_b: Bytes = statement_b.encode().into();

	// Both statements originate on node_1, the non-replica for either topic.
	assert_eq!(submit_statement(&rpc_1, &statement_a).await?, SubmitResult::New);
	assert_eq!(submit_statement(&rpc_1, &statement_b).await?, SubmitResult::New);

	// node_1's own subscription delivers the matching `topic_a` statement, independent of who
	// stores it.
	expect_statement_delivered(&mut sub_a, &expected_a, 20).await?;

	// node_1 decides retention synchronously on submit, so we probe right away. We check the two
	// specific statements, not a total count: the topic-selection probes above leave other
	// statements in node_1's store.
	assert!(stores_locally(&rpc_1, topic_a, &expected_a).await?); // kept by explicit affinity
	assert!(!stores_locally(&rpc_1, topic_b, &expected_b).await?); // dropped, no affinity

	// node_2 is the K=1 DHT replica for both topics, so both statements reach it by forwarding from
	// node_1 — including `topic_b`, which node_1 itself dropped. Forwarding is asynchronous, so we
	// poll until they arrive instead of sleeping a fixed time.
	wait_until_stored(&rpc_2, topic_a, &expected_a, 30).await?; // stored by DHT affinity
	wait_until_stored(&rpc_2, topic_b, &expected_b, 30).await?; // stored by DHT affinity

	Ok(())
}

/// A statement is stored only by the `K` nodes with DHT affinity to its topic, wherever it is
/// submitted.
///
/// Three nodes, `K=2`. With `K=2` over three nodes, any topic has exactly two DHT replicas and one
/// non-replica. Each node submits its own statement on the same topic. The two replicas each end up
/// storing all three statements (their own plus the two routed to them); the non-replica stores
/// none — it keeps no copy of its own submission and is not a routing target for the others.
///
/// We first wait for each node to learn the other two: affinity is computed over the peers a node
/// has learned, so it cannot tell whether it is among the `K` closest until it knows them all.
#[tokio::test(flavor = "multi_thread")]
async fn dht_affinity_works() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let names = ["alice", "bob", "charlie"];
	let replication_factor: usize = 2;
	let network = spawn_network_with_injected_allowances_v2(
		&names,
		names.len() as u32,
		replication_factor as u32,
		TEST_GOSSIP_TARGET,
	)
	.await?;

	let mut nodes = Vec::with_capacity(names.len());
	for name in names {
		nodes.push(network.get_node(name)?);
	}
	let mut rpcs = Vec::with_capacity(nodes.len());
	for node in &nodes {
		rpcs.push(node.rpc().await?);
	}

	// Each node must learn the other two before its K=2 affinity decision is meaningful.
	for node in &nodes {
		node.wait_metric_with_timeout(
			KNOWN_PEERS_METRIC,
			|peers| peers >= (names.len() - 1) as f64,
			120u64,
		)
		.await?;
	}

	// With K=2 over three nodes, exactly two nodes are the DHT replicas for any topic.
	let topic: Topic = [7u8; 32].into();

	// Each node submits its own statement on the topic (distinct authors, distinct payloads).
	let mut statements = Vec::with_capacity(nodes.len());
	for (i, rpc) in rpcs.iter().enumerate() {
		let keypair = get_keypair(i as u32);
		let statement = create_test_statement(
			&keypair,
			&[topic],
			None,
			vec![i as u8, 1, 2, 3],
			u32::MAX,
			7000 + i as u32,
		);
		let expected: Bytes = statement.encode().into();
		assert_eq!(submit_statement(rpc, &statement).await?, SubmitResult::New);
		statements.push(expected);
	}

	// Poll until the K=2 distribution settles: the two replicas each store all three statements
	// (own plus the two routed to them), the non-replica stores none. Forwarding is asynchronous,
	// so we retry rather than sleep a fixed time.
	const ATTEMPTS: u32 = 30;
	for attempt in 0..ATTEMPTS {
		let mut counts = Vec::with_capacity(rpcs.len());
		for rpc in &rpcs {
			let mut held = 0usize;
			for expected in &statements {
				if stores_locally(rpc, topic, expected).await? {
					held += 1;
				}
			}
			counts.push(held);
		}

		let replicas = counts.iter().filter(|held| **held == statements.len()).count();
		let non_replicas = counts.iter().filter(|held| **held == 0).count();
		if replicas == replication_factor && non_replicas == names.len() - replication_factor {
			return Ok(());
		}

		if attempt + 1 == ATTEMPTS {
			return Err(anyhow::anyhow!(
				"unexpected storage distribution {counts:?}; expected {replication_factor} replicas \
				 holding all {} statements and {} non-replicas holding none",
				statements.len(),
				names.len() - replication_factor,
			));
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	}

	Ok(())
}

/// Subscribing gives a node explicit affinity: it then receives matching statements even though it
/// is not a DHT replica for the topic and stored nothing before.
///
/// Two nodes, `K=1`. We pick a topic and find which node is its single DHT replica, then submit a
/// statement on that replica. The other node is not a replica, so it stores nothing. Once it
/// subscribes to the topic, explicit affinity makes it receive the statement.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_affinity_works() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let names = ["alice", "bob"];
	let replication_factor: usize = 1;
	let network = spawn_network_with_injected_allowances_v2(
		&names,
		names.len() as u32,
		replication_factor as u32,
		TEST_GOSSIP_TARGET,
	)
	.await?;

	let alice = network.get_node(names[0])?;
	let bob = network.get_node(names[1])?;
	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let keypair = get_keypair(0);

	// Each node must learn the other before its affinity decision is meaningful.
	for node in [alice, bob] {
		node.wait_metric_with_timeout(KNOWN_PEERS_METRIC, |peers| peers >= 1.0, 120u64)
			.await?;
	}

	let topic: Topic = [0x33u8; 32].into();

	// Find the DHT replica for `topic`: submit a throwaway statement on alice; the replica keeps it
	// (persistent, visible to the query API), a non-replica drops it (transient, never surfaced).
	let probe = create_test_statement(&keypair, &[topic], None, vec![0x99, 0xfe], u32::MAX, 9000);
	let probe_expected: Bytes = probe.encode().into();
	assert_eq!(submit_statement(&alice_rpc, &probe).await?, SubmitResult::New);

	let alice_is_replica = stores_locally(&alice_rpc, topic, &probe_expected).await?;
	let (replica_rpc, non_replica_rpc) =
		if alice_is_replica { (&alice_rpc, &bob_rpc) } else { (&bob_rpc, &alice_rpc) };

	// Submit the statement on the affine (replica) node.
	let statement =
		create_test_statement(&keypair, &[topic], None, vec![0xee, 1, 2, 3], u32::MAX, 100);
	let expected: Bytes = statement.encode().into();
	assert_eq!(submit_statement(replica_rpc, &statement).await?, SubmitResult::New);

	// Wait for propagation. The replica stores its own submission; the non-replica is farther from
	// the topic, so it is never a routing target and should receive nothing.
	tokio::time::sleep(Duration::from_secs(10)).await;

	// The replica holds the statement (DHT affinity).
	assert!(stores_locally(replica_rpc, topic, &expected).await?);

	// The non-replica has no statements: it is neither a DHT replica nor (yet) a subscriber.
	assert!(!stores_locally(non_replica_rpc, topic, &expected).await?);

	// Subscribing grants the non-replica explicit affinity; it should now receive the statement.
	let mut subscription = subscribe_topic(non_replica_rpc, topic).await?;
	expect_statement_delivered(&mut subscription, &expected, 20).await?;

	Ok(())
}

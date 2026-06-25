// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the v2 DHT-affinity statement routing.
//!
//! v2 routes a statement to the `K` nodes with affinity to its topic instead of flooding every
//! peer. It is enabled per node by the `STATEMENT_STORE_V2_DHT_ENABLED` environment variable (off
//! by default); the replication factor `K` and gossip target are set via CLI flags.

use super::common::{
	expect_one_statement, spawn_network_with_injected_allowances_v2, stores_locally,
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

	let network =
		spawn_network_with_injected_allowances_v2(&["charlie", "dave"], 4, 1, TEST_GOSSIP_TARGET)
			.await?;

	let node_1 = network.get_node("charlie")?;
	let node_2 = network.get_node("dave")?;
	let rpc_1 = node_1.rpc().await?;
	let rpc_2 = node_2.rpc().await?;
	let keypair = get_keypair(0);

	// Wait until node_1 has an open statement substream to node_2
	node_1
		.wait_metric_with_timeout(CONNECTED_PEERS_METRIC, |peers| peers >= 1.0, 120)
		.await?;

	// Pick two topics node_1 is not a DHT replica for; with K=1 over two nodes, node_2 is the
	// replica for both.
	let topics = select_topics_non_affine_to(&rpc_1, &keypair, 2).await?;
	let topic_a = topics[0];
	let topic_b = topics[1];

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
	assert_eq!(expect_one_statement(&mut sub_a, 20).await?, expected_a);

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

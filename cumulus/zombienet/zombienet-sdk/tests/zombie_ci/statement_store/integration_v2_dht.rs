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
use sp_core::Bytes;
use sp_statement_store::{SubmitResult, Topic};
use std::time::Duration;

const TEST_GOSSIP_TARGET: u32 = 3;
// Statement-store peers known to a node's topology, exported per node by the v2 DHT path.
const KNOWN_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_known_peers";

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

	let network =
		spawn_network_with_injected_allowances_v2(&["alice", "bob"], 4, 1, TEST_GOSSIP_TARGET)
			.await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let keypair = get_keypair(0);

	// Each node must learn the other before its affinity decision is meaningful.
	for node in [alice, bob] {
		node.wait_metric_with_timeout(KNOWN_PEERS_METRIC, |peers| peers >= 1.0, 120)
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
	assert_eq!(expect_one_statement(&mut subscription, 20).await?, expected);

	Ok(())
}

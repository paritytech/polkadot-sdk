// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the v2 DHT-affinity statement routing.
//!
//! v2 routes a statement to the `K` nodes with affinity to its topic instead of flooding every
//! peer. It is enabled per node by the `STATEMENT_STORE_V2_DHT_ENABLED` environment variable (off
//! by default); the replication factor `K` and gossip target are set via CLI flags.

use super::common::{spawn_network_with_injected_allowances_v2, stores_locally, submit_statement};
use codec::Encode;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::Bytes;
use sp_statement_store::{SubmitResult, Topic};
use std::time::Duration;

const TEST_GOSSIP_TARGET: u32 = 3;
// Statement-store peers known to a node's topology, exported per node by the v2 DHT path.
const KNOWN_PEERS_METRIC: &str = "substrate_sync_statement_v2dht_known_peers";

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

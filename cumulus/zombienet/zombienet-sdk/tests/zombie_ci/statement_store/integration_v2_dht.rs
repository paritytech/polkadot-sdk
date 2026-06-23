// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the v2 DHT-affinity statement routing.
//!
//! v2 routes a statement to the `K` nodes with affinity to its topic instead of
//! flooding every peer. It is enabled per node via `--enable-statement-store-v2-dht`
//! (off by default).
//!

use super::common::{
	count_storers, expect_one_statement, spawn_network_with_injected_allowances_v2,
	submit_statement, subscribe_topic,
};
use codec::Encode;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::Bytes;
use sp_statement_store::{SubmitResult, Topic};

const TEST_GOSSIP_TARGET: u32 = 3;

/// With `K=1`, a submitted statement reaches a subscriber and lands on exactly one
/// node, and a second statement still arrives after the subscriber re-subscribes.
#[tokio::test(flavor = "multi_thread")]
async fn v2_dht_k1_subscribe_submit_resubscribe() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network =
		spawn_network_with_injected_allowances_v2(&["charlie", "dave"], 8, 1, TEST_GOSSIP_TARGET)
			.await?;

	let node_a = network.get_node("charlie")?;
	let node_b = network.get_node("dave")?;

	let rpc_a = node_a.rpc().await?;
	let rpc_b = node_b.rpc().await?;

	let topic: Topic = [0u8; 32].into();
	let keypair = get_keypair(0);

	// Subscribe on A, submit on B, expect A to receive it.
	let statement = create_test_statement(&keypair, &[topic], None, vec![1, 2, 3], u32::MAX, 0);
	let expected: Bytes = statement.encode().into();

	let mut sub_a = subscribe_topic(&rpc_a, topic).await?;
	let result = submit_statement(&rpc_b, &statement).await?;
	assert_eq!(result, SubmitResult::New);

	let received = expect_one_statement(&mut sub_a, 20).await?;
	assert_eq!(received, expected);

	// K=1, so the statement lives on a single node rather than on everyone.
	let storers = count_storers(&[&rpc_a, &rpc_b], topic, &expected).await?;
	assert_eq!(storers, 1);

	// Re-subscribe and submit a second statement on the same topic.
	let statement2 = create_test_statement(&keypair, &[topic], None, vec![4, 5, 6], u32::MAX, 1);
	let expected2: Bytes = statement2.encode().into();

	drop(sub_a);
	let mut sub_a2 = subscribe_topic(&rpc_a, topic).await?;
	let result2 = submit_statement(&rpc_b, &statement2).await?;
	assert_eq!(result2, SubmitResult::New);

	let mut saw_s2 = false;
	let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
	while tokio::time::Instant::now() < deadline {
		if expect_one_statement(&mut sub_a2, 20).await? == expected2 {
			saw_s2 = true;
			break;
		}
	}
	assert!(saw_s2);

	Ok(())
}

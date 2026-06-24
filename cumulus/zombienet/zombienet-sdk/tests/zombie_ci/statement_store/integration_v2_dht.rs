// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the v2 DHT-affinity statement routing.
//!
//! v2 routes a statement to the `K` nodes with affinity to its topic instead of
//! flooding every peer. It is enabled per node via `--enable-statement-store-v2-dht`
//! (off by default).

use super::common::{
	count_storers, expect_one_statement, spawn_network_with_injected_allowances_v2,
	stores_locally, submit_statement, subscribe_topic,
};
use codec::Encode;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::Bytes;
use sp_statement_store::{SubmitResult, Topic};

const TEST_GOSSIP_TARGET: u32 = 3;

/// With `K=1`, a submitted statement reaches a subscriber and lands on exactly one node by DHT
/// affinity, and a second statement still arrives after the subscriber re-subscribes.
///
/// Three nodes: A subscribes, B submits, C is a bystander. A always keeps a copy (its subscription
/// grants explicit affinity), so a storer count over all three is confounded by A. We instead probe
/// the non-subscribers {B, C}: under `K=1` exactly one node in the network is the DHT replica, so
/// when A is not that replica, exactly one of {B, C} stores `s` — under flooding both would. We
/// scan topics until A is not the replica (~2/3 of topics with three nodes) so the count is
/// deterministic, then assert delivery, the K=1 count, and re-subscribe delivery on that topic.
#[tokio::test(flavor = "multi_thread")]
async fn v2_dht_k1_subscribe_submit_resubscribe() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network_with_injected_allowances_v2(
		&["charlie", "dave", "eve"],
		8,
		1,
		TEST_GOSSIP_TARGET,
	)
	.await?;

	let rpc_a = network.get_node("charlie")?.rpc().await?;
	let rpc_b = network.get_node("dave")?.rpc().await?;
	let rpc_c = network.get_node("eve")?.rpc().await?;

	let keypair = get_keypair(0);

	// Let a submitted statement propagate and any transient copy drop before probing storage.
	const SETTLE_SECS: u64 = 10;
	// A is the replica for ~1/3 of topics, so a handful of attempts finds a non-affine A.
	const MAX_TOPIC_ATTEMPTS: u8 = 12;

	for attempt in 0..MAX_TOPIC_ATTEMPTS {
		let topic: Topic = [attempt; 32].into();
		let statement = create_test_statement(
			&keypair,
			&[topic],
			None,
			vec![attempt, 1, 2, 3],
			u32::MAX,
			attempt as u32,
		);
		let expected: Bytes = statement.encode().into();

		// Subscribe on A, submit on B, expect A to receive it — the headline contract.
		let mut sub_a = subscribe_topic(&rpc_a, topic).await?;
		assert_eq!(submit_statement(&rpc_b, &statement).await?, SubmitResult::New);
		assert_eq!(expect_one_statement(&mut sub_a, 20).await?, expected);

		// Let propagation settle, then probe the non-subscribers. Probing subscribes, which grants
		// explicit affinity for *future* statements only, so it cannot retroactively store this one.
		tokio::time::sleep(std::time::Duration::from_secs(SETTLE_SECS)).await;
		let storers_bc = count_storers(&[&rpc_b, &rpc_c], topic, &expected).await?;

		if storers_bc == 0 {
			// A is the DHT replica for this topic, so storage is confined to A: delivery still holds,
			// but the K=1 routing count is untestable over the non-subscribers. Try another topic.
			drop(sub_a);
			continue;
		}

		// A is not the replica: exactly one non-subscriber (the DHT replica) stores `s`, reached by
		// forwarding. Under flooding both {B, C} would store it; K=1 routing keeps it on one.
		assert_eq!(
			storers_bc, 1,
			"with K=1 exactly one non-subscriber stores the statement (topic attempt {attempt})"
		);

		// Re-subscribe and submit a second statement on the same topic; A must receive it too.
		let statement2 = create_test_statement(
			&keypair,
			&[topic],
			None,
			vec![attempt, 4, 5, 6],
			u32::MAX,
			MAX_TOPIC_ATTEMPTS as u32 + attempt as u32,
		);
		let expected2: Bytes = statement2.encode().into();

		drop(sub_a);
		let mut sub_a2 = subscribe_topic(&rpc_a, topic).await?;
		assert_eq!(submit_statement(&rpc_b, &statement2).await?, SubmitResult::New);

		// On re-subscribe A first replays `s` (kept via explicit affinity), then `s2` arrives live.
		let mut saw_s2 = false;
		let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
		while tokio::time::Instant::now() < deadline {
			if expect_one_statement(&mut sub_a2, 20).await? == expected2 {
				saw_s2 = true;
				break;
			}
		}
		assert!(saw_s2, "subscriber did not receive the second statement after re-subscribing");

		return Ok(());
	}

	panic!(
		"node A was the DHT replica for every one of {MAX_TOPIC_ATTEMPTS} probed topics; \
		 could not exercise the K=1 non-subscriber storage count"
	);
}

/// A local RPC submission obeys the same affinity rule as a network-received statement: the
/// submitter keeps it only with DHT or explicit affinity. v1 kept every local submission; v2 must
/// keep the affine ones and drop the rest.
///
/// We check both sides on one node A, using the affinity we can set deterministically — a
/// subscription grants explicit affinity, unlike DHT affinity, which a node's private view of the
/// topology decides and RPC cannot predict:
/// - **keep:** A subscribes to `topic_keep`, then submits on it. A must keep it.
/// - **drop:** A submits on many topics it did not subscribe to, so its only possible affinity is
///   DHT. With `K=1` over three nodes A is the DHT replica for only a fraction, so it must keep
///   strictly fewer than it submitted.
///
/// Together these pin the gate from both ends: keeping every submission (v1, gate off) fails the
/// drop check, dropping every submission (gate too aggressive) fails the keep check. The
/// DHT-affine-keep case stays in the unit test — RPC cannot pin which topic A is the replica for.
///
/// We warm up first: until A has learned its peers it considers itself the closest to every topic
/// (no peer is nearer) and the retention snapshot the store reads lags the live topology by an
/// affinity tick, so an immediate submission is misjudged. The gate is only meaningful once A's
/// topology has converged.
#[tokio::test(flavor = "multi_thread")]
async fn v2_dht_local_submission_obeys_affinity() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network_with_injected_allowances_v2(
		&["charlie", "dave", "eve"],
		8,
		1,
		TEST_GOSSIP_TARGET,
	)
	.await?;

	let rpc_a = network.get_node("charlie")?.rpc().await?;

	let keypair = get_keypair(0);

	// Let A's topology converge so its affinity decisions are meaningful (see the doc comment).
	const WARMUP_SECS: u64 = 25;
	// Let a new subscription reach the retention snapshot the store reads (refreshed on the affinity
	// tick) before submitting on the subscribed topic.
	const AFFINITY_SECS: u64 = 5;
	// Let each submission's transient copy forward and drop before probing, so a not-yet-dropped
	// copy is not mistaken for a kept one.
	const SETTLE_SECS: u64 = 10;
	// Topics A did not subscribe to, distinct from `topic_keep`. With `K=1` over three nodes A is the
	// replica for only a fraction, so the larger the batch the stronger the "kept < submitted"
	// signal. `[i; 32]` spreads the dominant XOR byte across the three nodes, so A cannot be the
	// replica for all of them.
	const UNSUBSCRIBED_COUNT: u8 = 16;

	tokio::time::sleep(std::time::Duration::from_secs(WARMUP_SECS)).await;

	// Keep side: subscribing grants A explicit affinity for `topic_keep`. Hold the subscription open
	// through submission so the affinity stands.
	let topic_keep: Topic = [200u8; 32].into();
	let _keep_sub = subscribe_topic(&rpc_a, topic_keep).await?;
	tokio::time::sleep(std::time::Duration::from_secs(AFFINITY_SECS)).await;
	let keep_statement =
		create_test_statement(&keypair, &[topic_keep], None, vec![200, 1, 2, 3], u32::MAX, 200);
	let keep_expected: Bytes = keep_statement.encode().into();
	assert_eq!(submit_statement(&rpc_a, &keep_statement).await?, SubmitResult::New);

	// Drop side: submit on topics A never subscribed to, so only DHT affinity could keep them.
	let mut unsubscribed = Vec::with_capacity(UNSUBSCRIBED_COUNT as usize);
	for i in 0..UNSUBSCRIBED_COUNT {
		let topic: Topic = [i; 32].into();
		let statement =
			create_test_statement(&keypair, &[topic], None, vec![i, 1, 2, 3], u32::MAX, i as u32);
		let expected: Bytes = statement.encode().into();
		assert_eq!(submit_statement(&rpc_a, &statement).await?, SubmitResult::New);
		unsubscribed.push((topic, expected));
	}

	// Probing subscribes, which grants explicit affinity for *future* statements only, so it cannot
	// turn an already-dropped submission into a kept one.
	tokio::time::sleep(std::time::Duration::from_secs(SETTLE_SECS)).await;

	assert!(
		stores_locally(&rpc_a, topic_keep, &keep_expected).await?,
		"submitter dropped a submission on a topic it subscribed to; explicit affinity did not keep it"
	);

	let mut kept = 0usize;
	for (topic, expected) in &unsubscribed {
		if stores_locally(&rpc_a, *topic, expected).await? {
			kept += 1;
		}
	}

	log::info!("submitter kept {kept}/{UNSUBSCRIBED_COUNT} of its unsubscribed submissions");
	assert!(
		kept < UNSUBSCRIBED_COUNT as usize,
		"non-affine submitter kept all {UNSUBSCRIBED_COUNT} unsubscribed submissions; the affinity \
		 gate dropped none of them (v1 behavior)"
	);

	Ok(())
}

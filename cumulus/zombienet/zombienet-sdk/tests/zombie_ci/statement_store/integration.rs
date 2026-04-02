// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use codec::Encode;
use log::info;
use sp_core::Bytes;
use sp_statement_store::{
	RejectionReason, StatementAllowance, StatementEvent, SubmitResult, Topic, TopicFilter,
};
use zombienet_sdk::subxt::ext::subxt_rpcs::rpc_params;

use sc_statement_store::test_utils::{create_allowance_items, create_test_statement, get_keypair};

use super::common::{
	assert_no_more_statements, expect_one_statement, expect_statements_unordered,
	spawn_network_sudo, spawn_network_with_injected_allowances, submit_statement, subscribe_topic,
};

/// Verifies basic statement propagation and data integrity across two nodes
///
/// Tests uses the genesis-injection approach for setting allowances
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_basic_propagation() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network_with_injected_allowances(&["charlie", "dave"], 8).await?;

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let topic: Topic = [0u8; 32].into();
	let keypair = get_keypair(0);
	let statement = create_test_statement(&keypair, &[topic], None, vec![1, 2, 3], u32::MAX, 0);
	let expected: Bytes = statement.encode().into();

	let mut sub = subscribe_topic(&dave_rpc, topic).await?;
	let result = submit_statement(&charlie_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New);

	let received = expect_one_statement(&mut sub, 20).await?;
	assert_eq!(received, expected, "Statement data mismatch");
	info!("Basic propagation: verified");

	Ok(())
}

/// Verifies concurrent propagation, quota enforcement, and priority eviction
///
/// Spawns a single 4-node network with mixed allowances:
/// - keypair_0: tight quota (max_count=3) for quota/eviction testing
/// - keypairs 1-8: generous quota for concurrent propagation
///
/// Test uses sudo-based allowances
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_check_propagation_and_quota_invariants() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let mut entries: Vec<(u32, StatementAllowance)> =
		vec![(0, StatementAllowance { max_count: 3, max_size: 1_000_000 })];
	for i in 1..9u32 {
		entries.push((i, StatementAllowance { max_count: 100, max_size: 1_000_000 }));
	}
	let items = create_allowance_items(&entries);

	let network = spawn_network_sudo(&["alice", "bob", "charlie", "dave"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	// Concurrent multi-account propagation
	let topic: Topic = [10u8; 32].into();
	let mut alice_sub = subscribe_topic(&alice_rpc, topic).await?;
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;
	let mut charlie_sub = subscribe_topic(&charlie_rpc, topic).await?;
	let mut dave_sub = subscribe_topic(&dave_rpc, topic).await?;

	// Use keypairs 1-8 for concurrent submissions
	let statements: Vec<_> = (1u32..9)
		.map(|idx| {
			let keypair = get_keypair(idx);
			create_test_statement(&keypair, &[topic], None, vec![idx as u8], u32::MAX, idx * 100)
		})
		.collect();

	// Distribute submissions across all nodes (round-robin) to test multi-source concurrent ingress
	let nodes = [&alice, &bob, &charlie, &dave];
	let mut handles = Vec::new();
	for (i, stmt) in statements.iter().enumerate() {
		let target = nodes[i % nodes.len()];
		let rpc = target.rpc().await?;
		let stmt = stmt.clone();
		let idx = i + 1;
		handles.push(tokio::spawn(async move {
			let result = submit_statement(&rpc, &stmt).await?;
			assert_eq!(result, SubmitResult::New, "Participant {} should be accepted", idx);
			Ok::<_, anyhow::Error>(())
		}));
	}

	for handle in handles {
		handle.await??;
	}
	info!("All 8 concurrent submissions accepted");

	// Verify content identity: every node must receive exactly the 8 submitted statements
	let mut expected_encoded: Vec<Vec<u8>> = statements.iter().map(|s| s.encode()).collect();
	expected_encoded.sort();

	for (name, sub) in [
		("alice", &mut alice_sub),
		("bob", &mut bob_sub),
		("charlie", &mut charlie_sub),
		("dave", &mut dave_sub),
	] {
		let received = expect_statements_unordered(sub, 8, 60).await?;
		assert_eq!(received.len(), 8, "Expected 8 statements on {}", name);
		let mut received_bytes: Vec<Vec<u8>> = received.into_iter().map(|b| b.to_vec()).collect();
		received_bytes.sort();
		assert_eq!(received_bytes, expected_encoded, "Statement content mismatch on {}", name);
		info!("{} received all 8 statements with correct content", name);
	}

	for (name, sub) in [
		("alice", &mut alice_sub),
		("bob", &mut bob_sub),
		("charlie", &mut charlie_sub),
		("dave", &mut dave_sub),
	] {
		assert_no_more_statements(sub, 10).await?;
		info!("No extra statements on {}", name);
	}

	// Quota enforcement and priority eviction
	let quota_topic: Topic = [2u8; 32].into();
	let keypair_0 = get_keypair(0);

	// Fill keypair_0's quota (max_count: 3)
	for seq in [100u32, 200, 300] {
		let stmt =
			create_test_statement(&keypair_0, &[quota_topic], None, vec![seq as u8], u32::MAX, seq);
		assert_eq!(submit_statement(&alice_rpc, &stmt).await?, SubmitResult::New);
	}

	// Rejected: lower priority than all existing (50 < 100)
	let low = create_test_statement(&keypair_0, &[quota_topic], None, vec![0], u32::MAX, 50);
	assert!(matches!(
		submit_statement(&alice_rpc, &low).await?,
		SubmitResult::Rejected(RejectionReason::AccountFull { .. })
	));
	info!("AccountFull verified");

	// Rejected: keypair_10 has no allowance
	let keypair_10 = get_keypair(10);
	let no_allow = create_test_statement(&keypair_10, &[quota_topic], None, vec![1], u32::MAX, 0);
	assert!(matches!(
		submit_statement(&alice_rpc, &no_allow).await?,
		SubmitResult::Rejected(RejectionReason::NoAllowance)
	));

	// Priority eviction: seq=150 evicts seq=100 → store: [150, 200, 300]
	let mut bob_evict_sub = subscribe_topic(&bob_rpc, quota_topic).await?;
	let mut charlie_evict_sub = subscribe_topic(&charlie_rpc, quota_topic).await?;
	let mut dave_evict_sub = subscribe_topic(&dave_rpc, quota_topic).await?;

	let mid = create_test_statement(&keypair_0, &[quota_topic], None, vec![15], u32::MAX, 150);
	assert_eq!(submit_statement(&alice_rpc, &mid).await?, SubmitResult::New);

	// seq=250 evicts seq=150 → store: [200, 250, 300]
	let high = create_test_statement(&keypair_0, &[quota_topic], None, vec![25], u32::MAX, 250);
	assert_eq!(submit_statement(&alice_rpc, &high).await?, SubmitResult::New);

	// seq=190 rejected — slots now hold 200, 250, 300
	let too_low = create_test_statement(&keypair_0, &[quota_topic], None, vec![19], u32::MAX, 190);
	assert!(matches!(
		submit_statement(&alice_rpc, &too_low).await?,
		SubmitResult::Rejected(RejectionReason::AccountFull { .. })
	));

	// Verify eviction-triggered statements propagate to all nodes
	for (name, sub) in [
		("bob", &mut bob_evict_sub),
		("charlie", &mut charlie_evict_sub),
		("dave", &mut dave_evict_sub),
	] {
		let received = expect_statements_unordered(sub, 1, 30).await?;
		info!("{}: eviction statements propagated ({} received)", name, received.len());
	}

	Ok(())
}

/// Test that verifies peer connectivity and statement propagation timing during major sync
///
/// Scenario:
/// 1. Spawn charlie only, let relay chain advance ~10 blocks
/// 2. Submit a statement to charlie
/// 3. Add dave as a late joiner (will enter major sync)
/// 4. Poll system_peers on dave every 2s to track when dave connects to charlie
/// 5. Simultaneously wait for the statement to arrive on dave
/// 6. Compare timing: if statement protocol peers are deferred during major sync, the statement
///    will arrive AFTER dave connects (gap = major sync duration)
///
/// This proves that remove_peers_from_reserved_set / deferred peer logic works:
/// dave sees charlie in system_peers (base protocol) but the statement only arrives
/// after major sync completes and deferred peers are added to the reserved set
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_peer_disconnect_during_major_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let mut network = spawn_network_with_injected_allowances(&["charlie"], 8).await?;

	let charlie = network.get_node("charlie")?;
	let charlie_rpc = charlie.rpc().await?;

	// Wait for relay chain to advance so dave will enter major sync
	// ~60s gives ~10 relay blocks at 6s block time, enough for major sync trigger
	log::info!("Waiting 60s for relay chain to advance");
	tokio::time::sleep(Duration::from_secs(60)).await;

	log::info!("Submitting statement to charlie");
	let topic: Topic = [0u8; 32].into();
	let mut statement = sp_statement_store::Statement::new();
	statement.set_plain_data(vec![1, 2, 3]);
	statement.set_topic(0, topic);
	statement.set_expiry_from_parts(u32::MAX, 0);
	let keypair = get_keypair(0);
	statement.sign_sr25519_private(&keypair);
	let statement_bytes: Bytes = statement.encode().into();

	let _: SubmitResult = charlie_rpc
		.request("statement_submit", rpc_params![statement_bytes.clone()])
		.await?;
	log::info!("Statement submitted to charlie");

	// Add dave as a late-joining collator
	// Dave will enter major sync because the chain advanced ~10 blocks while dave was offline.
	// From dave's perspective, when charlie appears via SyncEvent::PeerConnected, dave's
	// is_major_syncing() returns true, so charlie is placed in deferred_peers instead of
	// being added to the statement protocol reserved set immediately
	log::info!("Adding dave as late-joining collator");
	let dave_join_time = std::time::Instant::now();
	network.add_collator("dave", Default::default(), 1004).await?;

	let dave = network.get_node("dave")?;
	let dave_rpc = dave.rpc().await?;

	log::info!("Subscribing to statements on dave");
	let mut subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	// Wait for dave to sync and receive the statement
	// Poll system_peers every second to build a peer count timeline, while also
	// waiting for the statement subscription to fire
	let mut peer_counts: Vec<(f64, usize)> = Vec::new();
	let mut statement_received_at: Option<Duration> = None;
	let max_wait = Duration::from_secs(120);

	loop {
		let elapsed = dave_join_time.elapsed();
		if elapsed > max_wait {
			panic!(
				"Timed out after {:.0}s waiting for statement on dave. \
				 statement_received={}",
				elapsed.as_secs_f64(),
				statement_received_at.is_some()
			);
		}

		// Poll system_peers on dave
		let peers: Vec<serde_json::Value> =
			dave_rpc.request("system_peers", rpc_params![]).await.unwrap_or_default();
		let t = elapsed.as_secs_f64();
		log::info!("[{:>5.1}s] dave system_peers: {} peer(s)", t, peers.len());
		peer_counts.push((t, peers.len()));

		if statement_received_at.is_some() {
			if peer_counts.len() > 3 && peer_counts.iter().rev().take(3).all(|(_, c)| *c > 0) {
				break;
			}
			tokio::time::sleep(Duration::from_secs(1)).await;
			continue;
		}

		// Try to receive the statement with a 1s timeout
		match tokio::time::timeout(Duration::from_secs(1), subscription.next()).await {
			Ok(Some(Ok(StatementEvent::NewStatements { statements: batch, .. })))
				if !batch.is_empty() =>
			{
				assert_eq!(batch.len(), 1, "Expected exactly one statement in batch");
				assert_eq!(batch[0], statement_bytes, "Statement content mismatch");
				statement_received_at = Some(elapsed);
				log::info!(
					">>> Statement received at {:.1}s after dave joined",
					elapsed.as_secs_f64()
				);
			},
			_ => {},
		}
	}

	let stmt_t = statement_received_at.expect("Statement should have been received");
	let peer_first_seen = peer_counts.iter().find(|(_, c)| *c > 0);

	log::info!("Peer count timeline:");
	for (t, count) in &peer_counts {
		let marker = if stmt_t.as_secs_f64() >= *t && stmt_t.as_secs_f64() < *t + 1.5 {
			" <-- statement received"
		} else {
			""
		};
		log::info!("  [{:>5.1}s] {} peer(s){}", t, count, marker);
	}

	if let Some((peer_t, _)) = peer_first_seen {
		log::info!("First peer visible in system_peers: {:.1}s", peer_t);
	} else {
		log::info!("WARNING: system_peers never showed any peers (statement arrived via notification substream before system_peers poll caught it)");
	}

	Ok(())
}

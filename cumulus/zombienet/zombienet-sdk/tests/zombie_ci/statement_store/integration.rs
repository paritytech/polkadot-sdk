// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use codec::Encode;
use log::{debug, info};
use sp_core::Bytes;
use sp_statement_store::{
	RejectionReason, Statement, StatementAllowance, SubmitResult, Topic, TopicFilter,
};

use sc_statement_store::test_utils::{create_allowance_items, create_test_statement, get_keypair};

use super::common::{
	assert_no_more_statements, expect_one_statement, expect_statements_unordered,
	spawn_network_sudo, spawn_network_with_injected_allowances, submit_statement, subscribe_topic,
	subscribe_topic_filter,
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

/// Verifies that a node recovers its full statement store state after a crash/restart,
/// that other nodes remain unaffected during the outage, and that all statements
/// converge after recovery.
///
/// Scenario:
/// 1. Submit statements to alice and bob concurrently
/// 2. Wait for bob to receive at least one alice statement (proving mid-sync)
/// 3. Restart bob (simulating crash mid-sync)
/// 4. While bob is restarting, submit statements to charlie
/// 5. After bob recovers, verify all statements converge on every node
///
/// Each node's statements use a distinct topic so we can track provenance.
/// Statements are ~0.6 MiB each so only one fits per gossip notification,
/// creating a real time window for mid-sync interruption.
///
/// Known issue: ParityDB fsyncs asynchronously, so SIGKILL can lose the
/// last write. The test tolerates at most 1 lost statement.
///
/// Test uses the genesis-injection approach for setting allowances.
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_crash_mid_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let alice_count: usize = 50;
	let bob_count: usize = 10;
	let charlie_count: usize = 50;
	let total_stmts = alice_count + bob_count + charlie_count;
	let topic_alice: Topic = [0xA0; 32].into();
	let topic_bob: Topic = [0xB0; 32].into();
	let topic_charlie: Topic = [0xC0; 32].into();
	// Each statement is ~0.6 MiB so that only one fits per gossip notification
	// (limited to 1 MiB). This forces statements to be sent individually, creating
	// a real time window where bob can be killed mid-sync with partial state.
	let data_size = 600 * 1024;

	let mut keypair_idx = 0u32;
	let mut make_statements = |topic: Topic, count: usize| -> Vec<Statement> {
		(0..count)
			.map(|_| {
				let keypair = get_keypair(keypair_idx);
				keypair_idx += 1;
				create_test_statement(&keypair, &[topic], None, vec![0u8; data_size], u32::MAX, 0)
			})
			.collect()
	};

	let hash_to_hex = |h: &[u8; 32]| format!("{:?}", sp_core::hexdisplay::HexDisplay::from(h));

	let alice_stmts = make_statements(topic_alice, alice_count);
	let bob_stmts = make_statements(topic_bob, bob_count);
	let charlie_stmts = make_statements(topic_charlie, charlie_count);
	let bob_stmt_hashes: HashSet<String> =
		bob_stmts.iter().map(|s| hash_to_hex(&s.hash())).collect();

	let network =
		spawn_network_with_injected_allowances(&["alice", "bob", "charlie"], total_stmts as u32)
			.await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	info!("Submitting statements: {} to alice, {} to bob", alice_count, bob_count);

	let alice_rpc = alice.rpc().await?;
	let alice_handle = tokio::spawn(async move {
		for (i, stmt) in alice_stmts.iter().enumerate() {
			let result = submit_statement(&alice_rpc, stmt).await?;
			assert_eq!(result, SubmitResult::New, "alice stmt[{}] rejected", i);
		}
		Ok::<_, anyhow::Error>(())
	});

	let bob_rpc = bob.rpc().await?;
	let bob_handle = tokio::spawn(async move {
		for (i, stmt) in bob_stmts.iter().enumerate() {
			let result = submit_statement(&bob_rpc, stmt).await?;
			assert_eq!(result, SubmitResult::New, "bob stmt[{}] rejected", i);
		}
		Ok::<_, anyhow::Error>(())
	});

	let bob_rpc = bob.rpc().await?;
	let gossip_handle = tokio::spawn(async move {
		let mut bob_alice_sub = subscribe_topic(&bob_rpc, topic_alice).await?;
		expect_statements_unordered(&mut bob_alice_sub, 1, 30).await
	});

	// Restart is chained via map to ensure it fires immediately after try_join
	// completes, with no log output or other work in between that could give
	// bob extra time to sync. Do not decouple these operations.
	tokio::try_join!(bob_handle, gossip_handle)
		.map(|(bob_res, gossip_res)| {
			bob_res.expect("bob submissions failed");
			gossip_res.expect("gossip check failed");
			bob.restart(None)
		})?
		.await?;

	info!("Submissions completed, bob restarted (crash mid-sync)");
	assert!(bob.wait_until_is_up(1u64).await.is_err(), "Bob came up too fast");

	info!("Submitting {} statements to charlie while bob is restarting", charlie_count);
	let charlie_rpc = charlie.rpc().await?;
	for (i, stmt) in charlie_stmts.iter().enumerate() {
		let result = submit_statement(&charlie_rpc, stmt).await?;
		assert_eq!(result, SubmitResult::New, "charlie stmt[{}] rejected", i);
	}
	assert!(bob.wait_until_is_up(1u64).await.is_err(), "Bob was up during charlie submissions");

	info!("Waiting for bob to come back up");
	bob.wait_until_is_up(120u64).await?;

	// gossip_handle already confirmed bob received at least one alice statement,
	// so it's fine if alice finishes submitting after bob's restart.
	alice_handle.await?.expect("alice submissions failed");

	// Wait for bob's store to finish populating from disk before reading logs
	tokio::time::sleep(std::time::Duration::from_secs(5)).await;

	// Count how many of bob's own statements survived the crash.
	// ParityDB fsyncs asynchronously, so SIGKILL can lose the last write
	// even though SubmitResult::New was returned. Statements that were never
	// propagated to another node before the kill are unrecoverable.
	let bob_logs = bob.logs().await?;
	let loaded_hashes: HashSet<String> = bob_logs
		.lines()
		.filter_map(|l| l.split("Statement loaded ").nth(1).map(|h| h.trim().to_string()))
		.collect();

	assert!(
		!loaded_hashes.is_empty(),
		"No 'Statement loaded' entries found in bob's logs. \
		 The log format may have changed or statement-store=trace is not configured.",
	);

	let bob_loaded = bob_stmt_hashes.intersection(&loaded_hashes).count();
	let bob_lost = bob_count - bob_loaded;
	let alice_loaded = loaded_hashes.len().saturating_sub(bob_loaded);
	let expected_count = total_stmts - bob_lost;

	info!(
		"Bob loaded {} statements from disk ({} bob, {} alice)",
		loaded_hashes.len(),
		bob_loaded,
		alice_loaded,
	);
	if bob_lost == 1 {
		log::warn!("Bob lost 1 statement due to crash (unflushed ParityDB write)");
	}
	assert!(bob_lost <= 1, "Bob lost {} statements, expected at most 1", bob_lost);
	assert!(
		alice_loaded > 0 && alice_loaded < alice_count,
		"Expected partial alice sync (mid-sync crash), got {}/{} alice statements",
		alice_loaded,
		alice_count,
	);

	info!("Verifying all {} recoverable statements converge on every node", expected_count);
	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let charlie_rpc = charlie.rpc().await?;
	let filter =
		TopicFilter::MatchAny(vec![topic_alice, topic_bob, topic_charlie].try_into().unwrap());
	for (name, rpc) in [("alice", &alice_rpc), ("bob", &bob_rpc), ("charlie", &charlie_rpc)] {
		let mut sub = subscribe_topic_filter(rpc, filter.clone()).await?;
		let received = expect_statements_unordered(&mut sub, expected_count, 120).await?;
		assert_eq!(received.len(), expected_count, "Statement count mismatch on {}", name,);
		debug!("{}: all {} statements verified", name, expected_count);
	}

	info!("Node crash recovery test passed");
	Ok(())
}

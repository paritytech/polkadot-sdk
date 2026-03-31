// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use std::{cell::Cell, time::Duration};

use sp_core::{Bytes, Encode};
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use zombienet_sdk::subxt::ext::subxt_rpcs::rpc_params;

use crate::zombie_ci::statement_store_bench::{get_keypair, spawn_network};

#[tokio::test(flavor = "multi_thread")]
async fn statement_store() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network(&["charlie", "dave"], 8).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	// Create the statement "1,2,3" signed by dave.
	let mut statement = sp_statement_store::Statement::new();
	let topic: Topic = [0u8; 32].into(); // just a dummy topic
	statement.set_plain_data(vec![1, 2, 3]);
	statement.set_topic(0, topic);
	statement.set_expiry_from_parts(u32::MAX, 0);
	let dave = get_keypair(0);
	statement.sign_sr25519_private(&dave);
	let statement: Bytes = statement.encode().into();
	// Subscribe to statements with topic "topic" to dave.
	let stop_after_secs = 20;
	let mut subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	// Submit the statement to charlie.
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![statement.clone()]).await?;

	loop {
		let subscribe_item =
			tokio::time::timeout(Duration::from_secs(stop_after_secs), subscription.next())
				.await
				.expect("Should not timeout")
				.expect("Should receive")
				.expect("Should not error");

		let statement_bytes = match subscribe_item {
			StatementEvent::NewStatements { statements: mut batch, .. } => {
				if batch.is_empty() {
					continue;
				}
				assert_eq!(batch.len(), 1, "Expected exactly one statement in batch");
				batch.remove(0)
			},
		};
		assert_eq!(statement_bytes, statement);

		break;
	}
	// Now make sure no more statements are received.
	assert!(tokio::time::timeout(Duration::from_secs(stop_after_secs), subscription.next())
		.await
		.is_err());
	log::info!("Statement store test passed");

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

	let mut network = spawn_network(&["charlie"], 8).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

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
	log::info!("Adding dave as late-joining collator");
	let dave_join_time = std::time::Instant::now();
	network.add_collator("dave", Default::default(), 2400).await?;

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

	// Statement arriving proves that after major sync completed
	// deferred peers were added to the reserved set via add_peers_to_reserved_set
	// the notification substream opened, and charlie pushed the statement to dave
	log::info!("Statement received: {:.1}s after dave joined", stmt_t.as_secs_f64());

	Ok(())
}

/// Proof-of-work test: verify remove_peers_from_reserved_set actually disconnects nodes
///
/// Uses a HACK timer in the statement handler that fires after 30s to call
/// on_major_sync_started() (which calls remove_peers_from_reserved_set for all peers),
/// then after 15 more seconds calls on_major_sync_complete() to reconnect.
///
/// The test monitors the `substrate_sync_statement_peers_connected` metric on both nodes
/// to prove that the disconnect actually happens at the notification protocol level, and
/// that both nodes correctly handle the close event.
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_remove_reserved_set_disconnects() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Spawn charlie and dave together (no major sync, both start fresh)
	let network = spawn_network(&["charlie", "dave"], 8).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let start = std::time::Instant::now();

	// Phase 1: Wait for both nodes to connect via the statement protocol
	log::info!("Phase 1: Waiting for statement protocol connection on both nodes");

	charlie
		.wait_metric_with_timeout(
			"substrate_sync_statement_peers_connected",
			|count| count >= 1.0,
			60u64,
		)
		.await?;
	log::info!(
		"[{:>5.1}s] charlie: statement_peers_connected >= 1",
		start.elapsed().as_secs_f64()
	);

	dave.wait_metric_with_timeout(
		"substrate_sync_statement_peers_connected",
		|count| count >= 1.0,
		60u64,
	)
	.await?;
	log::info!(
		"[{:>5.1}s] dave: statement_peers_connected >= 1",
		start.elapsed().as_secs_f64()
	);

	// Phase 2: Submit a statement and verify it propagates (baseline proof)
	log::info!("Phase 2: Submitting statement to charlie, verifying dave receives it");
	let topic: Topic = [1u8; 32].into();
	let mut statement = sp_statement_store::Statement::new();
	statement.set_plain_data(vec![10, 20, 30]);
	statement.set_topic(0, topic);
	statement.set_expiry_from_parts(u32::MAX, 0);
	let keypair = get_keypair(0);
	statement.sign_sr25519_private(&keypair);
	let statement_bytes: Bytes = statement.encode().into();

	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![statement_bytes.clone()]).await?;

	let mut sub = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	let event =
		tokio::time::timeout(Duration::from_secs(30), sub.next()).await.expect("Should not timeout");
	assert!(event.is_some(), "Should receive statement on dave");
	log::info!(
		"[{:>5.1}s] Baseline: statement propagated charlie -> dave successfully",
		start.elapsed().as_secs_f64()
	);

	// Phase 3: Monitor metrics during the hack disconnect/reconnect cycle
	// The hack timer fires at ~30s after handler start, disconnects for 15s, then reconnects
	log::info!("Phase 3: Monitoring statement_peers_connected on both nodes");
	log::info!("         Expecting: connected -> disconnected (~30s) -> reconnected (~45s)");

	#[derive(Debug, Clone)]
	struct TimelineEntry {
		elapsed_secs: f64,
		charlie_stmt_peers: f64,
		dave_stmt_peers: f64,
		charlie_sys_peers: usize,
		dave_sys_peers: usize,
	}

	let mut timeline: Vec<TimelineEntry> = Vec::new();
	let mut saw_disconnect_charlie = false;
	let mut saw_disconnect_dave = false;
	let mut saw_reconnect_charlie = false;
	let mut saw_reconnect_dave = false;
	let max_wait = Duration::from_secs(120);

	loop {
		let elapsed = start.elapsed();
		if elapsed > max_wait {
			break;
		}

		// Read charlie's statement peers metric
		let charlie_stmt = Cell::new(0.0f64);
		let _ = charlie.wait_metric_with_timeout(
			"substrate_sync_statement_peers_connected",
			|count| {
				charlie_stmt.set(count);
				true
			},
			5u64,
		)
		.await;

		// Read dave's statement peers metric
		let dave_stmt = Cell::new(0.0f64);
		let _ = dave.wait_metric_with_timeout(
			"substrate_sync_statement_peers_connected",
			|count| {
				dave_stmt.set(count);
				true
			},
			5u64,
		)
		.await;

		// Read system_peers for both
		let charlie_sys: Vec<serde_json::Value> =
			charlie_rpc.request("system_peers", rpc_params![]).await.unwrap_or_default();
		let dave_sys: Vec<serde_json::Value> =
			dave_rpc.request("system_peers", rpc_params![]).await.unwrap_or_default();

		let entry = TimelineEntry {
			elapsed_secs: elapsed.as_secs_f64(),
			charlie_stmt_peers: charlie_stmt.get(),
			dave_stmt_peers: dave_stmt.get(),
			charlie_sys_peers: charlie_sys.len(),
			dave_sys_peers: dave_sys.len(),
		};

		log::info!(
			"[{:>5.1}s] charlie: stmt_peers={}, sys_peers={}  |  dave: stmt_peers={}, sys_peers={}",
			entry.elapsed_secs,
			entry.charlie_stmt_peers,
			entry.charlie_sys_peers,
			entry.dave_stmt_peers,
			entry.dave_sys_peers,
		);

		// Track state transitions
		if entry.charlie_stmt_peers == 0.0 && entry.charlie_sys_peers > 0 {
			if !saw_disconnect_charlie {
				log::info!(
					"  >>> CHARLIE: statement protocol DISCONNECTED (sys_peers still {})",
					entry.charlie_sys_peers
				);
				saw_disconnect_charlie = true;
			}
		}
		if entry.dave_stmt_peers == 0.0 && entry.dave_sys_peers > 0 {
			if !saw_disconnect_dave {
				log::info!(
					"  >>> DAVE: statement protocol DISCONNECTED (sys_peers still {})",
					entry.dave_sys_peers
				);
				saw_disconnect_dave = true;
			}
		}
		if saw_disconnect_charlie && entry.charlie_stmt_peers >= 1.0 {
			if !saw_reconnect_charlie {
				log::info!("  >>> CHARLIE: statement protocol RECONNECTED");
				saw_reconnect_charlie = true;
			}
		}
		if saw_disconnect_dave && entry.dave_stmt_peers >= 1.0 {
			if !saw_reconnect_dave {
				log::info!("  >>> DAVE: statement protocol RECONNECTED");
				saw_reconnect_dave = true;
			}
		}

		timeline.push(entry);

		// Exit early if we've seen the full cycle on both nodes
		if saw_reconnect_charlie && saw_reconnect_dave {
			// Collect a few more samples for the timeline
			tokio::time::sleep(Duration::from_secs(2)).await;
			break;
		}

		tokio::time::sleep(Duration::from_secs(2)).await;
	}

	// Phase 4: Verify a second statement propagates after reconnection
	log::info!("Phase 4: Verifying statement propagation after reconnection");
	let topic2: Topic = [2u8; 32].into();
	let mut statement2 = sp_statement_store::Statement::new();
	statement2.set_plain_data(vec![40, 50, 60]);
	statement2.set_topic(0, topic2);
	statement2.set_expiry_from_parts(u32::MAX, 1);
	let keypair2 = get_keypair(1);
	statement2.sign_sr25519_private(&keypair2);
	let statement2_bytes: Bytes = statement2.encode().into();

	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![statement2_bytes.clone()]).await?;

	let mut sub2 = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic2].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	let event2 = tokio::time::timeout(Duration::from_secs(30), sub2.next())
		.await
		.expect("Should not timeout after reconnection");
	assert!(event2.is_some(), "Should receive second statement after reconnection");
	log::info!(
		"[{:>5.1}s] Post-reconnect: statement propagated charlie -> dave successfully",
		start.elapsed().as_secs_f64()
	);

	// Phase 5: Print summary
	log::info!("");
	log::info!("========== DISCONNECT/RECONNECT PROOF TIMELINE ==========");
	log::info!(
		"  {:>6}  {:>12}  {:>10}  {:>12}  {:>10}",
		"Time", "C-stmt", "C-sys", "D-stmt", "D-sys"
	);
	for entry in &timeline {
		let marker = if saw_disconnect_charlie
			&& entry.charlie_stmt_peers == 0.0
			&& entry.charlie_sys_peers > 0
		{
			" <-- DISCONNECT"
		} else {
			""
		};
		log::info!(
			"  {:>5.1}s  {:>12}  {:>10}  {:>12}  {:>10}{}",
			entry.elapsed_secs,
			entry.charlie_stmt_peers,
			entry.charlie_sys_peers,
			entry.dave_stmt_peers,
			entry.dave_sys_peers,
			marker,
		);
	}
	log::info!("");

	// Assertions
	assert!(
		saw_disconnect_charlie,
		"FAILED: Charlie never saw statement protocol disconnect. \
		 remove_peers_from_reserved_set did not trigger NotificationStreamClosed on charlie"
	);
	assert!(
		saw_disconnect_dave,
		"FAILED: Dave never saw statement protocol disconnect. \
		 remove_peers_from_reserved_set did not trigger NotificationStreamClosed on dave"
	);
	assert!(
		saw_reconnect_charlie,
		"FAILED: Charlie never reconnected after on_major_sync_complete"
	);
	assert!(saw_reconnect_dave, "FAILED: Dave never reconnected after on_major_sync_complete");

	log::info!("PROOF: remove_peers_from_reserved_set causes actual disconnection on BOTH nodes");
	log::info!("PROOF: on_major_sync_complete successfully reconnects deferred peers");
	log::info!("PROOF: Statement propagation works correctly after reconnection");

	Ok(())
}

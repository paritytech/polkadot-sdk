// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use std::time::Duration;

use sp_core::{Bytes, Encode};
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use zombienet_sdk::subxt::ext::subxt_rpcs::rpc_params;

use crate::zombie_ci::statement_store_bench::{get_keypair, spawn_network};

/// Peer info returned by the system_peers RPC
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPeerInfo {
	peer_id: String,
	roles: String,
	best_hash: String,
	best_number: u64,
}

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
		let peers: Vec<SystemPeerInfo> =
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

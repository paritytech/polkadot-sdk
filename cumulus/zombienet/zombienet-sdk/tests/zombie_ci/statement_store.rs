// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use std::time::Duration;

use sp_core::{Bytes, Encode};
use sp_statement_store::{StatementEvent, SubmitResult, Topic, TopicFilter};
use zombienet_sdk::subxt::ext::subxt_rpcs::rpc_params;

use crate::zombie_ci::statement_store_bench::{
	get_keypair, spawn_network, spawn_network_with_extra_args,
};

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

/// Like `statement_store` but the subscribing node (dave) runs with
/// `--statement-advertise-affinity` to test that the node advertises its affinity
/// based on active subscriptions and still receives matching statements.
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_with_affinity() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Dave runs with --statement-advertise-affinity.
	let mut extra_args = std::collections::HashMap::new();
	extra_args.insert("dave", vec!["--statement-advertise-affinity".to_string()]);

	let network = spawn_network_with_extra_args(&["charlie", "dave"], 8, &extra_args).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	// Create the statement "1,2,3" signed by dave.
	let mut statement = sp_statement_store::Statement::new();
	let topic: Topic = [0u8; 32].into();
	statement.set_plain_data(vec![1, 2, 3]);
	statement.set_topic(0, topic);
	statement.set_expiry_from_parts(u32::MAX, 0);
	let dave_keypair = get_keypair(0);
	statement.sign_sr25519_private(&dave_keypair);
	let statement: Bytes = statement.encode().into();

	// Subscribe to statements with topic on dave.
	// This subscription drives the affinity filter that dave advertises to charlie.
	let stop_after_secs = 30;
	let mut subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	// Give some time for the affinity to be advertised to charlie.
	tokio::time::sleep(Duration::from_secs(10)).await;

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
	log::info!("Statement store with affinity test passed");

	Ok(())
}

/// Tests that changing affinity triggers a re-sync that delivers previously-missed statements.
///
/// This specifically exercises the affinity re-sync path (`process_pending_affinities` →
/// `schedule_initial_sync_for_peer`) by:
/// 1. Restart dave so its initial sync with charlie completes with an empty store
/// 2. Subscribe on dave with a **non-matching** topic → affinity advertised (wrong filter)
/// 3. Submit a statement with the **real** topic to charlie → charlie's gossip checks dave's
///    affinity → no match → dave does NOT get it
/// 4. Subscribe on dave with the **correct** topic → affinity changes → re-sent to charlie
/// 5. Charlie's `process_pending_affinities` picks up the new affinity, calls
///    `schedule_initial_sync_for_peer` → re-syncs → dave gets the statement
///
/// The statement can ONLY reach dave via the affinity-driven re-sync. It was not in the
/// initial sync (empty store), and the first affinity filter excluded it from gossip.
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_with_affinity_resync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Dave runs with --statement-advertise-affinity.
	let mut extra_args = std::collections::HashMap::new();
	extra_args.insert("dave", vec!["--statement-advertise-affinity".to_string()]);

	let network = spawn_network_with_extra_args(&["charlie", "dave"], 8, &extra_args).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;

	// The statement uses topic [0u8; 32].
	let real_topic: Topic = [0u8; 32].into();
	// A different topic that will NOT match the statement.
	let wrong_topic: Topic = [1u8; 32].into();

	// Create the statement "1,2,3" signed with a known keypair, using `real_topic`.
	let mut statement = sp_statement_store::Statement::new();
	statement.set_plain_data(vec![1, 2, 3]);
	statement.set_topic(0, real_topic);
	statement.set_expiry_from_parts(u32::MAX, 0);
	let dave_keypair = get_keypair(0);
	statement.sign_sr25519_private(&dave_keypair);
	let statement: Bytes = statement.encode().into();

	// Restart dave so it gets a fresh connection. Initial sync runs against
	// charlie's empty store (we haven't submitted anything yet).
	log::info!("Restarting dave to get a fresh connection with empty initial sync");
	dave.restart(Some(Duration::from_secs(3))).await?;

	// Wait for dave to come back up and reconnect to charlie.
	log::info!("Waiting for dave to come back up and reconnect");
	tokio::time::sleep(Duration::from_secs(15)).await;

	// Subscribe on dave with the WRONG topic. This advertises an affinity filter
	// that does NOT match the statement we'll submit.
	let dave_rpc = dave.rpc().await?;
	log::info!("Subscribing on dave with wrong topic to set non-matching affinity");
	let _wrong_subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![wrong_topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	// Wait for the wrong affinity to propagate to charlie.
	tokio::time::sleep(Duration::from_secs(10)).await;

	// Submit the statement to charlie. Charlie checks dave's affinity (wrong_topic) →
	// does NOT match real_topic → statement is NOT forwarded to dave.
	log::info!("Submitting statement to charlie (dave's affinity won't match)");
	let _: SubmitResult =
		charlie_rpc.request("statement_submit", rpc_params![statement.clone()]).await?;

	// Give gossip time to run — dave should NOT receive the statement.
	log::info!("Verifying dave does NOT receive statement with wrong affinity");
	tokio::time::sleep(Duration::from_secs(10)).await;

	// Now subscribe on dave with the CORRECT topic. This changes the affinity filter
	// to include real_topic. The change is detected, new affinity is sent to charlie.
	// Charlie's process_pending_affinities triggers schedule_initial_sync_for_peer,
	// which re-syncs all statements matching the new affinity — delivering our statement.
	log::info!("Subscribing on dave with correct topic to trigger affinity re-sync");
	let stop_after_secs = 60;
	let mut subscription = dave_rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![real_topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;

	log::info!("Waiting for dave to receive the statement via affinity-driven re-sync");
	loop {
		let subscribe_item =
			tokio::time::timeout(Duration::from_secs(stop_after_secs), subscription.next())
				.await
				.expect("Should not timeout — dave should receive statement via affinity re-sync")
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
	log::info!("Statement store with affinity reconnect test passed");

	Ok(())
}

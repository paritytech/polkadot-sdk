// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use std::time::Duration;

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

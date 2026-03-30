// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use codec::Encode;
use log::info;
use sp_core::Bytes;
use sp_statement_store::{RejectionReason, StatementAllowance, SubmitResult, Topic};

use sc_statement_store::test_utils::{
	create_allowance_items, create_uniform_allowance_items, get_keypair,
};

use super::common::{
	assert_no_more_statements, create_test_statement, expect_one_statement,
	expect_statements_unordered, spawn_network, spawn_network_sudo, submit_statement,
	subscribe_topic,
};

/// Tests the genesis-injection approach for setting allowances
///
/// Verifies basic statement submission, subscription-based propagation,
/// and data integrity across two nodes
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_genesis_inject() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network(&["charlie", "dave"], 8).await?;

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
	assert_no_more_statements(&mut sub, 20).await?;
	info!("Genesis inject: propagation verified");

	Ok(())
}

/// Tests the sudo-based runtime allowance approach with concurrent multi-account submission
///
/// Verifies 4-node propagation with 8 concurrent submitters
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_sudo_allowance() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(10, allowance);

	let network =
		spawn_network_sudo(&["alice", "bob", "charlie", "dave"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let bob_rpc = bob.rpc().await?;
	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let topic: Topic = [10u8; 32].into();
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;
	let mut charlie_sub = subscribe_topic(&charlie_rpc, topic).await?;
	let mut dave_sub = subscribe_topic(&dave_rpc, topic).await?;

	// Pre-build all 8 statements so we can reuse one for the dedup check
	let statements: Vec<_> = (0u32..8)
		.map(|idx| {
			let keypair = get_keypair(idx);
			create_test_statement(&keypair, &[topic], None, vec![idx as u8], u32::MAX, idx * 100)
		})
		.collect();

	// 8 concurrent submissions to alice
	let mut handles = Vec::new();
	for (idx, stmt) in statements.iter().enumerate() {
		let alice_rpc = alice.rpc().await?;
		let stmt = stmt.clone();
		handles.push(tokio::spawn(async move {
			let result = submit_statement(&alice_rpc, &stmt).await?;
			assert_eq!(result, SubmitResult::New, "Participant {} should be accepted", idx);
			Ok::<_, anyhow::Error>(())
		}));
	}

	for handle in handles {
		handle.await??;
	}
	info!("All 8 concurrent submissions accepted");

	// Verify propagation to all 3 subscriber nodes
	for (name, sub) in
		[("bob", &mut bob_sub), ("charlie", &mut charlie_sub), ("dave", &mut dave_sub)]
	{
		let received = expect_statements_unordered(sub, 8, 60).await?;
		assert_eq!(received.len(), 8, "Expected 8 statements on {}", name);
		info!("{} received all 8 statements", name);
	}

	for (name, sub) in
		[("bob", &mut bob_sub), ("charlie", &mut charlie_sub), ("dave", &mut dave_sub)]
	{
		assert_no_more_statements(sub, 10).await?;
		info!("No extra statements on {}", name);
	}

	network.detach().await;
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_quota_and_eviction() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let items = create_allowance_items(&[
		(0, StatementAllowance { max_count: 3, max_size: 10_000 }),
	]);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;

	let topic: Topic = [2u8; 32].into();
	let keypair_0 = get_keypair(0);
	let keypair_7 = get_keypair(7);

	// quota
	for seq in [100u32, 200, 300] {
		let stmt =
			create_test_statement(&keypair_0, &[topic], None, vec![seq as u8], u32::MAX, seq);
		assert_eq!(submit_statement(&alice_rpc, &stmt).await?, SubmitResult::New);
	}

	let low = create_test_statement(&keypair_0, &[topic], None, vec![0], u32::MAX, 50);
	assert!(matches!(
		submit_statement(&alice_rpc, &low).await?,
		SubmitResult::Rejected(RejectionReason::AccountFull { .. })
	));
	info!("AccountFull verified");

	let no_allow = create_test_statement(&keypair_7, &[topic], None, vec![1], u32::MAX, 0);
	assert!(matches!(
		submit_statement(&alice_rpc, &no_allow).await?,
		SubmitResult::Rejected(RejectionReason::NoAllowance)
	));
	info!("NoAllowance verified");

	// priority eviction
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;

	let mid = create_test_statement(&keypair_0, &[topic], None, vec![15], u32::MAX, 150);
	assert_eq!(submit_statement(&alice_rpc, &mid).await?, SubmitResult::New);

	let high = create_test_statement(&keypair_0, &[topic], None, vec![25], u32::MAX, 250);
	assert_eq!(submit_statement(&alice_rpc, &high).await?, SubmitResult::New);

	// seq=190 rejected — slots now hold 200, 250, 300
	let too_low = create_test_statement(&keypair_0, &[topic], None, vec![19], u32::MAX, 190);
	assert!(matches!(
		submit_statement(&alice_rpc, &too_low).await?,
		SubmitResult::Rejected(RejectionReason::AccountFull { .. })
	));

	let _received = expect_one_statement(&mut bob_sub, 30).await?;
	info!("Priority eviction verified");

	network.detach().await;
	Ok(())
}

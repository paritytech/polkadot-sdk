// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use codec::Encode;
use log::info;
use sp_core::Bytes;
use sp_statement_store::{Channel, RejectionReason, StatementAllowance, SubmitResult, Topic};
use zombienet_sdk::subxt::ext::subxt_rpcs::rpc_params;

use super::common::{
	assert_no_more_statements, create_allowance_items, create_test_statement,
	create_uniform_allowance_items, expect_one_statement, expect_statements_unordered, get_keypair,
	spawn_network_sudo, submit_statement, subscribe_all, subscribe_topic,
	subscribe_topic_match_any,
};

/// Tests concurrent multi-account submission to verify no statements are lost
///
/// 8 accounts submit concurrently to the same topic on node A; a subscriber
/// on node B collects all 8 statements
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_concurrent_multi_account_submission() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(10, allowance);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	info!("Network is running...");

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let bob_rpc = bob.rpc().await?;

	let topic: Topic = [10u8; 32].into();
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;

	// Spawn 8 concurrent submissions to alice
	let mut handles = Vec::new();
	for idx in 0u32..8 {
		let alice_rpc = alice.rpc().await?;
		handles.push(tokio::spawn(async move {
			let keypair = get_keypair(idx);
			let statement =
				create_test_statement(&keypair, &[topic], None, vec![idx as u8], u32::MAX, idx * 100);
			let result = submit_statement(&alice_rpc, &statement).await?;
			assert_eq!(result, SubmitResult::New, "Participant {} should be accepted", idx);
			Ok::<_, anyhow::Error>(())
		}));
	}

	for handle in handles {
		handle.await??;
	}
	info!("All 8 concurrent submissions accepted");

	// Collect all 8 statements on bob (order-independent)
	let received = expect_statements_unordered(&mut bob_sub, 8, 60).await?;
	assert_eq!(received.len(), 8, "Expected 8 propagated statements");
	info!("All 8 statements propagated to bob");

	assert_no_more_statements(&mut bob_sub, 10).await?;
	info!("Concurrent multi-account submission test passed");
	network.detach().await;
	Ok(())
}

/// Tests priority eviction ordering: lowest-priority-first eviction.
///
/// Fills 3 slots, rejects a lower-priority insert, then verifies higher-priority
/// inserts evict the lowest existing statement
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_priority_eviction_ordering() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowances = vec![(0u32, StatementAllowance { max_count: 3, max_size: 10_000 })];
	let items = create_allowance_items(&allowances);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let alice_rpc = alice.rpc().await?;

	let topic: Topic = [20u8; 32].into();
	let keypair = get_keypair(0);

	// Fill 3 slots with seq=100, 200, 300
	for seq in [100u32, 200, 300] {
		let stmt = create_test_statement(&keypair, &[topic], None, vec![seq as u8], u32::MAX, seq);
		let result = submit_statement(&alice_rpc, &stmt).await?;
		assert_eq!(result, SubmitResult::New, "seq={} should be New", seq);
	}
	info!("Filled 3 slots (seq=100, 200, 300)");

	// Lower priority (seq=50) should be rejected with AccountFull
	let low = create_test_statement(&keypair, &[topic], None, vec![0], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &low).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::AccountFull { .. }) => {
			info!("seq=50 correctly rejected with AccountFull");
		},
		other => panic!("Expected AccountFull for seq=50, got: {:?}", other),
	}

	// Higher priority (seq=150) should evict seq=100 (the lowest)
	let mid = create_test_statement(&keypair, &[topic], None, vec![15], u32::MAX, 150);
	let result = submit_statement(&alice_rpc, &mid).await?;
	assert_eq!(result, SubmitResult::New, "seq=150 should evict seq=100");
	info!("seq=150 accepted, evicted seq=100");

	// Even higher (seq=250) should evict seq=150 (now the lowest)
	let high = create_test_statement(&keypair, &[topic], None, vec![25], u32::MAX, 250);
	let result = submit_statement(&alice_rpc, &high).await?;
	assert_eq!(result, SubmitResult::New, "seq=250 should evict seq=150");
	info!("seq=250 accepted, evicted seq=150");

	// Now slots hold seq=200, 250, 300. A seq=190 should be rejected
	let too_low = create_test_statement(&keypair, &[topic], None, vec![19], u32::MAX, 190);
	let result = submit_statement(&alice_rpc, &too_low).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::AccountFull { .. }) => {
			info!("seq=190 correctly rejected (slots hold 200, 250, 300)");
		},
		other => panic!("Expected AccountFull for seq=190, got: {:?}", other),
	}

	info!("Priority eviction ordering test passed");
	network.detach().await;
	Ok(())
}

/// Tests TopicFilter semantics: MatchAll, MatchAny, and Any
///
/// Verifies that each subscription filter receives the correct subset of
/// statements based on their topic combinations
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_multi_topic_and_subscriptions() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(8, allowance);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;

	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;

	let topic_a: Topic = [0xAAu8; 32].into();
	let topic_b: Topic = [0xBBu8; 32].into();
	let topic_c: Topic = [0xCCu8; 32].into();

	// Set up 4 subscriptions on bob
	// 1. MatchAll([topic_a]) - matches statements that have topic_a (among others)
	let mut sub_match_all_a = subscribe_topic(&bob_rpc, topic_a).await?;
	// 2. MatchAny([topic_a, topic_b]) - matches statements with topic_a OR topic_b
	let mut sub_match_any_ab = subscribe_topic_match_any(&bob_rpc, vec![topic_a, topic_b]).await?;
	// 3. MatchAll([topic_a, topic_b]) - needs BOTH topic_a AND topic_b
	// We use the RPC directly for MatchAll with multiple topics
	let mut sub_match_all_ab = bob_rpc
		.subscribe::<sp_statement_store::StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![sp_statement_store::TopicFilter::MatchAll(
				vec![topic_a, topic_b].try_into().expect("MatchAll topics")
			)],
			"statement_unsubscribeStatement",
		)
		.await?;
	// 4. Any - matches everything
	let mut sub_any = subscribe_all(&bob_rpc).await?;

	// Submit 4 statements with different topic combos
	let keypair_0 = get_keypair(0);
	let keypair_1 = get_keypair(1);
	let keypair_2 = get_keypair(2);
	let keypair_3 = get_keypair(3);

	// stmt_a: only topic_a
	let stmt_a = create_test_statement(&keypair_0, &[topic_a], None, vec![1], u32::MAX, 100);
	// stmt_b: only topic_b
	let stmt_b = create_test_statement(&keypair_1, &[topic_b], None, vec![2], u32::MAX, 100);
	// stmt_ab: topic_a + topic_b
	let stmt_ab =
		create_test_statement(&keypair_2, &[topic_a, topic_b], None, vec![3], u32::MAX, 100);
	// stmt_c: only topic_c (should not match topic_a or topic_b filters)
	let stmt_c = create_test_statement(&keypair_3, &[topic_c], None, vec![4], u32::MAX, 100);

	for (label, stmt) in [("A", &stmt_a), ("B", &stmt_b), ("AB", &stmt_ab), ("C", &stmt_c)] {
		let result = submit_statement(&alice_rpc, stmt).await?;
		assert_eq!(result, SubmitResult::New, "Statement {} should be New", label);
	}
	info!("Submitted 4 statements (A, B, AB, C)");

	// sub_match_all_a: should receive stmt_a, stmt_ab (both contain topic_a). 2 statements
	let match_all_a = expect_statements_unordered(&mut sub_match_all_a, 2, 30).await?;
	assert_eq!(match_all_a.len(), 2, "MatchAll(topic_a) should get 2 statements");
	info!("MatchAll(A) correctly received 2 statements");

	// sub_match_any_ab: should receive stmt_a, stmt_b, stmt_ab (any with topic_a or topic_b). 3
	let match_any_ab = expect_statements_unordered(&mut sub_match_any_ab, 3, 30).await?;
	assert_eq!(match_any_ab.len(), 3, "MatchAny(A,B) should get 3 statements");
	info!("MatchAny(A,B) correctly received 3 statements");

	// sub_match_all_ab: should receive only stmt_ab (has both topic_a AND topic_b). 1
	let match_all_ab = expect_statements_unordered(&mut sub_match_all_ab, 1, 30).await?;
	assert_eq!(match_all_ab.len(), 1, "MatchAll(A,B) should get 1 statement");
	info!("MatchAll(A,B) correctly received 1 statement");

	// sub_any: should receive all 4
	let any_all = expect_statements_unordered(&mut sub_any, 4, 30).await?;
	assert_eq!(any_all.len(), 4, "Any should get 4 statements");
	info!("Any correctly received 4 statements");

	// No more on any subscription
	assert_no_more_statements(&mut sub_match_all_a, 5).await?;
	assert_no_more_statements(&mut sub_match_any_ab, 5).await?;
	assert_no_more_statements(&mut sub_match_all_ab, 5).await?;
	assert_no_more_statements(&mut sub_any, 5).await?;

	info!("Multi-topic and subscriptions test passed");
	network.detach().await;
	Ok(())
}

/// Tests channel replacement rules
///
/// Verifies that a channel message can only be replaced by one with higher priority,
/// and that different channels are independent
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_channel_replacement() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowances = vec![(0u32, StatementAllowance { max_count: 5, max_size: 50_000 })];
	let items = create_allowance_items(&allowances);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let alice_rpc = alice.rpc().await?;

	let topic: Topic = [30u8; 32].into();
	let channel_1: Channel = [1u8; 32];
	let channel_2: Channel = [2u8; 32];
	let keypair = get_keypair(0);

	// Submit initial channel message with seq=100
	let stmt_100 = create_test_statement(&keypair, &[topic], Some(channel_1), vec![100], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_100).await?;
	assert_eq!(result, SubmitResult::New, "Channel 1 seq=100 should be New");
	info!("Channel 1: seq=100 accepted");

	// Try lower seq=50 on same channel -> ChannelPriorityTooLow
	let stmt_50 = create_test_statement(&keypair, &[topic], Some(channel_1), vec![50], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &stmt_50).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. }) => {
			info!("Channel 1: seq=50 correctly rejected with ChannelPriorityTooLow");
		},
		other => panic!("Expected ChannelPriorityTooLow for seq=50, got: {:?}", other),
	}

	// Try equal seq=100 on same channel -> ChannelPriorityTooLow
	let stmt_100_dup =
		create_test_statement(&keypair, &[topic], Some(channel_1), vec![101], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_100_dup).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. }) => {
			info!("Channel 1: seq=100 (dup) correctly rejected with ChannelPriorityTooLow");
		},
		other => panic!("Expected ChannelPriorityTooLow for equal seq=100, got: {:?}", other),
	}

	// Higher seq=200 on same channel -> replaces
	let stmt_200 = create_test_statement(&keypair, &[topic], Some(channel_1), vec![200], u32::MAX, 200);
	let result = submit_statement(&alice_rpc, &stmt_200).await?;
	assert_eq!(result, SubmitResult::New, "Channel 1 seq=200 should replace seq=100");
	info!("Channel 1: seq=200 accepted (replaced seq=100)");

	// Different channel is independent. seq=50 on channel_2 should succeed
	let stmt_ch2 = create_test_statement(&keypair, &[topic], Some(channel_2), vec![50], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &stmt_ch2).await?;
	assert_eq!(result, SubmitResult::New, "Channel 2 seq=50 should be independent");
	info!("Channel 2: seq=50 accepted (independent from channel 1)");

	info!("Channel replacement test passed");
	network.detach().await;
	Ok(())
}

/// Tests topic-based subscriber isolation across multiple nodes
///
/// Verifies that subscribers on different nodes only receive statements
/// matching their subscribed topics
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_subscriber_isolation() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(8, allowance);

	let network = spawn_network_sudo(&["alice", "bob", "charlie"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let charlie_rpc = charlie.rpc().await?;

	let topic_a: Topic = [0xA1u8; 32].into();
	let topic_b: Topic = [0xB2u8; 32].into();

	// Alice subscribes to topic_a, Bob subscribes to topic_b, Charlie subscribes to topic_a
	let mut alice_sub = subscribe_topic(&alice_rpc, topic_a).await?;
	let mut bob_sub = subscribe_topic(&bob_rpc, topic_b).await?;
	let mut charlie_sub = subscribe_topic(&charlie_rpc, topic_a).await?;

	let keypair_0 = get_keypair(0);
	let keypair_1 = get_keypair(1);

	// Submit topic_a statement via alice
	let stmt_a = create_test_statement(&keypair_0, &[topic_a], None, vec![0xA1], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_a).await?;
	assert_eq!(result, SubmitResult::New);
	info!("Submitted topic_a statement");

	// Submit topic_b statement via bob
	let stmt_b = create_test_statement(&keypair_1, &[topic_b], None, vec![0xB2], u32::MAX, 100);
	let result = submit_statement(&bob_rpc, &stmt_b).await?;
	assert_eq!(result, SubmitResult::New);
	info!("Submitted topic_b statement");

	// Alice (topic_a subscriber) should get topic_a propagated from network
	let _received_a = expect_one_statement(&mut alice_sub, 30).await?;
	info!("Alice received topic_a statement");

	// Charlie (topic_a subscriber) should get topic_a
	let _received_c = expect_one_statement(&mut charlie_sub, 30).await?;
	info!("Charlie received topic_a statement");

	// Bob (topic_b subscriber) should get topic_b
	let _received_b = expect_one_statement(&mut bob_sub, 30).await?;
	info!("Bob received topic_b statement");

	// Alice should NOT get topic_b (wrong filter)
	assert_no_more_statements(&mut alice_sub, 10).await?;
	info!("Alice correctly did not receive topic_b");

	// Bob should NOT get topic_a
	assert_no_more_statements(&mut bob_sub, 10).await?;
	info!("Bob correctly did not receive topic_a");

	// Charlie should NOT get topic_b
	assert_no_more_statements(&mut charlie_sub, 10).await?;
	info!("Charlie correctly did not receive topic_b");

	info!("Subscriber isolation test passed");
	network.detach().await;
	Ok(())
}

/// Tests high-throughput propagation without statement loss
///
/// 16 accounts submit 3 statements each (48 total), split across 2 nodes
/// A subscriber on a 3rd node collects all 48
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_high_throughput_propagation() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(20, allowance);

	let network = spawn_network_sudo(&["alice", "bob", "charlie"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	let charlie_rpc = charlie.rpc().await?;
	let topic: Topic = [40u8; 32].into();

	let mut charlie_sub = subscribe_topic(&charlie_rpc, topic).await?;

	// Submit 48 statements: participants 0-7 to alice, 8-15 to bob, 3 each
	let mut handles = Vec::new();
	for idx in 0u32..16 {
		let rpc = if idx < 8 { alice.rpc().await? } else { bob.rpc().await? };

		handles.push(tokio::spawn(async move {
			let keypair = get_keypair(idx);
			for msg in 0u32..3 {
				let seq = idx * 1000 + msg * 100;
				let stmt = create_test_statement(
					&keypair,
					&[topic],
					None,
					vec![idx as u8, msg as u8],
					u32::MAX,
					seq,
				);
				let result = submit_statement(&rpc, &stmt).await?;
				assert_eq!(
					result,
					SubmitResult::New,
					"Participant {} msg {} should be New",
					idx,
					msg
				);
			}
			Ok::<_, anyhow::Error>(())
		}));
	}

	for handle in handles {
		handle.await??;
	}
	info!("All 48 statements submitted across alice and bob");

	// Collect all 48 on charlie
	let received = expect_statements_unordered(&mut charlie_sub, 48, 120).await?;
	assert_eq!(received.len(), 48, "Charlie should receive all 48 statements");
	info!("Charlie received all 48 statements");

	assert_no_more_statements(&mut charlie_sub, 10).await?;
	info!("High-throughput propagation test passed");
	network.detach().await;
	Ok(())
}

/// Tests gossip-layer deduplication.
///
/// Submits a statement, confirms propagation, resubmits the same statement,
/// and verifies no re-propagation occurs
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_deduplication() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowance = StatementAllowance { max_count: 100, max_size: 1_000_000 };
	let items = create_uniform_allowance_items(8, allowance);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;

	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;

	let topic: Topic = [50u8; 32].into();
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;

	let keypair = get_keypair(0);
	let statement = create_test_statement(&keypair, &[topic], None, vec![1, 2, 3], u32::MAX, 100);
	let expected_bytes: Bytes = statement.encode().into();

	// First submission
	let result = submit_statement(&alice_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New);
	info!("First submission accepted");

	// Confirm propagation to bob
	let received = expect_one_statement(&mut bob_sub, 30).await?;
	assert_eq!(received, expected_bytes);
	info!("Statement propagated to bob");

	// Resubmit the exact same statement to alice
	let result = submit_statement(&alice_rpc, &statement).await?;
	match result {
		SubmitResult::Known | SubmitResult::New => {
			info!("Resubmit returned {:?} (expected Known or New for local source)", result);
		},
		other => panic!("Unexpected resubmit result: {:?}", other),
	}

	// Verify NO re-propagation to bob
	assert_no_more_statements(&mut bob_sub, 15).await?;
	info!("No re-propagation observed - deduplication working");

	info!("Deduplication test passed");
	network.detach().await;
	Ok(())
}

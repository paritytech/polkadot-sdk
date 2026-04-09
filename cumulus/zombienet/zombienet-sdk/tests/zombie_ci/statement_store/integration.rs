// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use super::common::{
	assert_no_more_statements, base_dir, collator_default_args,
	create_chain_spec_with_allowances, expect_one_statement, expect_statements_unordered,
	spawn_network_sudo, spawn_network_with_injected_allowances, submit_statement, subscribe_topic,
};
use codec::Encode;
use log::info;
use sc_statement_store::test_utils::{create_allowance_items, create_test_statement, get_keypair};
use sp_core::Bytes;
use sp_statement_store::{RejectionReason, StatementAllowance, SubmitResult, Topic};
use std::{cell::Cell, sync::Arc, time::Duration};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

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

async fn spawn_flooding_network(
	rate_limit: u32,
	participant_count: u32,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let base_dir = base_dir()?;
	let chain_spec_path = create_chain_spec_with_allowances(participant_count, &base_dir)?;

	let default_args = collator_default_args(participant_count);
	let mut bob_args = default_args.clone();
	bob_args.push(format!("--statement-rate-limit={rate_limit}").as_str().into());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			p.with_id(1004)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(default_args)
				.with_collator(|n| n.with_name("alice"))
				.with_collator(|n| n.with_name("bob").with_args(bob_args))
		})
		.with_global_settings(|global_settings| {
			global_settings.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
		})
		.build()
		.map_err(super::common::format_build_errors)?;

	let network = crate::utils::initialize_network(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());
	Ok(network)
}

/// Verifies sustained-rate flooding detection end-to-end.
///
/// RPC submission achieves ~600 statements/sec. Gossip batches them every 1s.
/// With bob's rate=200 (burst=1,000), each gossip batch (~600) is within burst
/// but exceeds the per-second rate. Tokens drain at ~400/sec, exhausting in ~2.5s.
/// Differentiator: bob.submitted_statements > 0 (early batches were accepted).
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_sustained_rate_flooding() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let batch_size = 1_000u32;
	let network = spawn_flooding_network(200, batch_size * 10).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;

	let alice_rpc = Arc::new(alice.rpc().await?);

	let submit_handle = tokio::spawn({
		let alice_rpc = Arc::clone(&alice_rpc);
		async move {
			let topic: Topic = [42u8; 32].into();
			for batch in 0u32..30 {
				let start = batch * batch_size;
				for idx in start..start + batch_size {
					let keypair = get_keypair(idx);
					let statement = create_test_statement(
						&keypair,
						&[topic],
						None,
						vec![idx as u8],
						u32::MAX,
						0,
					);
					let _ = submit_statement(&alice_rpc, &statement).await;
				}
				info!("Batch {}: submitted {} statements", batch, batch_size);
				tokio::time::sleep(Duration::from_secs(1)).await;
			}
		}
	});

	bob.wait_metric_with_timeout(
		"substrate_sync_statement_flooding_detected",
		|count| count >= 1.0,
		120u64,
	)
	.await?;
	info!("Bob detected sustained-rate flooding");

	submit_handle.abort();

	let bob_submitted = Cell::new(0.0f64);
	bob.wait_metric_with_timeout(
		"substrate_sub_statement_store_submitted_statements",
		|v| {
			bob_submitted.set(v);
			true
		},
		10u64,
	)
	.await?;
	assert!(
		bob_submitted.get() > 0.0,
		"Bob should have accepted early batches before flooding (got {})",
		bob_submitted.get()
	);
	info!(
		"Bob accepted {} statements before flooding (sustained, not burst)",
		bob_submitted.get()
	);

	let alice_flooding = Cell::new(0.0f64);
	alice
		.wait_metric_with_timeout(
			"substrate_sync_statement_flooding_detected",
			|v| {
				alice_flooding.set(v);
				true
			},
			10u64,
		)
		.await?;
	assert_eq!(alice_flooding.get() as u64, 0, "Alice should not detect flooding");
	info!("No false positives on alice");

	Ok(())
}

/// Verifies burst flooding detection end-to-end.
///
/// RPC submission achieves ~600 statements/sec, all buffered before the first gossip tick.
/// With bob's rate=50 (burst=250), the first gossip batch (~600) exceeds burst immediately.
/// Differentiator: bob.submitted_statements == 0 (no statements accepted before detection).
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_burst_flooding() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let burst_size = 1_000u32;
	let network = spawn_flooding_network(50, burst_size).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;

	let alice_rpc = alice.rpc().await?;
	let topic: Topic = [43u8; 32].into();

	for idx in 0..burst_size {
		let keypair = get_keypair(idx);
		let statement =
			create_test_statement(&keypair, &[topic], None, vec![idx as u8], u32::MAX, 0);
		let _ = submit_statement(&alice_rpc, &statement).await;
	}
	info!("Submitted {} statements to alice", burst_size);

	bob.wait_metric_with_timeout(
		"substrate_sync_statement_flooding_detected",
		|count| count >= 1.0,
		60u64,
	)
	.await?;
	info!("Bob detected burst flooding");

	let bob_submitted = Cell::new(0.0f64);
	bob.wait_metric_with_timeout(
		"substrate_sub_statement_store_submitted_statements",
		|v| {
			bob_submitted.set(v);
			true
		},
		10u64,
	)
	.await?;
	assert_eq!(
		bob_submitted.get() as u64,
		0,
		"Bob should not have accepted any statements (burst, not sustained)"
	);
	info!("Bob accepted 0 statements (burst flooding confirmed)");

	let alice_flooding = Cell::new(0.0f64);
	alice
		.wait_metric_with_timeout(
			"substrate_sync_statement_flooding_detected",
			|v| {
				alice_flooding.set(v);
				true
			},
			10u64,
		)
		.await?;
	assert_eq!(alice_flooding.get() as u64, 0, "Alice should not detect flooding");
	info!("No false positives on alice");

	Ok(())
}

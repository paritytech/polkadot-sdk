// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use codec::Encode;
use log::info;
use sp_core::{sr25519, Bytes, Pair};
use sp_statement_store::{RejectionReason, StatementAllowance, SubmitResult, Topic};
use subxt::{dynamic::Value, transactions::Signer};
use verifiable::{ring_vrf_impl::BandersnatchVrfVerifiable as Crypto, GenerateVerifiable};

use sc_statement_store::{
	subxt_client::{
		create_attest_call, create_consumer_registration_params, create_increase_allowance_call,
		submit_extrinsic, CustomConfig, MSG_PREFIX,
	},
	test_utils::{create_allowance_items, create_test_statement, get_keypair},
};

use super::common::{
	assert_no_more_statements, expect_one_statement, expect_statements_unordered,
	online_client_from_node, spawn_network, spawn_network_sudo,
	spawn_network_with_injected_allowances, submit_statement, subscribe_topic,
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

/// Tests statement store submit+propagate using a lite person registered via extrinsics
///
/// Unlike the basic tests that use genesis-baked allowances, this test registers a lite person
/// via real extrinsics (increase_attestation_allowance + attest), and then verifies the registered
/// candidate can submit and propagate statements
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_lite_person_submit_and_propagate() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network(&["alice", "bob"]).await?;

	let alice_node = network.get_node("alice")?;
	let bob_node = network.get_node("bob")?;
	let para_client = online_client_from_node(alice_node).await?;

	let alice = subxt_signer::sr25519::dev::alice();
	let alice_account_id =
		<subxt_signer::sr25519::Keypair as Signer<CustomConfig>>::account_id(&alice);

	// Grant one attestation allowance via sudo
	info!("Granting attestation allowance to Alice...");
	let increase_call = create_increase_allowance_call(alice_account_id.0.to_vec(), 1);
	let mut nonce = para_client.tx().await?.account_nonce(&alice_account_id).await?;
	info!("Alice nonce before increase_allowance: {nonce}");
	let _block_hash = submit_extrinsic(&para_client, &increase_call, &alice, nonce).await?;
	nonce += 1;
	info!("Attestation allowance granted");

	let candidate_pair = sr25519::Pair::from_seed(&[77u8; 32]);
	let candidate_account: [u8; 32] = candidate_pair.public().0;

	// Generate ring-VRF keypair
	let ring_secret = Crypto::new_secret([42u8; 32]);
	let ring_member = Crypto::member_from_secret(&ring_secret);
	let msg = {
		let candidate_encoded = candidate_account.encode();
		let ring_member_encoded = ring_member.encode();
		[MSG_PREFIX.as_slice(), &candidate_encoded, &ring_member_encoded].concat()
	};
	let candidate_sig = candidate_pair.sign(&msg);

	let proof_of_ownership =
		Crypto::sign(&ring_secret, &msg).expect("ring VRF signing should succeed");

	// Consumer registration: Alice registers herself as consumer.
	// The consumer signs the payload; verifier is Alice (the attest origin).
	let alice_sp_pair =
		sr25519::Pair::from_string("//Alice", None).expect("Alice dev key should be valid");
	let consumer_registration = create_consumer_registration_params(
		&alice_sp_pair,
		&alice_account_id.0,
		&alice_account_id.0,
	);

	info!("Submitting PeopleLite::attest call with nonce {nonce}...");
	let attest_call = create_attest_call(
		candidate_account.to_vec(),
		candidate_sig.0.to_vec(),
		ring_member.0.to_vec(),
		proof_of_ownership.to_vec(),
		Some(consumer_registration),
	);
	let block_hash = submit_extrinsic(&para_client, &attest_call, &alice, nonce).await?;
	info!(
		"Attest call succeeded — lite person registered with consumer allowance (block {block_hash:?})"
	);

	// Verify the candidate appears in LitePeople storage
	let lite_people_query =
		subxt::dynamic::storage::<([u8; 32],), Value>("PeopleLite", "LitePeople");
	let at_block = para_client.at_block(block_hash).await?;
	let entry = at_block.storage().try_fetch(lite_people_query, (candidate_account,)).await?;
	assert!(entry.is_some(), "Candidate should be registered in LitePeople storage");
	info!("Verified: candidate is present in LitePeople storage");

	// Wait for the attest block to finalize before submitting statements
	let at_block = para_client.at_block(block_hash).await?;
	let attest_block_number = at_block.block_number() as f64;
	info!("Waiting for attest block ({attest_block_number}) to finalize...");
	alice_node
		.wait_metric_with_timeout(
			"block_height{status=\"finalized\"}",
			|height| height >= attest_block_number,
			120u64,
		)
		.await?;
	info!("Attest block finalized");

	let bob_rpc = bob_node.rpc().await?;
	let topic: Topic = [0u8; 32].into();
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;

	// Statement must be signed by Alice (the consumer) who has the statement store allowance
	let statement =
		create_test_statement(&alice_sp_pair, &[topic], None, vec![1, 2, 3], u32::MAX, 0);
	let expected: Bytes = statement.encode().into();

	let alice_rpc = alice_node.rpc().await?;
	let result = submit_statement(&alice_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New);
	info!("Statement submitted to alice");

	// Statement should propagate to bob
	let received = expect_one_statement(&mut bob_sub, 20).await?;
	assert_eq!(received, expected);
	info!("Statement received on bob with correct data");

	assert_no_more_statements(&mut bob_sub, 20).await?;
	info!("Statement store lite person submit and propagate test passed");

	Ok(())
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Duration};

use anyhow::anyhow;
use codec::Encode;
use futures::StreamExt;
use log::info;
use sp_core::{Bytes, Pair};
use sp_statement_store::{
	statement_allowance_key, Channel, InvalidReason, RejectionReason, StatementAllowance,
	SubmitResult, Topic,
};
use zombienet_sdk::{
	subxt::{
		config::polkadot::PolkadotExtrinsicParamsBuilder,
		dynamic::Value,
		ext::{scale_value::value, subxt_rpcs::rpc_params},
		tx::{signer::Signer, DynamicPayload, TxStatus},
		utils::H256,
		OnlineClient, PolkadotConfig,
	},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

use super::common::{
	assert_no_more_statements, create_channel_statement, create_multi_topic_statement,
	create_test_statement, expect_statement, expect_statements_unordered, get_keypair,
	submit_statement, subscribe_all, subscribe_topic, subscribe_topic_match_any,
};

/// Creates storage items for custom per-participant allowances
fn create_allowance_items(allowances: &[(u32, StatementAllowance)]) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut items = Vec::with_capacity(allowances.len());
	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance.encode()));
	}
	items
}

/// Creates uniform allowance storage items for a range of participants
fn create_uniform_allowance_items(
	count: u32,
	allowance: StatementAllowance,
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let allowance_encoded = allowance.encode();
	let mut items = Vec::with_capacity(count as usize);
	for idx in 0..count {
		let keypair = get_keypair(idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance_encoded.clone()));
	}
	items
}

/// Creates a sudo -> frame_system::set_storage call to set statement allowances
fn create_set_storage_call(items: Vec<(Vec<u8>, Vec<u8>)>) -> DynamicPayload {
	let items_value: Vec<Value> = items
		.into_iter()
		.map(|(key, value)| value!((Value::from_bytes(key), Value::from_bytes(value))))
		.collect();

	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items_value })
		}],
	)
}

/// Submits an extrinsic with an explicit nonce and waits for it to be included in a block
async fn submit_sudo_extrinsic<S: Signer<PolkadotConfig>>(
	client: &OnlineClient<PolkadotConfig>,
	call: &DynamicPayload,
	signer: &S,
	nonce: u64,
) -> Result<
	zombienet_sdk::subxt::tx::TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	anyhow::Error,
> {
	let extensions = PolkadotExtrinsicParamsBuilder::new().immortal().nonce(nonce).build();

	let mut tx = client
		.tx()
		.create_signed(call, signer, extensions)
		.await?
		.submit_and_watch()
		.await?;

	while let Some(status) = tx.next().await.transpose()? {
		match status {
			TxStatus::InBestBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::InFinalizedBlock(ref tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow!("Error submitting sudo tx: {message}"));
			},
			_ => continue,
		}
	}

	Err(anyhow!("Transaction event stream ended without being included in a block"))
}

/// Waits for a transaction to finalize within a timeout
async fn wait_for_tx_finalization<Tx>(
	tx_stream: &mut Tx,
	timeout_secs: u64,
) -> Result<H256, anyhow::Error>
where
	Tx: futures::Stream<
			Item = Result<
				TxStatus<PolkadotConfig, OnlineClient<PolkadotConfig>>,
				zombienet_sdk::subxt::Error,
			>,
		> + Unpin,
{
	let watch_future = async {
		while let Some(status) = tx_stream.next().await.transpose()? {
			match status {
				TxStatus::InFinalizedBlock(ref tx_in_block) => {
					tx_in_block.wait_for_success().await?;
					return Ok(tx_in_block.block_hash());
				},
				TxStatus::Error { message } |
				TxStatus::Invalid { message } |
				TxStatus::Dropped { message } => {
					return Err(anyhow!("Tx error during finalization: {message}"));
				},
				_ => continue,
			}
		}
		Err(anyhow!("Transaction stream ended without finalization"))
	};

	tokio::time::timeout(Duration::from_secs(timeout_secs), watch_future)
		.await
		.map_err(|_| anyhow!("Timeout waiting for tx finalization after {}s", timeout_secs))?
}

/// Gets the current nonce for an account
async fn get_account_nonce(
	client: &OnlineClient<PolkadotConfig>,
	account_id: &<PolkadotConfig as zombienet_sdk::subxt::Config>::AccountId,
) -> Result<u64, anyhow::Error> {
	let nonce = client.tx().account_nonce(account_id).await?;
	Ok(nonce)
}

/// Sets statement allowances via sudo -> frame_system::set_storage extrinsic
async fn set_allowances_via_sudo(
	para_client: &OnlineClient<PolkadotConfig>,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	info!("Setting {} statement allowances via sudo...", items.len());

	let alice = zombienet_sdk::subxt_signer::sr25519::dev::alice();
	let alice_account_id = <zombienet_sdk::subxt_signer::sr25519::Keypair as Signer<
		PolkadotConfig,
	>>::account_id(&alice);

	let current_nonce = get_account_nonce(para_client, &alice_account_id).await?;
	let set_storage_call = create_set_storage_call(items);

	let mut tx_stream =
		submit_sudo_extrinsic(para_client, &set_storage_call, &alice, current_nonce).await?;
	let block_hash = wait_for_tx_finalization(&mut tx_stream, 120).await?;
	info!("Statement allowances set and finalized in block {:?}", block_hash);

	Ok(())
}

/// Spawns a network with the sudo-enabled chain spec and sets allowances at runtime
async fn spawn_network_sudo(
	collators: &[&str],
	allowance_items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let participant_count = allowance_items.len();

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
			let p = p
				.with_id(2104)
				.with_chain_spec_path("https://raw.githubusercontent.com/paritytech/chainspecs/denzelpenzel/versi-people-2101/versi/parachain/versi-people-2101/chainspec.json")
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", participant_count + 1000).as_str().into(),
					format!(
						"--rpc-max-subscriptions-per-connection={}",
						(participant_count * 16).max(32)
					)
						.as_str()
						.into(),
				])
				.with_collator(|n| n.with_name(collators[0]));

			collators[1..]
				.iter()
				.fold(p, |acc, &name| acc.with_collator(|n| n.with_name(name)))
		})
		.with_global_settings(|global_settings| {
			global_settings.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	// Wait for the parachain to produce blocks
	info!("Waiting for parachain to produce blocks...");
	let first_collator = collators[0];
	let node = network.get_node(first_collator)?;
	node.wait_metric("block_height{status=\"best\"}", |height| height >= 1.0)
		.await?;
	info!("Parachain is producing blocks");

	// Set statement allowances via sudo
	let para_client = node.wait_client::<PolkadotConfig>().await?;
	set_allowances_via_sudo(&para_client, allowance_items).await?;

	Ok(network)
}

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
				create_test_statement(&keypair, topic, vec![idx as u8], u32::MAX, idx * 100);
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
		let stmt = create_test_statement(&keypair, topic, vec![seq as u8], u32::MAX, seq);
		let result = submit_statement(&alice_rpc, &stmt).await?;
		assert_eq!(result, SubmitResult::New, "seq={} should be New", seq);
	}
	info!("Filled 3 slots (seq=100, 200, 300)");

	// Lower priority (seq=50) should be rejected with AccountFull
	let low = create_test_statement(&keypair, topic, vec![0], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &low).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::AccountFull { .. }) => {
			info!("seq=50 correctly rejected with AccountFull");
		},
		other => panic!("Expected AccountFull for seq=50, got: {:?}", other),
	}

	// Higher priority (seq=150) should evict seq=100 (the lowest)
	let mid = create_test_statement(&keypair, topic, vec![15], u32::MAX, 150);
	let result = submit_statement(&alice_rpc, &mid).await?;
	assert_eq!(result, SubmitResult::New, "seq=150 should evict seq=100");
	info!("seq=150 accepted, evicted seq=100");

	// Even higher (seq=250) should evict seq=150 (now the lowest)
	let high = create_test_statement(&keypair, topic, vec![25], u32::MAX, 250);
	let result = submit_statement(&alice_rpc, &high).await?;
	assert_eq!(result, SubmitResult::New, "seq=250 should evict seq=150");
	info!("seq=250 accepted, evicted seq=150");

	// Now slots hold seq=200, 250, 300. A seq=190 should be rejected
	let too_low = create_test_statement(&keypair, topic, vec![19], u32::MAX, 190);
	let result = submit_statement(&alice_rpc, &too_low).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::AccountFull { .. }) => {
			info!("seq=190 correctly rejected (slots hold 200, 250, 300)");
		},
		other => panic!("Expected AccountFull for seq=190, got: {:?}", other),
	}

	info!("Priority eviction ordering test passed");
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
	let stmt_a = create_test_statement(&keypair_0, topic_a, vec![1], u32::MAX, 100);
	// stmt_b: only topic_b
	let stmt_b = create_test_statement(&keypair_1, topic_b, vec![2], u32::MAX, 100);
	// stmt_ab: topic_a + topic_b
	let stmt_ab =
		create_multi_topic_statement(&keypair_2, &[topic_a, topic_b], vec![3], u32::MAX, 100);
	// stmt_c: only topic_c (should not match topic_a or topic_b filters)
	let stmt_c = create_test_statement(&keypair_3, topic_c, vec![4], u32::MAX, 100);

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
	let stmt_100 = create_channel_statement(&keypair, topic, channel_1, vec![100], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_100).await?;
	assert_eq!(result, SubmitResult::New, "Channel 1 seq=100 should be New");
	info!("Channel 1: seq=100 accepted");

	// Try lower seq=50 on same channel -> ChannelPriorityTooLow
	let stmt_50 = create_channel_statement(&keypair, topic, channel_1, vec![50], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &stmt_50).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. }) => {
			info!("Channel 1: seq=50 correctly rejected with ChannelPriorityTooLow");
		},
		other => panic!("Expected ChannelPriorityTooLow for seq=50, got: {:?}", other),
	}

	// Try equal seq=100 on same channel -> ChannelPriorityTooLow
	let stmt_100_dup =
		create_channel_statement(&keypair, topic, channel_1, vec![101], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_100_dup).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. }) => {
			info!("Channel 1: seq=100 (dup) correctly rejected with ChannelPriorityTooLow");
		},
		other => panic!("Expected ChannelPriorityTooLow for equal seq=100, got: {:?}", other),
	}

	// Higher seq=200 on same channel -> replaces
	let stmt_200 = create_channel_statement(&keypair, topic, channel_1, vec![200], u32::MAX, 200);
	let result = submit_statement(&alice_rpc, &stmt_200).await?;
	assert_eq!(result, SubmitResult::New, "Channel 1 seq=200 should replace seq=100");
	info!("Channel 1: seq=200 accepted (replaced seq=100)");

	// Different channel is independent. seq=50 on channel_2 should succeed
	let stmt_ch2 = create_channel_statement(&keypair, topic, channel_2, vec![50], u32::MAX, 50);
	let result = submit_statement(&alice_rpc, &stmt_ch2).await?;
	assert_eq!(result, SubmitResult::New, "Channel 2 seq=50 should be independent");
	info!("Channel 2: seq=50 accepted (independent from channel 1)");

	info!("Channel replacement test passed");
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
	let stmt_a = create_test_statement(&keypair_0, topic_a, vec![0xA1], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &stmt_a).await?;
	assert_eq!(result, SubmitResult::New);
	info!("Submitted topic_a statement");

	// Submit topic_b statement via bob
	let stmt_b = create_test_statement(&keypair_1, topic_b, vec![0xB2], u32::MAX, 100);
	let result = submit_statement(&bob_rpc, &stmt_b).await?;
	assert_eq!(result, SubmitResult::New);
	info!("Submitted topic_b statement");

	// Alice (topic_a subscriber) should get topic_a propagated from network
	let _received_a = expect_statement(&mut alice_sub, 30).await?;
	info!("Alice received topic_a statement");

	// Charlie (topic_a subscriber) should get topic_a
	let _received_c = expect_statement(&mut charlie_sub, 30).await?;
	info!("Charlie received topic_a statement");

	// Bob (topic_b subscriber) should get topic_b
	let _received_b = expect_statement(&mut bob_sub, 30).await?;
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
					topic,
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
	let statement = create_test_statement(&keypair, topic, vec![1, 2, 3], u32::MAX, 100);
	let expected_bytes: Bytes = statement.encode().into();

	// First submission
	let result = submit_statement(&alice_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New);
	info!("First submission accepted");

	// Confirm propagation to bob
	let received = expect_statement(&mut bob_sub, 30).await?;
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
	Ok(())
}

/// Tests boundary conditions and edge cases
///
/// Sub-tests: empty data, large data near limit, 4-topic statement,
/// and already-expired statement
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_edge_cases() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let allowances = vec![(0u32, StatementAllowance { max_count: 10, max_size: 2_000_000 })];
	let items = create_allowance_items(&allowances);

	let network = spawn_network_sudo(&["alice", "bob"], items).await?;

	let alice = network.get_node("alice")?;
	let alice_rpc = alice.rpc().await?;

	let topic: Topic = [60u8; 32].into();
	let keypair = get_keypair(0);

	// Empty data test
	let empty_stmt = create_test_statement(&keypair, topic, vec![], u32::MAX, 100);
	let result = submit_statement(&alice_rpc, &empty_stmt).await?;
	assert_eq!(result, SubmitResult::New, "Empty data statement should be accepted");
	info!("Edge case: empty data accepted");

	// Large data near limit test
	let large_data = vec![0xFFu8; 1_900_000];
	let large_stmt = create_test_statement(&keypair, topic, large_data, u32::MAX, 200);
	let result = submit_statement(&alice_rpc, &large_stmt).await?;
	assert_eq!(result, SubmitResult::New, "Large data statement should be accepted");
	info!("Edge case: large data (~1.9MB) accepted");

	// 4-topic statement
	let topics =
		[[0xA1u8; 32].into(), [0xA2u8; 32].into(), [0xA3u8; 32].into(), [0xA4u8; 32].into()];
	let multi_topic_stmt = create_multi_topic_statement(&keypair, &topics, vec![42], u32::MAX, 300);
	let result = submit_statement(&alice_rpc, &multi_topic_stmt).await?;
	assert_eq!(result, SubmitResult::New, "4-topic statement should be accepted");
	info!("Edge case: 4-topic statement accepted");

	// Already-expired statement
	let expired_stmt = create_test_statement(&keypair, topic, vec![99], 1, 400);
	let result = submit_statement(&alice_rpc, &expired_stmt).await?;
	match result {
		SubmitResult::Invalid(InvalidReason::AlreadyExpired) => {
			info!("Edge case: expired statement correctly rejected with AlreadyExpired");
		},
		other => panic!("Expected AlreadyExpired for expired statement, got: {:?}", other),
	}

	info!("Edge cases test passed");
	Ok(())
}

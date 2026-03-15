// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::{
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use codec::Encode;
use log::info;
use sp_core::{hexdisplay::HexDisplay, Bytes, Pair};
use sp_statement_store::{
	statement_allowance_key, RejectionReason, StatementAllowance, SubmitResult, Topic,
};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

use super::common::{
	assert_no_more_statements, create_chain_spec_with_allowances, create_test_statement,
	expect_one_statement, get_keypair, submit_statement, subscribe_topic,
};

const RPC_POOL_SIZE: usize = 10000;

fn current_unix_time() -> u32 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("System clock is before UNIX epoch")
		.as_secs() as u32
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

	let topic: Topic = [0u8; 32].into();
	let keypair = get_keypair(0);
	let statement = create_test_statement(&keypair, &[topic], None, vec![1, 2, 3], u32::MAX, 0);
	let expected: Bytes = statement.encode().into();

	let mut sub = subscribe_topic(&dave_rpc, topic).await?;
	let result = submit_statement(&charlie_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New);

	let received = expect_one_statement(&mut sub, 20).await?;
	assert_eq!(received, expected);
	assert_no_more_statements(&mut sub, 20).await?;
	log::info!("Statement store test passed");

	Ok(())
}

/// Tests multi-node statement propagation across 4 collator nodes
///
/// Submits a statement to one node and verifies it propagates to 3 other nodes
/// with data integrity, then checks no duplicate statements arrive
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_propagation_multi_node() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Spawn 4 collators with 8 participant allowances
	let network = spawn_network(&["alice", "bob", "charlie", "dave"], 8).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let alice_rpc = alice.rpc().await?;
	let bob_rpc = bob.rpc().await?;
	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let topic: Topic = [1u8; 32].into();

	// Subscribe on bob, charlie, dave before submitting
	let mut bob_sub = subscribe_topic(&bob_rpc, topic).await?;
	let mut charlie_sub = subscribe_topic(&charlie_rpc, topic).await?;
	let mut dave_sub = subscribe_topic(&dave_rpc, topic).await?;

	// Create and submit statement to alice
	let keypair = get_keypair(0);
	let statement = create_test_statement(&keypair, &[topic], None, vec![1, 2, 3], u32::MAX, 0);
	let expected_bytes: Bytes = statement.encode().into();

	let result = submit_statement(&alice_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New, "Statement should be accepted as new");
	log::info!("Statement submitted to alice, waiting for propagation to 3 nodes");

	// Verify propagation to each subscriber
	for (name, sub) in
		[("bob", &mut bob_sub), ("charlie", &mut charlie_sub), ("dave", &mut dave_sub)]
	{
		let received = expect_one_statement(sub, 30).await?;
		assert_eq!(received, expected_bytes, "Statement data mismatch on {}", name);
		log::info!("Statement received on {} with correct data", name);
	}

	for (name, sub) in
		[("bob", &mut bob_sub), ("charlie", &mut charlie_sub), ("dave", &mut dave_sub)]
	{
		assert_no_more_statements(sub, 10).await?;
		log::info!("No duplicate statements on {}", name);
	}

	log::info!("Multi-node propagation test passed");
	Ok(())
}

/// Tests that expired statements are cleaned up by the enforcement cycle
///
/// Submits a statement with a short expiry, waits for the enforcement cycle to
/// evict it, then verifies the statement can be re-submitted as new
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_expiration() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Spawn 2 collators with 8 participant allowances
	let network = spawn_network(&["charlie", "dave"], 8).await?;

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let topic: Topic = [3u8; 32].into();
	let mut dave_sub = subscribe_topic(&dave_rpc, topic).await?;

	let now = current_unix_time();
	let expiry_offset = 45;
	let keypair = get_keypair(0);

	// Submit a statement that expires in 45 sec
	let statement =
		create_test_statement(&keypair, &[topic], None, vec![10, 20, 30], now + expiry_offset, 0);
	let result = submit_statement(&charlie_rpc, &statement).await?;
	assert_eq!(result, SubmitResult::New, "Statement should be accepted as new");
	log::info!(
		"Submitted statement with expiry in {}s (at unix time {})",
		expiry_offset,
		now + expiry_offset
	);

	// Verify it propagated to dave
	let received = expect_one_statement(&mut dave_sub, 30).await?;
	let expected_bytes: Bytes = statement.encode().into();
	assert_eq!(received, expected_bytes, "Statement data mismatch on dave");
	log::info!("Statement received on dave, now waiting for expiration and enforcement");

	// Wait for the statement to expire and be fully purged
	// Enforcement is two-phase (ENFORCE_LIMITS_PERIOD=31s each) plus maintenance (29s)
	// Total worst case from expiry: 31 + 31 + 29 = ~91s. From creation: expiry_offset + 91s
	// We use 95 + 25 = 120s after expiry to give a 29s margin for CI variability
	let total_wait = expiry_offset + 95 + 25;
	let elapsed = current_unix_time().saturating_sub(now);
	let remaining_wait = total_wait.saturating_sub(elapsed);
	log::info!("Sleeping {}s for enforcement cycles and maintenance to complete", remaining_wait);
	tokio::time::sleep(Duration::from_secs(remaining_wait as u64)).await;

	// Re-submit with a new expiry
	let fresh_statement =
		create_test_statement(&keypair, &[topic], None, vec![10, 20, 30], u32::MAX, 0);
	let result = submit_statement(&charlie_rpc, &fresh_statement).await?;

	match result {
		SubmitResult::New => {
			log::info!("Statement re-submitted as New - original was fully purged");
		},
		SubmitResult::KnownExpired => {
			log::info!("Got KnownExpired, waiting 30s more for maintenance purge");
			tokio::time::sleep(Duration::from_secs(30)).await;
			let result = submit_statement(&charlie_rpc, &fresh_statement).await?;
			assert_eq!(
				result,
				SubmitResult::New,
				"Statement should be New after maintenance purge"
			);
			log::info!("Statement accepted as New after additional maintenance wait");
		},
		SubmitResult::Known => {
			panic!("Statement is still Known - enforcement has not run yet");
		},
		other => {
			panic!("Unexpected submit result: {:?}", other);
		},
	}

	log::info!("Expiration test passed");
	Ok(())
}

/// Tests per-account quota enforcement at submission time.
///
/// Verifies AccountFull, NoAllowance, DataTooLarge rejections and eviction
/// of lower-priority statements when a higher-priority one is submitted
#[tokio::test(flavor = "multi_thread")]
async fn statement_store_quota_enforcement() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Participants 0-6 get limited allowances; participant 7 gets no allowance
	let allowances: Vec<(u32, StatementAllowance)> = (0..7)
		.map(|idx| (idx, StatementAllowance { max_count: 3, max_size: 10_000 }))
		.collect();

	let network = spawn_network_with_custom_allowances(&["charlie", "dave"], &allowances).await?;

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	let topic: Topic = [2u8; 32].into();

	log::info!("Filling quota for participant 0 (max_count=3)");
	let keypair_0 = get_keypair(0);
	for seq in [100u32, 200, 300] {
		let statement =
			create_test_statement(&keypair_0, &[topic], None, vec![seq as u8], u32::MAX, seq);
		let result = submit_statement(&charlie_rpc, &statement).await?;
		assert_eq!(result, SubmitResult::New, "Statement with seq={} should be New", seq);
	}
	log::info!("Successfully submitted 3 statements for participant 0");

	// Submit lower priority statement
	log::info!("Verifying AccountFull rejection");
	let low_priority = create_test_statement(&keypair_0, &[topic], None, vec![0], u32::MAX, 50);
	let result = submit_statement(&charlie_rpc, &low_priority).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::AccountFull { .. }) => {
			log::info!("Rejected with AccountFull");
		},
		other => panic!("Expected AccountFull rejection, got: {:?}", other),
	}

	// Rejection for participant 7
	log::info!("Verifying NoAllowance rejection");
	let keypair_7 = get_keypair(7);
	let no_allowance_stmt = create_test_statement(&keypair_7, &[topic], None, vec![1], u32::MAX, 0);
	let result = submit_statement(&charlie_rpc, &no_allowance_stmt).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::NoAllowance) => {
			log::info!("Rejected with NoAllowance");
		},
		other => panic!("Expected NoAllowance rejection, got: {:?}", other),
	}

	log::info!("Verifying DataTooLarge rejection");
	let keypair_1 = get_keypair(1);
	let large_data = vec![0u8; 10_001];
	let large_stmt = create_test_statement(&keypair_1, &[topic], None, large_data, u32::MAX, 0);
	let result = submit_statement(&charlie_rpc, &large_stmt).await?;
	match result {
		SubmitResult::Rejected(RejectionReason::DataTooLarge { .. }) => {
			log::info!("Rejected with DataTooLarge");
		},
		other => panic!("Expected DataTooLarge rejection, got: {:?}", other),
	}

	log::info!("Verifying eviction with higher priority statement");
	let mut dave_sub = subscribe_topic(&dave_rpc, topic).await?;

	let high_priority = create_test_statement(&keypair_0, &[topic], None, vec![4], u32::MAX, 400);
	let result = submit_statement(&charlie_rpc, &high_priority).await?;
	assert_eq!(
		result,
		SubmitResult::New,
		"Higher priority statement should evict lowest and be accepted as New"
	);
	log::info!("Higher priority statement accepted, lowest priority was evicted");

	// Verify propagation of the new statement
	let _received = expect_one_statement(&mut dave_sub, 30).await?;
	log::info!("Higher priority statement propagated to dave");

	log::info!("Quota enforcement test passed");
	Ok(())
}

/// Spawns a network using a custom chain spec with injected statement allowances
async fn spawn_network(
	collators: &[&str],
	participant_count: u32,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	assert!(collators.len() >= 2);
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let chain_spec_path = create_chain_spec_with_allowances(participant_count, &base_dir)?;
	// Headroom for the ~5,000 subscriptions that
	// actually end up on each pooled conn (500 participants * 10 subscriptions each)
	let max_subs_per_conn = (participant_count * 16 / RPC_POOL_SIZE as u32).max(32);

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
				.with_id(2400)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", participant_count + 1000).as_str().into(),
					format!("--rpc-max-subscriptions-per-connection={max_subs_per_conn}")
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

	Ok(network)
}

/// Creates a custom chain spec with per-participant allowances
///
/// Each entry in `allowances` maps a participant index to a custom `StatementAllowance`
/// Participants not listed receive no allowance entry in the chain spec
fn create_chain_spec_with_custom_allowances(
	allowances: &[(u32, StatementAllowance)],
	base_dir: &Path,
) -> Result<PathBuf, anyhow::Error> {
	let chain_spec_template = include_str!("../people-westend-local-spec.json");
	let mut chain_spec: serde_json::Value = serde_json::from_str(chain_spec_template)
		.map_err(|e| anyhow!("Failed to parse chain spec JSON: {}", e))?;
	let genesis = chain_spec
		.get_mut("genesis")
		.and_then(|g| g.get_mut("raw"))
		.and_then(|r| r.get_mut("top"))
		.and_then(|t| t.as_object_mut())
		.ok_or_else(|| anyhow!("Failed to access genesis.raw.top in chain spec"))?;

	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();

		let storage_key = statement_allowance_key(account_id.0);
		let storage_key_hex = format!("0x{}", HexDisplay::from(&storage_key));
		let allowance_hex = format!("0x{}", HexDisplay::from(&allowance.encode()));

		info!("Injecting allowance for participant {}: {}", idx, allowance_hex);
		genesis.insert(storage_key_hex, serde_json::Value::String(allowance_hex));
	}

	let chain_spec_path = base_dir.join("people-westend-custom.json");
	let chain_spec_json = serde_json::to_string_pretty(&chain_spec)
		.map_err(|e| anyhow!("Failed to serialize chain spec: {}", e))?;

	std::fs::write(&chain_spec_path, chain_spec_json)
		.map_err(|e| anyhow!("Failed to write chain spec to file: {}", e))?;

	info!("Created custom chain spec at: {}", chain_spec_path.display());

	Ok(chain_spec_path)
}

/// Spawns a network using a custom chain spec with per-participant allowances
///
/// Unlike `spawn_network` which gives all participants the same high allowance,
/// this function accepts a list of `(participant_idx, allowance)` pairs allowing
/// fine-grained control over each participant's statement limits
async fn spawn_network_with_custom_allowances(
	collators: &[&str],
	allowances: &[(u32, StatementAllowance)],
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	assert!(collators.len() >= 2);
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let chain_spec_path = create_chain_spec_with_custom_allowances(allowances, &base_dir)?;

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
				.with_id(2400)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", allowances.len() + 100).as_str().into(),
					format!(
						"--rpc-max-subscriptions-per-connection={}",
						allowances.len() * 16 + 16
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

	Ok(network)
}

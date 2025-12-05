// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that validates the POV (Proof of Validity) reclaim mechanism correctly accounts for trie
//! node access.
//!
//! Storage reclaim returns unused storage proof space to enable more follow-up transactions.
//! However, the accurate accounting of storage proof size must consider the trie nodes accessed
//! during storage root computation, not just the storage items read by extrinsic.
//!
//! This test submits transactions that delete many storage entries (`kill_dev_entry`), causing
//! significant trie modifications. These modifications result in many new trie nodes being added to
//! the proof during storage root calculation. If the POV reclaim mechanism doesn't properly account
//! for these trie node accesses, the chain would overshoot the total PoV budget.
//!
//! **Expected behavior**: With the POV reclaim fix (gh-6020), this test should pass by ensuring all
//! transactions finalize. Without the fix, the test will fail due to POV size being exceeded.
//!
//! Network configuration is hardcoded via zombienet SDK API.

use anyhow::anyhow;
use serde_json::json;
use tracing::{error, info};

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use txtesttool::{
	execution_log::ExecutionLog,
	scenario::{ChainType, ScenarioBuilder},
};
use zombienet_orchestrator::network::node::NetworkNode;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;
const ACCOUNT_COUNT: usize = 100;
const FROM_SINGLE_ACCOUNT: usize = 200;
const TOTAL_COUNT: usize = ACCOUNT_COUNT * FROM_SINGLE_ACCOUNT;
const TEST_TIMEOUT_SECS: u64 = 3600; // 1 hour

#[tokio::test(flavor = "multi_thread")]
async fn overshooting_shall_not_happen_test() -> Result<(), anyhow::Error> {
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	// Ensure relaychain nodes are producing blocks
	for node in [alice, bob] {
		info!("Ensuring {} reports block production", node.name());
		node.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 2.0, 120u64)
			.await
			.expect("relaychain node should produce blocks");
	}

	// Ensure parachain collator is producing blocks
	info!("Ensuring charlie reports block production");
	charlie
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 2.0, 180u64)
		.await
		.expect("parachain collator should produce blocks");

	// Get WebSocket URI for charlie (parachain collator)
	let ws = charlie.ws_uri().to_string();
	let base_dir = network.base_dir().map(|s| s.to_string());

	// Build scenario executor using ScenarioBuilder
	// - Multiple accounts (ACCOUNT_COUNT total)
	// - Multiple transactions per account (FROM_SINGLE_ACCOUNT per account)
	// - nonce_from=0 means ready transactions (not future)
	info!(
		"Building scenario executor for {} accounts, {} txs each",
		ACCOUNT_COUNT, FROM_SINGLE_ACCOUNT
	);
	let mut builder = ScenarioBuilder::new()
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_last_id((ACCOUNT_COUNT - 1) as u32)
		.with_txs_count(FROM_SINGLE_ACCOUNT as u32)
		.with_nonce_from(Some(0))
		.with_watched_txs(true)
		.with_send_threshold(6_000)
		.with_block_monitoring(false)
		.with_chain_type(ChainType::Sub)
		.with_timeout_in_secs(TEST_TIMEOUT_SECS)
		.with_executor_id("overshooting-test".to_string())
		.with_tx_payload_builder_sub(|ctx| {
			let id = ctx.account.parse::<u128>().unwrap();
			let entries_per_account = 20;
			// Map each (nonce, id) pair to a unique 20-entry range in dev_data_entries.
			// Nonce selects a batch of (ACCOUNT_COUNT * 20) entries,
			// and id selects a specific 20-entry range within that batch.
			let start = txtesttool::subxt_transaction::dynamic::Value::u128(
				(entries_per_account * (ctx.nonce * (ACCOUNT_COUNT as u128) + id)) as u128,
			);
			let count = txtesttool::subxt_transaction::dynamic::Value::u128(entries_per_account);
			txtesttool::subxt_transaction::dynamic::tx(
				"TestPallet",
				"kill_dev_entry",
				vec![start, count],
			)
		});

	if let Some(dir) = base_dir {
		builder = builder.with_base_dir_path(dir);
	}

	let executor = builder.build().await;

	// Execute transactions and fetch the execution logs
	info!("Submitting {} transactions", TOTAL_COUNT);

	// Create executor future for concurrent polling with POV check
	let mut executor_future = std::pin::pin!(executor.execute());

	// Spawn a task to check for POV errors when alice reaches block 30
	let mut pov_check_task =
		tokio::spawn(check_pov_errors_at_block_30(alice.clone(), charlie.clone()));

	// Run executor and POV check in parallel, racing them
	let execution_logs = tokio::select! {
		execution_logs = &mut executor_future => {
			info!("Executor finished before block 30 checkpoint");
			pov_check_task.abort();
			execution_logs
		}
		pov_check_result = &mut pov_check_task => {
			pov_check_result??;
			info!("POV check passed at block 30, waiting for executor to complete");
			(&mut executor_future).await
		}
	};

	// Verify all transactions finalized
	let finalized_count = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();
	assert_eq!(
		finalized_count, TOTAL_COUNT,
		"Expected all {} transactions to finalize, but got {} finalized",
		TOTAL_COUNT, finalized_count
	);

	Ok(())
}

/// Checks for POV size exceeded errors in charlie's logs when alice reaches block 30.
///
/// Returns an error if any "Failed to submit collation" or "POVSizeExceeded" messages are found,
/// indicating that the POV exceeded the maximum allowed size.
async fn check_pov_errors_at_block_30(
	alice: NetworkNode,
	charlie: NetworkNode,
) -> Result<(), anyhow::Error> {
	info!("Waiting for alice (relaychain) to reach block 30");
	alice
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 30.0, 300u64)
		.await
		.map_err(|e| anyhow!("Failed to wait for block 30: {}", e))?;

	info!("At block 30 - checking charlie's logs for POV errors");
	let logs = charlie.logs().await?;
	let pov_error_lines = logs
		.lines()
		.filter(|line| {
			line.contains("Failed to submit collation") || line.contains("POVSizeExceeded")
		})
		.collect::<Vec<_>>();

	if !pov_error_lines.is_empty() {
		error!(
			"Found {} POV/collation submission errors in charlie's logs:",
			pov_error_lines.len()
		);
		for line in &pov_error_lines {
			error!("  {}", line);
		}
		return Err(anyhow!(
			"Found {} POV size exceeded or collation submission failures at block 30 checkpoint",
			pov_error_lines.len()
		));
	}
	info!("No POV errors found at block 30 checkpoint");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	info!("Building network config for overshooting test");

	let images = zombienet_sdk::environment::get_images_from_env();
	info!("Using images: {images:?}");

	// Network configuration based on asset-hub-high-pool-limit-fatp.toml
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					"-lparachain::candidate-validation=trace".into(),
					"-lparachain::candidate-validation=debug".into(),
					"-lparachain::pvf=debug".into(),
					"-lparachain::pvf-execute-worker=debug".into(),
					"-lparachain::candidate-backing=debug".into(),
					"-lcumulus-collator=debug".into(),
					"-lparachain-system=debug".into(),
					"-lwasm-heap=debug".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"executor_params": [{"MaxMemoryPages": 8192}]
						}
					}
				}))
				.with_node(|node| node.with_name("alice").validator(true))
				.with_node(|node| node.with_name("bob").validator(true))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--experimental-max-pov-percentage=100".into(),
					"--pool-kbytes=2048000".into(),
					"--pool-limit=500000".into(),
					"--pool-type=fork-aware".into(),
					"--rpc-max-connections=15000".into(),
					"--rpc-max-response-size=150".into(),
					"--rpc-max-subscriptions-per-connection=128000".into(),
					"--state-pruning=1024".into(),
					"-laura::cumulus=trace".into(),
					"-lbasic-authorship=trace".into(),
					"-lpeerset=info".into(),
					"-lsub-libp2p=info".into(),
					"-lsync=info".into(),
					"-ltxpool=debug".into(),
					"-lxxx=trace".into(),
					"-lruntime=trace".into(),
				])
				.with_genesis_overrides(json!({
					"testPallet": {
						"devDataEntries":2000000
					},
					"balances": {
						"devAccounts": [
							ACCOUNT_COUNT,
							1000000000000000000u64,
							"//Sender//{}"
						],
					}
				}))
				.with_collator(|n| n.with_name("charlie").validator(true))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	Ok(config)
}

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that sends one ready transaction from each of 20k accounts to a parachain collator.
//! Network configuration is hardcoded via zombienet SDK API based on
//! asset-hub-high-pool-limit-fatp.toml.

use anyhow::anyhow;
use serde_json::json;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use tracing::info;
use txtesttool::{
	execution_log::ExecutionLog,
	scenario::{ChainType, ScenarioBuilder},
};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;
const ACCOUNT_COUNT: usize = 100;
const FROM_SINGLE_ACCOUNT: usize = 200;
const TOTAL_COUNT: usize = ACCOUNT_COUNT * FROM_SINGLE_ACCOUNT;
const TEST_TIMEOUT_SECS: u64 = 3600; // 1 hour
									 //

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
		assert!(node
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 2.0, 120u64)
			.await
			.is_ok());
	}

	// Ensure parachain collator is producing blocks
	info!("Ensuring charlie reports block production");
	assert!(charlie
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 2.0, 180u64)
		.await
		.is_ok());

	// Get WebSocket URI for charlie (parachain collator)
	let ws = charlie.ws_uri().to_string();
	let base_dir = network.base_dir().map(|s| s.to_string());

	// Build scenario executor using ScenarioBuilder
	// - 20k accounts (start_id=0 to last_id=19999)
	// - 1 transaction per account
	// - nonce_from=0 means ready transactions (not future)
	info!("Building scenario executor for {} accounts", ACCOUNT_COUNT);
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
		.with_custom_sub_payload_builder(|ctx| {
			let id = ctx.account.parse::<u128>().unwrap();
			let entries_per_account = 5;
			let start = txtesttool::subxt_transaction::dynamic::Value::u128(
				(entries_per_account * (ctx.nonce * (ACCOUNT_COUNT as u128)+ id)) as u128,
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
	let execution_logs = executor.execute().await;

	// Count finalized transactions
	let finalized_count = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();

	assert_eq!(
		finalized_count, TOTAL_COUNT,
		"Expected all {} transactions to finalize, but got {} finalized",
		TOTAL_COUNT, finalized_count
	);

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

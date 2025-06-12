// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Testsuite of transaction pool integration tests.

pub mod zombienet;

use std::time::Duration;

use crate::zombienet::{
	default_zn_scenario_builder, relaychain_rococo_local_network_spec as relay,
	relaychain_rococo_local_network_spec::parachain_asset_hub_network_spec as para, NetworkSpawner,
};
use futures::future::join_all;
use tracing::info;
use txtesttool::{execution_log::ExecutionLog, scenario::ScenarioExecutor};
use zombienet::DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS;

// Test which sends future and ready txs from many accounts
// to an unlimited pool of a parachain collator based on the asset-hub-rococo runtime.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn send_future_and_ready_from_many_accounts_to_parachain() {
	let net = NetworkSpawner::from_toml_with_env_logger(para::HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("charlie").await.unwrap();

	// Create future & ready txs executors.
	let ws = net.node_rpc_uri("charlie").unwrap();
	let future_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_last_id(99)
		.with_nonce_from(Some(100))
		.with_txs_count(100)
		.with_executor_id("future-txs-executor".to_string())
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;
	let ready_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_last_id(99)
		.with_nonce_from(Some(0))
		.with_txs_count(100)
		.with_executor_id("ready-txs-executor".to_string())
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let (future_logs, ready_logs) = futures::future::join(
		future_scenario_executor.execute(),
		ready_scenario_executor.execute(),
	)
	.await;

	let finalized_future =
		future_logs.values().filter_map(|default_log| default_log.finalized()).count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();

	assert_eq!(finalized_future, 10_000);
	assert_eq!(finalized_ready, 10_000);
}

// Test which sends future and ready txs from many accounts
// to an unlimited pool of a relaychain node based on `rococo-local` runtime.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn send_future_and_ready_from_many_accounts_to_relaychain() {
	let net = NetworkSpawner::from_toml_with_env_logger(relay::HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the paracha validator to start block production & have its genesis block
	// finalized.
	net.wait_for_block_production("alice").await.unwrap();

	// Create future & ready txs executors.
	let ws = net.node_rpc_uri("alice").unwrap();
	let future_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_last_id(99)
		.with_nonce_from(Some(100))
		.with_txs_count(100)
		.with_executor_id("future-txs-executor".to_string())
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;
	let ready_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_last_id(99)
		.with_nonce_from(Some(0))
		.with_txs_count(100)
		.with_executor_id("ready-txs-executor".to_string())
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	// Execute transactions and fetch the execution logs.
	let (future_logs, ready_logs) = futures::future::join(
		future_scenario_executor.execute(),
		ready_scenario_executor.execute(),
	)
	.await;

	let finalized_future =
		future_logs.values().filter_map(|default_log| default_log.finalized()).count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();

	assert_eq!(finalized_future, 10_000);
	assert_eq!(finalized_ready, 10_000);
}

// Test which sends 5m transactions to parachain. Long execution time expected.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn send_5m_from_many_accounts_to_parachain() {
	let net = NetworkSpawner::from_toml_with_env_logger(para::HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("charlie").await.unwrap();

	// Create txs executor.
	let ws = net.node_rpc_uri("charlie").unwrap();
	let executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_last_id(999)
		.with_txs_count(5_000)
		.with_executor_id("txs-executor".to_string())
		.with_send_threshold(7500)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let execution_logs = executor.execute().await;
	let finalized_txs = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();

	assert_eq!(finalized_txs, 5_000_000);
}

// Test which sends 5m transactions to relaychain. Long execution time expected.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn send_5m_from_many_accounts_to_relaychain() {
	let net = NetworkSpawner::from_toml_with_env_logger(relay::HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("alice").await.unwrap();

	// Create txs executor.
	let ws = net.node_rpc_uri("alice").unwrap();
	let executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_last_id(999)
		.with_txs_count(5000)
		.with_executor_id("txs-executor".to_string())
		.with_send_threshold(7500)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let execution_logs = executor.execute().await;
	let finalized_txs = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();

	assert_eq!(finalized_txs, 5_000_000);
}

/// Internal test that allows to observe how transcactions are gossiped in the network. Requires
/// external tool to track transactions presence at nodes. Was used to evaluate some metrics of
/// existing transaction protocol.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn gossiping() {
	let net = NetworkSpawner::from_toml_with_env_logger(relay::HIGH_POOL_LIMIT_FATP_TRACE)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("a00").await.unwrap();

	// Create the txs executor.
	let ws = net.node_rpc_uri("a00").unwrap();
	let executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_last_id(999)
		.with_executor_id("txs-executor".to_string())
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let execution_logs = executor.execute().await;
	let finalized_txs = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();

	assert_eq!(finalized_txs, 1000);

	tracing::info!("BASEDIR: {:?}", net.base_dir_path());
}

/// Creates new transaction scenario executor and sends given batch of ready transactions to the
/// specified node. Single transaction is sent from single account.
async fn send_batch(
	net: &NetworkSpawner,
	node_name: &str,
	from: u32,
	to: u32,
	prio: u32,
) -> ScenarioExecutor {
	let ws = net.node_rpc_uri(node_name).unwrap();
	info!(from, to, prio, "send_batch");
	default_zn_scenario_builder(net)
		.with_rpc_uri(ws)
		.with_start_id(from)
		.with_last_id(to)
		.with_txs_count(1)
		.with_tip(prio.into())
		.with_executor_id(format!("txs-executor_{}_{}_{}", from, to, prio))
		.with_send_threshold(usize::MAX)
		.with_legacy_backend(true)
		.build()
		.await
}

/// Repeatedly sends batches of transactions to the specified node with priority provided by
/// closure.
///
/// This function loops indefinitely, adjusting the priority of the transaction batch each time
/// based on the provided function. Each batch is executed by an executor that times out after
/// period duration if not completed.
///
/// The progress of transactions is intentionally not monitored; the utility is intended for
/// transaction pool limits testing, where the accuracy of execution is challenging to monitor.
async fn batch_loop<F>(
	net: &NetworkSpawner,
	node_name: &str,
	from: u32,
	to: u32,
	priority: F,
	period: std::time::Duration,
) where
	F: Fn(u32) -> u32,
{
	let mut prio = 0;
	loop {
		prio = priority(prio);
		let executor = send_batch(&net, node_name, from, to, prio).await;
		let start = std::time::Instant::now();
		let _results = tokio::time::timeout(period, executor.execute()).await;
		let elapsed = start.elapsed();
		if elapsed < period {
			tokio::time::sleep(period - elapsed).await;
		}
	}
}

/// Tests the transaction pool limits by continuously sending transaction batches to a parachain
/// network node. This test checks the pool's behavior under high load by simulating multiple
/// senders with increasing priorities.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_limits_increasing_prio_parachain() {
	let net = NetworkSpawner::from_toml_with_env_logger(para::LOW_POOL_LIMIT_FATP)
		.await
		.unwrap();

	net.wait_for_block_production("charlie").await.unwrap();

	let mut executors = vec![];
	let senders_count = 25;
	let sender_batch = 2000;

	for i in 0..senders_count {
		let from = 0 + i * sender_batch;
		let to = from + sender_batch - 1;
		executors.push(batch_loop(
			&net,
			"charlie",
			from,
			to,
			|prio| prio + 1,
			Duration::from_secs(60),
		));
	}

	let _results = join_all(executors).await;
}

/// Tests the transaction pool limits by continuously sending transaction batches to a relaychain
/// network node. This test checks the pool's behavior under high load by simulating multiple
/// senders with increasing priorities.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_limits_increasing_prio_relaychain() {
	let net = NetworkSpawner::from_toml_with_env_logger(relay::LOW_POOL_LIMIT_FATP)
		.await
		.unwrap();

	net.wait_for_block_production("alice").await.unwrap();

	let mut executors = vec![];
	//this looks like current limit of what we can handle. A bit choky but almost no empty blocks.
	let senders_count = 50;
	let sender_batch = 2000;

	for i in 0..senders_count {
		let from = 0 + i * sender_batch;
		let to = from + sender_batch - 1;
		executors.push(batch_loop(
			&net,
			"alice",
			from,
			to,
			|prio| prio + 1,
			Duration::from_secs(15),
		));
	}

	let _results = join_all(executors).await;
}

/// Tests the transaction pool limits by continuously sending transaction batches to a relaychain
/// network node. This test checks the pool's behavior under high load by simulating multiple
/// senders with increasing priorities.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_limits_same_prio_relaychain() {
	let net = NetworkSpawner::from_toml_with_env_logger(relay::LOW_POOL_LIMIT_FATP)
		.await
		.unwrap();

	net.wait_for_block_production("alice").await.unwrap();

	let mut executors = vec![];
	let senders_count = 50;
	let sender_batch = 2000;

	for i in 0..senders_count {
		let from = 0 + i * sender_batch;
		let to = from + sender_batch - 1;
		executors.push(batch_loop(&net, "alice", from, to, |prio| prio, Duration::from_secs(15)));
	}

	let _results = join_all(executors).await;
}

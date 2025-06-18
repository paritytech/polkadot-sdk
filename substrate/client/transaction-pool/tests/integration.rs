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

// Testsuite of fatp integration tests.
pub mod zombienet;

use crate::zombienet::{
	default_zn_scenario_builder,
	relaychain_rococo_local_network_spec::{
		parachain_asset_hub_network_spec::HIGH_POOL_LIMIT_FATP as PARACHAIN_HIGH_POOL_LIMIT_FATP,
		HIGH_POOL_LIMIT_FATP as RELAYCHAIN_HIGH_POOL_LIMIT_FATP,
	},
	NetworkSpawner,
};
use txtesttool::execution_log::ExecutionLog;
use zombienet::DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS;

// Test which sends future and ready txs from many accounts
// to an unlimited pool of a parachain collator based on the asset-hub-rococo runtime.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn send_future_and_ready_from_many_accounts_to_parachain() {
	let net = NetworkSpawner::from_toml_with_env_logger(PARACHAIN_HIGH_POOL_LIMIT_FATP)
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
	let net = NetworkSpawner::from_toml_with_env_logger(RELAYCHAIN_HIGH_POOL_LIMIT_FATP)
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

// Send immportal and mortal txs. Some of the mortal txs are configured to get dropped
// while others to succeed. Mortal txs are future so not being able to become ready in time and
// included in blocks result in their dropping.
//
// Block length for rococo for user txs is 75% of maximum 5MB (per frame-system setup),
// so we get 3750KB. In the test scenario we aim for 5 txs per block roughly (not precesily)
// so to fill a block each user tx must have around 750kb.
#[tokio::test(flavor = "multi_thread")]
async fn send_future_mortal_txs() {
	let net = NetworkSpawner::from_toml_with_env_logger(RELAYCHAIN_HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("alice").await.unwrap();

	// Create txs executors.
	let ws = net.node_rpc_uri("alice").unwrap();
	let ready_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_nonce_from(Some(0))
		.with_txs_count(50)
		// Block length for rococo for user txs is 75% of maximum 5MB (per frame-system setup),
		// so we get 3750KB. In the test scenario we aim for 5 txs per block roughly (not precesily)
		// so to fill a block each user tx must have around 750kb. We aim for 5 txs per block
		// because we send 50 ready txs which we want to distribute over 10 blocks, so mortal txs
		// with lifetime lower than 5 should be declared invalid after the ready txs finalize, while
		// mortal txs with bigger lifetime should be finalized.
		.with_remark_recipe(750)
		.with_executor_id("ready-txs-executor".to_string())
		.build()
		.await;

	let mortal_scenario_invalid = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_nonce_from(Some(60))
		.with_txs_count(10)
		.with_executor_id("mortal-tx-executor-invalid".to_string())
		.with_mortality(5)
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;

	let mortal_scenario_success = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(0)
		.with_nonce_from(Some(50))
		.with_txs_count(10)
		.with_executor_id("mortal-tx-executor-success".to_string())
		.with_mortality(25)
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let (mortal_invalid_logs, ready_logs, mortal_succes_logs) = tokio::join!(
		mortal_scenario_invalid.execute(),
		ready_scenario_executor.execute(),
		mortal_scenario_success.execute(),
	);

	let mortal_invalid = mortal_invalid_logs
		.values()
		.filter(|default_log| default_log.get_invalid_reason().len() > 0)
		.count();
	let mortal_succesfull = mortal_succes_logs
		.values()
		.filter_map(|default_log| default_log.finalized())
		.count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();

	assert_eq!(mortal_invalid, 10);
	assert_eq!(mortal_succesfull, 10);
	assert_eq!(finalized_ready, 50);
}

// Send immortal and mortal txs. Some mortal txs have lower priority so they shouldn't get into
// blocks during their lifetime, and will be considered invalid, while other mortal txs have
// sufficient lifetime to be included in blocks, and are finalized successfully.
#[tokio::test(flavor = "multi_thread")]
async fn send_lower_priority_mortal_txs() {
	let net = NetworkSpawner::from_toml_with_env_logger(RELAYCHAIN_HIGH_POOL_LIMIT_FATP)
		.await
		.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("alice").await.unwrap();

	// Create txs executors.
	let ws = net.node_rpc_uri("alice").unwrap();
	let ready_scenario_executor = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(0)
		.with_nonce_from(Some(0))
		.with_txs_count(50)
		.with_executor_id("ready-txs-executor".to_string())
		// Block length for rococo for user txs is 75% of maximum 5MB (per frame-system setup),
		// so we get 3750KB. In the test scenario we aim for 5 txs per block roughly (not precesily)
		// so to fill a block each user tx must have around 750kb. We aim for 5 txs per block
		// because we send 50 ready txs which we want to distribute over 10 blocks, so mortal txs
		// with lifetime lower than 5 should be declared invalid after the ready txs finalize, while
		// mortal txs with bigger lifetime should be finalized.
		.with_remark_recipe(750)
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.with_tip(150)
		.build()
		.await;

	let mortal_scenario_invalid = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws.clone())
		.with_start_id(1)
		.with_nonce_from(Some(0))
		.with_txs_count(10)
		.with_executor_id("mortal-tx-executor-invalid".to_string())
		.with_mortality(5)
		// Same reasoning as for the ready immortal txs, we want these txs to be similarly big as
		// the immortal txs, so at all times if it comes to pick a ready txs to include it in a
		// block, an immortal tx should be picked instead (leaving these mortal txs to be picked
		// only after, when it is too late due to small lifetime).
		.with_remark_recipe(750)
		.with_tip(50)
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.build()
		.await;

	let mortal_scenario_success = default_zn_scenario_builder(&net)
		.with_rpc_uri(ws)
		.with_start_id(2)
		.with_nonce_from(Some(0))
		.with_txs_count(10)
		.with_executor_id("mortal-tx-executor-success".to_string())
		.with_mortality(20)
		// Same reasoning as for the ready immortal txs, we want these txs to be similarly big as
		// the immortal txs, so at all times if it comes to pick a ready txs to include it in a
		// block, an immortal tx should be picked instead (leaving these mortal txs to be picked
		// only after, which is fine for these mortal txs).
		.with_remark_recipe(750)
		.with_timeout_in_secs(DEFAULT_SEND_FUTURE_AND_READY_TXS_TESTS_TIMEOUT_IN_SECS)
		.with_tip(100)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let (mortal_invalid_logs, ready_logs, mortal_success_logs) = tokio::join!(
		mortal_scenario_invalid.execute(),
		ready_scenario_executor.execute(),
		mortal_scenario_success.execute(),
	);

	let mortal_invalid = mortal_invalid_logs
		.values()
		.filter(|default_log| default_log.get_invalid_reason().len() > 0)
		.count();
	let mortal_succesfull = mortal_success_logs
		.values()
		.filter_map(|default_log| default_log.finalized())
		.count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();

	assert_eq!(mortal_invalid, 10);
	assert_eq!(mortal_succesfull, 10);
	assert_eq!(finalized_ready, 50);
}

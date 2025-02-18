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

use futures::future::join;
use txtesttool::{
	execution_log::ExecutionLog,
	runner::RunnerFactory,
	scenario::{ScenarioExecutor, ScenarioPlanner, SendingScenario},
	transaction::TransactionRecipe,
};
use zombienet::NetworkSpawner;
use zombienet_sdk::subxt::OnlineClient;

// Test which sends future and ready txs from many accounts
// to an unlimited pool.
#[tokio::test(flavor = "multi_thread")]
async fn send_future_and_then_ready_from_many_accounts() {
	let net = NetworkSpawner::from_toml(ASSET_HUB_HIGH_POOL_LIMIT_OLDP_4_COLLATORS_SPEC_PATH)
		.await
		.unwrap();
	let charlie = net.get_node("charlie").unwrap();

	// Wait for one of the collators to come online.
	let charlie_client: OnlineClient<zombienet_sdk::subxt::config::SubstrateConfig> =
		charlie.wait_client_with_timeout(120u64).await.unwrap();
	let mut stream = charlie_client.blocks().subscribe_best().await.unwrap();
	loop {
		let Some(block) = stream.next().await else {
			continue;
		};

		if block.is_ok() {
			tracing::info!("found best block: {:#?}", block.unwrap().hash());
			break;
		}
	}

	// Shared params.
	let send_threshold = 20_000;
	let ws = "ws://127.0.0.1:9933";
	let recipe_future = TransactionRecipe::transfer();
	let recipe_ready = recipe_future.clone();
	let block_monitor = false;
	let watched_txs = true;

	// Scenarios and sinks.
	let scenario_future =
		SendingScenario::FromManyAccounts { start_id: 0, last_id: 99, from: Some(100), count: 100 };
	let scenario_ready =
		SendingScenario::FromManyAccounts { start_id: 0, last_id: 99, from: Some(0), count: 100 };

	let future_scenario_executor = ScenarioExecutor::new()
		.with_rpc_uri(ws)
		.with_txs_recipe(recipe_future)
		.with_block_monitoring(block_monitor)
		.with_start_id("0".to_string())
		.with_last_id(99)
		.with_nonce_from(Some(100))
		.with_txs_count(100)
		.with_watched_txs(watched_txs)
		.with_send_threshold(send_threshold)
		.build();
	let ready_scenario_executor = ScenarioExecutor::new()
		.with_rpc_uri(ws)
		.with_txs_recipe(recipe_ready)
		.with_block_monitoring(block_monitor)
		.with_start_id("0".to_string())
		.with_last_id(99)
		.with_nonce_from(Some(0))
		.with_txs_count(100)
		.with_watched_txs(watched_txs)
		.with_send_threshold(send_threshold)
		.build();
	let (_, future_logs) =
		future_scenario_executor.execute::<DefaultTxTask<SubstrateTransaction>>().await;
	let (_, ready_logs) =
		ready_scenario_executor.execute::<DefaultTxTask<SubstrateTransaction>>().await;

	let finalized_future =
		future_logs.values().filter_map(|default_log| default_log.finalized()).count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();
	assert_eq!(finalized_future, 10_000);
	assert_eq!(finalized_ready, 10_000);
}

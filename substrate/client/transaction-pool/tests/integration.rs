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
	cli::SendingScenario,
	execution_log::ExecutionLog,
	runner::RunnerFactory,
	scenario::ScenarioExecutor,
	subxt_transaction::{generate_sr25519_keypair, SubstrateTransactionsSink},
	transaction::TransactionRecipe,
	SubstrateTransactionBuilder,
};
use zombienet::NetworkSpawner;
use zombienet_sdk::subxt::OnlineClient;

// Test which sends future and ready txs from many accounts
// to an unlimited pool.
#[tokio::test(flavor = "multi_thread")]
async fn send_future_and_then_ready_from_many_accounts() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let net = NetworkSpawner::init_from_asset_hub_fatp_high_pool_limit_spec().await.unwrap();
	let collator = net.get_node("charlie").unwrap();

	// Wait for one of the collators to come online.
	let client: OnlineClient<zombienet_sdk::subxt::config::SubstrateConfig> =
		collator.wait_client_with_timeout(120u64).await.unwrap();
	let mut stream = client.blocks().subscribe_best().await.unwrap();
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
	let unwatched = false;

	// Scenarios and sinks.
	let scenario_future =
		SendingScenario::FromManyAccounts { start_id: 0, last_id: 99, from: Some(100), count: 100 };
	let scenario_ready =
		SendingScenario::FromManyAccounts { start_id: 0, last_id: 99, from: Some(0), count: 100 };

	let future_scenario_executor =
		ScenarioExecutor::new(ws, scenario_future, recipe_future, block_monitor).await;
	let ready_scenario_executor =
		ScenarioExecutor::new(ws, scenario_ready, recipe_ready, block_monitor).await;

	let ((future_stop_runner_tx, mut runner_future), future_queue_task) =
		RunnerFactory::substrate_runner(future_scenario_executor, send_threshold, unwatched).await;
	let ((ready_stop_runner_tx, mut runner_ready), ready_queue_task) =
		RunnerFactory::substrate_runner(ready_scenario_executor, send_threshold, unwatched).await;

	let (future_logs, ready_logs) = join(runner_future.run_poc2(), runner_ready.run_poc2()).await;
	let finalized_future =
		future_logs.values().filter_map(|default_log| default_log.finalized()).count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();
	assert_eq!(finalized_future, 100);
	assert_eq!(finalized_ready, 100);
}

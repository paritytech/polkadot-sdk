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

use tracing::{info_span, Instrument};
use txtesttool::execution_log::ExecutionLog;
use zombienet::{default_zn_scenario_builder, NetworkSpawner};

// Test which sends future and ready txs from many accounts
// to an unlimited pool.
#[tokio::test(flavor = "multi_thread")]
async fn send_future_and_ready_from_many_accounts_to_collator() {
	let net = NetworkSpawner::from_toml_with_env_logger(
		zombienet::asset_hub_based_network_spec_paths::HIGH_POOL_LIMIT_FATP,
	)
	.await
	.unwrap();

	// Wait for the parachain collator to start block production.
	net.wait_for_block_production("charlie").await.unwrap();

	// Create future & ready txs executors.
	let ws = net.node_rpc_uri("charlie").unwrap();
	let future_scenario_executor = default_zn_scenario_builder()
		.with_rpc_uri(ws.clone())
		.with_start_id("0".to_string())
		.with_last_id(99)
		.with_nonce_from(Some(100))
		.with_txs_count(100)
		.build()
		.await;
	let ready_scenario_executor = default_zn_scenario_builder()
		.with_rpc_uri(ws)
		.with_start_id("0".to_string())
		.with_last_id(99)
		.with_nonce_from(Some(0))
		.with_txs_count(100)
		.build()
		.await;

	// Execute transactions and fetch the execution logs.
	let (future_logs, ready_logs) = futures::future::join(
		future_scenario_executor.execute().instrument(info_span!("future-txs-executor")),
		ready_scenario_executor.execute().instrument(info_span!("ready-txs-executor")),
	)
	.await;

	let finalized_future =
		future_logs.values().filter_map(|default_log| default_log.finalized()).count();
	let finalized_ready =
		ready_logs.values().filter_map(|default_log| default_log.finalized()).count();

	assert_eq!(finalized_future, 10_000);
	assert_eq!(finalized_ready, 10_000);
}

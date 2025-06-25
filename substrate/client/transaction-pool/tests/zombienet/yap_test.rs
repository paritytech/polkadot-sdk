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

// Test inspired (copied) from:
// https://github.com/paritytech/polkadot-sdk/blob/85b71daf7aac59da4d2186b45d589c7c619f0981/polkadot/zombienet-sdk-tests/tests/elastic_scaling/slot_based_3cores.rs#L21
// and patched as in:
// https://github.com/paritytech/polkadot-sdk/pull/7220#issuecomment-2808830472

use crate::zombienet::{NetworkSpawner, ScenarioBuilderSharedParams};
use cumulus_zombienet_sdk_helpers::create_assign_core_call;
use serde_json::json;
use txtesttool::{execution_log::ExecutionLog, scenario::ScenarioBuilder};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn slot_based_3cores_test() -> Result<(), anyhow::Error> {
	let spawner = NetworkSpawner::with_closure(|| {
		let images = zombienet_sdk::environment::get_images_from_env();
		let names = ["alice", "bob", "charlie"];
		NetworkConfigBuilder::new()
			.with_relaychain(|r| {
				let r = r
					.with_chain("rococo-local")
					.with_default_command("polkadot")
					.with_default_image(images.polkadot.as_str())
					.with_default_args(vec![("-lparachain=debug").into()])
					.with_genesis_overrides(json!({
						"configuration": {
							"config": {
								"scheduler_params": {
									// Num cores is 2, because 1 extra will be added automatically when registering the para.
									"num_cores": 2,
									"max_validators_per_core": 1
								}
							}
						}
					}))
					.with_default_resources(|resources| {
						resources.with_request_cpu(4).with_request_memory("4G")
					})
					// Have to set a `with_node` outside of the loop below, so that `r` has the
					// right type.
					.with_node(|node| node.with_name(names[0]));

				(1..3).fold(r, |acc, i| acc.with_node(|node| node.with_name(names[i])))
			})
			.with_parachain(|p| {
				// Para 2200 uses the new RFC103-enabled collator which sends the UMP signal
				// commitment for selecting the core index
				p.with_id(2200)
					.with_default_command("polkadot-parachain")
					.with_default_image(images.cumulus.as_str())
					.with_chain("yap-rococo-local-2200")
					.with_genesis_overrides(json!({
						"balances": {
							"devAccounts": [
								100000, 1000000000000000000u64, "//Sender//{}"
							]
						}
					}))
					.with_default_args(vec![
						"--authoring=slot-based".into(),
						"--rpc-max-subscriptions-per-connection=256000".into(),
						"--rpc-max-connections=128000".into(),
						"--rpc-max-response-size=150".into(),
						"--pool-limit=2500000".into(),
						"--pool-kbytes=4048000".into(),
						"--pool-type=fork-aware".into(),
						("-lparachain=debug,aura=debug,txpool=debug,txpoolstat=debug").into(),
					])
					.with_collator(|n| n.with_name("dave").with_rpc_port(9944))
			})
	})
	.await
	.unwrap();

	let relay_node = spawner.network().get_node("alice")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();

	let assign_cores_call = create_assign_core_call(&[(0, 2200), (1, 2200)]);
	// Assign two extra cores to each parachain.
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_cores_call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	tracing::info!("2 more cores assigned to the parachain");

	// Wait for the parachain collator to start block production.
	spawner.wait_for_block_production("dave").await.unwrap();

	// Create txs executor.
	let ws = spawner.node_rpc_uri("dave").unwrap();
	let executor = {
		let shared_params = ScenarioBuilderSharedParams::default();
		ScenarioBuilder::new()
			.with_watched_txs(shared_params.watched_txs)
			.with_send_threshold(shared_params.send_threshold)
			.with_block_monitoring(shared_params.does_block_monitoring)
			.with_chain_type(shared_params.chain_type)
			.with_base_dir_path(spawner.base_dir_path().unwrap().to_string())
			.with_timeout_in_secs(21600) //6 hours
			.with_legacy_backend(true)
	}
	.with_rpc_uri(ws)
	.with_start_id(0)
	.with_last_id(99999)
	.with_txs_count(150)
	.with_executor_id("txs-executor".to_string())
	.with_send_threshold(25000)
	.build()
	.await;

	// Execute transactions and fetch the execution logs.
	let execution_logs = executor.execute().await;
	let finalized_txs = execution_logs.values().filter_map(|tx_log| tx_log.finalized()).count();

	assert_eq!(finalized_txs, 15_000_000);

	Ok(())
}

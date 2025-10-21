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
use sp_core::offchain::Duration;
use tokio::time::sleep;
use txtesttool::{execution_log::ExecutionLog, scenario::ScenarioBuilder};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn cyon_spawn_yap() -> Result<(), anyhow::Error> {
	tracing::error!("🔮 CYON Starting CYON YAP test");
	let spawner = NetworkSpawner::with_closure(|| {
		let images = zombienet_sdk::environment::get_images_from_env();
		let names = ["alice", "bob", "charlie"];
		NetworkConfigBuilder::new()
			.with_relaychain(|r| {
				let r = r
					.with_chain("rococo-local")
					.with_default_command("polkadot")
					.with_default_image(images.polkadot.as_str())
					.with_default_args(vec![("-lparachain=off,parachain::collator-protocol=warn").into()])
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
						},
						"sudo": {
							"key": "5EHEDwRzuFFhh8hJJbhRTacPLzVVuabjQA7rz22HCCyZ8o7b"
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
						("-lparachain=off,parachain::collator-protocol=warn,aura=off,aura::cumulus=error,txpool=off,txpoolstat=off").into(),
					])
					.with_collator(|n| n.with_name("dave").with_rpc_port(9944))
			})
	})
	.await
	.unwrap();

	// // Wait for the parachain collator to start block production.
	spawner.wait_for_block_production("dave").await.unwrap();

	tracing::error!("🔮 CYON YAP is running");

	let mut duration = 10_000;
	loop {
		if duration < u64::max_value() / 2 {
			duration *= 2;
		}
		tracing::info!("Sleeping for {}ms", duration);
		sleep(tokio::time::Duration::from_millis(duration)).await;
	}
}

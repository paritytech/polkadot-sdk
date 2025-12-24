// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput, assign_cores};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use tokio::{join, spawn, task::JoinHandle};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder, NetworkNode,
};

const PARA_ID: u32 = 2400;

/// A test that ensures that `PoV` bundling works.
///
/// Initially, one core is assigned. We expect the parachain to produce 12 block per relay core.
/// As we increase the number of cores via `assign_core`, we expect the blocks to spread over the
/// relay cores.
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_basic() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;
	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-1")?;
	let para_full_node = network.get_node("para-full-node")?;

	let handle = wait_for_block_and_restart_node(para_full_node.clone());

	let para_client = para_node.wait_client().await?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	assert_para_throughput(
		&relay_client,
		6,
		[(ParaId::from(PARA_ID), 4..7)],
		[(ParaId::from(PARA_ID), (para_client.clone(), 44..73))],
	)
	.await?;
	// 6 relay chain blocks
	assert_finality_lag(&para_client, 72).await?;

	assign_cores(&relay_client, PARA_ID, vec![0, 1]).await?;

	assert_para_throughput(
		&relay_client,
		6,
		[(ParaId::from(PARA_ID), 12..19)],
		[(ParaId::from(PARA_ID), (para_client.clone(), 44..73))],
	)
	.await?;
	assert_finality_lag(&para_client, 72).await?;

	assign_cores(&relay_client, PARA_ID, vec![2, 3, 4]).await?;

	assert_para_throughput(
		&relay_client,
		6,
		[(ParaId::from(PARA_ID), 24..37)],
		[(ParaId::from(PARA_ID), (para_client.clone(), 44..73))],
	)
	.await?;

	assert_finality_lag(&para_client, 72).await?;

	// Ensure we restarted the node successfully
	handle.await??;

	let para_full_client: OnlineClient<PolkadotConfig> = para_full_node.wait_client().await?;
	let mut full_best_blocks = para_full_client.blocks().subscribe_best().await?;
	let mut collator_best_blocks = para_client.blocks().subscribe_best().await?;

	let (Some(full_best), Some(best)) = join!(full_best_blocks.next(), collator_best_blocks.next())
	else {
		return Err(anyhow!("Failed to get a best block from the full node and the collator"))
	};

	let diff = full_best?.number().abs_diff(best?.number());
	if diff > 12 {
		return Err(anyhow!(
			"Best block difference between full node and collator of {diff} is too big!"
		))
	}

	log::info!("Test finished successfully");

	Ok(())
}

/// Wait for block `13` and then restart the node.
///
/// We take block `13`, because it should be near the beginning of a block bundle and we want to
/// test stopping the node while importing blocks in the middle of a bundle.
fn wait_for_block_and_restart_node(node: NetworkNode) -> JoinHandle<Result<(), anyhow::Error>> {
	spawn(async move {
		let para_client: OnlineClient<PolkadotConfig> = node.wait_client().await?;
		let mut best_blocks = para_client.blocks().subscribe_best().await?;

		loop {
			let Some(block) = best_blocks.next().await.transpose()? else {
				return Err(anyhow!("Node stopped before reaching the block to restart"))
			};

			if block.number() >= 13 {
				log::info!("Full node has imported block `13`, going to restart it");
				return node.restart(None).await
			}
		}
	})
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=trace").into()])
				.with_default_resources(|resources| {
					// These settings are applicable only for `k8s` provider.
					// Leaving them in case we switch to `k8s` some day.
					resources.with_request_cpu(4).with_request_memory("4G")
				})
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 7,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));
			(1..9).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("block-bundling")
				.with_default_args(vec![
					("--authoring").into(),
					("slot-based").into(),
					("-lparachain=trace,aura=trace").into(),
				])
				.with_genesis_overrides(json!({
					"testPallet": {
						"enableBigValueMove": true
					}
				}))
				.with_collator(|n| n.with_name("collator-0"))
				.with_collator(|n| n.with_name("collator-1"))
				.with_collator(|n| n.with_name("collator-2"))
				.with_collator(|n| n.with_name("para-full-node").validator(false))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}

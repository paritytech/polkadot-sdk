// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::anyhow;

use polkadot_primitives::Id as ParaId;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::assert_para_is_registered;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID_1: u32 = 2000;
const PARA_ID_2: u32 = 2001;

#[tokio::test(flavor = "multi_thread")]
async fn parachain_informant() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID_1), 10).await?;
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID_2), 10).await?;

	for name in ["para1-1", "para2-1"] {
		log::info!("Checking full node {name} is syncing");
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, 225u64)
			.await
			.is_ok());
	}

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// TODO
	// Network setup:
	// - relaychain nodes:
	//   - alice    - validator
	//   - bob      - validator
	//   - charlie  - validator
	// - parachain nodes
	//   - eve      - collator
	//   - ferdie   - collator
	//   - one      - collator
	//   - two      - full node
	//   - three    - full node
	//   - four     - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-relaychain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_1)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("para1-1"))
				.with_collator(|n| {
					n.with_name("para1-2").validator(false).with_args(vec![("-lsync=debug").into()])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_2)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("para2-1"))
				.with_collator(|n| {
					n.with_name("para2-2").validator(false).with_args(vec![("-lsync=debug").into()])
				})
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

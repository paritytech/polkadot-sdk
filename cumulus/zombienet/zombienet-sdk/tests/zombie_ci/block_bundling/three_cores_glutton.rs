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

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::network::{
	assert_finality_lag, assert_para_throughput, assign_cores,
};
use polkadot_primitives::Id as ParaId;
#[cfg(not(feature = "jam"))]
use serde_json::json;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::NetworkConfig;
#[cfg(not(feature = "jam"))]
use zombienet_sdk::NetworkConfigBuilder;

#[cfg(feature = "jam")]
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	genesis_build::{build_jam_genesis, polkavm_env},
	network::{
		base_dir, collator_args, copy_para_specs, path_str, work_dir, ORDINARY_NODE,
		VALIDATORS_PER_CORE,
	},
	para::{Para, TINY_CORES},
};

const PARA_ID: u32 = 2400;

/// A test that ensures that PoV bundling works with 3 cores and glutton consuming 10% ref time.
///
/// This test starts with 3 cores assigned and configures glutton to use 10% of ref time,
/// then validates that the parachain produces 72 blocks.
#[tokio::test(flavor = "multi_thread")]
async fn block_bundling_three_cores_glutton() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	#[cfg(not(feature = "jam"))]
	let config = build_network_config().await?;
	#[cfg(feature = "jam")]
	let (work_dir, _temp) = work_dir("block_bundling_three_cores_glutton")?;
	#[cfg(feature = "jam")]
	let config = {
		let binaries = binaries_or_err()?;
		let paras = vec![Para::new(PARA_ID, 0, &["collator-0", "collator-1", "collator-2"])];
		let genesis = build_jam_genesis(&binaries, &work_dir, &paras, TINY_CORES)?;
		let para_specs = copy_para_specs(&work_dir, &genesis.para_specs, &paras)?;
		let base_dir = base_dir(&work_dir)?;
		let jam_node = path_str(&binaries.jam_node)?;
		let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
		let omni_node = path_str(&binaries.omni_node)?;
		let authorizer_blob = path_str(&genesis.authorizer_blob)?;
		let overrides = genesis.overrides.clone();
		let no_overrides = std::collections::HashMap::new();
		let validators = TINY_CORES as usize * VALIDATORS_PER_CORE;

		zombienet_sdk::NetworkConfigBuilder::new()
			.with_jamchain(|jam| {
				let jam = jam.with_id("jam").with_default_command(jam_node.as_str());
				let jam = match genspec_node.as_deref() {
					Some(command) if command != jam_node.as_str() => {
						jam.with_chain_spec_command(command)
					},
					_ => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..validators).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env()))
			})
			.with_parachain(|p| {
				let p = p
					.with_id(paras[0].id)
					.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
					.with_chain_spec_path(para_specs[0].clone())
					.with_default_command(omni_node.as_str());
				let p = p.with_collator(|node| {
					node.with_name(paras[0].collators[0].as_str())
						.with_env(polkavm_env())
						.with_args(collator_args(
							&authorizer_blob,
							&no_overrides,
							&paras[0].collators[0],
							true,
						))
				});
				paras[0].collators[1..].iter().fold(p, |p, name| {
					p.with_collator(|node| {
						node.with_name(name.as_str())
							.with_env(polkavm_env())
							.with_args(collator_args(&authorizer_blob, &no_overrides, name, true))
					})
				})
			})
			.with_global_settings(|g| g.with_base_dir(base_dir))
			.build()
			.map_err(|e| {
				anyhow!(
					"config errs: {}",
					e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
				)
			})?
	};

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let para_node = network.get_node("collator-1")?;

	let para_client = para_node.wait_client().await?;

	// Assign cores 0 and 1 to start with 3 cores total (core 2 is assigned by Zombienet)
	assign_cores(&network, "validator-0", PARA_ID, vec![0, 1]).await?;

	// Wait for the parachain to produce 72 blocks with 3 cores and glutton active
	// With 3 cores, we expect roughly 3x throughput compared to single core
	// Adjusting expectations based on glutton consuming 80% of ref time
	assert_para_throughput(
		&network,
		"validator-0",
		6,
		[(ParaId::from(PARA_ID), 12..19)],
		[(ParaId::from(PARA_ID), (para_client.clone(), 44..73))],
	)
	.await?;

	assert_finality_lag(&network, "collator-1", 72).await?;
	log::info!("Test finished successfully - 72 blocks produced with 3 cores and glutton");
	Ok(())
}

#[cfg(not(feature = "jam"))]
async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
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
					resources.with_request_cpu(4).with_request_memory("4G")
				})
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));
			(1..9).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("block-bundling")
				.with_default_args(vec![
					("--authoring").into(),
					("slot-based").into(),
					("-lparachain=debug,aura=trace,runtime=trace").into(),
				])
				.with_genesis_overrides(json!({
				"glutton": {
					"compute": "0.1",
					"storage": "0",
					"trashDataCount": 5000,
					"blockLength": "0"
				}
				}))
				.with_collator(|n| n.with_name("collator-0"))
				.with_collator(|n| n.with_name("collator-1"))
				.with_collator(|n| n.with_name("collator-2"))
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

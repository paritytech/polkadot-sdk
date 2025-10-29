// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test we are producing 12-second parachain blocks if using an old collator, pre async-backing.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput};
use polkadot_primitives::{BlockNumber, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{ext::subxt_rpcs::rpc_params, OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn collator_protocol_spam() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=info,parachain::collator-protocol=trace,parachain::network-bridge-rx=trace").into(), // ,litep2p=trace,sub-libp2p=debug,sync=debug,
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 3,
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..3).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(1000)
				.with_default_command("adder-collator")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.cumulus_based(false)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-adder-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;

	let genesis_hash: String =
		relay_node.rpc().await?.request("chain_getBlockHash", rpc_params![0]).await?;

	log::info!("relaychain genesis hash: {:?}", genesis_hash);

	let mut addrs = Vec::new();

	for validator in ["validator-0", "validator-1", "validator-2"] {
		let rpc_client = network.get_node(validator)?.rpc().await?;
		let addresses: Vec<String> =
			rpc_client.request("system_localListenAddresses", rpc_params![]).await?;
		let local_address = addresses
			.into_iter()
			.filter(|addr| addr.contains("/ip4/127.0.0.1/"))
			.collect::<Vec<_>>()
			.pop();
		match local_address {
			Some(a) => addrs.push(a),
			None => log::error!("No local address for {}", validator),
		}
	}

	log::info!("ADDRESSES: {:?}", addrs);

	log::info!("sleeping");
	std::thread::sleep(std::time::Duration::from_secs(60 * 60));

	Ok(())
}

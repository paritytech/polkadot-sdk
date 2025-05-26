// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use subxt::{dynamic::Value, OnlineClient, SubstrateConfig};
use subxt_signer::sr25519::dev;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn parachain_extrinsic_gets_finalized() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;

	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	log::info!("Ensuring all nodes report 4 node roles");
	assert!(alice.wait_metric_with_timeout("node_roles", |p| p == 4.0, 10u64).await.is_ok());
	assert!(bob.wait_metric_with_timeout("node_roles", |p| p == 4.0, 10u64).await.is_ok());
	assert!(charlie
		.wait_metric_with_timeout("node_roles", |p| p == 4.0, 10u64)
		.await
		.is_ok());

	log::info!("Ensuring alice and bob are connected to at least 1 peer");
	assert!(alice.assert_with("sub_libp2p_peers_count", |p| p >= 1.0).await?);
	assert!(bob.assert_with("sub_libp2p_peers_count", |p| p >= 1.0).await?);

	log::info!("Ensuring all nodes report expected block height");
	assert!(alice
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 5.0, 60u64)
		.await
		.is_ok());
	assert!(bob
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 5.0, 60u64)
		.await
		.is_ok());
	assert!(charlie
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 2.0, 120u64)
		.await
		.is_ok());

	log::info!("Ensuring nodes do not report any error");
	for node in [alice, bob, charlie] {
		let result = node
			.wait_log_line_count_with_timeout(
				"error",
				false,
				LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(2)),
			)
			.await?;
		assert!(result.success(), "node {} reported error: {:?}", node.name(), result);
	}

	log::info!("Ensuring parachain extrinsic gets finalized");
	let call = subxt::dynamic::tx("System", "remark", vec![Value::from_bytes("xxx".as_bytes())]);
	let charlie_client: OnlineClient<SubstrateConfig> = charlie.wait_client().await?;

	let res = submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&charlie_client,
		&call,
		&dev::alice(),
		600u64,
	)
	.await;
	assert!(res.is_ok(), "Extrinsic failed to finalize: {:?}", res.unwrap_err());

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	log::info!("Spawning network");

	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain Nodes:
	// 	 - alice
	// 	 - bob
	// - parachain Nodes:
	//   - charlie - collator
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_chain("asset-hub-rococo-local")
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("charlie").validator(true).with_args(vec![
						("--force-authoring").into(),
						("-ltxpool=trace").into(),
						("--pool-type=fork-aware").into(),
					])
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

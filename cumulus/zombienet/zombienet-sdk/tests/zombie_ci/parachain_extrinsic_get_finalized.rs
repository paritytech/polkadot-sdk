// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use tokio::time::Duration;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use zombienet_orchestrator::network::node::{LogLineCount, LogLineCountOptions};
use zombienet_sdk::{
	subxt::{self, dynamic::Value, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn parachain_extrinsic_gets_finalized() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	for node in [alice, bob, charlie] {
		log::info!("Ensuring {} reports 4 node roles", node.name());
		assert!(node.wait_metric_with_timeout("node_roles", |p| p == 4.0, 60u64).await.is_ok());
	}

	for node in [alice, bob] {
		log::info!("Ensuring {} is connected to at least 1 peer", node.name());
		assert!(node
			.wait_metric_with_timeout("sub_libp2p_peers_count", |p| p >= 1.0, 60u64)
			.await
			.is_ok());
	}

	for (node, block_height, timeout) in
		[(alice, 5.0, 60u64), (bob, 5.0, 60u64), (charlie, 2.0, 120u64)]
	{
		log::info!("Ensuring all nodes report expected block height");
		assert!(node
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > block_height, timeout)
			.await
			.is_ok());
	}

	for node in [alice, bob, charlie] {
		log::info!("Ensuring {} does not report any error", node.name());

		let result = node
			.wait_log_line_count_with_timeout(
				".*kernel security feature.*not available.*error.",
				false,
				// Do not wait for more logs. The lines we are looking for appear at the node
				// startup, which we must have clearly reached if we are here.
				LogLineCountOptions::new(|n| n == 0, Duration::from_secs(0), false),
			)
			.await?;

		// If above line appeared then increase expected error count
		let mut error_cnt_expected = if result.success() { 0 } else { 1 };

		let result = node
			.wait_log_line_count_with_timeout(
				r"error=Pool\(InvalidTransaction\(InvalidTransaction::MandatoryValidation\)\)",
				false,
				LogLineCountOptions::new(move |n| n > 0, Duration::from_secs(2), true),
			)
			.await?;

		// We want to search for lines containing 'error' but we need to ignore below line (which
		// occasionally appears):
		//   error=Pool(InvalidTransaction(InvalidTransaction::MandatoryValidation))
		// In a perfect world, we could use a regex with look-around, e.g.
		//  (?!.*error=Pool(InvalidTransaction(InvalidTransaction::MandatoryValidation))).*error.*
		// but unfortunately, `regex` crate used by zombienet-sdk does not support look-arounds.
		//
		// Therefore, we will do as follows:
		// - First, search for lines containing:
		//   error=Pool(InvalidTransaction(InvalidTransaction::MandatoryValidation))
		// - Add the number of such occurrences to the expected number of 'error' lines
		// - Then, search for lines containing: error
		if let LogLineCount::TargetReached(cnt) = result {
			error_cnt_expected += cnt;
		}

		let result = node
			.wait_log_line_count_with_timeout(
				"error",
				false,
				// Do not wait here, we already waited in previous step
				LogLineCountOptions::new(
					move |n| n == error_cnt_expected,
					Duration::from_secs(0),
					false,
				),
			)
			.await?;
		assert!(result.success(), "node {} reported error: {:?}", node.name(), result);
	}

	log::info!("Ensuring parachain extrinsic gets finalized");
	let call = subxt::dynamic::tx("System", "remark", vec![Value::from_bytes("xxx".as_bytes())]);
	let charlie_client: OnlineClient<PolkadotConfig> = charlie.wait_client().await?;

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

	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
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

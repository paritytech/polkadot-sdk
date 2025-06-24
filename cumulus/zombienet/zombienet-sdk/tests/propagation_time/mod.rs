// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::ensure_para_throughput;
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
	subxt::{ext::futures, OnlineClient, PolkadotConfig},
	AddCollatorOptions, LocalFileSystem, Network, NetworkConfigBuilder, NetworkNode,
};

const PARA_ID: u32 = 2000;
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

#[ignore = "Slow test used to measure block propagation time in a sparsely connected network"]
#[tokio::test(flavor = "multi_thread")]
async fn sparsely_connected_network_block_propagation_time() -> Result<(), anyhow::Error> {
	log::warn!("This test is slow. It will take a long time to complete.");
	tokio::time::sleep(Duration::from_secs(3)).await;

	let mut num_failures = 0;
	let mut tests_remaining = 10;
	let mut propagation_times = Vec::new();

	// Try multiple tests to get a better average.
	while tests_remaining > 0 {
		log::info!("Running test with {tests_remaining} tests remaining");
		if num_failures > 5 {
			anyhow::bail!("Too many failures ({num_failures}), aborting further tests.");
		}
		match run_network().await {
			Ok(propagation_time) => {
				log::info!("Propagation time: {propagation_time} seconds");
				propagation_times.push(propagation_time);
			},
			Err(e) => {
				log::error!("Test failed: {e}");
				num_failures += 1;
			},
		}
		tests_remaining -= 1;
	}

	let avg = propagation_times.iter().sum::<f64>() / propagation_times.len() as f64;
	log::info!("Average propagation time: {avg} seconds");
	propagation_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
	let median = if propagation_times.len() % 2 == 0 {
		(propagation_times[propagation_times.len() / 2 - 1] +
			propagation_times[propagation_times.len() / 2]) /
			2.0
	} else {
		propagation_times[propagation_times.len() / 2]
	};
	log::info!("Median propagation time: {median} seconds");

	Ok(())
}

async fn run_network() -> Result<f64, anyhow::Error> {
	let NetworkActors { network, validator, collators } = initialize_network().await?;

	let relay_alice = network.get_node("alice")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_alice.wait_client().await?;

	log::info!("Ensuring parachain making progress");
	let has_throughput = ensure_para_throughput(
		&relay_client,
		10,
		[(ParaId::from(PARA_ID), 9..11)].into_iter().collect(),
	)
	.await?;
	if !has_throughput {
		anyhow::bail!("Parachain did not produce enough blocks");
	}

	log::info!("Validator has {} peers", validator.reports("sub_libp2p_peers_count").await?);

	// Pause the validator to stop block production.
	validator.pause().await?;
	// Wait for the collators to all reach the same block height through gossip.
	log::info!("Waiting for all nodes to reach consensus on the same block height");
	let block_height = timeout(wait_consensus(&collators)).await?;
	log::info!("All nodes reached consensus on block height {block_height}");
	// Wait for the validator to produce at least one more block.
	validator.resume().await?;
	log::info!("Waiting for validator to advance beyond block height {block_height}");
	timeout(wait_next_block(&[validator], block_height)).await?;
	log::info!("Validator advanced beyond block height {block_height}");
	// At this point, the new block will start to propagate through the network. Store the timestamp
	// so we can measure the propagation time.
	let start = Instant::now();
	// Wait for the new block to propagate to all collators.
	log::info!("Waiting for collators to propagate the new block");
	timeout(wait_next_block(&collators, block_height)).await?;
	log::info!("All collators received the new block");

	Ok(start.elapsed().as_secs_f64())
}

async fn timeout<F, T>(future: F) -> Result<T, anyhow::Error>
where
	F: futures::Future<Output = Result<T, anyhow::Error>>,
{
	tokio::time::timeout(Duration::from_secs(60), future).await?
}

async fn initialize_network() -> Result<NetworkActors, anyhow::Error> {
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
	//   - 1 validator
	//   - many sparsely connected collators
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(2).with_request_memory("2G")
				})
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| {
					n.with_name("validator").validator(true).with_args(vec![
						("--in-peers", "1").into(),
						("--out-peers", "1").into(),
						("--relay-chain-rpc-url", "{{ZOMBIE:alice:ws_uri}}").into(),
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

	// Spawn network
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let mut network = spawn_fn(config).await?;

	// Sparsely connected network of many nodes.
	let mut collators = Vec::new();
	let validator = network.get_node("validator")?.clone();
	let mut peer = validator.clone();
	for i in 0..20 {
		let collator =
			add_sparsely_connected_collator(&mut network, &images, format!("collator{i}"), peer)
				.await?;
		collators.push(collator.clone());
		peer = collator;
	}
	log::info!("Added sparsely connected collators");

	Ok(NetworkActors { network, validator, collators })
}

async fn add_sparsely_connected_collator(
	network: &mut Network<LocalFileSystem>,
	images: &zombienet_sdk::environment::Images,
	name: String,
	peer: NetworkNode,
) -> Result<NetworkNode, anyhow::Error> {
	network
		.add_collator(
			&name,
			AddCollatorOptions {
				command: Some("test-parachain".try_into().unwrap()),
				image: Some(images.cumulus.as_str().try_into().unwrap()),
				args: vec![
					"-lparachain=debug".into(),
					("--in-peers", "3").into(),
					("--out-peers", "3").into(),
					("--bootnodes", peer.multiaddr()).into(),
				],
				..Default::default()
			},
			PARA_ID,
		)
		.await?;
	network.get_node(&name).cloned().map_err(Into::into)
}

struct NetworkActors {
	network: Network<LocalFileSystem>,
	validator: NetworkNode,
	collators: Vec<NetworkNode>,
}

/// Wait for all of the nodes to reach consensus on the same block height.
async fn wait_consensus(nodes: &[NetworkNode]) -> Result<f64, anyhow::Error> {
	loop {
		let best_blocks =
			futures::future::try_join_all(nodes.iter().map(|node| node.reports(BEST_BLOCK_METRIC)))
				.await?;
		let first = best_blocks.first().expect("at least one node");
		if best_blocks.iter().all(|b| b == first) {
			return Ok(*first);
		}
		tokio::time::sleep(Duration::from_millis(300)).await;
	}
}

/// Wait for all of the nodes to advance beyond the given block height.
async fn wait_next_block(nodes: &[NetworkNode], block_height: f64) -> Result<(), anyhow::Error> {
	loop {
		let best_blocks =
			futures::future::try_join_all(nodes.iter().map(|node| node.reports(BEST_BLOCK_METRIC)))
				.await?;
		if best_blocks.iter().all(|&b| b > block_height) {
			return Ok(());
		}
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
}

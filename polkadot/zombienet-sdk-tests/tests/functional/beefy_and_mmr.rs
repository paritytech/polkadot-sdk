// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! BEEFY and MMR
//!
//! Runs a small `rococo-local` relay chain and verifies BEEFY and MMR behaviour.
//!
//! Checks performed:
//! - BEEFY metrics and finalized heads across validators.
//! - Pause/resume an "unstable" validator and confirm the chain continues finalizing and the
//!   validator set rotates.
//! - Use RPCs to inspect MMR leaves and to generate and verify MMR proofs

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use subxt::ext::subxt_rpcs::client::RpcParams;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	Network, NetworkConfig, NetworkConfigBuilder,
};

use crate::utils::{env_or_default, initialize_network, INTEGRATION_IMAGE_ENV};

const VALIDATOR_NAMES: [&str; 3] = ["validator-0", "validator-1", "validator-2"];
const UNSTABLE: &str = "validator-unstable";

#[derive(Debug, Serialize, Deserialize)]
struct LeavesProof {
	#[serde(rename = "blockHash")]
	block_hash: String,
	leaves: String,
	proof: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn beefy_and_mmr_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	// Check authority status
	log::info!("Checking authority status for validators");
	for &name in VALIDATOR_NAMES.iter().chain(std::iter::once(&UNSTABLE)) {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("node_roles", |v| v == 4.0, 30u64)
			.await
			.map_err(|e| anyhow!("Node {} role check failed: {}", name, e))?;
	}

	// Initial BEEFY validator set id should be 0
	log::info!("Checking initial substrate_beefy_validator_set_id == 0");
	for &name in VALIDATOR_NAMES.iter().chain(std::iter::once(&UNSTABLE)) {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("substrate_beefy_validator_set_id", |v| v == 0.0, 30u64)
			.await
			.map_err(|e| anyhow!("Initial validator_set_id check failed on {}: {}", name, e))?;
	}

	// Verify voting happens and first mandatory block is finalized within 60s
	log::info!("Waiting for substrate_beefy_best_block >= 1");
	for &name in VALIDATOR_NAMES.iter().chain(std::iter::once(&UNSTABLE)) {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 1.0, 60u64)
			.await
			.map_err(|e| anyhow!("Best block not reported on {}: {}", name, e))?;
	}

	// Pause unstable validator and ensure chain continues
	log::info!("Pausing unstable validator: {}", UNSTABLE);
	let unstable = network.get_node(UNSTABLE)?;
	unstable.pause().await?;

	// Wait for validator set rotation (>=1 then >=2)
	log::info!("Waiting for validator set id to increase to >=1");
	let observer = network.get_node("validator-0")?;
	observer
		.wait_metric_with_timeout("substrate_beefy_validator_set_id", |v| v >= 1.0, 180u64)
		.await
		.map_err(|e| anyhow!("Validator set id did not increase to >=1: {}", e))?;
	log::info!("Waiting for validator set id to increase to >=2");
	observer
		.wait_metric_with_timeout("substrate_beefy_validator_set_id", |v| v >= 2.0, 180u64)
		.await
		.map_err(|e| anyhow!("Validator set id did not increase to >=2: {}", e))?;

	// Verify BEEFY finalized mandatory block (>=21)
	observer
		.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 21.0, 180u64)
		.await
		.map_err(|e| anyhow!("BEEFY best block did not reach 21: {}", e))?;

	// Verify BEEFY finalized heads across all validators
	log::info!("Checking BEEFY finalized heads across all validators");
	verify_beefy_finalized_heads(
		&network,
		&VALIDATOR_NAMES
			.iter()
			.copied()
			.chain(std::iter::once(UNSTABLE))
			.collect::<Vec<_>>(),
	)
	.await?;

	// Verify MMR leaves count
	log::info!("Checking MMR leaves count");
	verify_mmr_leaves(&network, "validator-0", 21).await?;

	// Verify MMR proof generation and verification
	log::info!("Generating and verifying MMR proofs");
	// Note: Only verify on active validators
	verify_mmr_proofs(&network, "validator-0", &VALIDATOR_NAMES).await?;

	// Resume unstable and verify it catches up
	log::info!("Resuming unstable validator: {}", UNSTABLE);
	unstable.resume().await?;
	unstable
		.wait_metric_with_timeout("substrate_beefy_validator_set_id", |v| v >= 2.0, 60u64)
		.await
		.map_err(|e| anyhow!("Unstable did not catch up validator_set_id: {}", e))?;
	unstable
		.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 21.0, 60u64)
		.await
		.map_err(|e| anyhow!("Unstable did not catch up best_block: {}", e))?;

	log::info!("BEEFY and MMR test (metrics & pause/resume) finished successfully");

	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());

	let mut builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		let r = r
			.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(polkadot_image.as_str())
			.with_default_args(vec![
				"--log=beefy=debug".into(),
				"--enable-offchain-indexing=true".into(),
			]);

		// Add validator nodes and unstable validator
		// NOTE: Once we update zombienet-sdk to newer version >v0.3.13, the following can be
		// replaced with a group
		r.with_node(|node| node.with_name("validator-0"))
			.with_node(|node| node.with_name("validator-1"))
			.with_node(|node| node.with_name("validator-2"))
			.with_node(|node| node.with_name(UNSTABLE))
	});

	builder = builder.with_global_settings(|global_settings| {
		match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		}
	});

	builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})
}

/// Verify BEEFY finalized heads across all validator nodes
async fn verify_beefy_finalized_heads(
	network: &Network<zombienet_sdk::LocalFileSystem>,
	node_names: &[&str],
) -> Result<(), anyhow::Error> {
	#[derive(Debug)]
	struct FinalizedHeadInfo {
		node_name: String,
		finalized_head: String,
		finalized_height: u32,
	}

	// Get finalized head for each node
	let mut finalized_heads = Vec::new();
	for &node_name in node_names {
		let node = network.get_node(node_name)?;
		let rpc_client = node.rpc().await?;
		// Get BEEFY finalized head
		let finalized_head: String =
			rpc_client.request("beefy_getFinalizedHead", RpcParams::new()).await?;

		// Get header for the finalized head to extract block number
		let mut header_params = RpcParams::new();
		header_params.push(finalized_head.clone())?;
		let header: serde_json::Value =
			rpc_client.request("chain_getHeader", header_params).await?;
		let finalized_height = header["number"]
			.as_str()
			.ok_or_else(|| anyhow!("Failed to get block number from header"))?;
		let finalized_height = u32::from_str_radix(finalized_height.trim_start_matches("0x"), 16)?;

		finalized_heads.push(FinalizedHeadInfo {
			node_name: node_name.to_string(),
			finalized_head: finalized_head.clone(),
			finalized_height,
		});

		log::info!(
			"Node {} has BEEFY finalized head {} at height {}",
			node_name,
			finalized_head,
			finalized_height
		);
	}

	// Find the node with the highest finalized height
	let highest = finalized_heads
		.iter()
		.max_by_key(|info| info.finalized_height)
		.ok_or_else(|| anyhow!("No finalized heads found"))?;

	log::info!(
		"Highest finalized height is {} on node {}",
		highest.finalized_height,
		highest.node_name
	);

	// Get all block hashes up to the highest finalized height from that node
	let highest_node = network.get_node(&highest.node_name)?;
	let highest_rpc = highest_node.rpc().await?;

	let mut block_hashes = Vec::new();
	for block_number in 0..=highest.finalized_height {
		let mut hash_params = RpcParams::new();
		hash_params.push(block_number)?;
		let block_hash: String = highest_rpc.request("chain_getBlockHash", hash_params).await?;
		block_hashes.push(block_hash);
	}

	// Verify that all nodes have finalized height >= 21 and their finalized head matches
	for info in &finalized_heads {
		if info.finalized_height < 21 {
			return Err(anyhow!(
				"Node {} has finalized height {} which is less than 21",
				info.node_name,
				info.finalized_height
			));
		}

		let expected_hash = &block_hashes[info.finalized_height as usize];
		if &info.finalized_head != expected_hash {
			return Err(anyhow!(
				"Node {} finalized head {} does not match expected hash {} at height {}",
				info.node_name,
				info.finalized_head,
				expected_hash,
				info.finalized_height
			));
		}
	}

	log::info!("All BEEFY finalized heads verified successfully");

	Ok(())
}

/// Verify MMR leaves count
async fn verify_mmr_leaves(
	network: &Network<zombienet_sdk::LocalFileSystem>,
	node_name: &str,
	min_leaves: u64,
) -> Result<(), anyhow::Error> {
	use codec::Decode;

	let node = network.get_node(node_name)?;
	let client: OnlineClient<PolkadotConfig> = node.wait_client().await?;

	// Get latest block hash
	let latest_block = client.blocks().at_latest().await?;
	let block_hash = latest_block.hash();

	// Call MmrApi_mmr_leaf_count to get the number of leaves
	let leaf_count_bytes = client
		.runtime_api()
		.at(block_hash)
		.call_raw("MmrApi_mmr_leaf_count", None)
		.await?;

	let leaf_count_result: Result<u64, String> = Decode::decode(&mut &leaf_count_bytes[..])
		.map_err(|e| anyhow!("Failed to decode MMR leaf count: {}", e))?;

	let leaves_count = leaf_count_result.map_err(|e| anyhow!("MMR leaf count error: {}", e))?;

	log::info!("MMR leaves count: {}", leaves_count);

	if leaves_count < min_leaves {
		return Err(anyhow!(
			"MMR leaves count {} is less than minimum {}",
			leaves_count,
			min_leaves
		));
	}

	Ok(())
}

/// Generate and verify MMR proofs across all validators
async fn verify_mmr_proofs(
	network: &Network<zombienet_sdk::LocalFileSystem>,
	node_name: &str,
	validator_names: &[&str],
) -> Result<(), anyhow::Error> {
	let node = network.get_node(node_name)?;
	let client: OnlineClient<PolkadotConfig> = node.wait_client().await?;
	let rpc_client = node.rpc().await?;

	// Get block hash at height 21 by subscribing to finalized blocks
	let block_21 = 21u32;
	let mut blocks_sub = client.blocks().subscribe_finalized().await?;
	let at_block_hash = loop {
		if let Some(block) = blocks_sub.next().await {
			let block = block?;
			if block.number() >= block_21 {
				break block.hash();
			}
		}
	};

	log::info!("Testing MMR at block 21: {:?}", at_block_hash);

	// Get MMR root using RPC
	let mut root_params = RpcParams::new();
	root_params.push(format!("{at_block_hash:?}"))?;
	let root: String = rpc_client.request("mmr_root", root_params).await?;
	log::info!("MMR root at block 21: {}", root);

	// Generate proof using RPC
	let mut proof_params = RpcParams::new();
	proof_params.push(vec![1u32, 9, 20])?;
	proof_params.push(Some(block_21))?;
	proof_params.push(format!("{at_block_hash:?}"))?;
	let proof: LeavesProof = rpc_client.request("mmr_generateProof", proof_params).await?;
	log::info!("Generated MMR proof at block hash: {}", proof.block_hash);

	// Verify proof on all validators (stateful)
	for &validator in validator_names {
		let val_node = network.get_node(validator)?;
		let val_rpc = val_node.rpc().await?;

		let mut verify_params = RpcParams::new();
		verify_params.push(&proof)?;
		let verify_result: bool = val_rpc.request("mmr_verifyProof", verify_params).await?;

		if !verify_result {
			return Err(anyhow!("MMR proof verification failed on {}", validator));
		}
		log::info!("MMR proof verified (stateful) on {}", validator);
	}

	// Verify proof on all validators (stateless)
	for &validator in validator_names {
		let val_node = network.get_node(validator)?;
		let val_rpc = val_node.rpc().await?;

		let mut verify_stateless_params = RpcParams::new();
		verify_stateless_params.push(root.clone())?;
		verify_stateless_params.push(&proof)?;
		let verify_result: bool =
			val_rpc.request("mmr_verifyProofStateless", verify_stateless_params).await?;

		if !verify_result {
			return Err(anyhow!("MMR proof verification (stateless) failed on {}", validator));
		}
		log::info!("MMR proof verified (stateless) on {}", validator);
	}

	log::info!("All MMR proofs verified successfully");

	Ok(())
}

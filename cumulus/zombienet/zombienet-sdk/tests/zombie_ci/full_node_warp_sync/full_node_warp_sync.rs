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
use std::time::Duration;

use polkadot_primitives::Id as ParaId;

use crate::{
	utils::{initialize_network, BEST_BLOCK_METRIC},
	zombie_ci::full_node_warp_sync::common::{
		add_relaychain_node, build_network_config, PARA_BEST_BLOCK_TO_WAIT_FOR, PARA_ID,
	},
};
use cumulus_zombienet_sdk_helpers::assert_para_is_registered;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkNode,
};

pub const RELAY_BEST_BLOCK_TO_WAIT_FOR: f64 = 70.0;

// Asserting Warp sync requires at least sync=debug level
async fn assert_warp_sync(node: &NetworkNode) -> Result<(), anyhow::Error> {
	let option_1_line = LogLineCountOptions::new(|n| n == 1, Duration::from_secs(20), false);

	log::info!("Asserting Warp sync for node {}", node.name());
	// We are interested only in Relaychain Warp sync (relaychain and parachain nodes),
	// thus exclude exclude lines containing "[Parachain]"
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Started warp sync with [0-9]+ peers",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync is not started"));
	}
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting import of [0-9]+ blocks.*\(origin: WarpSync\)",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync block import is not started"));
	}
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Imported [0-9]+ out of [0-9]+ blocks.*\(origin: WarpSync\)",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync block import is not progressing"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Warp sync is complete",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync is not complete"));
	}

	Ok(())
}

// Asserting Gap sync requires at least sync=debug level
async fn assert_gap_sync(node: &NetworkNode) -> Result<(), anyhow::Error> {
	let option_1_line = LogLineCountOptions::new(|n| n == 1, Duration::from_secs(20), false);
	let option_at_least_5_lines =
		LogLineCountOptions::new(|n| n >= 5, Duration::from_secs(20), false);

	log::info!("Asserting Gap sync for node {}", node.name());
	// We are interested only in Relaychain Gap sync (relaychain and parachain nodes),
	// thus exclude exclude lines containing "[Parachain]"
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting gap sync",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync not started"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting import of [0-9]+ blocks.*\(origin: GapSync\)",
			false,
			option_at_least_5_lines.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync block imports are not started"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Imported [0-9]+ out of [0-9]+ blocks.*\(origin: GapSync\)",
			false,
			option_at_least_5_lines,
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync block imports are not progressing"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Block history download is complete",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync is not complete"));
	}

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn full_node_warp_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config(true).await?;
	let mut network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID), 10).await?;

	// Assert warp and gap syncs only for relaychain.
	// "five" is not warp syncing the relaychain
	for name in ["dave", "eve", "four"] {
		assert_warp_sync(network.get_node(name)?).await?;
		assert_gap_sync(network.get_node(name)?).await?;
	}

	// Check relaychain progress
	for name in ["dave", "eve"] {
		log::info!("Checking full node {name} is syncing");
		network
			.get_node(name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|b| b >= RELAY_BEST_BLOCK_TO_WAIT_FOR,
				225u64,
			)
			.await?;
	}

	// Check parachain progress
	for name in ["one", "two", "three", "four", "five"] {
		log::info!("Checking full node {name} is syncing");
		network
			.get_node(name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|b| b >= PARA_BEST_BLOCK_TO_WAIT_FOR,
				225u64,
			)
			.await?;
	}

	// Pause some nodes (to prevent below added nodes syncing from them)
	for name in ["alice", "bob", "one", "two"] {
		log::info!("Pausing node {name}");
		network.get_node(name)?.pause().await?;
	}

	// Add ferdie dynamically
	log::info!("Adding ferdie to the network");
	add_relaychain_node(&mut network, "ferdie", true).await?;

	log::info!("Waiting for ferdie to be up");
	network.get_node("ferdie")?.wait_until_is_up(60u64).await?;

	// Assert warp and gap sync for ferdie
	assert_warp_sync(network.get_node("ferdie")?).await?;
	assert_gap_sync(network.get_node("ferdie")?).await?;

	// Check progress for ferdie
	log::info!("Checking full node ferdie  is syncing");
	network
		.get_node("ferdie")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= RELAY_BEST_BLOCK_TO_WAIT_FOR, 225u64)
		.await?;

	Ok(())
}

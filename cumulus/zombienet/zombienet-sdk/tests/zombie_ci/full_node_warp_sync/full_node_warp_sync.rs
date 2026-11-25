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

use anyhow::{anyhow, bail};
use std::time::Duration;

use polkadot_primitives::Id as ParaId;

use crate::{
	utils::{initialize_network, BEST_BLOCK_METRIC},
	zombie_ci::full_node_warp_sync::common::{
		add_parachain_node, build_network_config, PARA_BEST_BLOCK_TO_WAIT_FOR, PARA_ID,
	},
};
use cumulus_zombienet_sdk_helpers::assert_para_is_registered;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkNode,
};

pub const RELAY_BEST_BLOCK_TO_WAIT_FOR: f64 = 70.0;

// Generous to accommodate slow CI machines, the wait ends as soon as the line is logged.
const SYNC_LOG_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const GAP_SYNC_DONE: &str = "Block history download is complete";

/// Waits until the parachain of `node` completed gap sync, then returns its parachain lines.
///
/// Gap sync starts after warp and state sync, so all lines of the three are logged by then.
async fn parachain_log_after_gap_sync(node: &NetworkNode) -> Result<Vec<String>, anyhow::Error> {
	let result = node
		.wait_log_line_count_with_timeout(
			format!(r"\[Parachain\] {GAP_SYNC_DONE}"),
			false,
			LogLineCountOptions::new(|n| n >= 1, SYNC_LOG_TIMEOUT, false),
		)
		.await?;
	if !result.success() {
		bail!("{}: gap sync not complete within {SYNC_LOG_TIMEOUT:?}", node.name());
	}

	Ok(node
		.logs()
		.await?
		.lines()
		.filter(|line| line.contains("[Parachain] "))
		.map(Into::into)
		.collect())
}

/// Parses `(imported, count)` from an import queue `Imported X out of Y blocks` line.
fn imported_out_of(line: &str) -> Option<(u32, u32)> {
	let (_, rest) = line.split_once("Imported ")?;
	let (imported, rest) = rest.split_once(" out of ")?;
	let (count, _) = rest.split_once(" blocks")?;
	Some((imported.parse().ok()?, count.parse().ok()?))
}

/// Checks that blocks of `origin` were imported and that no import batch failed.
///
/// The number of batches depends on the order in which peers answer, so it is not checked.
fn assert_block_imports(node: &str, lines: &[String], origin: &str) -> Result<(), anyhow::Error> {
	let origin = format!("(origin: {origin})");
	let lines = lines.iter().filter(|line| line.contains(&origin));

	if !lines.clone().any(|line| line.contains("Starting import of ")) {
		bail!("{node}: no {origin} block import started");
	}
	let batches: Vec<_> = lines.filter_map(|line| imported_out_of(line)).collect();
	if batches.is_empty() {
		bail!("{node}: no {origin} block import finished");
	}
	if let Some((imported, count)) = batches.iter().find(|(imported, count)| imported != count) {
		bail!("{node}: only {imported} out of {count} {origin} blocks imported");
	}

	Ok(())
}

/// The parachain warp sync target comes from the relay chain, so there are no warp proofs and
/// no `WarpSync` imports, only the state of the target block is downloaded.
fn assert_warp_sync(name: &str, lines: &[String]) -> Result<(), anyhow::Error> {
	if !lines.iter().any(|line| line.contains("Warp sync is complete")) {
		bail!("{name}: warp sync not complete");
	}
	if !lines.iter().any(|line| line.contains("State sync is complete")) {
		bail!("{name}: state sync not complete");
	}

	Ok(())
}

fn assert_gap_sync(name: &str, lines: &[String], is_archive: bool) -> Result<(), anyhow::Error> {
	if !lines.iter().any(|line| line.contains("Starting gap sync ")) {
		bail!("{name}: gap sync not started");
	}
	assert_block_imports(name, lines, "GapSync")?;

	// Archive nodes download bodies, the others only headers and justifications. Every stats
	// line and the final summary have to agree with that.
	for line in lines
		.iter()
		.filter(|line| line.contains("Gap sync cumulative stats: ") || line.contains(GAP_SYNC_DONE))
	{
		let body_bytes: u64 = line
			.split_once("body: ")
			.and_then(|(_, rest)| rest.split_once(" B"))
			.and_then(|(bytes, _)| bytes.parse().ok())
			.ok_or_else(|| anyhow!("{name}: no body size in gap sync stats: {line}"))?;
		if (body_bytes > 0) != is_archive {
			bail!("{name}: archive mode is {is_archive}, but gap sync stats are: {line}");
		}
	}

	Ok(())
}

// Asserting Gap sync requires at least sync=trace level
async fn assert_warp_and_gap_sync(
	node: &NetworkNode,
	is_archive: bool,
) -> Result<(), anyhow::Error> {
	let name = node.name();
	log::info!("Asserting parachain Warp and Gap sync for node {name}");

	let lines = parachain_log_after_gap_sync(node).await?;
	assert_warp_sync(name, &lines)?;
	assert_gap_sync(name, &lines, is_archive)
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

	// "four" and "five" warp sync the parachain, the other collators start from the snapshot.
	// Both use the default blocks pruning (archive-canonical).
	for name in ["four", "five"] {
		assert_warp_and_gap_sync(network.get_node(name)?, true).await?;
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

	// Add ferdie dynamically, it has to sync the parachain from the warp synced nodes
	log::info!("Adding ferdie to the network");
	add_parachain_node(&mut network, "ferdie").await?;

	log::info!("Waiting for ferdie to be up");
	network.get_node("ferdie")?.wait_until_is_up(60u64).await?;

	// Non-archive (--blocks-pruning 256), so gap sync downloads headers only
	assert_warp_and_gap_sync(network.get_node("ferdie")?, false).await?;

	log::info!("Checking full node ferdie is syncing");
	network
		.get_node("ferdie")?
		.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= PARA_BEST_BLOCK_TO_WAIT_FOR, 225u64)
		.await?;

	Ok(())
}

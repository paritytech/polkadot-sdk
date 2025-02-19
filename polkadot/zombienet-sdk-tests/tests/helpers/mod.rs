// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use polkadot_primitives::Id as ParaId;
use std::{collections::HashMap, ops::Range};
use subxt::{OnlineClient, PolkadotConfig};
use tokio::time::{sleep, Duration};

#[subxt::subxt(runtime_metadata_path = "metadata-files/rococo-local.scale")]
pub mod rococo {}

// Helper function for asserting the throughput of parachains (total number of backed candidates in
// a window of relay chain blocks), after the first session change.
pub async fn assert_para_throughput(
	relay_client: &OnlineClient<PolkadotConfig>,
	stop_at: u32,
	expected_candidate_ranges: HashMap<ParaId, Range<u32>>,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut candidate_count: HashMap<ParaId, u32> = HashMap::new();
	let mut current_block_count = 0;
	let mut had_first_session_change = false;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());
		let events = block.events().await?;
		let is_session_change = events.has::<rococo::session::events::NewSession>()?;

		if !had_first_session_change && is_session_change {
			had_first_session_change = true;
		}

		if had_first_session_change && !is_session_change {
			current_block_count += 1;

			for event in events.find::<rococo::para_inclusion::events::CandidateBacked>() {
				*(candidate_count.entry(event?.0.descriptor.para_id.0.into()).or_default()) += 1;
			}
		}

		if current_block_count == stop_at {
			break;
		}
	}

	log::info!(
		"Reached {} finalized relay chain blocks that contain backed candidates. The per-parachain distribution is: {:#?}",
		stop_at,
		candidate_count
	);

	for (para_id, expected_candidate_range) in expected_candidate_ranges {
		let actual = candidate_count
			.get(&para_id)
			.expect("ParaId did not have any backed candidates");
		assert!(
			expected_candidate_range.contains(actual),
			"Candidate count {actual} not within range {expected_candidate_range:?}"
		);
	}

	Ok(())
}

// Helper function for retrieving the latest finalized block height and asserting it's within a
// range.
pub async fn assert_finalized_block_height(
	client: &OnlineClient<PolkadotConfig>,
	expected_range: Range<u32>,
) -> Result<(), anyhow::Error> {
	if let Some(block) = client.blocks().subscribe_finalized().await?.next().await {
		let height = block?.number();
		log::info!("Finalized block number {height}");

		assert!(
			expected_range.contains(&height),
			"Finalized block number {height} not within range {expected_range:?}"
		);
	}
	Ok(())
}

/// Assert that finality has not stalled.
pub async fn assert_blocks_are_being_finalized(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let mut finalized_blocks = client.blocks().subscribe_finalized().await?;
	let first_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();
	sleep(Duration::from_secs(12)).await;
	let second_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();

	assert!(second_measurement > first_measurement);

	Ok(())
}

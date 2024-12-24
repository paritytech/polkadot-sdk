// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use super::rococo;
use std::{collections::HashMap, ops::Range};
use subxt::{OnlineClient, PolkadotConfig};

// Helper function for asserting the throughput of parachains (total number of backed candidates in
// a window of relay chain blocks), after the first session change.
pub async fn assert_para_throughput(
	relay_client: &OnlineClient<PolkadotConfig>,
	stop_at: u32,
	expected_candidate_ranges: HashMap<u32, Range<u32>>,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut candidate_count: HashMap<u32, u32> = HashMap::new();
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
				*(candidate_count.entry(event?.0.descriptor.para_id.0).or_default()) += 1;
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

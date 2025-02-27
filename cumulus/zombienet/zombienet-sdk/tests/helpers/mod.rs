// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use codec::Decode;
use polkadot_primitives::{vstaging::CandidateReceiptV2, Id as ParaId};
use std::{collections::HashMap, ops::Range};
use subxt::{
	blocks::Block, events::Events, ext::scale_value::value, tx::DynamicPayload, utils::H256,
	OnlineClient, PolkadotConfig,
};
use tokio::join;

// Maximum number of blocks to wait for a session change.
// If it does not arrive for whatever reason, we should not wait forever.
const WAIT_MAX_BLOCKS_FOR_SESSION: u32 = 50;

/// Create a batch call to assign cores to a parachain.
pub fn create_assign_core_call(cores: &[u32], para_id: u32) -> DynamicPayload {
	let mut assign_cores = vec![];
	for core in cores.iter() {
		assign_cores.push(value! {
			Coretime(assign_core { core : *core, begin: 0, assignment: ((Task(para_id), 57600)), end_hint: None() })
		});
	}

	subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: assign_cores })
		}],
	)
}

/// Find an event in subxt `Events` and attempt to decode the fields fo the event.
fn find_event_and_decode_fields<T: Decode>(
	events: &Events<PolkadotConfig>,
	pallet: &str,
	variant: &str,
) -> Result<Vec<T>, anyhow::Error> {
	let mut result = vec![];
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == variant {
			let field_bytes = event.field_bytes().to_vec();
			result.push(T::decode(&mut &field_bytes[..])?);
		}
	}
	Ok(result)
}

// Helper function for asserting the throughput of parachains (total number of backed candidates in
// a window of relay chain blocks), after the first session change.
// Blocks with session changes are generally ignores.
pub async fn assert_para_throughput(
	relay_client: &OnlineClient<PolkadotConfig>,
	stop_after: u32,
	expected_candidate_ranges: HashMap<ParaId, Range<u32>>,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	let mut candidate_count: HashMap<ParaId, u32> = HashMap::new();
	let mut current_block_count = 0;

	let valid_para_ids: Vec<ParaId> = expected_candidate_ranges.keys().cloned().collect();

	// Wait for the first session, block production on the parachain will start after that.
	wait_for_first_session_change(&mut blocks_sub).await?;

	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());
		let events = block.events().await?;
		let is_session_change = events.iter().any(|event| {
			event.as_ref().is_ok_and(|event| {
				event.pallet_name() == "Session" && event.variant_name() == "NewSession"
			})
		});

		// Do not count blocks with session changes, no backed blocks there.
		if is_session_change {
			continue
		}

		current_block_count += 1;

		let receipts = find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
			&events,
			"ParaInclusion",
			"CandidateBacked",
		)?;

		for receipt in receipts {
			let para_id = receipt.descriptor.para_id();
			log::debug!("Block backed for para_id {para_id}");
			if !valid_para_ids.contains(&para_id) {
				return Err(anyhow!("Invalid ParaId detected: {}", para_id));
			};
			*(candidate_count.entry(para_id).or_default()) += 1;
		}

		if current_block_count == stop_after {
			break;
		}
	}

	log::info!(
		"Reached {} finalized relay chain blocks that contain backed candidates. The per-parachain distribution is: {:#?}",
		stop_after,
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

/// Wait for the first block with a session change.
///
/// The session change is detected by inspecting the events in the block.
async fn wait_for_first_session_change(
	blocks_sub: &mut subxt::backend::StreamOfResults<
		Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	>,
) -> Result<(), anyhow::Error> {
	let mut waited_block_num = 0;
	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		log::debug!("Finalized relay chain block {}", block.number());
		let events = block.events().await?;
		let is_session_change = events.iter().any(|event| {
			event.as_ref().is_ok_and(|event| {
				event.pallet_name() == "Session" && event.variant_name() == "NewSession"
			})
		});

		if is_session_change {
			return Ok(())
		}

		if waited_block_num >= WAIT_MAX_BLOCKS_FOR_SESSION {
			return Err(anyhow::format_err!("Waited for {WAIT_MAX_BLOCKS_FOR_SESSION}, a new session should have been arrived by now."));
		}

		waited_block_num += 1;
	}
	Ok(())
}

// Helper function that asserts the maximum finality lag.
pub async fn assert_finality_lag(
	client: &OnlineClient<PolkadotConfig>,
	maximum_lag: u32,
) -> Result<(), anyhow::Error> {
	let mut best_stream = client.blocks().subscribe_best().await?;
	let mut fut_stream = client.blocks().subscribe_finalized().await?;
	let (Some(Ok(best)), Some(Ok(finalized))) = join!(best_stream.next(), fut_stream.next()) else {
		return Err(anyhow::format_err!("Unable to fetch best an finalized block!"));
	};
	let finality_lag = best.number() - finalized.number();
	assert!(finality_lag <= maximum_lag, "Expected finality to lag by a maximum of {maximum_lag} blocks, but was lagging by {finality_lag} blocks.");
	Ok(())
}

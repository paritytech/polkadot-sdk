// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use codec::{Compact, Decode};
use cumulus_primitives_core::{relay_chain, rpsr_digest::RPSR_CONSENSUS_ID};
use futures::stream::StreamExt;
use polkadot_primitives::{vstaging::CandidateReceiptV2, Id as ParaId};
use std::{
	cmp::max,
	collections::{HashMap, HashSet},
	ops::Range,
};
use tokio::{
	join,
	time::{sleep, Duration},
};
use zombienet_sdk::subxt::{
	blocks::Block, config::substrate::DigestItem, events::Events, ext::scale_value::value,
	tx::DynamicPayload, utils::H256, OnlineClient, PolkadotConfig,
};

// Maximum number of blocks to wait for a session change.
// If it does not arrive for whatever reason, we should not wait forever.
const WAIT_MAX_BLOCKS_FOR_SESSION: u32 = 50;

/// Create a batch call to assign cores to a parachain.
pub fn create_assign_core_call(core_and_para: &[(u32, u32)]) -> DynamicPayload {
	let mut assign_cores = vec![];
	for (core, para_id) in core_and_para.iter() {
		assign_cores.push(value! {
			Coretime(assign_core { core : *core, begin: 0, assignment: ((Task(*para_id), 57600)), end_hint: None() })
		});
	}

	zombienet_sdk::subxt::tx::dynamic(
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
pub async fn assert_finalized_para_throughput(
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
// Helper function for asserting the throughput of parachains (total number of backed candidates in
// a window of relay chain blocks), after the first session change.
// Blocks with session changes are generally ignores.
pub async fn assert_para_throughput(
	relay_client: &OnlineClient<PolkadotConfig>,
	stop_after: u32,
	expected_candidate_ranges: HashMap<ParaId, Range<u32>>,
) -> Result<(), anyhow::Error> {
	// Check on backed blocks in all imported relay chain blocks. The slot-based collator
	// builds on the best fork currently. It can happen that it builds on a fork which is not
	// getting finalized, in which case we will lose some blocks. This makes it harder to build
	// stable asserts. Once we are building on older relay parents, this can be changed to
	// finalized blocks again.
	let mut blocks_sub = relay_client.blocks().subscribe_all().await?;
	let mut candidate_count: HashMap<ParaId, (u32, u32)> = HashMap::new();
	let mut start_height: Option<u32> = None;

	let valid_para_ids: Vec<ParaId> = expected_candidate_ranges.keys().cloned().collect();

	// Wait for the first session, block production on the parachain will start after that.
	wait_for_first_session_change(&mut blocks_sub).await?;

	let mut session_change_seen_at = 0u32;
	while let Some(block) = blocks_sub.next().await {
		let block = block?;
		let block_number = Into::<u32>::into(block.number());

		let events = block.events().await?;
		let mut para_ids_to_increment: HashSet<ParaId> = Default::default();
		let is_session_change = events.iter().any(|event| {
			event.as_ref().is_ok_and(|event| {
				event.pallet_name() == "Session" && event.variant_name() == "NewSession"
			})
		});

		// Do not count blocks with session changes, no backed blocks there.
		if is_session_change {
			if block_number == session_change_seen_at {
				continue;
			}

			// Increment the start height to account for a block level that has no
			// backed blocks.
			start_height = start_height.map(|h| h + 1);
			session_change_seen_at = block_number;
			continue;
		}

		let receipts = find_event_and_decode_fields::<CandidateReceiptV2<H256>>(
			&events,
			"ParaInclusion",
			"CandidateBacked",
		)?;

		for receipt in receipts {
			let para_id = receipt.descriptor.para_id();
			if !valid_para_ids.contains(&para_id) {
				return Err(anyhow!("Invalid ParaId detected: {}", para_id));
			};
			log::debug!(
				"Block backed for para_id {para_id} at relay: #{} ({})",
				block.number(),
				block.hash()
			);
			let (counter, accounted_block_height) = candidate_count.entry(para_id).or_default();
			if block_number > *accounted_block_height {
				*counter += 1;
				// Increment later to count multiple descriptors in the same block.
				para_ids_to_increment.insert(para_id);
			}
		}

		for para_id in para_ids_to_increment.iter() {
			candidate_count.entry(*para_id).or_default().1 = block_number;
		}

		if block_number - *start_height.get_or_insert_with(|| block_number - 1) >= stop_after {
			log::info!(
				"Finished condition: block_height: {:?}, start_height: {:?}",
				block.number(),
				start_height
			);
			break;
		}
	}

	log::info!(
		"Reached {} relay chain blocks that contain backed candidates. The per-parachain distribution is: {:#?}",
		stop_after,
		candidate_count
	);

	for (para_id, expected_candidate_range) in expected_candidate_ranges {
		let actual = candidate_count
			.get(&para_id)
			.expect("ParaId did not have any backed candidates");
		assert!(
			expected_candidate_range.contains(&actual.0),
			"Candidate count {} not within range {expected_candidate_range:?}",
			actual.0
		);
	}

	Ok(())
}

/// Wait for the first block with a session change.
///
/// The session change is detected by inspecting the events in the block.
pub async fn wait_for_first_session_change(
	blocks_sub: &mut zombienet_sdk::subxt::backend::StreamOfResults<
		Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	>,
) -> Result<(), anyhow::Error> {
	wait_for_nth_session_change(blocks_sub, 1).await
}

/// Wait for the first block with the Nth session change.
///
/// The session change is detected by inspecting the events in the block.
pub async fn wait_for_nth_session_change(
	blocks_sub: &mut zombienet_sdk::subxt::backend::StreamOfResults<
		Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	>,
	mut sessions_to_wait: u32,
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
			sessions_to_wait -= 1;
			if sessions_to_wait == 0 {
				return Ok(())
			}

			waited_block_num = 0;
		} else {
			if waited_block_num >= WAIT_MAX_BLOCKS_FOR_SESSION {
				return Err(anyhow::format_err!("Waited for {WAIT_MAX_BLOCKS_FOR_SESSION}, a new session should have been arrived by now."));
			}

			waited_block_num += 1;
		}
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

	log::info!(
		"Finality lagged by {finality_lag} blocks, maximum expected was {maximum_lag} blocks"
	);

	assert!(finality_lag <= maximum_lag, "Expected finality to lag by a maximum of {maximum_lag} blocks, but was lagging by {finality_lag} blocks.");
	Ok(())
}

/// Assert that finality has not stalled.
pub async fn assert_blocks_are_being_finalized(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let sleep_duration = Duration::from_secs(12);
	let mut finalized_blocks = client.blocks().subscribe_finalized().await?;
	let first_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();
	sleep(sleep_duration).await;
	let second_measurement = finalized_blocks
		.next()
		.await
		.ok_or(anyhow::anyhow!("Can't get finalized block from stream"))??
		.number();

	log::info!(
		"Finalized {} blocks within {sleep_duration:?}",
		second_measurement - first_measurement
	);
	assert!(second_measurement > first_measurement);

	Ok(())
}

/// Asserts that parachain blocks have the correct relay parent offset.
///
/// # Arguments
///
/// * `relay_client` - Client connected to a relay chain node
/// * `para_client` - Client connected to a parachain node
/// * `offset` - Expected minimum offset between relay parent and highest seen relay block
/// * `block_limit` - Number of parachain blocks to verify before completing
pub async fn assert_relay_parent_offset(
	relay_client: &OnlineClient<PolkadotConfig>,
	para_client: &OnlineClient<PolkadotConfig>,
	offset: u32,
	block_limit: u32,
) -> Result<(), anyhow::Error> {
	let mut relay_block_stream = relay_client.blocks().subscribe_all().await?;

	// First parachain header #0 does not contains RSPR digest item.
	let mut para_block_stream = para_client.blocks().subscribe_all().await?.skip(1);
	let mut highest_relay_block_seen = 0;
	let mut num_para_blocks_seen = 0;
	loop {
		tokio::select! {
			Some(Ok(relay_block)) = relay_block_stream.next() => {
				highest_relay_block_seen = max(relay_block.number(), highest_relay_block_seen);
				if highest_relay_block_seen > 15 && num_para_blocks_seen == 0 {
					return Err(anyhow!("No parachain blocks produced!"))
				}
			},
			Some(Ok(para_block)) = para_block_stream.next() => {
				let logs = &para_block.header().digest.logs;

				let Some((_, relay_parent_number)): Option<(H256, u32)> = logs.iter().find_map(extract_relay_parent_storage_root) else {
					return Err(anyhow!("No RPSR digest found in header #{}", para_block.number()));
				};
				log::debug!("Parachain block #{} was built on relay parent #{relay_parent_number}, highest seen was {highest_relay_block_seen}", para_block.number());
				assert!(highest_relay_block_seen < offset || relay_parent_number <= highest_relay_block_seen.saturating_sub(offset), "Relay parent is not at the correct offset! relay_parent: #{relay_parent_number} highest_seen_relay_block: #{highest_relay_block_seen}");
				num_para_blocks_seen += 1;
				if num_para_blocks_seen >= block_limit {
					log::info!("Successfully verified relay parent offset of {offset} for {num_para_blocks_seen} parachain blocks.");
					break;
				}
			}
		}
	}
	Ok(())
}

/// Extract relay parent information from the digest logs.
fn extract_relay_parent_storage_root(
	digest: &DigestItem,
) -> Option<(relay_chain::Hash, relay_chain::BlockNumber)> {
	match digest {
		DigestItem::Consensus(id, val) if id == &RPSR_CONSENSUS_ID => {
			let (h, n): (relay_chain::Hash, Compact<relay_chain::BlockNumber>) =
				Decode::decode(&mut &val[..]).ok()?;

			Some((h, n.0))
		},
		_ => None,
	}
}

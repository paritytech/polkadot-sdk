// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use crate::LOG_TARGET;
use codec::Codec;
use cumulus_primitives_aura::Slot;
use cumulus_primitives_core::BlockT;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_primitives::{Block as RelayBlock, Header as RelayHeader};
use sc_client_api::UsageProvider;
use sc_consensus_aura::SlotDuration;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::AppPublic;
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_runtime::traits::{Header as HeaderT, Member};
use sp_timestamp::Timestamp;
use std::{
	cmp::{max, min},
	sync::Arc,
	time::Duration,
};

use super::relay_chain_data_cache::RelayChainDataCache;

/// Lower limits of allowed block production interval.
/// Defensive mechanism, corresponds to 12 cores at 6 second block time.
const BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS: Duration = Duration::from_millis(500);

/// Theoretically, the block production is capped at `BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS`.
/// In practice, there might be slight deviations due to timing inaccuracies and delays.
///
/// This constant is taken into account while adjusting the authoring duration to fit into the slot.
/// Therefore, it will only reduce the authoring duration if we are within the
/// `BLOCK_PRODUCTION_ADJUSTMENT_MS` threshold of the next slot.
///
/// ### 12 cores 500ms blocks
///
/// For example, for 12 cores 500ms blocks: the next slot is scheduled in 490ms due to delays.
/// In that case, we still want to attempt producing the block, as missing the slot would be worse
/// than producing slightly too fast.
const BLOCK_PRODUCTION_THRESHOLD_MS: Duration = Duration::from_millis(100);

/// The amount of time the authoring duration of the last block production attempt
/// should be reduced by to fit into the slot timing.
const BLOCK_PRODUCTION_ADJUSTMENT_MS: Duration = Duration::from_millis(1000);

#[derive(Debug)]
pub(crate) struct SlotInfo {
	pub timestamp: Timestamp,
	pub slot: Slot,
}

/// Manages block-production timings based on chain parameters and assigned cores.
#[derive(Debug)]
pub(crate) struct SlotTimer<Block, Client, P> {
	/// Parachain client that is used for runtime calls
	client: Arc<Client>,
	/// Offset the current time by this duration.
	time_offset: Duration,
	/// Last reported core count.
	last_reported_core_num: Option<u32>,
	/// Slot duration of the relay chain. This is used to compute how man block-production
	/// attempts we should trigger per relay chain block.
	relay_slot_duration: Duration,
	/// Stores the latest slot that was reported by [`Self::wait_until_next_slot`].
	last_reported_slot: Option<Slot>,
	_marker: std::marker::PhantomData<(Block, Box<dyn Fn(P) + Send + Sync + 'static>)>,
}

/// Compute when to try block-authoring next.
/// The exact time point is determined by the slot duration of relay- and parachain as
/// well as the last observed core count. If more cores are available, we attempt to author blocks
/// for them.
///
/// Returns a tuple with:
/// - `Duration`: How long to wait until the next slot.
/// - `Slot`: The AURA slot used for authoring
fn compute_next_wake_up_time(
	para_slot_duration: SlotDuration,
	relay_slot_duration: Duration,
	core_count: Option<u32>,
	time_now: Duration,
	time_offset: Duration,
) -> (Duration, Slot) {
	let para_slots_per_relay_block =
		(relay_slot_duration.as_millis() / para_slot_duration.as_millis() as u128) as u32;
	let assigned_core_num = core_count.unwrap_or(1);

	// Trigger at least once per relay block, if we have for example 12 second slot duration,
	// we should still produce two blocks if we are scheduled on every relay block.
	let mut block_production_interval = min(para_slot_duration.as_duration(), relay_slot_duration);

	if assigned_core_num > para_slots_per_relay_block &&
		para_slot_duration.as_duration() >= relay_slot_duration
	{
		block_production_interval =
			max(relay_slot_duration / assigned_core_num, BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS);
		tracing::debug!(
			target: LOG_TARGET,
			?block_production_interval,
			"Expected to produce for {assigned_core_num} cores but only have {para_slots_per_relay_block} slots. Attempting to produce multiple blocks per slot."
		);
	}

	let (duration, timestamp) =
		time_until_next_attempt(time_now, block_production_interval, time_offset);
	let aura_slot = Slot::from_timestamp(timestamp, para_slot_duration);
	(duration, aura_slot)
}

/// Compute the time until the next slot changes.
///
/// Returns None if the next slot cannot be computed.
fn compute_time_until_next_slot_change(
	para_slot_duration: SlotDuration,
	time_now: Duration,
	time_offset: Duration,
	last_reported_slot: Slot,
) -> Option<(Duration, Slot)> {
	let now = time_now.saturating_sub(time_offset);
	let next_slot = last_reported_slot + Slot::from(1);

	let Some(next_slot_timestamp) = next_slot.timestamp(para_slot_duration) else {
		return None;
	};
	let remaining_time = next_slot_timestamp.as_duration().saturating_sub(now);

	Some((remaining_time, next_slot))
}

/// Returns current duration since Unix epoch.
pub(super) fn duration_now() -> Duration {
	use std::time::SystemTime;
	let now = SystemTime::now();
	now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(|e| {
		panic!("Current time {:?} is before Unix epoch. Something is wrong: {:?}", now, e)
	})
}

/// Adjust the authoring duration.
fn adjust_authoring_duration(
	mut authoring_duration: Duration,
	next_block: (Duration, Slot),
	next_slot_change: (Duration, Slot),
	different_authors: bool,
) -> Option<Duration> {
	let (duration, next_block_slot) = next_block;
	let (duration_until_next_slot, next_slot) = next_slot_change;

	// The authoring of blocks must stop 1 second before the slot ends.
	let duration_until_deadline =
		duration_until_next_slot.saturating_sub(BLOCK_PRODUCTION_ADJUSTMENT_MS);
	tracing::debug!(
		target: LOG_TARGET,
		?authoring_duration,
		?duration,
		?next_block_slot,
		?duration_until_next_slot,
		?next_slot,
		?duration_until_deadline,
		?different_authors,
		"Adjusting authoring duration for slot.",
	);

	// Ensure no blocks are produced in the last second of the slot,
	// regardless of authoring duration.
	if duration_until_deadline == Duration::ZERO {
		if different_authors {
			tracing::debug!(
				target: LOG_TARGET,
				?duration_until_next_slot,
				?next_slot,
				"Not enough time left in the slot to adjust authoring duration. Skipping block production for the slot."
			);

			return None;
		}

		// If authors are the same, we can still attempt producing the block
		// considering the next block duration.
		return Some(authoring_duration.min(duration));
	}

	// Clamp the authoring duration to fit into the slot deadline only if authors are different.
	// For most cases, the deadline is farther in the future than the authoring duration.
	if different_authors && authoring_duration >= duration_until_deadline {
		authoring_duration = duration_until_deadline;

		// Ensure we are not going below the minimum interval within a reasonable threshold.
		// For 12 cores, we might have a scenario where the last 3 blocks are skipped:
		// - Block 10: next slot change in 1.493s:
		// 	 - After adjusting the deadline: 1.493s - 1s = 0.493s the block could be produced
		//     without issues.
		// - Block 11: next slot change in 0.993s - skipped by the deadline
		// - Block 12: next slot change in 0.493s - skipped by the deadline
		if authoring_duration <
			BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS.saturating_sub(BLOCK_PRODUCTION_THRESHOLD_MS)
		{
			tracing::debug!(
				target: LOG_TARGET,
				?authoring_duration,
				?next_slot,
				"Authoring duration is below minimum. Skipping block production for the slot."
			);
			return None;
		}
	}

	// The `duration` intends to slightly adjust when then block production
	// attempt happens. This goes slightly below the `BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS`
	// threshold.
	Some(authoring_duration.min(duration))
}

/// Returns the duration until the next block production should be attempted.
/// Returns:
/// - Duration: The duration until the next attempt.
fn time_until_next_attempt(
	now: Duration,
	block_production_interval: Duration,
	offset: Duration,
) -> (Duration, Timestamp) {
	let now = now.as_millis().saturating_sub(offset.as_millis());

	let next_slot_time = ((now + block_production_interval.as_millis()) /
		block_production_interval.as_millis()) *
		block_production_interval.as_millis();
	let remaining_millis = next_slot_time - now;
	(Duration::from_millis(remaining_millis as u64), Timestamp::from(next_slot_time as u64))
}

impl<Block, Client, P> SlotTimer<Block, Client, P>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + UsageProvider<Block> + Send + Sync + 'static,
	Client::Api: AuraApi<Block, P::Public>,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	/// Create a new slot timer.
	pub fn new_with_offset(
		client: Arc<Client>,
		time_offset: Duration,
		relay_slot_duration: Duration,
	) -> Self {
		Self {
			client,
			time_offset,
			last_reported_core_num: None,
			relay_slot_duration,
			last_reported_slot: Default::default(),
			_marker: Default::default(),
		}
	}

	/// Inform the slot timer about the last seen number of cores.
	pub fn update_scheduling(&mut self, num_cores_next_block: u32) {
		self.last_reported_core_num = Some(num_cores_next_block);
	}

	/// Returns the slot and how much time left until the next block production attempt.
	pub fn time_until_next_block(&mut self, slot_duration: SlotDuration) -> (Duration, Slot) {
		compute_next_wake_up_time(
			slot_duration,
			self.relay_slot_duration,
			self.last_reported_core_num,
			duration_now(),
			self.time_offset,
		)
	}

	/// Compute the time until the next slot changes.
	fn time_until_next_slot_change(
		&mut self,
		slot_duration: SlotDuration,
	) -> Option<(Duration, Slot)> {
		compute_time_until_next_slot_change(
			slot_duration,
			duration_now(),
			self.time_offset,
			self.last_reported_slot.unwrap_or_default(),
		)
	}

	/// Check if two slots have different authors based on AURA round-robin algorithm.
	///
	/// Returns true if the authors for the two slots are different.
	fn check_different_slot_authors(&self, slot: Slot, next_slot: Slot) -> bool {
		let best_hash = self.client.usage_info().chain.best_hash;

		let mut runtime_api = self.client.runtime_api();
		runtime_api.set_call_context(sp_core::traits::CallContext::Onchain);
		let Ok(authorities) = runtime_api.authorities(best_hash) else {
			// Presume they are different, this will adjust the slot authoring duration more
			// conservatively.
			return true;
		};

		let authorities_len = authorities.len() as u64;
		if authorities_len <= 1 {
			return false;
		}

		let author1_idx = *slot % authorities_len;
		let author2_idx = *next_slot % authorities_len;

		author1_idx != author2_idx
	}

	/// Adjust the authoring duration to fit into the slot timing.
	///
	/// Returns the adjusted authoring duration and the slot that it corresponds to.
	pub fn adjust_authoring_duration(&mut self, authoring_duration: Duration) -> Option<Duration> {
		let Ok(slot_duration) = crate::slot_duration(&*self.client) else {
			tracing::error!(target: LOG_TARGET, "Failed to fetch slot duration from runtime.");
			return None;
		};

		let next_block = self.time_until_next_block(slot_duration);
		let Some(next_slot_change) = self.time_until_next_slot_change(slot_duration) else {
			tracing::error!(
				target: LOG_TARGET,
				"Failed to compute time until next slot change. Using unadjusted authoring duration."
			);
			return Some(authoring_duration);
		};

		// Check if authors at current and next slots are different
		let current_slot = self.last_reported_slot.unwrap_or(next_block.1);
		let different_authors = self.check_different_slot_authors(current_slot, next_slot_change.1);

		adjust_authoring_duration(
			authoring_duration,
			next_block,
			next_slot_change,
			different_authors,
		)
	}

	/// Returns a future that resolves when the next block production should be attempted
	/// and the relay chain has a current block available.
	///
	/// After determining the correct slot timing, this method also waits for the
	/// best relay chain block to be from the current relay chain slot. This ensures
	/// the collator doesn't build on a stale relay parent when relay block
	/// propagation exceeds the configured time offset at a slot boundary.
	///
	/// Returns:
	/// - `Ok(header)` — ready to build, with the fresh relay header
	/// - `Err(())` — fatal error (e.g. cannot fetch slot duration, relay chain connection lost,
	///   notification stream closed)
	pub async fn wait_until_next_slot<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		best_notifications: &mut (impl Stream<Item = RelayHeader> + Unpin),
	) -> Result<RelayHeader, ()>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		let slot_duration = match crate::slot_duration(&*self.client) {
			Ok(d) => d,
			Err(error) => {
				tracing::error!(target: LOG_TARGET, %error, "Failed to fetch slot duration from runtime.");
				return Err(());
			},
		};

		let (time_until_next_attempt, mut next_aura_slot) =
			self.time_until_next_block(slot_duration);

		tracing::trace!(
			target: LOG_TARGET,
			?time_until_next_attempt,
			aura_slot = ?next_aura_slot,
			last_reported = ?self.last_reported_slot,
			"Determined next block production opportunity."
		);

		match self.last_reported_slot {
			// If we already reported a slot, we don't want to skip a slot. But we also don't want
			// to go through all the slots if a node was halted for some reason.
			Some(ls) if ls + 1 < next_aura_slot && next_aura_slot <= ls + 3 => {
				next_aura_slot = ls + 1u64;
			},
			None | Some(_) => {
				tracing::trace!(target: LOG_TARGET, ?time_until_next_attempt, "Sleeping until the next slot.");
				tokio::time::sleep(time_until_next_attempt).await;
			},
		}

		tracing::debug!(
			target: LOG_TARGET,
			?slot_duration,
			aura_slot = ?next_aura_slot,
			"New block production opportunity."
		);

		self.last_reported_slot = Some(next_aura_slot);

		// Wait for the best relay block to be from the current relay
		// chain slot. If propagation exceeded `slot_offset`, this
		// blocks until a new-best notification arrives.
		// See: https://github.com/paritytech/polkadot-sdk/pull/11453
		wait_for_current_relay_block(
			relay_client,
			relay_chain_data_cache,
			best_notifications,
			self.time_offset,
			self.relay_slot_duration,
		)
		.await
		.ok_or(())
	}
}

/// Returns `true` if the best relay chain block is from the current relay chain
/// slot. Uses the wall clock adjusted by `slot_offset`.
fn is_best_relay_block_current(
	best_relay_slot: u64,
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> bool {
	let now = duration_now().saturating_sub(slot_offset);
	is_best_relay_block_current_at(best_relay_slot, now, relay_chain_slot_duration)
}

/// Pure logic for the relay block freshness check, taking the current time as
/// a parameter for testability.
fn is_best_relay_block_current_at(
	best_relay_slot: u64,
	now: Duration,
	relay_chain_slot_duration: Duration,
) -> bool {
	let current_relay_slot = now.as_millis() as u64 / relay_chain_slot_duration.as_millis() as u64;
	best_relay_slot >= current_relay_slot
}

/// Wait until the best relay chain block is from the current relay chain slot.
///
/// If the current best block is already current, returns its hash immediately.
/// Otherwise waits for a new-best notification and re-checks. This ensures
/// the collator doesn't build on a stale relay parent when relay block
/// propagation exceeds `slot_offset` at a slot boundary.
///
/// Returns the best relay block header, or `None` on error.
pub(super) async fn wait_for_current_relay_block<RelayClient>(
	relay_client: &RelayClient,
	relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
	best_notifications: &mut (impl Stream<Item = RelayHeader> + Unpin),
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> Option<RelayHeader>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let relay_best_hash = relay_client.best_block_hash().await.ok()?;
	let mut first_best_header = Some(
		relay_chain_data_cache
			.get_mut_relay_chain_data(relay_best_hash)
			.await
			.ok()
			.map(|d| d.relay_parent_header.clone())?,
	);

	loop {
		// Drain buffered notifications.
		while let Some(maybe_header) = best_notifications.next().now_or_never() {
			first_best_header = Some(maybe_header?);
		}

		let best_header = match first_best_header.take() {
			Some(h) => h,
			None => best_notifications.next().await?, // Block until one arrives.
		};

		let best_slot = sc_consensus_babe::find_pre_digest::<RelayBlock>(&best_header)
			.map(|d| d.slot())
			.ok()?;

		if is_best_relay_block_current(*best_slot, slot_offset, relay_chain_slot_duration) {
			return Some(best_header);
		}

		tracing::debug!(
			target: LOG_TARGET,
			?relay_best_hash,
			relay_best_num = %best_header.number(),
			?best_slot,
			"Best relay block is stale, waiting for fresh one."
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use sc_consensus_aura::SlotDuration;
	const RELAY_CHAIN_SLOT_DURATION: u64 = 6000;

	#[rstest]
	// Test that different now timestamps have correct impact
	//                    ||||
	#[case(6000, Some(1), 1000, 0, 5000)]
	#[case(6000, Some(1), 0, 0, 6000)]
	#[case(6000, Some(1), 6000, 0, 6000)]
	#[case(6000, Some(0), 6000, 0, 6000)]
	// Test that `None` core defaults to 1
	//           ||||
	#[case(6000, None, 1000, 0, 5000)]
	#[case(6000, None, 0, 0, 6000)]
	#[case(6000, None, 6000, 0, 6000)]
	// Test that offset affects the current time correctly
	//                          ||||
	#[case(6000, Some(1), 1000, 1000, 6000)]
	#[case(6000, Some(1), 12000, 2000, 2000)]
	#[case(6000, Some(1), 12000, 6000, 6000)]
	#[case(6000, Some(1), 12000, 7000, 1000)]
	// Test that number of cores affects the block production interval
	//           |||||||
	#[case(6000, Some(3), 12000, 0, 2000)]
	#[case(6000, Some(2), 12000, 0, 3000)]
	#[case(6000, Some(3), 11999, 0, 1)]
	// High core count
	//           ||||||||
	#[case(6000, Some(12), 0, 0, 500)]
	/// Test that the minimum block interval is respected
	/// at high core counts.
	///          |||||||||
	#[case(6000, Some(100), 0, 0, 500)]
	// Test that slot_duration works correctly
	//     ||||
	#[case(2000, Some(1), 1000, 0, 1000)]
	#[case(2000, Some(1), 3000, 0, 1000)]
	#[case(2000, Some(1), 10000, 0, 2000)]
	#[case(2000, Some(2), 1000, 0, 1000)]
	// Cores are ignored if relay_slot_duration != para_slot_duration
	//           |||||||
	#[case(2000, Some(3), 3000, 0, 1000)]
	// For long slot durations, we should still check
	// every relay chain block for the slot.
	//     |||||
	#[case(12000, None, 0, 0, 6000)]
	#[case(12000, None, 6100, 0, 5900)]
	#[case(12000, None, 6000, 2000, 2000)]
	#[case(12000, Some(2), 6000, 0, 3000)]
	#[case(12000, Some(3), 6000, 0, 2000)]
	#[case(12000, Some(3), 8100, 0, 1900)]
	fn test_get_next_slot(
		#[case] para_slot_millis: u64,
		#[case] core_count: Option<u32>,
		#[case] time_now: u64,
		#[case] offset_millis: u64,
		#[case] expected_wait_duration: u128,
	) {
		let para_slot_duration = SlotDuration::from_millis(para_slot_millis); // 6 second slots
		let relay_slot_duration = Duration::from_millis(RELAY_CHAIN_SLOT_DURATION);
		let time_now = Duration::from_millis(time_now); // 1 second passed
		let offset = Duration::from_millis(offset_millis);

		let (wait_duration, _) = compute_next_wake_up_time(
			para_slot_duration,
			relay_slot_duration,
			core_count,
			time_now,
			offset,
		);

		assert_eq!(wait_duration.as_millis(), expected_wait_duration, "Wait time mismatch.");
		// Should wait 5 seconds
	}

	#[rstest]
	// Basic slot change scenarios
	#[case(6000, 0, 0, Slot::from(0), 6000, Slot::from(1))]
	#[case(6000, 1000, 0, Slot::from(0), 5000, Slot::from(1))]
	#[case(6000, 6000, 0, Slot::from(1), 6000, Slot::from(2))]
	#[case(6000, 12000, 0, Slot::from(2), 6000, Slot::from(3))]
	// Test with offset
	#[case(6000, 1000, 1000, Slot::from(0), 6000, Slot::from(1))]
	#[case(6000, 2000, 1000, Slot::from(0), 5000, Slot::from(1))]
	#[case(6000, 6000, 3000, Slot::from(0), 3000, Slot::from(1))]
	// Different slot durations
	#[case(3000, 1000, 0, Slot::from(0), 2000, Slot::from(1))]
	#[case(3000, 3000, 0, Slot::from(1), 3000, Slot::from(2))]
	#[case(12000, 6000, 0, Slot::from(0), 6000, Slot::from(1))]
	#[case(12000, 12000, 0, Slot::from(1), 12000, Slot::from(2))]
	// Edge cases - at slot boundary
	#[case(6000, 5999, 0, Slot::from(0), 1, Slot::from(1))]
	#[case(6000, 11999, 0, Slot::from(1), 1, Slot::from(2))]
	fn test_compute_time_until_next_slot_change(
		#[case] para_slot_millis: u64,
		#[case] time_now: u64,
		#[case] offset_millis: u64,
		#[case] last_reported_slot: Slot,
		#[case] expected_duration: u128,
		#[case] expected_next_slot: Slot,
	) {
		let para_slot_duration = SlotDuration::from_millis(para_slot_millis);
		let time_now = Duration::from_millis(time_now);
		let offset = Duration::from_millis(offset_millis);

		let result = compute_time_until_next_slot_change(
			para_slot_duration,
			time_now,
			offset,
			last_reported_slot,
		);

		assert!(result.is_some(), "Expected result to be Some");
		let (duration, next_slot) = result.unwrap();
		assert_eq!(duration.as_millis(), expected_duration, "Duration mismatch");
		assert_eq!(next_slot, expected_next_slot, "Next slot mismatch");
	}

	#[rstest]
	// Various scenarios for 2s block production adjustment.
	#[case::blocks_2s_fits_next_block(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(2000), Slot::from(1)), // Next block
		(Duration::from_millis(4000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(2000)), // Expected
	)]
	#[case::blocks_2s_closer_next_slot(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1950), Slot::from(1)), // Next block
		(Duration::from_millis(4000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(1950)), // Expected
	)]
	#[case::blocks_2s_closer_next_slot_bigger(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1500), Slot::from(1)), // Next block
		(Duration::from_millis(4000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(1500)), // Expected
	)]
	#[case::blocks_2s_reduce_by_1s(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(2000), Slot::from(1)), // Next block
		(Duration::from_millis(2000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(1000)), // Expected
	)]
	#[case::blocks_2s_reduce_by_1s_plus_offset(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1950), Slot::from(1)), // Next block
		(Duration::from_millis(1950), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(950)), // Expected
	)]
	#[case::blocks_2s_reduce_to_minimum(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1400), Slot::from(1)), // Next block
		(Duration::from_millis(1400), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(400)), // Expected
	)]
	#[case::blocks_2s_reduce_below_minimum(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1300), Slot::from(1)), // Next block
		(Duration::from_millis(1300), Slot::from(2)), // Next slot change
		true, // Different authors
		None, // Expected to reduce below minimum
	)]
	#[case::blocks_2s_same_author(
		Duration::from_millis(2000), // Authoring duration
		(Duration::from_millis(1400), Slot::from(1)), // Next block
		(Duration::from_millis(1400), Slot::from(2)), // Next slot change
		false, // Different authors
		Some(Duration::from_millis(1400)), // Expected no adjustment for last second.
	)]
	// Various scenarios for 500ms block production adjustment.
	#[case::blocks_500ms_fits_next_block(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(500), Slot::from(1)), // Next block
		(Duration::from_millis(2000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(500)), // Expected
	)]
	#[case::blocks_500ms_closer_next_slot(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(450), Slot::from(1)), // Next block
		(Duration::from_millis(2000), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(450)), // Expected
	)]
	#[case::blocks_500ms_closer_next_slot_bigger(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(400), Slot::from(1)), // Next block
		(Duration::from_millis(1500), Slot::from(2)), // Next slot change
		true, // Different authors
		Some(Duration::from_millis(400)), // Expected
	)]
	#[case::blocks_500ms_reduce_by_1s(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(500), Slot::from(1)), // Next block
		(Duration::from_millis(1000), Slot::from(2)), // Next slot change
		true, // Different authors
		None, // Expected
	)]
	#[case::blocks_500ms_reduce_by_1s_closer(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(500), Slot::from(1)), // Next block
		(Duration::from_millis(500), Slot::from(2)), // Next slot change
		true, // Different authors
		None, // Expected
	)]
	// If we are producing with 1 collator for 500ms authoring duration,
	// we must produce the last two slots and ignore the 1s adjustment.
	#[case::blocks_500ms_same_author(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(410), Slot::from(1)), // Next block
		(Duration::from_millis(1000), Slot::from(2)), // Next slot change
		false, // Different authors
		Some(Duration::from_millis(410)), // Expected no adjustment for last second.
	)]
	#[case::blocks_500ms_same_author_closer(
		Duration::from_millis(500), // Authoring duration
		(Duration::from_millis(400), Slot::from(1)), // Next block
		(Duration::from_millis(400), Slot::from(2)), // Next slot change
		false, // Different authors
		Some(Duration::from_millis(400)), // Expected no adjustment for last second.
	)]
	fn test_adjust_authoring_duration(
		#[case] authoring_duration: Duration,
		#[case] next_block: (Duration, Slot),
		#[case] next_slot_change: (Duration, Slot),
		#[case] different_authors: bool,
		#[case] expected: Option<Duration>,
	) {
		sp_tracing::init_for_tests();

		let result = adjust_authoring_duration(
			authoring_duration,
			next_block,
			next_slot_change,
			different_authors,
		);
		tracing::debug!("Adjusted authoring duration: {:?}", result);
		assert_eq!(result, expected);
	}

	const RELAY_SLOT_DURATION: Duration = Duration::from_secs(6);

	/// Simulate the wall clock at a specific point within a relay slot.
	///
	/// `relay_slot` is the current relay chain slot number, `ms_into_slot` is
	/// how far into that slot we are (0..6000).
	fn now_at(relay_slot: u64, ms_into_slot: u64) -> Duration {
		Duration::from_millis(relay_slot * 6000 + ms_into_slot)
	}

	// ---------------------------------------------------------------
	// Tests for `is_best_relay_block_current_at`
	// ---------------------------------------------------------------

	#[test]
	fn best_block_in_current_slot_is_current() {
		// Wall clock in slot 804, best block from slot 804 → current.
		assert!(is_best_relay_block_current_at(804, now_at(804, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_in_previous_slot_is_stale() {
		// Wall clock in slot 805, best block from slot 804 → stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn the_bug_scenario_best_block_stale_at_slot_boundary() {
		// THE BUG: wall clock just crossed into slot 805 (17ms in),
		// but best relay block is still from slot 804. Stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 17), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_current_after_new_relay_block_arrives() {
		// New relay block (slot 805) arrives. Wall clock in slot 805.
		assert!(is_best_relay_block_current_at(805, now_at(805, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_from_future_slot_is_current() {
		// Should not happen, but must not panic.
		assert!(is_best_relay_block_current_at(810, now_at(805, 0), RELAY_SLOT_DURATION));
	}

	#[test]
	fn stale_at_exact_slot_boundary() {
		// Exactly at the start of slot 805.
		// Best from 804 → stale (804 < 805).
		assert!(!is_best_relay_block_current_at(804, now_at(805, 0), RELAY_SLOT_DURATION));
		// Best from 805 → current.
		assert!(is_best_relay_block_current_at(805, now_at(805, 0), RELAY_SLOT_DURATION));
	}

	#[test]
	fn current_at_end_of_slot() {
		// 5999ms into slot 804 — still in slot 804.
		// Best from 804 → current.
		assert!(is_best_relay_block_current_at(804, now_at(804, 5999), RELAY_SLOT_DURATION));
	}

	#[test]
	fn no_wait_needed_during_normal_building() {
		// During elastic scaling in slot 804: best is from 804,
		// wall clock is mid-slot 804. No wait needed.
		for ms in (0..6000).step_by(500) {
			assert!(
				is_best_relay_block_current_at(804, now_at(804, ms), RELAY_SLOT_DURATION),
				"Should be current at {}ms into slot 804",
				ms
			);
		}
	}

	#[test]
	fn wait_needed_when_slot_advances() {
		// Wall clock moves to slot 805, best still from 804.
		// This is the race condition — must detect as stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 0), RELAY_SLOT_DURATION));
		assert!(!is_best_relay_block_current_at(804, now_at(805, 17), RELAY_SLOT_DURATION));
		assert!(!is_best_relay_block_current_at(804, now_at(805, 500), RELAY_SLOT_DURATION));
	}
}

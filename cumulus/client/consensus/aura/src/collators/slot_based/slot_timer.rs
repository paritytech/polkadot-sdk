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
use sc_client_api::UsageProvider;
use sc_consensus_aura::SlotDuration;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_runtime::traits::Member;
use sp_timestamp::Timestamp;
use std::{
	cmp::{max, min},
	sync::Arc,
	time::Duration,
};

/// Lower limits of allowed block production interval.
/// Defensive mechanism, corresponds to 12 cores at 6 second block time.
const BLOCK_PRODUCTION_MINIMUM_INTERVAL_MS: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub(crate) struct SlotInfo {
	pub timestamp: Timestamp,
	pub slot: Slot,
}

/// Manages block-production timings based on chain parameters and assigned cores.
#[derive(Debug)]
pub(crate) struct SlotTimer<Block, Client, P> {
	/// Client that is used for runtime calls
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

/// Returns current duration since Unix epoch.
fn duration_now() -> Duration {
	use std::time::SystemTime;
	let now = SystemTime::now();
	now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(|e| {
		panic!("Current time {:?} is before Unix epoch. Something is wrong: {:?}", now, e)
	})
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
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + UsageProvider<Block>,
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
			last_reported_slot: None,
			_marker: Default::default(),
		}
	}

	/// Inform the slot timer about the last seen number of cores.
	pub fn update_scheduling(&mut self, num_cores_next_block: u32) {
		self.last_reported_core_num = Some(num_cores_next_block);
	}

	/// Returns a future that resolves when the next block production should be attempted.
	pub async fn wait_until_next_slot(&mut self) -> Result<(), ()> {
		let Ok(slot_duration) = crate::slot_duration(&*self.client) else {
			tracing::error!(target: LOG_TARGET, "Failed to fetch slot duration from runtime.");
			return Err(())
		};

		let (time_until_next_attempt, mut next_aura_slot) = compute_next_wake_up_time(
			slot_duration,
			self.relay_slot_duration,
			self.last_reported_core_num,
			duration_now(),
			self.time_offset,
		);

		match self.last_reported_slot {
			// If we already reported a slot, we don't want to skip a slot. But we also don't want
			// to go through all the slots if a node was halted for some reason.
			Some(ls) if ls + 1 < next_aura_slot && next_aura_slot <= ls + 3 => {
				next_aura_slot = ls + 1u64;
			},
			None | Some(_) => {
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
		Ok(())
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

		assert_eq!(wait_duration.as_millis(), expected_wait_duration, "Wait time mismatch."); // Should wait 5 seconds
	}
}

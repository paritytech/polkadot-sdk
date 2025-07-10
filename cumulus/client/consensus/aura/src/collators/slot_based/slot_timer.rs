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
use cumulus_primitives_aura::Slot;
use sc_consensus_aura::SlotDuration;
use sp_timestamp::Timestamp;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct SlotInfo {
	pub timestamp: Timestamp,
	pub slot: Slot,
}

/// Manages block-production timings based on chain parameters.
#[derive(Debug)]
pub(crate) struct SlotTimer {
	/// Offset the current time by this duration.
	time_offset: Duration,
	/// Slot duration of the relay chain. This is used to compute when to wake up for
	/// block production attempts.
	relay_slot_duration: Duration,
	/// Stores the latest slot that was reported by [`Self::wait_until_next_slot`].
	last_reported_slot: Option<Slot>,
}

/// Returns current duration since Unix epoch.
fn duration_now() -> Duration {
	use std::time::SystemTime;
	let now = SystemTime::now();
	now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(|e| {
		panic!("Current time {:?} is before Unix epoch. Something is wrong: {:?}", now, e)
	})
}

/// Returns the duration until the next block production slot and the timestamp at this slot.
fn time_until_next_slot(
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

impl SlotTimer {
	/// Create a new slot timer.
	pub fn new_with_offset(time_offset: Duration, relay_slot_duration: Duration) -> Self {
		Self { time_offset, relay_slot_duration, last_reported_slot: None }
	}

	/// Returns a future that resolves when the next block production should be attempted.
	pub async fn wait_until_next_slot(&mut self) -> Result<(), ()> {
		let (time_until_next_attempt, timestamp) =
			time_until_next_slot(duration_now(), self.relay_slot_duration, self.time_offset);

		// Calculate the current slot using the relay chain slot duration
		let relay_slot_duration_for_slot = SlotDuration::from(self.relay_slot_duration);
		let mut current_slot = Slot::from_timestamp(timestamp, relay_slot_duration_for_slot);

		match self.last_reported_slot {
			// If we already reported a slot, we don't want to skip a slot. But we also don't want
			// to go through all the slots if a node was halted for some reason.
			Some(ls) if ls + 1 < current_slot && current_slot <= ls + 3 => {
				current_slot = ls + 1u64;
				// Don't sleep since we're catching up
				tracing::debug!(
					target: LOG_TARGET,
					last_slot = ?ls,
					current_slot = ?current_slot,
					"Catching up on skipped slot."
				);
			},
			None | Some(_) => {
				tracing::trace!(
					target: LOG_TARGET,
					time_to_sleep = ?time_until_next_attempt,
					"Feeling sleepy 😴"
				);

				// Sleep based on relay chain timing
				tokio::time::sleep(time_until_next_attempt).await;
			},
		}

		tracing::debug!(
			target: LOG_TARGET,
			relay_slot_duration = ?self.relay_slot_duration,
			current_slot = ?current_slot,
			"New block production slot."
		);

		// Update internal slot tracking
		self.last_reported_slot = Some(current_slot);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	const RELAY_CHAIN_SLOT_DURATION: u64 = 6000;

	#[rstest]
	// Test that different now timestamps have correct impact
	#[case(1000, 0, 5000)]
	#[case(0, 0, 6000)]
	#[case(6000, 0, 6000)]
	// Test that offset affects the current time correctly
	#[case(1000, 1000, 6000)]
	#[case(12000, 2000, 2000)]
	#[case(12000, 6000, 6000)]
	#[case(12000, 7000, 1000)]
	// Test basic timing with relay slot duration
	#[case(11999, 0, 1)]
	fn test_get_next_slot(
		#[case] time_now: u64,
		#[case] offset_millis: u64,
		#[case] expected_wait_duration: u128,
	) {
		let relay_slot_duration = Duration::from_millis(RELAY_CHAIN_SLOT_DURATION);
		let time_now = Duration::from_millis(time_now);
		let offset = Duration::from_millis(offset_millis);

		let (wait_duration, _) = time_until_next_slot(time_now, relay_slot_duration, offset);

		assert_eq!(wait_duration.as_millis(), expected_wait_duration, "Wait time mismatch.");
	}
}

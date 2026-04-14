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

use crate::{collators::slot_based::relay_chain_data_cache::RelayChainDataCache, LOG_TARGET};
use cumulus_primitives_aura::Slot;
use cumulus_relay_chain_interface::{PHeader, RelayChainInterface};
use futures::prelude::*;
use polkadot_node_subsystem::gen::{stream::Stream, FutureExt};
use polkadot_primitives::{Block as RelayBlock, Header as RelayHeader};
use sc_consensus_aura::SlotDuration;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{pin::Pin, time::Duration};

/// Whether a relay chain block's slot is still in progress or already finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayChainSlotStatus {
	/// The block's BABE slot is behind the current wall-clock slot (finished).
	Finished,
	/// The block's BABE slot matches or is ahead of the current wall-clock slot (in progress).
	InProgress,
}

/// Tracks relay chain scheduling information, including the relay best block hash
/// and whether its slot is still in progress.
///
/// With elastic scaling (multiple cores), the para slot timer fires multiple times
/// per relay chain slot. This struct provides methods to fetch and inspect relay
/// chain state for scheduling decisions.
pub(crate) struct SchedulingInfo {
	best_notifications: Pin<Box<dyn Stream<Item = PHeader> + Send>>,
	slot_offset: Duration,
	relay_best_header: Option<RelayHeader>,
}

impl SchedulingInfo {
	pub(crate) fn new(
		best_notifications: Pin<Box<dyn Stream<Item = PHeader> + Send>>,
		slot_offset: Duration,
	) -> Self {
		Self { best_notifications, slot_offset, relay_best_header: None }
	}

	/// Returns the relay chain block hash to use as the starting point for finding
	/// descendants (and ultimately the relay parent).
	///
	/// - V3 (`v3_enabled = true`): uses the last finished RC slot block. If the relay best block's
	///   slot is still in progress, falls back to its parent.
	/// - V2 (`v3_enabled = false`): uses `relay_best_hash` directly.
	///
	/// Calls [`Self::fetch_relay_best_header`] internally.
	pub async fn descendants_start<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		relay_chain_slot_duration: Duration,
		v3_enabled: bool,
	) -> Option<RelayHeader>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		let relay_best_header = self
			.fetch_relay_best_header(
				relay_client,
				relay_chain_data_cache,
				relay_chain_slot_duration,
			)
			.await?
			.clone();

		if !v3_enabled {
			return Some(relay_best_header);
		}

		match Self::compute_slot_status(&relay_best_header, relay_chain_slot_duration)? {
			RelayChainSlotStatus::Finished => Some(relay_best_header),
			RelayChainSlotStatus::InProgress => {
				let relay_best_hash = *relay_best_header.parent_hash();
				let relay_best_header = relay_chain_data_cache
					.get_mut_relay_chain_data(relay_best_hash)
					.await
					.ok()
					.map(|d| d.relay_parent_header.clone())?;
				self.relay_best_header = Some(relay_best_header);
				self.relay_best_header.clone()
			},
		}
	}

	/// Fetches the relay chain best block hash and caches it.
	async fn fetch_relay_best_header<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		relay_chain_slot_duration: Duration,
	) -> Option<&RelayHeader>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		// Wait for the best relay block to be from the current relay
		// chain slot. If propagation exceeded `slot_offset`, this
		// blocks until a new-best notification arrives.
		// See: https://github.com/paritytech/polkadot-sdk/pull/11453
		let Some(relay_best_header) = wait_for_current_relay_block(
			relay_client,
			relay_chain_data_cache,
			&mut self.best_notifications,
			self.slot_offset,
			relay_chain_slot_duration,
		)
		.await
		else {
			tracing::warn!(
				target: crate::LOG_TARGET,
				"Unable to fetch latest relay chain block hash."
			);
			return None;
		};

		self.relay_best_header = Some(relay_best_header);
		self.relay_best_header.as_ref()
	}

	/// Extracts the BABE slot from a relay header and compares it against the
	/// current wall-clock slot to determine the slot status.
	fn compute_slot_status(
		header: &RelayHeader,
		relay_chain_slot_duration: Duration,
	) -> Option<RelayChainSlotStatus> {
		let hash = header.hash();
		let babe_slot = match sc_consensus_babe::find_pre_digest::<RelayBlock>(header) {
			Ok(pre_digest) => pre_digest.slot(),
			Err(err) => {
				tracing::error!(
					target: LOG_TARGET,
					?hash,
					?err,
					"Relay chain block does not contain a BABE pre-digest.",
				);
				return None;
			},
		};

		let slot_duration_ms = relay_chain_slot_duration.as_millis() as u64;
		let current_slot =
			Slot::from_timestamp(Timestamp::current(), SlotDuration::from_millis(slot_duration_ms));

		let status = if babe_slot < current_slot {
			tracing::debug!(
				target: LOG_TARGET,
				?hash,
				?babe_slot,
				?current_slot,
				"Relay chain block belongs to a finished slot.",
			);
			RelayChainSlotStatus::Finished
		} else {
			tracing::debug!(
				target: LOG_TARGET,
				?hash,
				?babe_slot,
				?current_slot,
				"Relay chain block belongs to the current in-progress slot.",
			);
			RelayChainSlotStatus::InProgress
		};

		Some(status)
	}
}

/// Returns `true` if the best relay chain block is from the current relay chain
/// slot. Uses the wall clock adjusted by `slot_offset`.
fn is_best_relay_block_current(
	best_relay_slot: u64,
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> bool {
	let now = super::slot_timer::duration_now().saturating_sub(slot_offset);
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
/// Returns the best relay block hash, or `None` on error.
pub(crate) async fn wait_for_current_relay_block<RelayClient>(
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

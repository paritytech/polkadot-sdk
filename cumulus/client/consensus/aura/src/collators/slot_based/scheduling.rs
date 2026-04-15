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

fn get_babe_slot(header: &RelayHeader) -> Option<Slot> {
	match sc_consensus_babe::find_pre_digest::<RelayBlock>(header) {
		Ok(pre_digest) => Some(pre_digest.slot()),
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				hash = %header.hash(),
				?err,
				"Relay chain block does not contain a BABE pre-digest.",
			);
			None
		},
	}
}

fn get_current_relay_slot_at(
	now: Duration,
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> Slot {
	let now = now.saturating_sub(slot_offset);
	Slot::from_timestamp(
		Timestamp::from(now),
		SlotDuration::from_millis(relay_chain_slot_duration.as_millis() as u64),
	)
}

/// Wait until the best relay chain block is from the current relay chain slot.
///
/// If the current best block is already current, returns its hash immediately.
/// Otherwise, waits for a new-best notification and re-checks. This ensures
/// the collator doesn't build on a stale scheduling parent when relay block
/// propagation exceeds `slot_offset` at a slot boundary.
///
/// Returns the best relay block hash, or `None` on error.
pub(crate) async fn wait_for_current_relay_block<RelayClient>(
	relay_client: &RelayClient,
	relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
	best_notifications: &mut (impl Stream<Item = RelayHeader> + Unpin),
	relay_chain_slot_duration: Duration,
	slot_offset: Duration,
) -> Option<RelayHeader>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let relay_best_hash = relay_client.best_block_hash().await.ok()?;
	let mut maybe_best_header = Some(
		relay_chain_data_cache
			.get_mut_relay_chain_data(relay_best_hash)
			.await
			.ok()
			.map(|d| d.relay_parent_header.clone())?,
	);

	loop {
		// Drain buffered notifications.
		while let Some(maybe_header) = best_notifications.next().now_or_never() {
			maybe_best_header = Some(maybe_header?);
		}

		let best_header = match maybe_best_header.take() {
			Some(h) => h,
			None => best_notifications.next().await?, // Block until one arrives.
		};
		let best_slot = get_babe_slot(&best_header)?;
		let current_relay_slot = get_current_relay_slot_at(
			Timestamp::current().as_duration(),
			slot_offset,
			relay_chain_slot_duration,
		);
		if best_slot >= current_relay_slot {
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

/// Tracks relay chain scheduling information, including the relay best block hash
/// and whether its slot is still in progress.
///
/// With elastic scaling (multiple cores), the para slot timer fires multiple times
/// per relay chain slot. This struct provides methods to fetch and inspect relay
/// chain state for scheduling decisions.
pub(crate) struct SchedulingInfo {
	best_notifications: Pin<Box<dyn Stream<Item = PHeader> + Send>>,
	relay_chain_slot_duration: Duration,
	slot_offset: Duration,
}

impl SchedulingInfo {
	pub fn new(
		best_notifications: Pin<Box<dyn Stream<Item = PHeader> + Send>>,
		relay_chain_slot_duration: Duration,
		slot_offset: Duration,
	) -> Self {
		Self { best_notifications, relay_chain_slot_duration, slot_offset }
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
		v3_enabled: bool,
	) -> Option<RelayHeader>
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
			self.relay_chain_slot_duration,
			self.slot_offset,
		)
		.await
		else {
			tracing::warn!(
				target: LOG_TARGET,
				"Unable to fetch latest relay chain block hash."
			);
			return None;
		};

		if !v3_enabled {
			return Some(relay_best_header);
		}

		let babe_slot = get_babe_slot(&relay_best_header)?;
		let current_relay_slot = get_current_relay_slot_at(
			Timestamp::current().as_duration(),
			Duration::from_millis(0),
			self.relay_chain_slot_duration,
		);
		if babe_slot < current_relay_slot {
			Some(relay_best_header)
		} else {
			let relay_best_hash = *relay_best_header.parent_hash();
			let relay_best_header = relay_chain_data_cache
				.get_mut_relay_chain_data(relay_best_hash)
				.await
				.ok()
				.map(|d| d.relay_parent_header.clone())?;
			Some(relay_best_header)
		}
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

	#[test]
	fn get_current_relay_slot_at_works_correctly() {
		// beginning of slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(804, 0),
				Duration::from_millis(0),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);

		// end of slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(804, 5999),
				Duration::from_millis(0),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);

		// offset, but still inside slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(805, 500),
				Duration::from_millis(500),
				RELAY_SLOT_DURATION
			),
			Slot::from(805)
		);

		// offset => previous slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(805, 500),
				Duration::from_millis(501),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);
	}
}

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

use crate::{
	collators::{slot_based::relay_chain_data_cache::RelayChainDataCache, RelayHash, RelayHeader},
	LOG_TARGET,
};
use cumulus_client_consensus_common::get_relay_slot;
use cumulus_primitives_aura::Slot;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::{
	prelude::*,
	stream::{Fuse, FusedStream},
};
use polkadot_node_subsystem::gen::{stream::Stream, FutureExt};
use polkadot_primitives::{node_features::FeatureIndex, Block as RelayBlock};
use sc_consensus_aura::SlotDuration;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{pin::Pin, time::Duration};

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

fn get_current_relay_slot(slot_offset: Duration, relay_chain_slot_duration: Duration) -> Slot {
	get_current_relay_slot_at(
		Timestamp::current().as_duration(),
		slot_offset,
		relay_chain_slot_duration,
	)
}

/// Tracks relay chain scheduling information, including the relay best block hash
/// and whether its slot is still in progress.
///
/// With elastic scaling (multiple cores), the para slot timer fires multiple times
/// per relay chain slot. This struct provides methods to fetch and inspect relay
/// chain state for scheduling decisions.
pub(crate) struct SchedulingInfo {
	best_notifications: Fuse<Pin<Box<dyn Stream<Item = RelayHeader> + Send>>>,
	relay_slot_duration: Duration,
	slot_offset: Duration,
}

impl SchedulingInfo {
	pub fn new(relay_chain_slot_duration: Duration, slot_offset: Duration) -> Self {
		let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> =
			Box::pin(futures::stream::empty());
		let mut stream = stream.fuse();
		// Make sure the fused stream is marked as terminated.
		stream.next().now_or_never();

		Self {
			best_notifications: stream,
			relay_slot_duration: relay_chain_slot_duration,
			slot_offset,
		}
	}

	pub fn should_reset_best_notifications(&self) -> bool {
		self.best_notifications.is_terminated()
	}

	pub fn reset_best_notifications(
		&mut self,
		best_notifications: Pin<Box<dyn Stream<Item = RelayHeader> + Send>>,
	) {
		self.best_notifications = best_notifications.fuse();
	}

	async fn is_v3_enabled_on_relay<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		at: RelayHash,
	) -> bool
	where
		RelayClient: RelayChainInterface,
	{
		let node_features = match relay_client.node_features(at).await {
			Ok(node_features) => node_features,
			Err(err) => {
				tracing::warn!(
					target: LOG_TARGET,
					?at,
					?err,
					"Unable to fetch node features for relay chain. \
					Will use Scheduling V2 by default"
				);
				return false;
			},
		};
		FeatureIndex::CandidateReceiptV3.is_set(&node_features)
	}

	async fn get_relay_header<RelayClient>(
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		hash: RelayHash,
	) -> Option<RelayHeader>
	where
		RelayClient: RelayChainInterface + 'static,
	{
		relay_chain_data_cache
			.get_mut(hash)
			.await
			.ok()
			.map(|data| data.relay_parent_header.clone())
	}

	/// Wait until we find a scheduling parent block that is not stale.
	///
	/// If the current best block is already a valid scheduling parent, returns its hash
	/// immediately. Otherwise, waits for a new-best notification and re-checks.
	/// For v2 This ensures the collator doesn't build on a stale scheduling parent when
	/// relay block propagation exceeds `slot_offset` at a slot boundary.
	/// See: https://github.com/paritytech/polkadot-sdk/pull/11453
	///
	/// Returns `None` on error.
	pub(crate) async fn wait_for_scheduling_parent<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		v3_enabled_on_para: bool,
	) -> Option<(RelayHeader, bool)>
	where
		RelayClient: RelayChainInterface + 'static,
	{
		let best_relay_hash = relay_client.best_block_hash().await.ok()?;
		let mut maybe_best_relay_header =
			Self::get_relay_header(relay_chain_data_cache, best_relay_hash).await;

		loop {
			// Drain buffered notifications.
			while let Some(Some(header)) = self.best_notifications.next().now_or_never() {
				maybe_best_relay_header = Some(header);
			}

			let best_header = match maybe_best_relay_header.take() {
				Some(header) => header,
				None => {
					if self.best_notifications.is_terminated() {
						return None;
					}

					self.best_notifications.next().await?
				},
			};
			let v3_enabled = v3_enabled_on_para &&
				self.is_v3_enabled_on_relay(relay_client, best_header.hash()).await;

			let best_relay_slot = get_relay_slot(&best_header)?;

			// V2
			if !v3_enabled {
				let current_relay_slot =
					get_current_relay_slot(self.slot_offset, self.relay_slot_duration);
				if best_relay_slot >= current_relay_slot {
					return Some((best_header, false));
				}
				continue;
			}

			// V3
			let current_relay_slot =
				get_current_relay_slot(Duration::ZERO, self.relay_slot_duration);
			if best_relay_slot < current_relay_slot {
				return Some((best_header, true));
			}

			// The scheduling parent should be part of the same session as the best
			// relay block.
			// If the current header contains a session change log, then it will be
			// part of a new session, while the scheduling parent will be part of the old one.
			if sc_consensus_babe::contains_epoch_change::<RelayBlock>(&best_header) {
				return None;
			}
			let best_header_hash = *best_header.parent_hash();
			let best_header =
				Self::get_relay_header(relay_chain_data_cache, best_header_hash).await?;

			return Some((best_header, true));
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

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
use cumulus_primitives_core::relay_chain::BlockId;
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_primitives::{Block as RelayBlock, Hash as RelayHash};
use sc_consensus_aura::SlotDuration;
use sp_timestamp::Timestamp;
use std::time::Duration;

/// Whether a relay chain block's slot is still in progress or already finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotStatus {
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
#[derive(Default)]
pub(crate) struct SchedulingInfo {
	/// The relay chain best block hash.
	relay_best_hash: Option<RelayHash>,
	/// Whether the relay best block belongs to the current (in-progress) relay chain slot.
	relay_best_slot_status: Option<SlotStatus>,
}

impl SchedulingInfo {
	/// Returns the cached relay chain best block hash.
	pub fn relay_best_hash(&self) -> Option<RelayHash> {
		self.relay_best_hash
	}

	/// Returns the slot status of the relay best block, fetching it if not yet known.
	///
	/// Requires [`Self::fetch_relay_best_hash`] to have been called first.
	pub async fn relay_best_slot_status<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_slot_duration: Duration,
	) -> Option<SlotStatus>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		if let Some(status) = self.relay_best_slot_status {
			return Some(status);
		}

		let relay_best_hash = self.relay_best_hash?;
		self.check_slot_status(relay_client, relay_best_hash, relay_chain_slot_duration)
			.await
	}

	/// Fetches the relay chain best block hash and caches it.
	pub async fn fetch_relay_best_hash<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
	) -> Option<RelayHash>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		match relay_client.best_block_hash().await {
			Ok(hash) => {
				self.relay_best_hash = Some(hash);
				// Reset slot status since we have a new relay best hash.
				self.relay_best_slot_status = None;
				Some(hash)
			},
			Err(err) => {
				tracing::warn!(
					target: LOG_TARGET,
					?err,
					"Unable to fetch latest relay chain block hash.",
				);
				None
			},
		}
	}

	/// Fetches the header for the given relay chain block hash, extracts its BABE slot,
	/// and compares it against the current wall-clock relay chain slot to determine
	/// whether the block belongs to the current in-progress slot or an already-finished
	/// one.
	///
	/// If `block_hash` matches the cached `relay_best_hash`, the result is also stored
	/// in `relay_best_slot_status`.
	async fn check_slot_status<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		block_hash: RelayHash,
		relay_chain_slot_duration: Duration,
	) -> Option<SlotStatus>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		let relay_header = match relay_client.header(BlockId::Hash(block_hash)).await {
			Ok(Some(header)) => header,
			Ok(None) => {
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					"Relay chain block header not found.",
				);
				return None;
			},
			Err(err) => {
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					?err,
					"Failed to fetch relay chain block header.",
				);
				return None;
			},
		};

		let babe_slot = match sc_consensus_babe::find_pre_digest::<RelayBlock>(&relay_header) {
			Ok(pre_digest) => pre_digest.slot(),
			Err(err) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					?err,
					"Relay chain block does not contain a BABE pre-digest.",
				);
				return None;
			},
		};

		let slot_duration_ms = relay_chain_slot_duration.as_millis() as u64;
		let current_slot = Slot::from_timestamp(
			Timestamp::current(),
			SlotDuration::from_millis(slot_duration_ms),
		);

		let status = if babe_slot < current_slot {
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				?babe_slot,
				?current_slot,
				"Relay chain block belongs to a finished slot.",
			);
			SlotStatus::Finished
		} else {
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				?babe_slot,
				?current_slot,
				"Relay chain block belongs to the current in-progress slot.",
			);
			SlotStatus::InProgress
		};

		// Cache the status if this is the relay best hash.
		if self.relay_best_hash == Some(block_hash) {
			self.relay_best_slot_status = Some(status);
		}

		Some(status)
	}
}

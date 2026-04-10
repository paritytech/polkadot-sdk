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
use polkadot_primitives::{Block as RelayBlock, Hash as RelayHash, Header as RelayHeader};
use sc_consensus_aura::SlotDuration;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::time::Duration;

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
#[derive(Default)]
pub(crate) enum SchedulingInfo {
	#[default]
	Uninitialized,
	Initialized {
		relay_best_hash: RelayHash,
		maybe_relay_best_header: Option<RelayHeader>,
	},
}

impl SchedulingInfo {
	/// Returns the slot status of the relay best block, recomputed against the
	/// current wall-clock time on each call.
	///
	/// Lazily fetches the relay best block header if not already cached, or if the
	/// cached header's hash differs from the current `relay_best_hash`.
	///
	/// Requires [`Self::fetch_relay_best_hash`] to have been called first.
	async fn relay_best_slot_status<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_slot_duration: Duration,
	) -> Option<RelayChainSlotStatus>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		let (relay_best_hash, maybe_relay_best_header) = match self {
			SchedulingInfo::Uninitialized => return None,
			SchedulingInfo::Initialized { relay_best_hash, maybe_relay_best_header } => {
				(*relay_best_hash, maybe_relay_best_header)
			},
		};

		// Fetch the header if not cached or if it belongs to a different block.
		let relay_best_header = match maybe_relay_best_header {
			Some(header) => header,
			None => match relay_client.header(BlockId::Hash(relay_best_hash)).await {
				Ok(Some(header)) => {
					*maybe_relay_best_header = Some(header);
					maybe_relay_best_header.as_ref()?
				},
				Ok(None) => {
					tracing::warn!(
						target: LOG_TARGET,
						?relay_best_hash,
						"Relay best block header not found.",
					);
					return None;
				},
				Err(err) => {
					tracing::warn!(
						target: LOG_TARGET,
						?relay_best_hash,
						?err,
						"Failed to fetch relay best block header.",
					);
					return None;
				},
			},
		};

		Self::compute_slot_status(relay_best_header, relay_chain_slot_duration)
	}

	/// Returns the relay chain block hash to use as the starting point for finding
	/// descendants (and ultimately the relay parent).
	///
	/// - V3 (`v3_enabled = true`): uses the last finished RC slot block. If the relay best block's
	///   slot is still in progress, falls back to its parent.
	/// - V2 (`v3_enabled = false`): uses `relay_best_hash` directly.
	///
	/// Calls [`Self::fetch_relay_best_hash`] internally.
	pub async fn descendants_start<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
		relay_chain_slot_duration: Duration,
		v3_enabled: bool,
	) -> Option<RelayHash>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		let relay_best_hash = self.fetch_relay_best_hash(relay_client).await?;
		if !v3_enabled {
			return Some(relay_best_hash);
		}

		match self.relay_best_slot_status(relay_client, relay_chain_slot_duration).await? {
			RelayChainSlotStatus::Finished => Some(relay_best_hash),
			RelayChainSlotStatus::InProgress => {
				let maybe_relay_best_header = match self {
					SchedulingInfo::Uninitialized => None,
					SchedulingInfo::Initialized { relay_best_hash: _, maybe_relay_best_header } => {
						maybe_relay_best_header.as_ref()
					},
				};
				Some(*maybe_relay_best_header?.parent_hash())
			},
		}
	}

	/// Fetches the relay chain best block hash and caches it.
	async fn fetch_relay_best_hash<RelayClient>(
		&mut self,
		relay_client: &RelayClient,
	) -> Option<RelayHash>
	where
		RelayClient: RelayChainInterface + Clone + 'static,
	{
		match relay_client.best_block_hash().await {
			Ok(hash) => {
				let maybe_relay_best_hash = match &self {
					SchedulingInfo::Uninitialized => None,
					SchedulingInfo::Initialized { relay_best_hash, .. } => Some(relay_best_hash),
				};
				if maybe_relay_best_hash != Some(&hash) {
					*self =
						Self::Initialized { relay_best_hash: hash, maybe_relay_best_header: None };
				}
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

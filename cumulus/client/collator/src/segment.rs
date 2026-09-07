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

//! Turning collations into segments and handing them to the collator protocol.

use crate::{
	collation::{build_segment, SegmentToDistribute},
	metrics::Metrics,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_node_subsystem::messages::{CollatorProtocolMessage, Segment};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{Hash, Id as ParaId, SessionIndex, TransposedClaimQueue};
use schnellru::{ByLength, LruMap};

const LOG_TARGET: &str = "cumulus-collator::segment";

#[cfg(test)]
mod tests;

/// Builds the candidate receipts for our collations and hands them to the collator protocol,
/// which distributes them to the validators backing the target core.
pub struct SegmentDistributor<RClient> {
	relay_client: RClient,
	overseer_handle: OverseerHandle,
	para_id: ParaId,
	metrics: Metrics,
	// The validator set size is needed for erasure coding and only changes on session
	// boundaries, so it is worth caching. Two entries are enough to cover a session change.
	validator_counts: LruMap<SessionIndex, usize>,
}

impl<RClient: RelayChainInterface> SegmentDistributor<RClient> {
	/// Create a new distributor for the given para.
	pub fn new(
		relay_client: RClient,
		overseer_handle: OverseerHandle,
		para_id: ParaId,
		metrics: Metrics,
	) -> Self {
		Self {
			relay_client,
			overseer_handle,
			para_id,
			metrics,
			validator_counts: LruMap::new(ByLength::new(2)),
		}
	}

	/// Build the segment and send it to the collator protocol. Errors are logged, since there is
	/// nothing the caller can do about a collation that cannot be turned into a candidate.
	/// `claim_queue` is the transposed claim queue at the segment's scheduling anchor. It is the
	/// caller's because no `RelayChainInterface` implementation caches it, so fetching it here
	/// would cost an uncached round trip per core.
	pub async fn distribute(
		&mut self,
		segment: SegmentToDistribute,
		claim_queue: TransposedClaimQueue,
	) {
		let core_index = segment.core_index;
		let anchor = segment.scheduling.anchor();

		let Some(n_validators) = self.n_validators(anchor, segment.scheduling.session()).await
		else {
			return;
		};

		let timer = self.metrics.time_submit_collation();

		let segment = match build_segment(segment, self.para_id, n_validators, &claim_queue) {
			Ok(segment) => segment,
			Err(error) => {
				if let Some(timer) = timer {
					timer.stop_and_discard();
				}
				tracing::error!(
					target: LOG_TARGET,
					?error,
					?core_index,
					?anchor,
					"Failed to build segment",
				);
				return;
			},
		};

		let n_candidates = match &segment {
			Segment::V2(_) => 1u64,
			Segment::V3 { candidates, .. } => candidates.len() as u64,
		};
		self.metrics.on_collations_generated(n_candidates);

		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			?anchor,
			"Distributing segment",
		);

		self.overseer_handle
			.send_msg(
				CollatorProtocolMessage::DistributeSegment {
					core_index,
					para_id: self.para_id,
					segment,
				},
				"DistributeSegment",
			)
			.await;
	}

	async fn n_validators(
		&mut self,
		relay_parent: Hash,
		session_index: SessionIndex,
	) -> Option<usize> {
		if let Some(n_validators) = self.validator_counts.get(&session_index) {
			return Some(*n_validators);
		}

		// Key and value are both read at `anchor`, so they always describe the same session.
		match self.relay_client.validators(relay_parent).await {
			Ok(validators) => {
				let n_validators = validators.len();
				self.validator_counts.insert(session_index, n_validators);
				Some(n_validators)
			},
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?error,
					?relay_parent,
					"Failed to query validators, not distributing segment",
				);
				None
			},
		}
	}
}

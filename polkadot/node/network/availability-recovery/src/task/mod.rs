// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Main recovery task logic. Runs recovery strategies.

#![warn(missing_docs)]

mod strategy;

pub use self::strategy::{
	FetchChunks, FetchChunksParams, FetchFull, FetchFullParams, FetchSystematicChunks,
	FetchSystematicChunksParams, RecoveryStrategy, State,
};

#[cfg(test)]
pub use self::strategy::{REGULAR_CHUNKS_REQ_RETRY_LIMIT, SYSTEMATIC_CHUNKS_REQ_RETRY_LIMIT};

use crate::{metrics::Metrics, ErasureTask, PostRecoveryCheck, LOG_TARGET};

use parity_scale_codec::Encode;
use polkadot_node_primitives::AvailableData;
use polkadot_node_subsystem::{messages::AvailabilityStoreMessage, overseer, RecoveryError};
use polkadot_primitives::{AuthorityDiscoveryId, CandidateHash, Hash};
use sc_network::ProtocolName;

use futures::channel::{mpsc, oneshot};
use std::collections::VecDeque;

/// Recovery parameters common to all strategies in a `RecoveryTask`.
#[derive(Clone)]
pub struct RecoveryParams {
	/// Discovery ids of `validators`.
	pub validator_authority_keys: Vec<AuthorityDiscoveryId>,

	/// Number of validators.
	pub n_validators: usize,

	/// The number of regular chunks needed.
	pub threshold: usize,

	/// The number of systematic chunks needed.
	pub systematic_threshold: usize,

	/// A hash of the relevant candidate.
	pub candidate_hash: CandidateHash,

	/// The root of the erasure encoding of the candidate.
	pub erasure_root: Hash,

	/// Metrics to report.
	pub metrics: Metrics,

	/// Do not request data from availability-store. Useful for collators.
	pub bypass_availability_store: bool,

	/// The type of check to perform after available data was recovered.
	pub post_recovery_check: PostRecoveryCheck,

	/// The blake2-256 hash of the PoV.
	pub pov_hash: Hash,

	/// Protocol name for ChunkFetchingV1.
	pub req_v1_protocol_name: ProtocolName,

	/// Protocol name for ChunkFetchingV2.
	pub req_v2_protocol_name: ProtocolName,

	/// Whether or not chunk mapping is enabled.
	pub chunk_mapping_enabled: bool,

	/// Channel to the erasure task handler.
	pub erasure_task_tx: mpsc::Sender<ErasureTask>,
}

/// A stateful reconstruction of availability data in reference to
/// a candidate hash.
pub struct RecoveryTask<Sender: overseer::AvailabilityRecoverySenderTrait> {
	sender: Sender,
	params: RecoveryParams,
	strategies: VecDeque<Box<dyn RecoveryStrategy<Sender>>>,
	state: State,
}

impl<Sender> RecoveryTask<Sender>
where
	Sender: overseer::AvailabilityRecoverySenderTrait,
{
	/// Instantiate a new recovery task.
	pub fn new(
		sender: Sender,
		params: RecoveryParams,
		strategies: VecDeque<Box<dyn RecoveryStrategy<Sender>>>,
	) -> Self {
		Self { sender, params, strategies, state: State::new() }
	}

	async fn in_availability_store(&mut self) -> Option<AvailableData> {
		if !self.params.bypass_availability_store {
			let (tx, rx) = oneshot::channel();
			self.sender
				.send_message(AvailabilityStoreMessage::QueryAvailableData(
					self.params.candidate_hash,
					tx,
				))
				.await;

			match rx.await {
				Ok(Some(data)) => return Some(data),
				Ok(None) => {},
				Err(oneshot::Canceled) => {
					gum::warn!(
						target: LOG_TARGET,
						candidate_hash = ?self.params.candidate_hash,
						"Failed to reach the availability store",
					)
				},
			}
		}

		None
	}

	/// Run this recovery task to completion. It will loop through the configured strategies
	/// in-order and return whenever the first one recovers the full `AvailableData`.
	pub async fn run(mut self) -> Result<AvailableData, RecoveryError> {
		if let Some(data) = self.in_availability_store().await {
			return Ok(data)
		}

		self.params.metrics.on_recovery_started();

		let _timer = self.params.metrics.time_full_recovery();

		while let Some(current_strategy) = self.strategies.pop_front() {
			let display_name = current_strategy.display_name();
			let strategy_type = current_strategy.strategy_type();

			gum::debug!(
				target: LOG_TARGET,
				candidate_hash = ?self.params.candidate_hash,
				"Starting `{}` strategy",
				display_name
			);

			let res = current_strategy.run(&mut self.state, &mut self.sender, &self.params).await;

			match res {
				Err(RecoveryError::Unavailable) =>
					if self.strategies.front().is_some() {
						gum::debug!(
							target: LOG_TARGET,
							candidate_hash = ?self.params.candidate_hash,
							"Recovery strategy `{}` did not conclude. Trying the next one.",
							display_name
						);
						continue
					},
				Err(err) => {
					match &err {
						RecoveryError::Invalid =>
							self.params.metrics.on_recovery_invalid(strategy_type),
						_ => self.params.metrics.on_recovery_failed(strategy_type),
					}
					return Err(err)
				},
				Ok(data) => {
					self.params.metrics.on_recovery_succeeded(strategy_type, data.encoded_size());
					return Ok(data)
				},
			}
		}

		// We have no other strategies to try.
		gum::warn!(
			target: LOG_TARGET,
			candidate_hash = ?self.params.candidate_hash,
			"Recovery of available data failed.",
		);

		self.params.metrics.on_recovery_failed("all");

		Err(RecoveryError::Unavailable)
	}
}

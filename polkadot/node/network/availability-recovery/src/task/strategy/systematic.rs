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

use crate::{
	futures_undead::FuturesUndead,
	task::{
		strategy::{
			do_post_recovery_check, is_unavailable, OngoingRequests, N_PARALLEL,
			SYSTEMATIC_CHUNKS_REQ_RETRY_LIMIT,
		},
		RecoveryParams, RecoveryStrategy, State,
	},
	LOG_TARGET,
};

use polkadot_node_primitives::AvailableData;
use polkadot_node_subsystem::{overseer, RecoveryError};
use polkadot_primitives::{ChunkIndex, ValidatorIndex};

use std::collections::VecDeque;

/// Parameters needed for fetching systematic chunks.
pub struct FetchSystematicChunksParams {
	/// Validators that hold the systematic chunks.
	pub validators: Vec<(ChunkIndex, ValidatorIndex)>,
	/// Validators in the backing group, to be used as a backup for requesting systematic chunks.
	pub backers: Vec<ValidatorIndex>,
}

/// `RecoveryStrategy` that attempts to recover the systematic chunks from the validators that
/// hold them, in order to bypass the erasure code reconstruction step, which is costly.
pub struct FetchSystematicChunks {
	/// Systematic recovery threshold.
	threshold: usize,
	/// Validators that hold the systematic chunks.
	validators: Vec<(ChunkIndex, ValidatorIndex)>,
	/// Backers to be used as a backup.
	backers: Vec<ValidatorIndex>,
	/// Collection of in-flight requests.
	requesting_chunks: OngoingRequests,
}

impl FetchSystematicChunks {
	/// Instantiate a new systematic chunks strategy.
	pub fn new(params: FetchSystematicChunksParams) -> Self {
		Self {
			threshold: params.validators.len(),
			validators: params.validators,
			backers: params.backers,
			requesting_chunks: FuturesUndead::new(),
		}
	}

	fn is_unavailable(
		unrequested_validators: usize,
		in_flight_requests: usize,
		systematic_chunk_count: usize,
		threshold: usize,
	) -> bool {
		is_unavailable(
			systematic_chunk_count,
			in_flight_requests,
			unrequested_validators,
			threshold,
		)
	}

	/// Desired number of parallel requests.
	///
	/// For the given threshold (total required number of chunks) get the desired number of
	/// requests we want to have running in parallel at this time.
	fn get_desired_request_count(&self, chunk_count: usize, threshold: usize) -> usize {
		// Upper bound for parallel requests.
		let max_requests_boundary = std::cmp::min(N_PARALLEL, threshold);
		// How many chunks are still needed?
		let remaining_chunks = threshold.saturating_sub(chunk_count);
		// Actual number of requests we want to have in flight in parallel:
		// We don't have to make up for any error rate, as an error fetching a systematic chunk
		// results in failure of the entire strategy.
		std::cmp::min(max_requests_boundary, remaining_chunks)
	}

	async fn attempt_systematic_recovery<Sender: overseer::AvailabilityRecoverySenderTrait>(
		&mut self,
		state: &mut State,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		let strategy_type = RecoveryStrategy::<Sender>::strategy_type(self);
		let recovery_duration = common_params.metrics.time_erasure_recovery(strategy_type);
		let reconstruct_duration = common_params.metrics.time_erasure_reconstruct(strategy_type);
		let chunks = state
			.received_chunks
			.range(
				ChunkIndex(0)..
					ChunkIndex(
						u32::try_from(self.threshold)
							.expect("validator count should not exceed u32"),
					),
			)
			.map(|(_, chunk)| chunk.chunk.clone())
			.collect::<Vec<_>>();

		let available_data = polkadot_erasure_coding::reconstruct_from_systematic_v1(
			common_params.n_validators,
			chunks,
		);

		match available_data {
			Ok(data) => {
				drop(reconstruct_duration);

				// Attempt post-recovery check.
				do_post_recovery_check(common_params, data)
					.await
					.map_err(|e| {
						recovery_duration.map(|rd| rd.stop_and_discard());
						e
					})
					.map(|data| {
						gum::trace!(
							target: LOG_TARGET,
							candidate_hash = ?common_params.candidate_hash,
							erasure_root = ?common_params.erasure_root,
							"Data recovery from systematic chunks complete",
						);
						data
					})
			},
			Err(err) => {
				reconstruct_duration.map(|rd| rd.stop_and_discard());
				recovery_duration.map(|rd| rd.stop_and_discard());

				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					?err,
					"Systematic data recovery error",
				);

				Err(RecoveryError::Invalid)
			},
		}
	}
}

#[async_trait::async_trait]
impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender>
	for FetchSystematicChunks
{
	fn display_name(&self) -> &'static str {
		"Fetch systematic chunks"
	}

	fn strategy_type(&self) -> &'static str {
		"systematic_chunks"
	}

	async fn run(
		mut self: Box<Self>,
		state: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		// First query the store for any chunks we've got.
		if !common_params.bypass_availability_store {
			let local_chunk_indices = state.populate_from_av_store(common_params, sender).await;

			for (_, our_c_index) in &local_chunk_indices {
				// If we are among the systematic validators but hold an invalid chunk, we cannot
				// perform the systematic recovery. Fall through to the next strategy.
				if self.validators.iter().any(|(c_index, _)| c_index == our_c_index) &&
					!state.received_chunks.contains_key(our_c_index)
				{
					gum::debug!(
						target: LOG_TARGET,
						candidate_hash = ?common_params.candidate_hash,
						erasure_root = ?common_params.erasure_root,
						requesting = %self.requesting_chunks.len(),
						total_requesting = %self.requesting_chunks.total_len(),
						n_validators = %common_params.n_validators,
						chunk_index = ?our_c_index,
						"Systematic chunk recovery is not possible. We are among the systematic validators but hold an invalid chunk",
					);
					return Err(RecoveryError::Unavailable)
				}
			}
		}

		// No need to query the validators that have the chunks we already received or that we know
		// don't have the data from previous strategies.
		self.validators.retain(|(c_index, v_index)| {
			!state.received_chunks.contains_key(c_index) &&
				state.can_retry_request(
					&(common_params.validator_authority_keys[v_index.0 as usize].clone(), *v_index),
					SYSTEMATIC_CHUNKS_REQ_RETRY_LIMIT,
				)
		});

		let mut systematic_chunk_count = state
			.received_chunks
			.range(ChunkIndex(0)..ChunkIndex(self.threshold as u32))
			.count();

		// Safe to `take` here, as we're consuming `self` anyway and we're not using the
		// `validators` or `backers` fields in other methods.
		let mut validators_queue: VecDeque<_> = std::mem::take(&mut self.validators)
			.into_iter()
			.map(|(_, validator_index)| {
				(
					common_params.validator_authority_keys[validator_index.0 as usize].clone(),
					validator_index,
				)
			})
			.collect();
		let mut backers: Vec<_> = std::mem::take(&mut self.backers)
			.into_iter()
			.map(|validator_index| {
				common_params.validator_authority_keys[validator_index.0 as usize].clone()
			})
			.collect();

		loop {
			// If received_chunks has `systematic_chunk_threshold` entries, attempt to recover the
			// data.
			if systematic_chunk_count >= self.threshold {
				return self.attempt_systematic_recovery::<Sender>(state, common_params).await
			}

			if Self::is_unavailable(
				validators_queue.len(),
				self.requesting_chunks.total_len(),
				systematic_chunk_count,
				self.threshold,
			) {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					%systematic_chunk_count,
					requesting = %self.requesting_chunks.len(),
					total_requesting = %self.requesting_chunks.total_len(),
					n_validators = %common_params.n_validators,
					systematic_threshold = ?self.threshold,
					"Data recovery from systematic chunks is not possible",
				);

				return Err(RecoveryError::Unavailable)
			}

			let desired_requests_count =
				self.get_desired_request_count(systematic_chunk_count, self.threshold);
			let already_requesting_count = self.requesting_chunks.len();
			gum::debug!(
				target: LOG_TARGET,
				?common_params.candidate_hash,
				?desired_requests_count,
				total_received = ?systematic_chunk_count,
				systematic_threshold = ?self.threshold,
				?already_requesting_count,
				"Requesting systematic availability chunks for a candidate",
			);

			let strategy_type = RecoveryStrategy::<Sender>::strategy_type(&*self);

			state
				.launch_parallel_chunk_requests(
					strategy_type,
					common_params,
					sender,
					desired_requests_count,
					&mut validators_queue,
					&mut self.requesting_chunks,
				)
				.await;

			let _ = state
				.wait_for_chunks(
					strategy_type,
					common_params,
					SYSTEMATIC_CHUNKS_REQ_RETRY_LIMIT,
					&mut validators_queue,
					&mut self.requesting_chunks,
					&mut backers,
					|unrequested_validators,
					 in_flight_reqs,
					 // Don't use this chunk count, as it may contain non-systematic chunks.
					 _chunk_count,
					 new_systematic_chunk_count| {
						systematic_chunk_count = new_systematic_chunk_count;

						let is_unavailable = Self::is_unavailable(
							unrequested_validators,
							in_flight_reqs,
							systematic_chunk_count,
							self.threshold,
						);

						systematic_chunk_count >= self.threshold || is_unavailable
					},
				)
				.await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_erasure_coding::systematic_recovery_threshold;

	#[test]
	fn test_get_desired_request_count() {
		let num_validators = 100;
		let threshold = systematic_recovery_threshold(num_validators).unwrap();

		let systematic_chunks_task = FetchSystematicChunks::new(FetchSystematicChunksParams {
			validators: vec![(1.into(), 1.into()); num_validators],
			backers: vec![],
		});
		assert_eq!(systematic_chunks_task.get_desired_request_count(0, threshold), threshold);
		assert_eq!(systematic_chunks_task.get_desired_request_count(5, threshold), threshold - 5);
		assert_eq!(
			systematic_chunks_task.get_desired_request_count(num_validators * 2, threshold),
			0
		);
		assert_eq!(systematic_chunks_task.get_desired_request_count(0, N_PARALLEL * 2), N_PARALLEL);
		assert_eq!(systematic_chunks_task.get_desired_request_count(N_PARALLEL, N_PARALLEL + 2), 2);
	}
}

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
			REGULAR_CHUNKS_REQ_RETRY_LIMIT,
		},
		RecoveryParams, State,
	},
	ErasureTask, RecoveryStrategy, LOG_TARGET,
};

use polkadot_node_primitives::AvailableData;
use polkadot_node_subsystem::{overseer, RecoveryError};
use polkadot_primitives::ValidatorIndex;

use futures::{channel::oneshot, SinkExt};
use rand::seq::SliceRandom;
use std::collections::VecDeque;

/// Parameters specific to the `FetchChunks` strategy.
pub struct FetchChunksParams {
	pub n_validators: usize,
}

/// `RecoveryStrategy` that requests chunks from validators, in parallel.
pub struct FetchChunks {
	/// How many requests have been unsuccessful so far.
	error_count: usize,
	/// Total number of responses that have been received, including failed ones.
	total_received_responses: usize,
	/// A shuffled array of validator indices.
	validators: VecDeque<ValidatorIndex>,
	/// Collection of in-flight requests.
	requesting_chunks: OngoingRequests,
}

impl FetchChunks {
	/// Instantiate a new strategy.
	pub fn new(params: FetchChunksParams) -> Self {
		// Shuffle the validators to make sure that we don't request chunks from the same
		// validators over and over.
		let mut validators: VecDeque<ValidatorIndex> =
			(0..params.n_validators).map(|i| ValidatorIndex(i as u32)).collect();
		validators.make_contiguous().shuffle(&mut rand::thread_rng());

		Self {
			error_count: 0,
			total_received_responses: 0,
			validators,
			requesting_chunks: FuturesUndead::new(),
		}
	}

	fn is_unavailable(
		unrequested_validators: usize,
		in_flight_requests: usize,
		chunk_count: usize,
		threshold: usize,
	) -> bool {
		is_unavailable(chunk_count, in_flight_requests, unrequested_validators, threshold)
	}

	/// Desired number of parallel requests.
	///
	/// For the given threshold (total required number of chunks) get the desired number of
	/// requests we want to have running in parallel at this time.
	fn get_desired_request_count(&self, chunk_count: usize, threshold: usize) -> usize {
		// Upper bound for parallel requests.
		// We want to limit this, so requests can be processed within the timeout and we limit the
		// following feedback loop:
		// 1. Requests fail due to timeout
		// 2. We request more chunks to make up for it
		// 3. Bandwidth is spread out even more, so we get even more timeouts
		// 4. We request more chunks to make up for it ...
		let max_requests_boundary = std::cmp::min(N_PARALLEL, threshold);
		// How many chunks are still needed?
		let remaining_chunks = threshold.saturating_sub(chunk_count);
		// What is the current error rate, so we can make up for it?
		let inv_error_rate =
			self.total_received_responses.checked_div(self.error_count).unwrap_or(0);
		// Actual number of requests we want to have in flight in parallel:
		std::cmp::min(
			max_requests_boundary,
			remaining_chunks + remaining_chunks.checked_div(inv_error_rate).unwrap_or(0),
		)
	}

	async fn attempt_recovery<Sender: overseer::AvailabilityRecoverySenderTrait>(
		&mut self,
		state: &mut State,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		let recovery_duration = common_params
			.metrics
			.time_erasure_recovery(RecoveryStrategy::<Sender>::strategy_type(self));

		// Send request to reconstruct available data from chunks.
		let (avilable_data_tx, available_data_rx) = oneshot::channel();

		let mut erasure_task_tx = common_params.erasure_task_tx.clone();
		erasure_task_tx
			.send(ErasureTask::Reconstruct(
				common_params.n_validators,
				// Safe to leave an empty vec in place, as we're stopping the recovery process if
				// this reconstruct fails.
				std::mem::take(&mut state.received_chunks)
					.into_iter()
					.map(|(c_index, chunk)| (c_index, chunk.chunk))
					.collect(),
				avilable_data_tx,
			))
			.await
			.map_err(|_| RecoveryError::ChannelClosed)?;

		let available_data_response =
			available_data_rx.await.map_err(|_| RecoveryError::ChannelClosed)?;

		match available_data_response {
			// Attempt post-recovery check.
			Ok(data) => do_post_recovery_check(common_params, data)
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
						"Data recovery from chunks complete",
					);
					data
				}),
			Err(err) => {
				recovery_duration.map(|rd| rd.stop_and_discard());
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					?err,
					"Data recovery error",
				);

				Err(RecoveryError::Invalid)
			},
		}
	}
}

#[async_trait::async_trait]
impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender> for FetchChunks {
	fn display_name(&self) -> &'static str {
		"Fetch chunks"
	}

	fn strategy_type(&self) -> &'static str {
		"regular_chunks"
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
			self.validators.retain(|validator_index| {
				!local_chunk_indices.iter().any(|(v_index, _)| v_index == validator_index)
			});
		}

		// No need to query the validators that have the chunks we already received or that we know
		// don't have the data from previous strategies.
		self.validators.retain(|v_index| {
			!state.received_chunks.values().any(|c| v_index == &c.validator_index) &&
				state.can_retry_request(
					&(common_params.validator_authority_keys[v_index.0 as usize].clone(), *v_index),
					REGULAR_CHUNKS_REQ_RETRY_LIMIT,
				)
		});

		// Safe to `take` here, as we're consuming `self` anyway and we're not using the
		// `validators` field in other methods.
		let mut validators_queue: VecDeque<_> = std::mem::take(&mut self.validators)
			.into_iter()
			.map(|validator_index| {
				(
					common_params.validator_authority_keys[validator_index.0 as usize].clone(),
					validator_index,
				)
			})
			.collect();

		loop {
			// If received_chunks has more than threshold entries, attempt to recover the data.
			// If that fails, or a re-encoding of it doesn't match the expected erasure root,
			// return Err(RecoveryError::Invalid).
			// Do this before requesting any chunks because we may have enough of them coming from
			// past RecoveryStrategies.
			if state.chunk_count() >= common_params.threshold {
				return self.attempt_recovery::<Sender>(state, common_params).await
			}

			if Self::is_unavailable(
				validators_queue.len(),
				self.requesting_chunks.total_len(),
				state.chunk_count(),
				common_params.threshold,
			) {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					received = %state.chunk_count(),
					requesting = %self.requesting_chunks.len(),
					total_requesting = %self.requesting_chunks.total_len(),
					n_validators = %common_params.n_validators,
					"Data recovery from chunks is not possible",
				);

				return Err(RecoveryError::Unavailable)
			}

			let desired_requests_count =
				self.get_desired_request_count(state.chunk_count(), common_params.threshold);
			let already_requesting_count = self.requesting_chunks.len();
			gum::debug!(
				target: LOG_TARGET,
				?common_params.candidate_hash,
				?desired_requests_count,
				error_count= ?self.error_count,
				total_received = ?self.total_received_responses,
				threshold = ?common_params.threshold,
				?already_requesting_count,
				"Requesting availability chunks for a candidate",
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

			let (total_responses, error_count) = state
				.wait_for_chunks(
					strategy_type,
					common_params,
					REGULAR_CHUNKS_REQ_RETRY_LIMIT,
					&mut validators_queue,
					&mut self.requesting_chunks,
					&mut vec![],
					|unrequested_validators,
					 in_flight_reqs,
					 chunk_count,
					 _systematic_chunk_count| {
						chunk_count >= common_params.threshold ||
							Self::is_unavailable(
								unrequested_validators,
								in_flight_reqs,
								chunk_count,
								common_params.threshold,
							)
					},
				)
				.await;

			self.total_received_responses += total_responses;
			self.error_count += error_count;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_erasure_coding::recovery_threshold;

	#[test]
	fn test_get_desired_request_count() {
		let n_validators = 100;
		let threshold = recovery_threshold(n_validators).unwrap();

		let mut fetch_chunks_task = FetchChunks::new(FetchChunksParams { n_validators });
		assert_eq!(fetch_chunks_task.get_desired_request_count(0, threshold), threshold);
		fetch_chunks_task.error_count = 1;
		fetch_chunks_task.total_received_responses = 1;
		// We saturate at threshold (34):
		assert_eq!(fetch_chunks_task.get_desired_request_count(0, threshold), threshold);

		// We saturate at the parallel limit.
		assert_eq!(fetch_chunks_task.get_desired_request_count(0, N_PARALLEL + 2), N_PARALLEL);

		fetch_chunks_task.total_received_responses = 2;
		// With given error rate - still saturating:
		assert_eq!(fetch_chunks_task.get_desired_request_count(1, threshold), threshold);
		fetch_chunks_task.total_received_responses = 10;
		// error rate: 1/10
		// remaining chunks needed: threshold (34) - 9
		// expected: 24 * (1+ 1/10) = (next greater integer) = 27
		assert_eq!(fetch_chunks_task.get_desired_request_count(9, threshold), 27);
		// We saturate at the parallel limit.
		assert_eq!(fetch_chunks_task.get_desired_request_count(9, N_PARALLEL + 9), N_PARALLEL);

		fetch_chunks_task.error_count = 0;
		// With error count zero - we should fetch exactly as needed:
		assert_eq!(fetch_chunks_task.get_desired_request_count(10, threshold), threshold - 10);
	}
}

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

//! Recovery task and associated strategies.

#![warn(missing_docs)]

use crate::{
	futures_undead::FuturesUndead, is_chunk_valid, is_unavailable, metrics::Metrics, ErasureTask,
	PostRecoveryCheck, LOG_TARGET,
};
use futures::{channel::oneshot, SinkExt};
use parity_scale_codec::Encode;
#[cfg(not(test))]
use polkadot_node_network_protocol::request_response::CHUNK_REQUEST_TIMEOUT;
use polkadot_node_network_protocol::request_response::{
	self as req_res, outgoing::RequestError, OutgoingRequest, Recipient, Requests,
};
use polkadot_node_primitives::{AvailableData, ErasureChunk};
use polkadot_node_subsystem::{
	messages::{AvailabilityStoreMessage, NetworkBridgeTxMessage},
	overseer, RecoveryError,
};
use polkadot_primitives::{AuthorityDiscoveryId, CandidateHash, Hash, ValidatorIndex};
use rand::seq::SliceRandom;
use sc_network::{IfDisconnected, OutboundFailure, RequestFailure};
use std::{
	collections::{HashMap, VecDeque},
	time::Duration,
};

// How many parallel recovery tasks should be running at once.
const N_PARALLEL: usize = 50;

/// Time after which we consider a request to have failed
///
/// and we should try more peers. Note in theory the request times out at the network level,
/// measurements have shown, that in practice requests might actually take longer to fail in
/// certain occasions. (The very least, authority discovery is not part of the timeout.)
///
/// For the time being this value is the same as the timeout on the networking layer, but as this
/// timeout is more soft than the networking one, it might make sense to pick different values as
/// well.
#[cfg(not(test))]
const TIMEOUT_START_NEW_REQUESTS: Duration = CHUNK_REQUEST_TIMEOUT;
#[cfg(test)]
const TIMEOUT_START_NEW_REQUESTS: Duration = Duration::from_millis(100);

#[async_trait::async_trait]
/// Common trait for runnable recovery strategies.
pub trait RecoveryStrategy<Sender: overseer::AvailabilityRecoverySenderTrait>: Send {
	/// Main entry point of the strategy.
	async fn run(
		&mut self,
		state: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError>;

	/// Return the name of the strategy for logging purposes.
	fn display_name(&self) -> &'static str;
}

/// Recovery parameters common to all strategies in a `RecoveryTask`.
pub struct RecoveryParams {
	/// Discovery ids of `validators`.
	pub validator_authority_keys: Vec<AuthorityDiscoveryId>,

	/// Number of validators.
	pub n_validators: usize,

	/// The number of chunks needed.
	pub threshold: usize,

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
}

/// Intermediate/common data that must be passed between `RecoveryStrategy`s belonging to the
/// same `RecoveryTask`.
pub struct State {
	/// Chunks received so far.
	received_chunks: HashMap<ValidatorIndex, ErasureChunk>,
}

impl State {
	fn new() -> Self {
		Self { received_chunks: HashMap::new() }
	}

	fn insert_chunk(&mut self, validator: ValidatorIndex, chunk: ErasureChunk) {
		self.received_chunks.insert(validator, chunk);
	}

	fn chunk_count(&self) -> usize {
		self.received_chunks.len()
	}

	/// Retrieve the local chunks held in the av-store (either 0 or 1).
	async fn populate_from_av_store<Sender: overseer::AvailabilityRecoverySenderTrait>(
		&mut self,
		params: &RecoveryParams,
		sender: &mut Sender,
	) -> Vec<ValidatorIndex> {
		let (tx, rx) = oneshot::channel();
		sender
			.send_message(AvailabilityStoreMessage::QueryAllChunks(params.candidate_hash, tx))
			.await;

		match rx.await {
			Ok(chunks) => {
				// This should either be length 1 or 0. If we had the whole data,
				// we wouldn't have reached this stage.
				let chunk_indices: Vec<_> = chunks.iter().map(|c| c.index).collect();

				for chunk in chunks {
					if is_chunk_valid(params, &chunk) {
						gum::trace!(
							target: LOG_TARGET,
							candidate_hash = ?params.candidate_hash,
							validator_index = ?chunk.index,
							"Found valid chunk on disk"
						);
						self.insert_chunk(chunk.index, chunk);
					} else {
						gum::error!(
							target: LOG_TARGET,
							"Loaded invalid chunk from disk! Disk/Db corruption _very_ likely - please fix ASAP!"
						);
					};
				}

				chunk_indices
			},
			Err(oneshot::Canceled) => {
				gum::warn!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					"Failed to reach the availability store"
				);

				vec![]
			},
		}
	}

	/// Launch chunk requests in parallel, according to the parameters.
	async fn launch_parallel_chunk_requests<Sender>(
		&mut self,
		params: &RecoveryParams,
		sender: &mut Sender,
		desired_requests_count: usize,
		validators: &mut VecDeque<ValidatorIndex>,
		requesting_chunks: &mut FuturesUndead<
			Result<Option<ErasureChunk>, (ValidatorIndex, RequestError)>,
		>,
	) where
		Sender: overseer::AvailabilityRecoverySenderTrait,
	{
		let candidate_hash = &params.candidate_hash;
		let already_requesting_count = requesting_chunks.len();

		let mut requests = Vec::with_capacity(desired_requests_count - already_requesting_count);

		while requesting_chunks.len() < desired_requests_count {
			if let Some(validator_index) = validators.pop_back() {
				let validator = params.validator_authority_keys[validator_index.0 as usize].clone();
				gum::trace!(
					target: LOG_TARGET,
					?validator,
					?validator_index,
					?candidate_hash,
					"Requesting chunk",
				);

				// Request data.
				let raw_request = req_res::v1::ChunkFetchingRequest {
					candidate_hash: params.candidate_hash,
					index: validator_index,
				};

				let (req, res) = OutgoingRequest::new(Recipient::Authority(validator), raw_request);
				requests.push(Requests::ChunkFetchingV1(req));

				params.metrics.on_chunk_request_issued();
				let timer = params.metrics.time_chunk_request();

				requesting_chunks.push(Box::pin(async move {
					let _timer = timer;
					match res.await {
						Ok(req_res::v1::ChunkFetchingResponse::Chunk(chunk)) =>
							Ok(Some(chunk.recombine_into_chunk(&raw_request))),
						Ok(req_res::v1::ChunkFetchingResponse::NoSuchChunk) => Ok(None),
						Err(e) => Err((validator_index, e)),
					}
				}));
			} else {
				break
			}
		}

		sender
			.send_message(NetworkBridgeTxMessage::SendRequests(
				requests,
				IfDisconnected::TryConnect,
			))
			.await;
	}

	/// Wait for a sufficient amount of chunks to reconstruct according to the provided `params`.
	async fn wait_for_chunks(
		&mut self,
		params: &RecoveryParams,
		validators: &mut VecDeque<ValidatorIndex>,
		requesting_chunks: &mut FuturesUndead<
			Result<Option<ErasureChunk>, (ValidatorIndex, RequestError)>,
		>,
		can_conclude: impl Fn(usize, usize, usize, &RecoveryParams, usize) -> bool,
	) -> (usize, usize) {
		let metrics = &params.metrics;

		let mut total_received_responses = 0;
		let mut error_count = 0;

		// Wait for all current requests to conclude or time-out, or until we reach enough chunks.
		// We also declare requests undead, once `TIMEOUT_START_NEW_REQUESTS` is reached and will
		// return in that case for `launch_parallel_requests` to fill up slots again.
		while let Some(request_result) =
			requesting_chunks.next_with_timeout(TIMEOUT_START_NEW_REQUESTS).await
		{
			total_received_responses += 1;

			match request_result {
				Ok(Some(chunk)) =>
					if is_chunk_valid(params, &chunk) {
						metrics.on_chunk_request_succeeded();
						gum::trace!(
							target: LOG_TARGET,
							candidate_hash = ?params.candidate_hash,
							validator_index = ?chunk.index,
							"Received valid chunk",
						);
						self.insert_chunk(chunk.index, chunk);
					} else {
						metrics.on_chunk_request_invalid();
						error_count += 1;
					},
				Ok(None) => {
					metrics.on_chunk_request_no_such_chunk();
					error_count += 1;
				},
				Err((validator_index, e)) => {
					error_count += 1;

					gum::trace!(
						target: LOG_TARGET,
						candidate_hash= ?params.candidate_hash,
						err = ?e,
						?validator_index,
						"Failure requesting chunk",
					);

					match e {
						RequestError::InvalidResponse(_) => {
							metrics.on_chunk_request_invalid();

							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?params.candidate_hash,
								err = ?e,
								?validator_index,
								"Chunk fetching response was invalid",
							);
						},
						RequestError::NetworkError(err) => {
							// No debug logs on general network errors - that became very spammy
							// occasionally.
							if let RequestFailure::Network(OutboundFailure::Timeout) = err {
								metrics.on_chunk_request_timeout();
							} else {
								metrics.on_chunk_request_error();
							}

							validators.push_front(validator_index);
						},
						RequestError::Canceled(_) => {
							metrics.on_chunk_request_error();

							validators.push_front(validator_index);
						},
					}
				},
			}

			// Stop waiting for requests when we either can already recover the data
			// or have gotten firm 'No' responses from enough validators.
			if can_conclude(
				validators.len(),
				requesting_chunks.total_len(),
				self.chunk_count(),
				params,
				error_count,
			) {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					received_chunks_count = ?self.chunk_count(),
					requested_chunks_count = ?requesting_chunks.len(),
					threshold = ?params.threshold,
					"Can conclude availability for a candidate",
				);
				break
			}
		}

		(total_received_responses, error_count)
	}
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

		while let Some(mut current_strategy) = self.strategies.pop_front() {
			gum::debug!(
				target: LOG_TARGET,
				candidate_hash = ?self.params.candidate_hash,
				"Starting `{}` strategy",
				current_strategy.display_name(),
			);

			let res = current_strategy.run(&mut self.state, &mut self.sender, &self.params).await;

			match res {
				Err(RecoveryError::Unavailable) =>
					if self.strategies.front().is_some() {
						gum::debug!(
							target: LOG_TARGET,
							candidate_hash = ?self.params.candidate_hash,
							"Recovery strategy `{}` did not conclude. Trying the next one.",
							current_strategy.display_name(),
						);
						continue
					},
				Err(err) => {
					match &err {
						RecoveryError::Invalid => self.params.metrics.on_recovery_invalid(),
						_ => self.params.metrics.on_recovery_failed(),
					}
					return Err(err)
				},
				Ok(data) => {
					self.params.metrics.on_recovery_succeeded(data.encoded_size());
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
		self.params.metrics.on_recovery_failed();

		Err(RecoveryError::Unavailable)
	}
}

/// `RecoveryStrategy` that sequentially tries to fetch the full `AvailableData` from
/// already-connected validators in the configured validator set.
pub struct FetchFull {
	params: FetchFullParams,
}

pub struct FetchFullParams {
	/// Validators that will be used for fetching the data.
	pub validators: Vec<ValidatorIndex>,
	/// Channel to the erasure task handler.
	pub erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

impl FetchFull {
	/// Create a new `FetchFull` recovery strategy.
	pub fn new(mut params: FetchFullParams) -> Self {
		params.validators.shuffle(&mut rand::thread_rng());
		Self { params }
	}
}

#[async_trait::async_trait]
impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender> for FetchFull {
	fn display_name(&self) -> &'static str {
		"Full recovery from backers"
	}

	async fn run(
		&mut self,
		_: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		loop {
			// Pop the next validator, and proceed to next fetch_chunks_task if we're out.
			let validator_index =
				self.params.validators.pop().ok_or_else(|| RecoveryError::Unavailable)?;

			// Request data.
			let (req, response) = OutgoingRequest::new(
				Recipient::Authority(
					common_params.validator_authority_keys[validator_index.0 as usize].clone(),
				),
				req_res::v1::AvailableDataFetchingRequest {
					candidate_hash: common_params.candidate_hash,
				},
			);

			sender
				.send_message(NetworkBridgeTxMessage::SendRequests(
					vec![Requests::AvailableDataFetchingV1(req)],
					IfDisconnected::ImmediateError,
				))
				.await;

			match response.await {
				Ok(req_res::v1::AvailableDataFetchingResponse::AvailableData(data)) => {
					let maybe_data = match common_params.post_recovery_check {
						PostRecoveryCheck::Reencode => {
							let (reencode_tx, reencode_rx) = oneshot::channel();
							self.params
								.erasure_task_tx
								.send(ErasureTask::Reencode(
									common_params.n_validators,
									common_params.erasure_root,
									data,
									reencode_tx,
								))
								.await
								.map_err(|_| RecoveryError::ChannelClosed)?;

							reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?
						},
						PostRecoveryCheck::PovHash =>
							(data.pov.hash() == common_params.pov_hash).then_some(data),
					};

					match maybe_data {
						Some(data) => {
							gum::trace!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								"Received full data",
							);

							return Ok(data)
						},
						None => {
							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								?validator_index,
								"Invalid data response",
							);

							// it doesn't help to report the peer with req/res.
							// we'll try the next backer.
						},
					};
				},
				Ok(req_res::v1::AvailableDataFetchingResponse::NoSuchData) => {},
				Err(e) => gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					?validator_index,
					err = ?e,
					"Error fetching full available data."
				),
			}
		}
	}
}

/// `RecoveryStrategy` that requests chunks from validators, in parallel.
pub struct FetchChunks {
	/// How many requests have been unsuccessful so far.
	error_count: usize,
	/// Total number of responses that have been received, including failed ones.
	total_received_responses: usize,
	/// Collection of in-flight requests.
	requesting_chunks: FuturesUndead<Result<Option<ErasureChunk>, (ValidatorIndex, RequestError)>>,
	/// A random shuffling of the validators which indicates the order in which we connect to the
	/// validators and request the chunk from them.
	validators: VecDeque<ValidatorIndex>,
	/// Channel to the erasure task handler.
	erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

/// Parameters specific to the `FetchChunks` strategy.
pub struct FetchChunksParams {
	/// Total number of validators.
	pub n_validators: usize,
	/// Channel to the erasure task handler.
	pub erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

impl FetchChunks {
	/// Instantiate a new strategy.
	pub fn new(params: FetchChunksParams) -> Self {
		let mut shuffling: Vec<_> = (0..params.n_validators)
			.map(|i| ValidatorIndex(i.try_into().expect("number of validators must fit in a u32")))
			.collect();
		shuffling.shuffle(&mut rand::thread_rng());

		Self {
			error_count: 0,
			total_received_responses: 0,
			requesting_chunks: FuturesUndead::new(),
			validators: shuffling.into(),
			erasure_task_tx: params.erasure_task_tx,
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

	async fn attempt_recovery(
		&mut self,
		state: &mut State,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		let recovery_duration = common_params.metrics.time_erasure_recovery();

		// Send request to reconstruct available data from chunks.
		let (avilable_data_tx, available_data_rx) = oneshot::channel();
		self.erasure_task_tx
			.send(ErasureTask::Reconstruct(
				common_params.n_validators,
				// Safe to leave an empty vec in place, as we're stopping the recovery process if
				// this reconstruct fails.
				std::mem::take(&mut state.received_chunks),
				avilable_data_tx,
			))
			.await
			.map_err(|_| RecoveryError::ChannelClosed)?;

		let available_data_response =
			available_data_rx.await.map_err(|_| RecoveryError::ChannelClosed)?;

		match available_data_response {
			Ok(data) => {
				let maybe_data = match common_params.post_recovery_check {
					PostRecoveryCheck::Reencode => {
						// Send request to re-encode the chunks and check merkle root.
						let (reencode_tx, reencode_rx) = oneshot::channel();
						self.erasure_task_tx
							.send(ErasureTask::Reencode(
								common_params.n_validators,
								common_params.erasure_root,
								data,
								reencode_tx,
							))
							.await
							.map_err(|_| RecoveryError::ChannelClosed)?;

						reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?.or_else(|| {
							gum::trace!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								erasure_root = ?common_params.erasure_root,
								"Data recovery error - root mismatch",
							);
							None
						})
					},
					PostRecoveryCheck::PovHash =>
						(data.pov.hash() == common_params.pov_hash).then_some(data).or_else(|| {
							gum::trace!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								pov_hash = ?common_params.pov_hash,
								"Data recovery error - PoV hash mismatch",
							);
							None
						}),
				};

				if let Some(data) = maybe_data {
					gum::trace!(
						target: LOG_TARGET,
						candidate_hash = ?common_params.candidate_hash,
						erasure_root = ?common_params.erasure_root,
						"Data recovery from chunks complete",
					);

					Ok(data)
				} else {
					recovery_duration.map(|rd| rd.stop_and_discard());

					Err(RecoveryError::Invalid)
				}
			},
			Err(err) => {
				recovery_duration.map(|rd| rd.stop_and_discard());
				gum::trace!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					?err,
					"Data recovery error ",
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

	async fn run(
		&mut self,
		state: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		// First query the store for any chunks we've got.
		if !common_params.bypass_availability_store {
			let local_chunk_indices = state.populate_from_av_store(common_params, sender).await;
			self.validators.retain(|i| !local_chunk_indices.contains(i));
		}

		// No need to query the validators that have the chunks we already received.
		self.validators.retain(|i| !state.received_chunks.contains_key(i));

		loop {
			// If received_chunks has more than threshold entries, attempt to recover the data.
			// If that fails, or a re-encoding of it doesn't match the expected erasure root,
			// return Err(RecoveryError::Invalid).
			// Do this before requesting any chunks because we may have enough of them coming from
			// past RecoveryStrategies.
			if state.chunk_count() >= common_params.threshold {
				return self.attempt_recovery(state, common_params).await
			}

			if Self::is_unavailable(
				self.validators.len(),
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
			state
				.launch_parallel_chunk_requests(
					common_params,
					sender,
					desired_requests_count,
					&mut self.validators,
					&mut self.requesting_chunks,
				)
				.await;

			let (total_responses, error_count) = state
				.wait_for_chunks(
					common_params,
					&mut self.validators,
					&mut self.requesting_chunks,
					|unrequested_validators, reqs, chunk_count, params, _error_count| {
						chunk_count >= params.threshold ||
							Self::is_unavailable(
								unrequested_validators,
								reqs,
								chunk_count,
								params.threshold,
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
	fn parallel_request_calculation_works_as_expected() {
		let num_validators = 100;
		let threshold = recovery_threshold(num_validators).unwrap();
		let (erasure_task_tx, _erasure_task_rx) = futures::channel::mpsc::channel(16);

		let mut fetch_chunks_task =
			FetchChunks::new(FetchChunksParams { n_validators: 100, erasure_task_tx });
		assert_eq!(fetch_chunks_task.get_desired_request_count(0, threshold), threshold);
		fetch_chunks_task.error_count = 1;
		fetch_chunks_task.total_received_responses = 1;
		// We saturate at threshold (34):
		assert_eq!(fetch_chunks_task.get_desired_request_count(0, threshold), threshold);

		fetch_chunks_task.total_received_responses = 2;
		// With given error rate - still saturating:
		assert_eq!(fetch_chunks_task.get_desired_request_count(1, threshold), threshold);
		fetch_chunks_task.total_received_responses += 8;
		// error rate: 1/10
		// remaining chunks needed: threshold (34) - 9
		// expected: 24 * (1+ 1/10) = (next greater integer) = 27
		assert_eq!(fetch_chunks_task.get_desired_request_count(9, threshold), 27);
		fetch_chunks_task.error_count = 0;
		// With error count zero - we should fetch exactly as needed:
		assert_eq!(fetch_chunks_task.get_desired_request_count(10, threshold), threshold - 10);
	}
}

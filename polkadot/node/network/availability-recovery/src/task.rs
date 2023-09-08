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

use crate::{
	futures_undead::FuturesUndead, is_chunk_valid, is_unavailable, metrics::Metrics, ErasureTask,
	LOG_TARGET,
};
use futures::{channel::oneshot, SinkExt};
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

pub struct RecoveryParams {
	/// Discovery ids of `validators`.
	pub(crate) validator_authority_keys: Vec<AuthorityDiscoveryId>,

	/// Number of validators relevant to this `RecoveryTask`.
	pub(crate) n_validators: usize,

	/// The number of pieces needed.
	pub(crate) threshold: usize,

	/// A hash of the relevant candidate.
	pub(crate) candidate_hash: CandidateHash,

	/// The root of the erasure encoding of the para block.
	pub(crate) erasure_root: Hash,

	/// Metrics to report
	pub(crate) metrics: Metrics,

	/// Do not request data from availability-store
	pub(crate) bypass_availability_store: bool,
}
/// Represents intermediate data that must be passed between `RecoveryStrategy`s belonging to the
/// same `RecoveryTask` or data that is used by state methods common to multiple RecoveryStrategies.
pub struct State {
	/// Chunks received so far.
	received_chunks: HashMap<ValidatorIndex, ErasureChunk>,
	requesting_chunks: FuturesUndead<Result<Option<ErasureChunk>, (ValidatorIndex, RequestError)>>,
}

impl State {
	fn new() -> Self {
		Self { received_chunks: HashMap::new(), requesting_chunks: FuturesUndead::new() }
	}

	fn insert_chunk(&mut self, validator: ValidatorIndex, chunk: ErasureChunk) {
		self.received_chunks.insert(validator, chunk);
	}

	fn chunk_count(&self) -> usize {
		self.received_chunks.len()
	}

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

	async fn launch_parallel_chunk_requests<Sender>(
		&mut self,
		params: &RecoveryParams,
		sender: &mut Sender,
		desired_requests_count: usize,
		validators: &mut VecDeque<ValidatorIndex>,
	) where
		Sender: overseer::AvailabilityRecoverySenderTrait,
	{
		let candidate_hash = &params.candidate_hash;
		let already_requesting_count = self.requesting_chunks.len();

		let mut requests = Vec::with_capacity(desired_requests_count - already_requesting_count);

		while self.requesting_chunks.len() < desired_requests_count {
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

				self.requesting_chunks.push(Box::pin(async move {
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
		can_conclude: impl Fn(usize, usize, usize, &RecoveryParams, usize) -> bool,
	) -> (usize, usize) {
		let metrics = &params.metrics;

		let mut total_received_responses = 0;
		let mut error_count = 0;

		// Wait for all current requests to conclude or time-out, or until we reach enough chunks.
		// We also declare requests undead, once `TIMEOUT_START_NEW_REQUESTS` is reached and will
		// return in that case for `launch_parallel_requests` to fill up slots again.
		while let Some(request_result) =
			self.requesting_chunks.next_with_timeout(TIMEOUT_START_NEW_REQUESTS).await
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
				self.requesting_chunks.total_len(),
				self.chunk_count(),
				params,
				error_count,
			) {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					received_chunks_count = ?self.chunk_count(),
					requested_chunks_count = ?self.requesting_chunks.len(),
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
pub struct RecoveryTask<Sender> {
	sender: Sender,
	/// The common parameters of the recovery process, regardless of the strategy.
	params: RecoveryParams,
	strategy: RecoveryStrategy,
	state: State,
}

impl<Sender> RecoveryTask<Sender> {
	pub fn new(sender: Sender, params: RecoveryParams, strategy: RecoveryStrategy) -> Self {
		Self { sender, params, strategy, state: State::new() }
	}
}

impl<Sender> RecoveryTask<Sender>
where
	Sender: overseer::AvailabilityRecoverySenderTrait,
{
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

	pub async fn run(mut self) -> Result<AvailableData, RecoveryError> {
		if let Some(data) = self.in_availability_store().await {
			return Ok(data)
		}

		self.params.metrics.on_recovery_started();

		let _timer = self.params.metrics.time_full_recovery();

		let res = loop {
			let (current_strategy, next_strategy) = self.strategy.pop_first();
			self.strategy = next_strategy;

			// Make sure we are not referencing futures from past RecoveryStrategy runs.
			if self.state.requesting_chunks.total_len() != 0 {
				self.state.requesting_chunks = FuturesUndead::new();
			}

			let recovery_strategy_name = current_strategy.display_name();

			if let Some(name) = recovery_strategy_name {
				gum::info!(
					target: LOG_TARGET,
					candidate_hash = ?self.params.candidate_hash,
					"Starting `{}` strategy",
					&name,
				);
			}

			let res = match current_strategy {
				RecoveryStrategy::Nil => Err(RecoveryError::Unavailable),
				RecoveryStrategy::FullFromBackers(inner, _) =>
					inner.run(&mut self.state, &mut self.sender, &self.params).await,
				RecoveryStrategy::ChunksFromValidators(inner, _) =>
					inner.run(&mut self.state, &mut self.sender, &self.params).await,
			};

			match res {
				Err(RecoveryError::Unavailable) => {
					if !matches!(&self.strategy, RecoveryStrategy::Nil) {
						if let Some(recovery_strategy_name) = recovery_strategy_name {
							gum::warn!(
								target: LOG_TARGET,
								candidate_hash = ?self.params.candidate_hash,
								"Recovery strategy `{}` did not conclude. Trying the next one.",
								recovery_strategy_name,
							);
						}
						continue
					} else {
						// We have no other strategies to try.
						gum::error!(
							target: LOG_TARGET,
							candidate_hash = ?self.params.candidate_hash,
							"Recovery of available data failed.",
						);
						break Err(RecoveryError::Unavailable)
					}
				},
				Err(err) => break Err(err),
				Ok(data) => break Ok(data),
			}
		};

		match &res {
			Ok(_) => self.params.metrics.on_recovery_succeeded(),
			Err(RecoveryError::Invalid) => self.params.metrics.on_recovery_invalid(),
			Err(_) => self.params.metrics.on_recovery_failed(),
		}

		res
	}
}

pub enum RecoveryStrategy {
	Nil,
	FullFromBackers(FetchFull, Box<RecoveryStrategy>),
	ChunksFromValidators(FetchChunks, Box<RecoveryStrategy>),
}

impl RecoveryStrategy {
	pub fn new() -> Box<Self> {
		Box::new(RecoveryStrategy::Nil)
	}

	fn display_name(&self) -> Option<&'static str> {
		match self {
			Self::Nil => None,
			Self::FullFromBackers(_, _) => Some("Full recovery from backers"),
			Self::ChunksFromValidators(_, _) => Some("Chunks recovery"),
		}
	}

	pub fn then_fetch_full_from_backers(self: Box<Self>, params: FetchFullParams) -> Box<Self> {
		match *self {
			Self::Nil => Box::new(Self::FullFromBackers(FetchFull::new(params), self)),
			Self::ChunksFromValidators(task, next) => {
				let next = next.then_fetch_full_from_backers(params);
				Box::new(Self::ChunksFromValidators(task, next))
			},
			Self::FullFromBackers(task, next) => {
				let next = next.then_fetch_full_from_backers(params);
				Box::new(Self::FullFromBackers(task, next))
			},
		}
	}

	pub fn then_fetch_chunks_from_validators(
		self: Box<Self>,
		params: FetchChunksParams,
	) -> Box<Self> {
		match *self {
			Self::Nil => Box::new(Self::ChunksFromValidators(FetchChunks::new(params), self)),
			Self::ChunksFromValidators(task, next) => {
				let next = next.then_fetch_chunks_from_validators(params);
				Box::new(Self::ChunksFromValidators(task, next))
			},
			Self::FullFromBackers(task, next) => {
				let next = next.then_fetch_chunks_from_validators(params);
				Box::new(Self::FullFromBackers(task, next))
			},
		}
	}

	fn pop_first(self: Self) -> (Self, Self) {
		match self {
			Self::Nil => (Self::Nil, Self::Nil),
			Self::FullFromBackers(inner, next) =>
				(Self::FullFromBackers(inner, Box::new(Self::Nil)), *next),
			Self::ChunksFromValidators(inner, next) =>
				(Self::ChunksFromValidators(inner, Box::new(Self::Nil)), *next),
		}
	}
}

pub struct FetchFull {
	params: FetchFullParams,
}

pub struct FetchFullParams {
	pub(crate) group_name: &'static str,
	pub(crate) validators: Vec<ValidatorIndex>,
	pub(crate) skip_if: Box<dyn Fn() -> bool + Send>,
	// channel to the erasure task handler.
	pub(crate) erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

impl FetchFull {
	fn new(params: FetchFullParams) -> Self {
		Self { params }
	}

	async fn run<Sender: overseer::AvailabilityRecoverySenderTrait>(
		mut self,
		_: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		if (self.params.skip_if)() {
			gum::trace!(
				target: LOG_TARGET,
				candidate_hash = ?common_params.candidate_hash,
				erasure_root = ?common_params.erasure_root,
				"Skipping requesting availability data from {}",
				self.params.group_name
			);

			return Err(RecoveryError::Unavailable)
		}

		gum::trace!(
			target: LOG_TARGET,
			candidate_hash = ?common_params.candidate_hash,
			erasure_root = ?common_params.erasure_root,
			"Requesting full availability data from {}",
			self.params.group_name
		);
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

					let reencode_response =
						reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?;

					if let Some(data) = reencode_response {
						gum::trace!(
							target: LOG_TARGET,
							candidate_hash = ?common_params.candidate_hash,
							"Received full data",
						);

						return Ok(data)
					} else {
						gum::debug!(
							target: LOG_TARGET,
							candidate_hash = ?common_params.candidate_hash,
							?validator_index,
							"Invalid data response",
						);

						// it doesn't help to report the peer with req/res.
					}
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

pub struct FetchChunks {
	/// How many request have been unsuccessful so far.
	error_count: usize,
	/// Total number of responses that have been received.
	///
	/// including failed ones.
	total_received_responses: usize,

	/// a random shuffling of the validators which indicates the order in which we connect to the
	/// validators and request the chunk from them.
	validators: VecDeque<ValidatorIndex>,

	// channel to the erasure task handler.
	erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

pub struct FetchChunksParams {
	pub(crate) n_validators: usize,
	// channel to the erasure task handler.
	pub(crate) erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
}

impl FetchChunks {
	fn new(params: FetchChunksParams) -> Self {
		let mut shuffling: Vec<_> = (0..params.n_validators)
			.map(|i| ValidatorIndex(i.try_into().expect("number of validators must fit in a u32")))
			.collect();
		shuffling.shuffle(&mut rand::thread_rng());

		Self {
			error_count: 0,
			total_received_responses: 0,
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

	async fn run<Sender: overseer::AvailabilityRecoverySenderTrait>(
		mut self,
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
				state.requesting_chunks.total_len(),
				state.chunk_count(),
				common_params.threshold,
			) {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?common_params.candidate_hash,
					erasure_root = ?common_params.erasure_root,
					received = %state.chunk_count(),
					requesting = %state.requesting_chunks.len(),
					total_requesting = %state.requesting_chunks.total_len(),
					n_validators = %common_params.n_validators,
					"Data recovery from chunks is not possible",
				);

				return Err(RecoveryError::Unavailable)
			}

			let desired_requests_count =
				self.get_desired_request_count(state.chunk_count(), common_params.threshold);
			let already_requesting_count = state.requesting_chunks.len();
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
				)
				.await;

			let (total_responses, error_count) = state
				.wait_for_chunks(
					common_params,
					&mut self.validators,
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

				let reencode_response =
					reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?;

				if let Some(data) = reencode_response {
					gum::trace!(
						target: LOG_TARGET,
						candidate_hash = ?common_params.candidate_hash,
						erasure_root = ?common_params.erasure_root,
						"Data recovery from chunks complete",
					);

					Ok(data)
				} else {
					recovery_duration.map(|rd| rd.stop_and_discard());
					gum::trace!(
						target: LOG_TARGET,
						candidate_hash = ?common_params.candidate_hash,
						erasure_root = ?common_params.erasure_root,
						"Data recovery error - root mismatch",
					);

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

#[cfg(test)]
mod tests {
	use std::ops::Deref;

	use super::*;
	use assert_matches::assert_matches;
	use polkadot_erasure_coding::recovery_threshold;
	use RecoveryStrategy::*;

	impl std::fmt::Debug for RecoveryStrategy {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			match self {
				Nil => write!(f, "Nil"),
				ChunksFromValidators(_, next) =>
					write!(f, "{:?} -> {}", self.display_name(), format!("{next:?}")),
				FullFromBackers(_, next) =>
					write!(f, "{:?} -> {}", self.display_name(), format!("{next:?}")),
			}
		}
	}

	#[test]
	fn test_recovery_strategy_linked_list_ops() {
		let fetch_full_params = FetchFullParams {
			group_name: "backers",
			validators: vec![],
			skip_if: Box::new(|| true),
			erasure_task_tx: futures::channel::mpsc::channel(0).0,
		};
		let fetch_full_params_2 = FetchFullParams {
			group_name: "approval-checkers",
			validators: vec![],
			skip_if: Box::new(|| true),
			erasure_task_tx: futures::channel::mpsc::channel(0).0,
		};

		let fetch_chunks_params = FetchChunksParams {
			n_validators: 2,
			erasure_task_tx: futures::channel::mpsc::channel(0).0,
		};
		let fetch_chunks_params_2 = FetchChunksParams {
			n_validators: 3,
			erasure_task_tx: futures::channel::mpsc::channel(0).0,
		};
		let recovery_strategy = RecoveryStrategy::new()
			.then_fetch_full_from_backers(fetch_full_params)
			.then_fetch_full_from_backers(fetch_full_params_2)
			.then_fetch_chunks_from_validators(fetch_chunks_params)
			.then_fetch_chunks_from_validators(fetch_chunks_params_2);

		// Check that the builder correctly chains strategies.
		assert_matches!(
				recovery_strategy.deref(),
				FullFromBackers(_, next)
					if matches!(next.deref(), FullFromBackers(_, next)
						if matches!(next.deref(), ChunksFromValidators(_, next)
							if matches!(next.deref(), ChunksFromValidators(_, next)
								if matches!(next.deref(), Nil)
							)
						)
					)
		);

		// Check the order for the `pop_first` operation.
		let (current, next) = recovery_strategy.pop_first();
		assert_matches!(current, FullFromBackers(task, next) if task.params.group_name == "backers" && matches!(*next, Nil));
		assert_matches!(&next, FullFromBackers(task, _) if task.params.group_name == "approval-checkers");

		let (current, next) = next.pop_first();
		assert_matches!(current, FullFromBackers(task, next) if task.params.group_name == "approval-checkers" && matches!(*next, Nil));
		assert_matches!(&next, ChunksFromValidators(task, _) if task.validators.len() == 2);

		let (current, next) = next.pop_first();
		assert_matches!(current, ChunksFromValidators(task, next) if task.validators.len() == 2 && matches!(*next, Nil));
		assert_matches!(&next, ChunksFromValidators(task, _) if task.validators.len() == 3);

		let (current, next) = next.pop_first();
		assert_matches!(current, ChunksFromValidators(task, next) if task.validators.len() == 3 && matches!(*next, Nil));
		assert_matches!(&next, Nil);

		let (current, next) = next.pop_first();
		assert_matches!(current, Nil);
		assert_matches!(next, Nil);
	}

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

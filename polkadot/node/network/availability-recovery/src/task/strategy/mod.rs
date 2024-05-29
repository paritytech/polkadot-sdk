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

//! Recovery strategies.

mod chunks;
mod full;
mod systematic;

pub use self::{
	chunks::{FetchChunks, FetchChunksParams},
	full::{FetchFull, FetchFullParams},
	systematic::{FetchSystematicChunks, FetchSystematicChunksParams},
};
use crate::{
	futures_undead::FuturesUndead, ErasureTask, PostRecoveryCheck, RecoveryParams, LOG_TARGET,
};

use futures::{channel::oneshot, SinkExt};
use codec::Decode;
use polkadot_erasure_coding::branch_hash;
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
use polkadot_primitives::{AuthorityDiscoveryId, BlakeTwo256, ChunkIndex, HashT, ValidatorIndex};
use sc_network::{IfDisconnected, OutboundFailure, ProtocolName, RequestFailure};
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	time::Duration,
};

// How many parallel chunk fetching requests should be running at once.
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

/// The maximum number of times systematic chunk recovery will try making a request for a given
/// (validator,chunk) pair, if the error was not fatal. Added so that we don't get stuck in an
/// infinite retry loop.
pub const SYSTEMATIC_CHUNKS_REQ_RETRY_LIMIT: u32 = 2;
/// The maximum number of times regular chunk recovery will try making a request for a given
/// (validator,chunk) pair, if the error was not fatal. Added so that we don't get stuck in an
/// infinite retry loop.
pub const REGULAR_CHUNKS_REQ_RETRY_LIMIT: u32 = 5;

// Helpful type alias for tracking ongoing chunk requests.
type OngoingRequests = FuturesUndead<(
	AuthorityDiscoveryId,
	ValidatorIndex,
	Result<(Option<ErasureChunk>, ProtocolName), RequestError>,
)>;

const fn is_unavailable(
	received_chunks: usize,
	requesting_chunks: usize,
	unrequested_validators: usize,
	threshold: usize,
) -> bool {
	received_chunks + requesting_chunks + unrequested_validators < threshold
}

/// Check validity of a chunk.
fn is_chunk_valid(params: &RecoveryParams, chunk: &ErasureChunk) -> bool {
	let anticipated_hash =
		match branch_hash(&params.erasure_root, chunk.proof(), chunk.index.0 as usize) {
			Ok(hash) => hash,
			Err(e) => {
				gum::debug!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					chunk_index = ?chunk.index,
					error = ?e,
					"Invalid Merkle proof",
				);
				return false
			},
		};
	let erasure_chunk_hash = BlakeTwo256::hash(&chunk.chunk);
	if anticipated_hash != erasure_chunk_hash {
		gum::debug!(
			target: LOG_TARGET,
			candidate_hash = ?params.candidate_hash,
			chunk_index = ?chunk.index,
			"Merkle proof mismatch"
		);
		return false
	}
	true
}

/// Perform the validity checks after recovery.
async fn do_post_recovery_check(
	params: &RecoveryParams,
	data: AvailableData,
) -> Result<AvailableData, RecoveryError> {
	let mut erasure_task_tx = params.erasure_task_tx.clone();
	match params.post_recovery_check {
		PostRecoveryCheck::Reencode => {
			// Send request to re-encode the chunks and check merkle root.
			let (reencode_tx, reencode_rx) = oneshot::channel();
			erasure_task_tx
				.send(ErasureTask::Reencode(
					params.n_validators,
					params.erasure_root,
					data,
					reencode_tx,
				))
				.await
				.map_err(|_| RecoveryError::ChannelClosed)?;

			reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?.ok_or_else(|| {
				gum::trace!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					erasure_root = ?params.erasure_root,
					"Data recovery error - root mismatch",
				);
				RecoveryError::Invalid
			})
		},
		PostRecoveryCheck::PovHash => {
			let pov = data.pov.clone();
			(pov.hash() == params.pov_hash).then_some(data).ok_or_else(|| {
				gum::trace!(
					target: LOG_TARGET,
					candidate_hash = ?params.candidate_hash,
					expected_pov_hash = ?params.pov_hash,
					actual_pov_hash = ?pov.hash(),
					"Data recovery error - PoV hash mismatch",
				);
				RecoveryError::Invalid
			})
		},
	}
}

#[async_trait::async_trait]
/// Common trait for runnable recovery strategies.
pub trait RecoveryStrategy<Sender: overseer::AvailabilityRecoverySenderTrait>: Send {
	/// Main entry point of the strategy.
	async fn run(
		mut self: Box<Self>,
		state: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError>;

	/// Return the name of the strategy for logging purposes.
	fn display_name(&self) -> &'static str;

	/// Return the strategy type for use as a metric label.
	fn strategy_type(&self) -> &'static str;
}

/// Utility type used for recording the result of requesting a chunk from a validator.
enum ErrorRecord {
	NonFatal(u32),
	Fatal,
}

/// Helper struct used for the `received_chunks` mapping.
/// Compared to `ErasureChunk`, it doesn't need to hold the `ChunkIndex` (because it's the key used
/// for the map) and proof, but needs to hold the `ValidatorIndex` instead.
struct Chunk {
	/// The erasure-encoded chunk of data belonging to the candidate block.
	chunk: Vec<u8>,
	/// The validator index that corresponds to this chunk. Not always the same as the chunk index.
	validator_index: ValidatorIndex,
}

/// Intermediate/common data that must be passed between `RecoveryStrategy`s belonging to the
/// same `RecoveryTask`.
pub struct State {
	/// Chunks received so far.
	/// This MUST be a `BTreeMap` in order for systematic recovery to work (the algorithm assumes
	/// that chunks are ordered by their index). If we ever switch this to some non-ordered
	/// collection, we need to add a sort step to the systematic recovery.
	received_chunks: BTreeMap<ChunkIndex, Chunk>,

	/// A record of errors returned when requesting a chunk from a validator.
	recorded_errors: HashMap<(AuthorityDiscoveryId, ValidatorIndex), ErrorRecord>,
}

impl State {
	pub fn new() -> Self {
		Self { received_chunks: BTreeMap::new(), recorded_errors: HashMap::new() }
	}

	fn insert_chunk(&mut self, chunk_index: ChunkIndex, chunk: Chunk) {
		self.received_chunks.insert(chunk_index, chunk);
	}

	fn chunk_count(&self) -> usize {
		self.received_chunks.len()
	}

	fn systematic_chunk_count(&self, systematic_threshold: usize) -> usize {
		self.received_chunks
			.range(ChunkIndex(0)..ChunkIndex(systematic_threshold as u32))
			.count()
	}

	fn record_error_fatal(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		validator_index: ValidatorIndex,
	) {
		self.recorded_errors.insert((authority_id, validator_index), ErrorRecord::Fatal);
	}

	fn record_error_non_fatal(
		&mut self,
		authority_id: AuthorityDiscoveryId,
		validator_index: ValidatorIndex,
	) {
		self.recorded_errors
			.entry((authority_id, validator_index))
			.and_modify(|record| {
				if let ErrorRecord::NonFatal(ref mut count) = record {
					*count = count.saturating_add(1);
				}
			})
			.or_insert(ErrorRecord::NonFatal(1));
	}

	fn can_retry_request(
		&self,
		key: &(AuthorityDiscoveryId, ValidatorIndex),
		retry_threshold: u32,
	) -> bool {
		match self.recorded_errors.get(key) {
			None => true,
			Some(entry) => match entry {
				ErrorRecord::Fatal => false,
				ErrorRecord::NonFatal(count) if *count < retry_threshold => true,
				ErrorRecord::NonFatal(_) => false,
			},
		}
	}

	/// Retrieve the local chunks held in the av-store (should be either 0 or 1).
	async fn populate_from_av_store<Sender: overseer::AvailabilityRecoverySenderTrait>(
		&mut self,
		params: &RecoveryParams,
		sender: &mut Sender,
	) -> Vec<(ValidatorIndex, ChunkIndex)> {
		let (tx, rx) = oneshot::channel();
		sender
			.send_message(AvailabilityStoreMessage::QueryAllChunks(params.candidate_hash, tx))
			.await;

		match rx.await {
			Ok(chunks) => {
				// This should either be length 1 or 0. If we had the whole data,
				// we wouldn't have reached this stage.
				let chunk_indices: Vec<_> = chunks
					.iter()
					.map(|(validator_index, chunk)| (*validator_index, chunk.index))
					.collect();

				for (validator_index, chunk) in chunks {
					if is_chunk_valid(params, &chunk) {
						gum::trace!(
							target: LOG_TARGET,
							candidate_hash = ?params.candidate_hash,
							chunk_index = ?chunk.index,
							"Found valid chunk on disk"
						);
						self.insert_chunk(
							chunk.index,
							Chunk { chunk: chunk.chunk, validator_index },
						);
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
		strategy_type: &str,
		params: &RecoveryParams,
		sender: &mut Sender,
		desired_requests_count: usize,
		validators: &mut VecDeque<(AuthorityDiscoveryId, ValidatorIndex)>,
		requesting_chunks: &mut OngoingRequests,
	) where
		Sender: overseer::AvailabilityRecoverySenderTrait,
	{
		let candidate_hash = params.candidate_hash;
		let already_requesting_count = requesting_chunks.len();

		let to_launch = desired_requests_count - already_requesting_count;
		let mut requests = Vec::with_capacity(to_launch);

		gum::trace!(
			target: LOG_TARGET,
			?candidate_hash,
			"Attempting to launch {} requests",
			to_launch
		);

		while requesting_chunks.len() < desired_requests_count {
			if let Some((authority_id, validator_index)) = validators.pop_back() {
				gum::trace!(
					target: LOG_TARGET,
					?authority_id,
					?validator_index,
					?candidate_hash,
					"Requesting chunk",
				);

				// Request data.
				let raw_request_v2 =
					req_res::v2::ChunkFetchingRequest { candidate_hash, index: validator_index };
				let raw_request_v1 = req_res::v1::ChunkFetchingRequest::from(raw_request_v2);

				let (req, res) = OutgoingRequest::new_with_fallback(
					Recipient::Authority(authority_id.clone()),
					raw_request_v2,
					raw_request_v1,
				);
				requests.push(Requests::ChunkFetching(req));

				params.metrics.on_chunk_request_issued(strategy_type);
				let timer = params.metrics.time_chunk_request(strategy_type);
				let v1_protocol_name = params.req_v1_protocol_name.clone();
				let v2_protocol_name = params.req_v2_protocol_name.clone();

				let chunk_mapping_enabled = params.chunk_mapping_enabled;
				let authority_id_clone = authority_id.clone();

				requesting_chunks.push(Box::pin(async move {
					let _timer = timer;
					let res = match res.await {
						Ok((bytes, protocol)) =>
							if v2_protocol_name == protocol {
								match req_res::v2::ChunkFetchingResponse::decode(&mut &bytes[..]) {
									Ok(req_res::v2::ChunkFetchingResponse::Chunk(chunk)) =>
										Ok((Some(chunk.into()), protocol)),
									Ok(req_res::v2::ChunkFetchingResponse::NoSuchChunk) =>
										Ok((None, protocol)),
									Err(e) => Err(RequestError::InvalidResponse(e)),
								}
							} else if v1_protocol_name == protocol {
								// V1 protocol version must not be used when chunk mapping node
								// feature is enabled, because we can't know the real index of the
								// returned chunk.
								// This case should never be reached as long as the
								// `AvailabilityChunkMapping` feature is only enabled after the
								// v1 version is removed. Still, log this.
								if chunk_mapping_enabled {
									gum::info!(
										target: LOG_TARGET,
										?candidate_hash,
										authority_id = ?authority_id_clone,
										"Another validator is responding on /req_chunk/1 protocol while the availability chunk \
										mapping feature is enabled in the runtime. All validators must switch to /req_chunk/2."
									);
								}

								match req_res::v1::ChunkFetchingResponse::decode(&mut &bytes[..]) {
									Ok(req_res::v1::ChunkFetchingResponse::Chunk(chunk)) => Ok((
										Some(chunk.recombine_into_chunk(&raw_request_v1)),
										protocol,
									)),
									Ok(req_res::v1::ChunkFetchingResponse::NoSuchChunk) =>
										Ok((None, protocol)),
									Err(e) => Err(RequestError::InvalidResponse(e)),
								}
							} else {
								Err(RequestError::NetworkError(RequestFailure::UnknownProtocol))
							},

						Err(e) => Err(e),
					};

					(authority_id, validator_index, res)
				}));
			} else {
				break
			}
		}

		if requests.len() != 0 {
			sender
				.send_message(NetworkBridgeTxMessage::SendRequests(
					requests,
					IfDisconnected::TryConnect,
				))
				.await;
		}
	}

	/// Wait for a sufficient amount of chunks to reconstruct according to the provided `params`.
	async fn wait_for_chunks(
		&mut self,
		strategy_type: &str,
		params: &RecoveryParams,
		retry_threshold: u32,
		validators: &mut VecDeque<(AuthorityDiscoveryId, ValidatorIndex)>,
		requesting_chunks: &mut OngoingRequests,
		// If supplied, these validators will be used as a backup for requesting chunks. They
		// should hold all chunks. Each of them will only be used to query one chunk.
		backup_validators: &mut Vec<AuthorityDiscoveryId>,
		// Function that returns `true` when this strategy can conclude. Either if we got enough
		// chunks or if it's impossible.
		mut can_conclude: impl FnMut(
			// Number of validators left in the queue
			usize,
			// Number of in flight requests
			usize,
			// Number of valid chunks received so far
			usize,
			// Number of valid systematic chunks received so far
			usize,
		) -> bool,
	) -> (usize, usize) {
		let metrics = &params.metrics;

		let mut total_received_responses = 0;
		let mut error_count = 0;

		// Wait for all current requests to conclude or time-out, or until we reach enough chunks.
		// We also declare requests undead, once `TIMEOUT_START_NEW_REQUESTS` is reached and will
		// return in that case for `launch_parallel_requests` to fill up slots again.
		while let Some(res) = requesting_chunks.next_with_timeout(TIMEOUT_START_NEW_REQUESTS).await
		{
			total_received_responses += 1;

			let (authority_id, validator_index, request_result) = res;

			let mut is_error = false;

			match request_result {
				Ok((maybe_chunk, protocol)) => {
					match protocol {
						name if name == params.req_v1_protocol_name =>
							params.metrics.on_chunk_response_v1(),
						name if name == params.req_v2_protocol_name =>
							params.metrics.on_chunk_response_v2(),
						_ => {},
					}

					match maybe_chunk {
						Some(chunk) =>
							if is_chunk_valid(params, &chunk) {
								metrics.on_chunk_request_succeeded(strategy_type);
								gum::trace!(
									target: LOG_TARGET,
									candidate_hash = ?params.candidate_hash,
									?authority_id,
									?validator_index,
									"Received valid chunk",
								);
								self.insert_chunk(
									chunk.index,
									Chunk { chunk: chunk.chunk, validator_index },
								);
							} else {
								metrics.on_chunk_request_invalid(strategy_type);
								error_count += 1;
								// Record that we got an invalid chunk so that subsequent strategies
								// don't try requesting this again.
								self.record_error_fatal(authority_id.clone(), validator_index);
								is_error = true;
							},
						None => {
							metrics.on_chunk_request_no_such_chunk(strategy_type);
							gum::trace!(
								target: LOG_TARGET,
								candidate_hash = ?params.candidate_hash,
								?authority_id,
								?validator_index,
								"Validator did not have the chunk",
							);
							error_count += 1;
							// Record that the validator did not have this chunk so that subsequent
							// strategies don't try requesting this again.
							self.record_error_fatal(authority_id.clone(), validator_index);
							is_error = true;
						},
					}
				},
				Err(err) => {
					error_count += 1;

					gum::trace!(
						target: LOG_TARGET,
						candidate_hash= ?params.candidate_hash,
						?err,
						?authority_id,
						?validator_index,
						"Failure requesting chunk",
					);

					is_error = true;

					match err {
						RequestError::InvalidResponse(_) => {
							metrics.on_chunk_request_invalid(strategy_type);

							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?params.candidate_hash,
								?err,
								?authority_id,
								?validator_index,
								"Chunk fetching response was invalid",
							);

							// Record that we got an invalid chunk so that this or
							// subsequent strategies don't try requesting this again.
							self.record_error_fatal(authority_id.clone(), validator_index);
						},
						RequestError::NetworkError(err) => {
							// No debug logs on general network errors - that became very
							// spammy occasionally.
							if let RequestFailure::Network(OutboundFailure::Timeout) = err {
								metrics.on_chunk_request_timeout(strategy_type);
							} else {
								metrics.on_chunk_request_error(strategy_type);
							}

							// Record that we got a non-fatal error so that this or
							// subsequent strategies will retry requesting this only a
							// limited number of times.
							self.record_error_non_fatal(authority_id.clone(), validator_index);
						},
						RequestError::Canceled(_) => {
							metrics.on_chunk_request_error(strategy_type);

							// Record that we got a non-fatal error so that this or
							// subsequent strategies will retry requesting this only a
							// limited number of times.
							self.record_error_non_fatal(authority_id.clone(), validator_index);
						},
					}
				},
			}

			if is_error {
				// First, see if we can retry the request.
				if self.can_retry_request(&(authority_id.clone(), validator_index), retry_threshold)
				{
					validators.push_front((authority_id, validator_index));
				} else {
					// Otherwise, try requesting from a backer as a backup, if we've not already
					// requested the same chunk from it.

					let position = backup_validators.iter().position(|v| {
						!self.recorded_errors.contains_key(&(v.clone(), validator_index))
					});
					if let Some(position) = position {
						// Use swap_remove because it's faster and we don't care about order here.
						let backer = backup_validators.swap_remove(position);
						validators.push_front((backer, validator_index));
					}
				}
			}

			if can_conclude(
				validators.len(),
				requesting_chunks.total_len(),
				self.chunk_count(),
				self.systematic_chunk_count(params.systematic_threshold),
			) {
				gum::debug!(
					target: LOG_TARGET,
					validators_len = validators.len(),
					candidate_hash = ?params.candidate_hash,
					received_chunks_count = ?self.chunk_count(),
					requested_chunks_count = ?requesting_chunks.len(),
					threshold = ?params.threshold,
					"Can conclude availability recovery strategy",
				);
				break
			}
		}

		(total_received_responses, error_count)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{tests::*, Metrics, RecoveryStrategy, RecoveryTask};
	use assert_matches::assert_matches;
	use futures::{
		channel::mpsc::{self, UnboundedReceiver},
		executor, future, Future, FutureExt, StreamExt,
	};
	use codec::Error as DecodingError;
	use polkadot_erasure_coding::{recovery_threshold, systematic_recovery_threshold};
	use polkadot_node_network_protocol::request_response::Protocol;
	use polkadot_node_primitives::{BlockData, PoV};
	use polkadot_node_subsystem::{AllMessages, TimeoutExt};
	use polkadot_node_subsystem_test_helpers::{
		derive_erasure_chunks_with_proofs_and_root, sender_receiver, TestSubsystemSender,
	};
	use polkadot_primitives::{CandidateHash, HeadData, PersistedValidationData};
	use polkadot_primitives_test_helpers::dummy_hash;
	use sp_keyring::Sr25519Keyring;
	use std::sync::Arc;

	const TIMEOUT: Duration = Duration::from_secs(1);

	impl Default for RecoveryParams {
		fn default() -> Self {
			let validators = vec![
				Sr25519Keyring::Ferdie,
				Sr25519Keyring::Alice.into(),
				Sr25519Keyring::Bob.into(),
				Sr25519Keyring::Charlie,
				Sr25519Keyring::Dave,
				Sr25519Keyring::One,
				Sr25519Keyring::Two,
			];
			let (erasure_task_tx, _erasure_task_rx) = mpsc::channel(10);

			Self {
				validator_authority_keys: validator_authority_id(&validators),
				n_validators: validators.len(),
				threshold: recovery_threshold(validators.len()).unwrap(),
				systematic_threshold: systematic_recovery_threshold(validators.len()).unwrap(),
				candidate_hash: CandidateHash(dummy_hash()),
				erasure_root: dummy_hash(),
				metrics: Metrics::new_dummy(),
				bypass_availability_store: false,
				post_recovery_check: PostRecoveryCheck::Reencode,
				pov_hash: dummy_hash(),
				req_v1_protocol_name: "/req_chunk/1".into(),
				req_v2_protocol_name: "/req_chunk/2".into(),
				chunk_mapping_enabled: true,
				erasure_task_tx,
			}
		}
	}

	impl RecoveryParams {
		fn create_chunks(&mut self) -> Vec<ErasureChunk> {
			let available_data = dummy_available_data();
			let (chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				self.n_validators,
				&available_data,
				|_, _| {},
			);

			self.erasure_root = erasure_root;
			self.pov_hash = available_data.pov.hash();

			chunks
		}
	}

	fn dummy_available_data() -> AvailableData {
		let validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		AvailableData {
			validation_data,
			pov: Arc::new(PoV { block_data: BlockData(vec![42; 64]) }),
		}
	}

	fn test_harness<RecvFut: Future<Output = ()>, TestFut: Future<Output = ()>>(
		receiver_future: impl FnOnce(UnboundedReceiver<AllMessages>) -> RecvFut,
		test: impl FnOnce(TestSubsystemSender) -> TestFut,
	) {
		let (sender, receiver) = sender_receiver();

		let test_fut = test(sender);
		let receiver_future = receiver_future(receiver);

		futures::pin_mut!(test_fut);
		futures::pin_mut!(receiver_future);

		executor::block_on(future::join(test_fut, receiver_future)).1
	}

	#[test]
	fn test_recorded_errors() {
		let retry_threshold = 2;
		let mut state = State::new();

		let alice = Sr25519Keyring::Alice.public();
		let bob = Sr25519Keyring::Bob.public();
		let eve = Sr25519Keyring::Eve.public();

		assert!(state.can_retry_request(&(alice.into(), 0.into()), retry_threshold));
		assert!(state.can_retry_request(&(alice.into(), 0.into()), 0));
		state.record_error_non_fatal(alice.into(), 0.into());
		assert!(state.can_retry_request(&(alice.into(), 0.into()), retry_threshold));
		state.record_error_non_fatal(alice.into(), 0.into());
		assert!(!state.can_retry_request(&(alice.into(), 0.into()), retry_threshold));
		state.record_error_non_fatal(alice.into(), 0.into());
		assert!(!state.can_retry_request(&(alice.into(), 0.into()), retry_threshold));

		assert!(state.can_retry_request(&(alice.into(), 0.into()), 5));

		state.record_error_fatal(bob.into(), 1.into());
		assert!(!state.can_retry_request(&(bob.into(), 1.into()), retry_threshold));
		state.record_error_non_fatal(bob.into(), 1.into());
		assert!(!state.can_retry_request(&(bob.into(), 1.into()), retry_threshold));

		assert!(state.can_retry_request(&(eve.into(), 4.into()), 0));
		assert!(state.can_retry_request(&(eve.into(), 4.into()), retry_threshold));
	}

	#[test]
	fn test_populate_from_av_store() {
		let params = RecoveryParams::default();

		// Failed to reach the av store
		{
			let params = params.clone();
			let candidate_hash = params.candidate_hash;
			let mut state = State::new();

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
					receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAllChunks(hash, tx)) => {
						assert_eq!(hash, candidate_hash);
						drop(tx);
					});
				},
				|mut sender| async move {
					let local_chunk_indices =
						state.populate_from_av_store(&params, &mut sender).await;

					assert_eq!(state.chunk_count(), 0);
					assert_eq!(local_chunk_indices.len(), 0);
				},
			);
		}

		// Found invalid chunk
		{
			let mut params = params.clone();
			let candidate_hash = params.candidate_hash;
			let mut state = State::new();
			let chunks = params.create_chunks();

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
					receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAllChunks(hash, tx)) => {
						assert_eq!(hash, candidate_hash);
						let mut chunk = chunks[0].clone();
						chunk.index = 3.into();
						tx.send(vec![(2.into(), chunk)]).unwrap();
					});
				},
				|mut sender| async move {
					let local_chunk_indices =
						state.populate_from_av_store(&params, &mut sender).await;

					assert_eq!(state.chunk_count(), 0);
					assert_eq!(local_chunk_indices.len(), 1);
				},
			);
		}

		// Found valid chunk
		{
			let mut params = params.clone();
			let candidate_hash = params.candidate_hash;
			let mut state = State::new();
			let chunks = params.create_chunks();

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
					receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAllChunks(hash, tx)) => {
						assert_eq!(hash, candidate_hash);
						tx.send(vec![(4.into(), chunks[1].clone())]).unwrap();
					});
				},
				|mut sender| async move {
					let local_chunk_indices =
						state.populate_from_av_store(&params, &mut sender).await;

					assert_eq!(state.chunk_count(), 1);
					assert_eq!(local_chunk_indices.len(), 1);
				},
			);
		}
	}

	#[test]
	fn test_launch_parallel_chunk_requests() {
		let params = RecoveryParams::default();
		let alice: AuthorityDiscoveryId = Sr25519Keyring::Alice.public().into();
		let bob: AuthorityDiscoveryId = Sr25519Keyring::Bob.public().into();
		let eve: AuthorityDiscoveryId = Sr25519Keyring::Eve.public().into();

		// No validators to request from.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			let mut validators = VecDeque::new();

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|mut sender| async move {
					state
						.launch_parallel_chunk_requests(
							"regular",
							&params,
							&mut sender,
							3,
							&mut validators,
							&mut ongoing_reqs,
						)
						.await;

					assert_eq!(ongoing_reqs.total_len(), 0);
				},
			);
		}

		// Has validators but no need to request more.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			let mut validators = VecDeque::new();
			validators.push_back((alice.clone(), ValidatorIndex(1)));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|mut sender| async move {
					state
						.launch_parallel_chunk_requests(
							"regular",
							&params,
							&mut sender,
							0,
							&mut validators,
							&mut ongoing_reqs,
						)
						.await;

					assert_eq!(ongoing_reqs.total_len(), 0);
				},
			);
		}

		// Has validators but no need to request more.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			ongoing_reqs.push(async { todo!() }.boxed());
			ongoing_reqs.soft_cancel();
			let mut validators = VecDeque::new();
			validators.push_back((alice.clone(), ValidatorIndex(1)));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|mut sender| async move {
					state
						.launch_parallel_chunk_requests(
							"regular",
							&params,
							&mut sender,
							0,
							&mut validators,
							&mut ongoing_reqs,
						)
						.await;

					assert_eq!(ongoing_reqs.total_len(), 1);
					assert_eq!(ongoing_reqs.len(), 0);
				},
			);
		}

		// Needs to request more.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			ongoing_reqs.push(async { todo!() }.boxed());
			ongoing_reqs.soft_cancel();
			ongoing_reqs.push(async { todo!() }.boxed());
			let mut validators = VecDeque::new();
			validators.push_back((alice.clone(), 0.into()));
			validators.push_back((bob, 1.into()));
			validators.push_back((eve, 2.into()));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
						receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
						AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(requests, _)) if requests.len()
== 3 					);
				},
				|mut sender| async move {
					state
						.launch_parallel_chunk_requests(
							"regular",
							&params,
							&mut sender,
							10,
							&mut validators,
							&mut ongoing_reqs,
						)
						.await;

					assert_eq!(ongoing_reqs.total_len(), 5);
					assert_eq!(ongoing_reqs.len(), 4);
				},
			);
		}

		// Check network protocol versioning.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			let mut validators = VecDeque::new();
			validators.push_back((alice, 0.into()));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					match receiver.next().timeout(TIMEOUT).await.unwrap().unwrap() {
						AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(
							mut requests,
							_,
						)) => {
							assert_eq!(requests.len(), 1);
							// By default, we should use the new protocol version with a fallback on
							// the older one.
							let (protocol, request) = requests.remove(0).encode_request();
							assert_eq!(protocol, Protocol::ChunkFetchingV2);
							assert_eq!(
								request.fallback_request.unwrap().1,
								Protocol::ChunkFetchingV1
							);
						},
						_ => unreachable!(),
					}
				},
				|mut sender| async move {
					state
						.launch_parallel_chunk_requests(
							"regular",
							&params,
							&mut sender,
							10,
							&mut validators,
							&mut ongoing_reqs,
						)
						.await;

					assert_eq!(ongoing_reqs.total_len(), 1);
					assert_eq!(ongoing_reqs.len(), 1);
				},
			);
		}
	}

	#[test]
	fn test_wait_for_chunks() {
		let params = RecoveryParams::default();
		let retry_threshold = 2;

		// No ongoing requests.
		{
			let params = params.clone();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			let mut validators = VecDeque::new();

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|_| async move {
					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut vec![],
							|_, _, _, _| false,
						)
						.await;
					assert_eq!(total_responses, 0);
					assert_eq!(error_count, 0);
					assert_eq!(state.chunk_count(), 0);
				},
			);
		}

		// Complex scenario.
		{
			let mut params = params.clone();
			let chunks = params.create_chunks();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[0].clone(),
					0.into(),
					Ok((Some(chunks[0].clone()), "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.soft_cancel();
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[1].clone(),
					1.into(),
					Ok((Some(chunks[1].clone()), "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[2].clone(),
					2.into(),
					Ok((None, "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[3].clone(),
					3.into(),
					Err(RequestError::from(DecodingError::from("err"))),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[4].clone(),
					4.into(),
					Err(RequestError::NetworkError(RequestFailure::NotConnected)),
				))
				.boxed(),
			);

			let mut validators: VecDeque<_> = (5..params.n_validators as u32)
				.map(|i| (params.validator_authority_keys[i as usize].clone(), i.into()))
				.collect();
			validators.push_back((
				Sr25519Keyring::AliceStash.public().into(),
				ValidatorIndex(params.n_validators as u32),
			));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|_| async move {
					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut vec![],
							|_, _, _, _| false,
						)
						.await;
					assert_eq!(total_responses, 5);
					assert_eq!(error_count, 3);
					assert_eq!(state.chunk_count(), 2);

					let mut expected_validators: VecDeque<_> = (4..params.n_validators as u32)
						.map(|i| (params.validator_authority_keys[i as usize].clone(), i.into()))
						.collect();
					expected_validators.push_back((
						Sr25519Keyring::AliceStash.public().into(),
						ValidatorIndex(params.n_validators as u32),
					));

					assert_eq!(validators, expected_validators);

					// This time we'll go over the recoverable error threshold.
					ongoing_reqs.push(
						future::ready((
							params.validator_authority_keys[4].clone(),
							4.into(),
							Err(RequestError::NetworkError(RequestFailure::NotConnected)),
						))
						.boxed(),
					);

					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut vec![],
							|_, _, _, _| false,
						)
						.await;
					assert_eq!(total_responses, 1);
					assert_eq!(error_count, 1);
					assert_eq!(state.chunk_count(), 2);

					validators.pop_front();
					let mut expected_validators: VecDeque<_> = (5..params.n_validators as u32)
						.map(|i| (params.validator_authority_keys[i as usize].clone(), i.into()))
						.collect();
					expected_validators.push_back((
						Sr25519Keyring::AliceStash.public().into(),
						ValidatorIndex(params.n_validators as u32),
					));

					assert_eq!(validators, expected_validators);

					// Check that can_conclude returning true terminates the loop.
					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut vec![],
							|_, _, _, _| true,
						)
						.await;
					assert_eq!(total_responses, 0);
					assert_eq!(error_count, 0);
					assert_eq!(state.chunk_count(), 2);

					assert_eq!(validators, expected_validators);
				},
			);
		}

		// Complex scenario with backups in the backing group.
		{
			let mut params = params.clone();
			let chunks = params.create_chunks();
			let mut state = State::new();
			let mut ongoing_reqs = OngoingRequests::new();
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[0].clone(),
					0.into(),
					Ok((Some(chunks[0].clone()), "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.soft_cancel();
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[1].clone(),
					1.into(),
					Ok((Some(chunks[1].clone()), "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[2].clone(),
					2.into(),
					Ok((None, "".into())),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[3].clone(),
					3.into(),
					Err(RequestError::from(DecodingError::from("err"))),
				))
				.boxed(),
			);
			ongoing_reqs.push(
				future::ready((
					params.validator_authority_keys[4].clone(),
					4.into(),
					Err(RequestError::NetworkError(RequestFailure::NotConnected)),
				))
				.boxed(),
			);

			let mut validators: VecDeque<_> = (5..params.n_validators as u32)
				.map(|i| (params.validator_authority_keys[i as usize].clone(), i.into()))
				.collect();
			validators.push_back((
				Sr25519Keyring::Eve.public().into(),
				ValidatorIndex(params.n_validators as u32),
			));

			let mut backup_backers = vec![
				params.validator_authority_keys[2].clone(),
				params.validator_authority_keys[0].clone(),
				params.validator_authority_keys[4].clone(),
				params.validator_authority_keys[3].clone(),
				Sr25519Keyring::AliceStash.public().into(),
				Sr25519Keyring::BobStash.public().into(),
			];

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|_| async move {
					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut backup_backers,
							|_, _, _, _| false,
						)
						.await;
					assert_eq!(total_responses, 5);
					assert_eq!(error_count, 3);
					assert_eq!(state.chunk_count(), 2);

					let mut expected_validators: VecDeque<_> = (5..params.n_validators as u32)
						.map(|i| (params.validator_authority_keys[i as usize].clone(), i.into()))
						.collect();
					expected_validators.push_back((
						Sr25519Keyring::Eve.public().into(),
						ValidatorIndex(params.n_validators as u32),
					));
					// We picked a backer as a backup for chunks 2 and 3.
					expected_validators
						.push_front((params.validator_authority_keys[0].clone(), 2.into()));
					expected_validators
						.push_front((params.validator_authority_keys[2].clone(), 3.into()));
					expected_validators
						.push_front((params.validator_authority_keys[4].clone(), 4.into()));

					assert_eq!(validators, expected_validators);

					// This time we'll go over the recoverable error threshold for chunk 4.
					ongoing_reqs.push(
						future::ready((
							params.validator_authority_keys[4].clone(),
							4.into(),
							Err(RequestError::NetworkError(RequestFailure::NotConnected)),
						))
						.boxed(),
					);

					validators.pop_front();

					let (total_responses, error_count) = state
						.wait_for_chunks(
							"regular",
							&params,
							retry_threshold,
							&mut validators,
							&mut ongoing_reqs,
							&mut backup_backers,
							|_, _, _, _| false,
						)
						.await;
					assert_eq!(total_responses, 1);
					assert_eq!(error_count, 1);
					assert_eq!(state.chunk_count(), 2);

					expected_validators.pop_front();
					expected_validators
						.push_front((Sr25519Keyring::AliceStash.public().into(), 4.into()));

					assert_eq!(validators, expected_validators);
				},
			);
		}
	}

	#[test]
	fn test_recovery_strategy_run() {
		let params = RecoveryParams::default();

		struct GoodStrategy;
		#[async_trait::async_trait]
		impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender> for GoodStrategy {
			fn display_name(&self) -> &'static str {
				"GoodStrategy"
			}

			fn strategy_type(&self) -> &'static str {
				"good_strategy"
			}

			async fn run(
				mut self: Box<Self>,
				_state: &mut State,
				_sender: &mut Sender,
				_common_params: &RecoveryParams,
			) -> Result<AvailableData, RecoveryError> {
				Ok(dummy_available_data())
			}
		}

		struct UnavailableStrategy;
		#[async_trait::async_trait]
		impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender>
			for UnavailableStrategy
		{
			fn display_name(&self) -> &'static str {
				"UnavailableStrategy"
			}

			fn strategy_type(&self) -> &'static str {
				"unavailable_strategy"
			}

			async fn run(
				mut self: Box<Self>,
				_state: &mut State,
				_sender: &mut Sender,
				_common_params: &RecoveryParams,
			) -> Result<AvailableData, RecoveryError> {
				Err(RecoveryError::Unavailable)
			}
		}

		struct InvalidStrategy;
		#[async_trait::async_trait]
		impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender>
			for InvalidStrategy
		{
			fn display_name(&self) -> &'static str {
				"InvalidStrategy"
			}

			fn strategy_type(&self) -> &'static str {
				"invalid_strategy"
			}

			async fn run(
				mut self: Box<Self>,
				_state: &mut State,
				_sender: &mut Sender,
				_common_params: &RecoveryParams,
			) -> Result<AvailableData, RecoveryError> {
				Err(RecoveryError::Invalid)
			}
		}

		// No recovery strategies.
		{
			let mut params = params.clone();
			let strategies = VecDeque::new();
			params.bypass_availability_store = true;

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|sender| async move {
					let task = RecoveryTask::new(sender, params, strategies);

					assert_eq!(task.run().await.unwrap_err(), RecoveryError::Unavailable);
				},
			);
		}

		// If we have the data in av-store, returns early.
		{
			let params = params.clone();
			let strategies = VecDeque::new();
			let candidate_hash = params.candidate_hash;

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
					receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAvailableData(hash, tx)) => {
						assert_eq!(hash, candidate_hash);
						tx.send(Some(dummy_available_data())).unwrap();
					});
				},
				|sender| async move {
					let task = RecoveryTask::new(sender, params, strategies);

					assert_eq!(task.run().await.unwrap(), dummy_available_data());
				},
			);
		}

		// Strategy returning `RecoveryError::Invalid`` will short-circuit the entire task.
		{
			let mut params = params.clone();
			params.bypass_availability_store = true;
			let mut strategies: VecDeque<Box<dyn RecoveryStrategy<TestSubsystemSender>>> =
				VecDeque::new();
			strategies.push_back(Box::new(InvalidStrategy));
			strategies.push_back(Box::new(GoodStrategy));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					// Shouldn't send any requests.
					assert!(receiver.next().timeout(TIMEOUT).await.unwrap().is_none());
				},
				|sender| async move {
					let task = RecoveryTask::new(sender, params, strategies);

					assert_eq!(task.run().await.unwrap_err(), RecoveryError::Invalid);
				},
			);
		}

		// Strategy returning `Unavailable` will fall back to the next one.
		{
			let params = params.clone();
			let candidate_hash = params.candidate_hash;
			let mut strategies: VecDeque<Box<dyn RecoveryStrategy<TestSubsystemSender>>> =
				VecDeque::new();
			strategies.push_back(Box::new(UnavailableStrategy));
			strategies.push_back(Box::new(GoodStrategy));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
						receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAvailableData(hash, tx)) => {
							assert_eq!(hash, candidate_hash);
							tx.send(Some(dummy_available_data())).unwrap();
					});
				},
				|sender| async move {
					let task = RecoveryTask::new(sender, params, strategies);

					assert_eq!(task.run().await.unwrap(), dummy_available_data());
				},
			);
		}

		// More complex scenario.
		{
			let params = params.clone();
			let candidate_hash = params.candidate_hash;
			let mut strategies: VecDeque<Box<dyn RecoveryStrategy<TestSubsystemSender>>> =
				VecDeque::new();
			strategies.push_back(Box::new(UnavailableStrategy));
			strategies.push_back(Box::new(UnavailableStrategy));
			strategies.push_back(Box::new(GoodStrategy));
			strategies.push_back(Box::new(InvalidStrategy));

			test_harness(
				|mut receiver: UnboundedReceiver<AllMessages>| async move {
					assert_matches!(
						receiver.next().timeout(TIMEOUT).await.unwrap().unwrap(),
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAvailableData(hash, tx)) => {
							assert_eq!(hash, candidate_hash);
							tx.send(Some(dummy_available_data())).unwrap();
					});
				},
				|sender| async move {
					let task = RecoveryTask::new(sender, params, strategies);

					assert_eq!(task.run().await.unwrap(), dummy_available_data());
				},
			);
		}
	}

	#[test]
	fn test_is_unavailable() {
		assert_eq!(is_unavailable(0, 0, 0, 0), false);
		assert_eq!(is_unavailable(2, 2, 2, 0), false);
		// Already reached the threshold.
		assert_eq!(is_unavailable(3, 0, 10, 3), false);
		assert_eq!(is_unavailable(3, 2, 0, 3), false);
		assert_eq!(is_unavailable(3, 2, 10, 3), false);
		// It's still possible to reach the threshold
		assert_eq!(is_unavailable(0, 0, 10, 3), false);
		assert_eq!(is_unavailable(0, 0, 3, 3), false);
		assert_eq!(is_unavailable(1, 1, 1, 3), false);
		// Not possible to reach the threshold
		assert_eq!(is_unavailable(0, 0, 0, 3), true);
		assert_eq!(is_unavailable(2, 3, 2, 10), true);
	}
}

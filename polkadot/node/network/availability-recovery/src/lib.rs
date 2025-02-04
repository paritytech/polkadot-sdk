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

//! Availability Recovery Subsystem of Polkadot.

#![warn(missing_docs)]

use std::{
	collections::{BTreeMap, VecDeque},
	iter::Iterator,
	num::NonZeroUsize,
	pin::Pin,
};

use futures::{
	channel::oneshot,
	future::{Future, FutureExt, RemoteHandle},
	pin_mut,
	prelude::*,
	sink::SinkExt,
	stream::{FuturesUnordered, StreamExt},
	task::{Context, Poll},
};
use sc_network::ProtocolName;
use schnellru::{ByLength, LruMap};
use task::{
	FetchChunks, FetchChunksParams, FetchFull, FetchFullParams, FetchSystematicChunks,
	FetchSystematicChunksParams,
};

use polkadot_erasure_coding::{
	branches, obtain_chunks_v1, recovery_threshold, systematic_recovery_threshold,
	Error as ErasureEncodingError,
};
use task::{RecoveryParams, RecoveryStrategy, RecoveryTask};

use error::{log_error, Error, FatalError, Result};
use polkadot_node_network_protocol::{
	request_response::{
		v1 as request_v1, v2 as request_v2, IncomingRequestReceiver, IsRequest, ReqProtocolNames,
	},
	UnifiedReputationChange as Rep,
};
use polkadot_node_primitives::AvailableData;
use polkadot_node_subsystem::{
	errors::RecoveryError,
	messages::{AvailabilityRecoveryMessage, AvailabilityStoreMessage},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem,
	SubsystemContext, SubsystemError,
};
use polkadot_node_subsystem_util::{
	availability_chunks::availability_chunk_indices,
	runtime::{ExtendedSessionInfo, RuntimeInfo},
};
use polkadot_primitives::{
	node_features, vstaging::CandidateReceiptV2 as CandidateReceipt, BlockNumber, CandidateHash,
	ChunkIndex, CoreIndex, GroupIndex, Hash, SessionIndex, ValidatorIndex,
};

mod error;
mod futures_undead;
mod metrics;
mod task;
pub use metrics::Metrics;

#[cfg(test)]
mod tests;

type RecoveryResult = std::result::Result<AvailableData, RecoveryError>;

const LOG_TARGET: &str = "parachain::availability-recovery";

// Size of the LRU cache where we keep recovered data.
const LRU_SIZE: u32 = 16;

const COST_INVALID_REQUEST: Rep = Rep::CostMajor("Peer sent unparsable request");

/// PoV size limit in bytes for which prefer fetching from backers. (conservative, Polkadot for now)
pub(crate) const CONSERVATIVE_FETCH_CHUNKS_THRESHOLD: usize = 1 * 1024 * 1024;
/// PoV size limit in bytes for which prefer fetching from backers. (Kusama and all testnets)
pub const FETCH_CHUNKS_THRESHOLD: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq)]
/// The strategy we use to recover the PoV.
pub enum RecoveryStrategyKind {
	/// We try the backing group first if PoV size is lower than specified, then fallback to
	/// validator chunks.
	BackersFirstIfSizeLower(usize),
	/// We try the backing group first if PoV size is lower than specified, then fallback to
	/// systematic chunks. Regular chunk recovery as a last resort.
	BackersFirstIfSizeLowerThenSystematicChunks(usize),

	/// The following variants are only helpful for integration tests.
	///
	/// We always try the backing group first, then fallback to validator chunks.
	#[allow(dead_code)]
	BackersFirstAlways,
	/// We always recover using validator chunks.
	#[allow(dead_code)]
	ChunksAlways,
	/// First try the backing group. Then systematic chunks.
	#[allow(dead_code)]
	BackersThenSystematicChunks,
	/// Always recover using systematic chunks, fall back to regular chunks.
	#[allow(dead_code)]
	SystematicChunks,
}

/// The Availability Recovery Subsystem.
pub struct AvailabilityRecoverySubsystem {
	/// PoV recovery strategy to use.
	recovery_strategy_kind: RecoveryStrategyKind,
	// If this is true, do not request data from the availability store.
	/// This is the useful for nodes where the
	/// availability-store subsystem is not expected to run,
	/// such as collators.
	bypass_availability_store: bool,
	/// Receiver for available data requests.
	req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
	/// Metrics for this subsystem.
	metrics: Metrics,
	/// The type of check to perform after available data was recovered.
	post_recovery_check: PostRecoveryCheck,
	/// Full protocol name for ChunkFetchingV1.
	req_v1_protocol_name: ProtocolName,
	/// Full protocol name for ChunkFetchingV2.
	req_v2_protocol_name: ProtocolName,
}

#[derive(Clone, PartialEq, Debug)]
/// The type of check to perform after available data was recovered.
enum PostRecoveryCheck {
	/// Reencode the data and check erasure root. For validators.
	Reencode,
	/// Only check the pov hash. For collators only.
	PovHash,
}

/// Expensive erasure coding computations that we want to run on a blocking thread.
enum ErasureTask {
	/// Reconstructs `AvailableData` from chunks given `n_validators`.
	Reconstruct(
		usize,
		BTreeMap<ChunkIndex, Vec<u8>>,
		oneshot::Sender<std::result::Result<AvailableData, ErasureEncodingError>>,
	),
	/// Re-encode `AvailableData` into erasure chunks in order to verify the provided root hash of
	/// the Merkle tree.
	Reencode(usize, Hash, AvailableData, oneshot::Sender<Option<AvailableData>>),
}

/// Re-encode the data into erasure chunks in order to verify
/// the root hash of the provided Merkle tree, which is built
/// on-top of the encoded chunks.
///
/// This (expensive) check is necessary, as otherwise we can't be sure that some chunks won't have
/// been tampered with by the backers, which would result in some validators considering the data
/// valid and some invalid as having fetched different set of chunks. The checking of the Merkle
/// proof for individual chunks only gives us guarantees, that we have fetched a chunk belonging to
/// a set the backers have committed to.
///
/// NOTE: It is fine to do this check with already decoded data, because if the decoding failed for
/// some validators, we can be sure that chunks have been tampered with (by the backers) or the
/// data was invalid to begin with. In the former case, validators fetching valid chunks will see
/// invalid data as well, because the root won't match. In the latter case the situation is the
/// same for anyone anyways.
fn reconstructed_data_matches_root(
	n_validators: usize,
	expected_root: &Hash,
	data: &AvailableData,
	metrics: &Metrics,
) -> bool {
	let _timer = metrics.time_reencode_chunks();

	let chunks = match obtain_chunks_v1(n_validators, data) {
		Ok(chunks) => chunks,
		Err(e) => {
			gum::debug!(
				target: LOG_TARGET,
				err = ?e,
				"Failed to obtain chunks",
			);
			return false
		},
	};

	let branches = branches(&chunks);

	branches.root() == *expected_root
}

/// Accumulate all awaiting sides for some particular `AvailableData`.
struct RecoveryHandle {
	candidate_hash: CandidateHash,
	remote: RemoteHandle<RecoveryResult>,
	awaiting: Vec<oneshot::Sender<RecoveryResult>>,
}

impl Future for RecoveryHandle {
	type Output = Option<(CandidateHash, RecoveryResult)>;

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut indices_to_remove = Vec::new();
		for (i, awaiting) in self.awaiting.iter_mut().enumerate().rev() {
			if let Poll::Ready(()) = awaiting.poll_canceled(cx) {
				indices_to_remove.push(i);
			}
		}

		// these are reverse order, so remove is fine.
		for index in indices_to_remove {
			gum::debug!(
				target: LOG_TARGET,
				candidate_hash = ?self.candidate_hash,
				"Receiver for available data dropped.",
			);

			self.awaiting.swap_remove(index);
		}

		if self.awaiting.is_empty() {
			gum::debug!(
				target: LOG_TARGET,
				candidate_hash = ?self.candidate_hash,
				"All receivers for available data dropped.",
			);

			return Poll::Ready(None)
		}

		let remote = &mut self.remote;
		futures::pin_mut!(remote);
		let result = futures::ready!(remote.poll(cx));

		for awaiting in self.awaiting.drain(..) {
			let _ = awaiting.send(result.clone());
		}

		Poll::Ready(Some((self.candidate_hash, result)))
	}
}

/// Cached result of an availability recovery operation.
#[derive(Debug, Clone)]
enum CachedRecovery {
	/// Availability was successfully retrieved before.
	Valid(AvailableData),
	/// Availability was successfully retrieved before, but was found to be invalid.
	Invalid,
}

impl CachedRecovery {
	/// Convert back to	`Result` to deliver responses.
	fn into_result(self) -> RecoveryResult {
		match self {
			Self::Valid(d) => Ok(d),
			Self::Invalid => Err(RecoveryError::Invalid),
		}
	}
}

impl TryFrom<RecoveryResult> for CachedRecovery {
	type Error = ();
	fn try_from(o: RecoveryResult) -> std::result::Result<CachedRecovery, Self::Error> {
		match o {
			Ok(d) => Ok(Self::Valid(d)),
			Err(RecoveryError::Invalid) => Ok(Self::Invalid),
			// We don't want to cache unavailable state, as that state might change, so if
			// requested again we want to try again!
			Err(RecoveryError::Unavailable) => Err(()),
			Err(RecoveryError::ChannelClosed) => Err(()),
		}
	}
}

struct State {
	/// Each recovery task is implemented as its own async task,
	/// and these handles are for communicating with them.
	ongoing_recoveries: FuturesUnordered<RecoveryHandle>,

	/// A recent block hash for which state should be available.
	live_block: (BlockNumber, Hash),

	/// An LRU cache of recently recovered data.
	availability_lru: LruMap<CandidateHash, CachedRecovery>,

	/// Cached runtime info.
	runtime_info: RuntimeInfo,
}

impl Default for State {
	fn default() -> Self {
		Self {
			ongoing_recoveries: FuturesUnordered::new(),
			live_block: (0, Hash::default()),
			availability_lru: LruMap::new(ByLength::new(LRU_SIZE)),
			runtime_info: RuntimeInfo::new(None),
		}
	}
}

#[overseer::subsystem(AvailabilityRecovery, error=SubsystemError, prefix=self::overseer)]
impl<Context> AvailabilityRecoverySubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self
			.run(ctx)
			.map_err(|e| SubsystemError::with_origin("availability-recovery", e))
			.boxed();
		SpawnedSubsystem { name: "availability-recovery-subsystem", future }
	}
}

/// Handles a signal from the overseer.
/// Returns true if subsystem receives a deadly signal.
async fn handle_signal(state: &mut State, signal: OverseerSignal) -> bool {
	match signal {
		OverseerSignal::Conclude => true,
		OverseerSignal::ActiveLeaves(ActiveLeavesUpdate { activated, .. }) => {
			// if activated is non-empty, set state.live_block to the highest block in `activated`
			if let Some(activated) = activated {
				if activated.number > state.live_block.0 {
					state.live_block = (activated.number, activated.hash)
				}
			}

			false
		},
		OverseerSignal::BlockFinalized(_, _) => false,
	}
}

/// Machinery around launching recovery tasks into the background.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn launch_recovery_task<Context>(
	state: &mut State,
	ctx: &mut Context,
	response_sender: oneshot::Sender<RecoveryResult>,
	recovery_strategies: VecDeque<Box<dyn RecoveryStrategy<<Context as SubsystemContext>::Sender>>>,
	params: RecoveryParams,
) -> Result<()> {
	let candidate_hash = params.candidate_hash;
	let recovery_task = RecoveryTask::new(ctx.sender().clone(), params, recovery_strategies);

	let (remote, remote_handle) = recovery_task.run().remote_handle();

	state.ongoing_recoveries.push(RecoveryHandle {
		candidate_hash,
		remote: remote_handle,
		awaiting: vec![response_sender],
	});

	ctx.spawn("recovery-task", Box::pin(remote))
		.map_err(|err| Error::SpawnTask(err))
}

/// Handles an availability recovery request.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn handle_recover<Context>(
	state: &mut State,
	ctx: &mut Context,
	receipt: CandidateReceipt,
	session_index: SessionIndex,
	backing_group: Option<GroupIndex>,
	response_sender: oneshot::Sender<RecoveryResult>,
	metrics: &Metrics,
	erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
	recovery_strategy_kind: RecoveryStrategyKind,
	bypass_availability_store: bool,
	post_recovery_check: PostRecoveryCheck,
	maybe_core_index: Option<CoreIndex>,
	req_v1_protocol_name: ProtocolName,
	req_v2_protocol_name: ProtocolName,
) -> Result<()> {
	let candidate_hash = receipt.hash();

	if let Some(result) =
		state.availability_lru.get(&candidate_hash).cloned().map(|v| v.into_result())
	{
		return response_sender.send(result).map_err(|_| Error::CanceledResponseSender)
	}

	if let Some(i) =
		state.ongoing_recoveries.iter_mut().find(|i| i.candidate_hash == candidate_hash)
	{
		i.awaiting.push(response_sender);
		return Ok(())
	}

	let session_info_res = state
		.runtime_info
		.get_session_info_by_index(ctx.sender(), state.live_block.1, session_index)
		.await;

	match session_info_res {
		Ok(ExtendedSessionInfo { session_info, node_features, .. }) => {
			let mut backer_group = None;
			let n_validators = session_info.validators.len();
			let systematic_threshold = systematic_recovery_threshold(n_validators)?;
			let mut recovery_strategies: VecDeque<
				Box<dyn RecoveryStrategy<<Context as SubsystemContext>::Sender>>,
			> = VecDeque::with_capacity(3);

			if let Some(backing_group) = backing_group {
				if let Some(backing_validators) = session_info.validator_groups.get(backing_group) {
					let mut small_pov_size = true;

					match recovery_strategy_kind {
						RecoveryStrategyKind::BackersFirstIfSizeLower(fetch_chunks_threshold) |
						RecoveryStrategyKind::BackersFirstIfSizeLowerThenSystematicChunks(
							fetch_chunks_threshold,
						) => {
							// Get our own chunk size to get an estimate of the PoV size.
							let chunk_size: Result<Option<usize>> =
								query_chunk_size(ctx, candidate_hash).await;
							if let Ok(Some(chunk_size)) = chunk_size {
								let pov_size_estimate = chunk_size * systematic_threshold;
								small_pov_size = pov_size_estimate < fetch_chunks_threshold;

								if small_pov_size {
									gum::trace!(
										target: LOG_TARGET,
										?candidate_hash,
										pov_size_estimate,
										fetch_chunks_threshold,
										"Prefer fetch from backing group",
									);
								}
							} else {
								// we have a POV limit but were not able to query the chunk size, so
								// don't use the backing group.
								small_pov_size = false;
							}
						},
						_ => {},
					};

					match (&recovery_strategy_kind, small_pov_size) {
						(RecoveryStrategyKind::BackersFirstAlways, _) |
						(RecoveryStrategyKind::BackersFirstIfSizeLower(_), true) |
						(
							RecoveryStrategyKind::BackersFirstIfSizeLowerThenSystematicChunks(_),
							true,
						) |
						(RecoveryStrategyKind::BackersThenSystematicChunks, _) =>
							recovery_strategies.push_back(Box::new(FetchFull::new(
								FetchFullParams { validators: backing_validators.to_vec() },
							))),
						_ => {},
					};

					backer_group = Some(backing_validators);
				}
			}

			let chunk_mapping_enabled = if let Some(&true) = node_features
				.get(usize::from(node_features::FeatureIndex::AvailabilityChunkMapping as u8))
				.as_deref()
			{
				true
			} else {
				false
			};

			// We can only attempt systematic recovery if we received the core index of the
			// candidate and chunk mapping is enabled.
			if let Some(core_index) = maybe_core_index {
				if matches!(
					recovery_strategy_kind,
					RecoveryStrategyKind::BackersThenSystematicChunks |
						RecoveryStrategyKind::SystematicChunks |
						RecoveryStrategyKind::BackersFirstIfSizeLowerThenSystematicChunks(_)
				) && chunk_mapping_enabled
				{
					let chunk_indices =
						availability_chunk_indices(Some(node_features), n_validators, core_index)?;

					let chunk_indices: VecDeque<_> = chunk_indices
						.iter()
						.enumerate()
						.map(|(v_index, c_index)| {
							(
								*c_index,
								ValidatorIndex(
									u32::try_from(v_index)
										.expect("validator count should not exceed u32"),
								),
							)
						})
						.collect();

					// Only get the validators according to the threshold.
					let validators = chunk_indices
						.clone()
						.into_iter()
						.filter(|(c_index, _)| {
							usize::try_from(c_index.0)
								.expect("usize is at least u32 bytes on all modern targets.") <
								systematic_threshold
						})
						.collect();

					recovery_strategies.push_back(Box::new(FetchSystematicChunks::new(
						FetchSystematicChunksParams {
							validators,
							backers: backer_group.map(|v| v.to_vec()).unwrap_or_else(|| vec![]),
						},
					)));
				}
			}

			recovery_strategies.push_back(Box::new(FetchChunks::new(FetchChunksParams {
				n_validators: session_info.validators.len(),
			})));

			let session_info = session_info.clone();

			let n_validators = session_info.validators.len();

			launch_recovery_task(
				state,
				ctx,
				response_sender,
				recovery_strategies,
				RecoveryParams {
					validator_authority_keys: session_info.discovery_keys.clone(),
					n_validators,
					threshold: recovery_threshold(n_validators)?,
					systematic_threshold,
					candidate_hash,
					erasure_root: receipt.descriptor.erasure_root(),
					metrics: metrics.clone(),
					bypass_availability_store,
					post_recovery_check,
					pov_hash: receipt.descriptor.pov_hash(),
					req_v1_protocol_name,
					req_v2_protocol_name,
					chunk_mapping_enabled,
					erasure_task_tx,
				},
			)
			.await
		},
		Err(_) => {
			response_sender
				.send(Err(RecoveryError::Unavailable))
				.map_err(|_| Error::CanceledResponseSender)?;

			Err(Error::SessionInfoUnavailable(state.live_block.1))
		},
	}
}

/// Queries the full `AvailableData` from av-store.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn query_full_data<Context>(
	ctx: &mut Context,
	candidate_hash: CandidateHash,
) -> Result<Option<AvailableData>> {
	let (tx, rx) = oneshot::channel();
	ctx.send_message(AvailabilityStoreMessage::QueryAvailableData(candidate_hash, tx))
		.await;

	rx.await.map_err(Error::CanceledQueryFullData)
}

/// Queries a chunk from av-store.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn query_chunk_size<Context>(
	ctx: &mut Context,
	candidate_hash: CandidateHash,
) -> Result<Option<usize>> {
	let (tx, rx) = oneshot::channel();
	ctx.send_message(AvailabilityStoreMessage::QueryChunkSize(candidate_hash, tx))
		.await;

	rx.await.map_err(Error::CanceledQueryFullData)
}

#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
impl AvailabilityRecoverySubsystem {
	/// Create a new instance of `AvailabilityRecoverySubsystem` suitable for collator nodes,
	/// which never requests the `AvailabilityStoreSubsystem` subsystem and only checks the POV hash
	/// instead of reencoding the available data.
	pub fn for_collator(
		fetch_chunks_threshold: Option<usize>,
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		req_protocol_names: &ReqProtocolNames,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind: RecoveryStrategyKind::BackersFirstIfSizeLower(
				fetch_chunks_threshold.unwrap_or(CONSERVATIVE_FETCH_CHUNKS_THRESHOLD),
			),
			bypass_availability_store: true,
			post_recovery_check: PostRecoveryCheck::PovHash,
			req_receiver,
			metrics,
			req_v1_protocol_name: req_protocol_names
				.get_name(request_v1::ChunkFetchingRequest::PROTOCOL),
			req_v2_protocol_name: req_protocol_names
				.get_name(request_v2::ChunkFetchingRequest::PROTOCOL),
		}
	}

	/// Create an optimised new instance of `AvailabilityRecoverySubsystem` suitable for validator
	/// nodes, which:
	/// - for small POVs (over the `fetch_chunks_threshold` or the
	///   `CONSERVATIVE_FETCH_CHUNKS_THRESHOLD`), it attempts full recovery from backers, if backing
	///   group supplied.
	/// - for large POVs, attempts systematic recovery, if core_index supplied and
	///   AvailabilityChunkMapping node feature is enabled.
	/// - as a last resort, attempt regular chunk recovery from all validators.
	pub fn for_validator(
		fetch_chunks_threshold: Option<usize>,
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		req_protocol_names: &ReqProtocolNames,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind:
				RecoveryStrategyKind::BackersFirstIfSizeLowerThenSystematicChunks(
					fetch_chunks_threshold.unwrap_or(CONSERVATIVE_FETCH_CHUNKS_THRESHOLD),
				),
			bypass_availability_store: false,
			post_recovery_check: PostRecoveryCheck::Reencode,
			req_receiver,
			metrics,
			req_v1_protocol_name: req_protocol_names
				.get_name(request_v1::ChunkFetchingRequest::PROTOCOL),
			req_v2_protocol_name: req_protocol_names
				.get_name(request_v2::ChunkFetchingRequest::PROTOCOL),
		}
	}

	/// Customise the recovery strategy kind
	/// Currently only useful for tests.
	#[cfg(any(test, feature = "subsystem-benchmarks"))]
	pub fn with_recovery_strategy_kind(
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		req_protocol_names: &ReqProtocolNames,
		metrics: Metrics,
		recovery_strategy_kind: RecoveryStrategyKind,
	) -> Self {
		Self {
			recovery_strategy_kind,
			bypass_availability_store: false,
			post_recovery_check: PostRecoveryCheck::Reencode,
			req_receiver,
			metrics,
			req_v1_protocol_name: req_protocol_names
				.get_name(request_v1::ChunkFetchingRequest::PROTOCOL),
			req_v2_protocol_name: req_protocol_names
				.get_name(request_v2::ChunkFetchingRequest::PROTOCOL),
		}
	}

	/// Starts the inner subsystem loop.
	pub async fn run<Context>(self, mut ctx: Context) -> std::result::Result<(), FatalError> {
		let mut state = State::default();
		let Self {
			mut req_receiver,
			metrics,
			recovery_strategy_kind,
			bypass_availability_store,
			post_recovery_check,
			req_v1_protocol_name,
			req_v2_protocol_name,
		} = self;

		let (erasure_task_tx, erasure_task_rx) = futures::channel::mpsc::channel(16);
		let mut erasure_task_rx = erasure_task_rx.fuse();

		// `ThreadPoolBuilder` spawns the tasks using `spawn_blocking`. For each worker there will
		// be a `mpsc` channel created. Each of these workers take the `Receiver` and poll it in an
		// infinite loop. All of the sender ends of the channel are sent as a vec which we then use
		// to create a `Cycle` iterator. We use this iterator to assign work in a round-robin
		// fashion to the workers in the pool.
		//
		// How work is dispatched to the pool from the recovery tasks:
		// - Once a recovery task finishes retrieving the availability data, it needs to reconstruct
		//   from chunks and/or
		// re-encode the data which are heavy CPU computations.
		// To do so it sends an `ErasureTask` to the main loop via the `erasure_task` channel, and
		// waits for the results over a `oneshot` channel.
		// - In the subsystem main loop we poll the `erasure_task_rx` receiver.
		// - We forward the received `ErasureTask` to the `next()` sender yielded by the `Cycle`
		//   iterator.
		// - Some worker thread handles it and sends the response over the `oneshot` channel.

		// Create a thread pool with 2 workers.
		let mut to_pool = ThreadPoolBuilder::build(
			// Pool is guaranteed to have at least 1 worker thread.
			NonZeroUsize::new(2).expect("There are 2 threads; qed"),
			metrics.clone(),
			&mut ctx,
		)
		.into_iter()
		.cycle();

		loop {
			let recv_req = req_receiver.recv(|| vec![COST_INVALID_REQUEST]).fuse();
			pin_mut!(recv_req);
			let res = futures::select! {
				erasure_task = erasure_task_rx.next() => {
					match erasure_task {
						Some(task) => {
							to_pool
								.next()
								.expect("Pool size is `NonZeroUsize`; qed")
								.send(task)
								.await
								.map_err(|_| RecoveryError::ChannelClosed)
						},
						None => {
							Err(RecoveryError::ChannelClosed)
						}
					}.map_err(Into::into)
				}
				signal = ctx.recv().fuse() => {
					match signal {
						Ok(signal) => {
							match signal {
								FromOrchestra::Signal(signal) => if handle_signal(
									&mut state,
									signal,
								).await {
									gum::debug!(target: LOG_TARGET, "subsystem concluded");
									return Ok(());
								} else {
									Ok(())
								},
								FromOrchestra::Communication {
									msg: AvailabilityRecoveryMessage::RecoverAvailableData(
										receipt,
										session_index,
										maybe_backing_group,
										maybe_core_index,
										response_sender,
									)
								} => handle_recover(
										&mut state,
										&mut ctx,
										receipt,
										session_index,
										maybe_backing_group,
										response_sender,
										&metrics,
										erasure_task_tx.clone(),
										recovery_strategy_kind.clone(),
										bypass_availability_store,
										post_recovery_check.clone(),
										maybe_core_index,
										req_v1_protocol_name.clone(),
										req_v2_protocol_name.clone(),
									).await
							}
						},
						Err(e) => Err(Error::SubsystemReceive(e))
					}
				}
				in_req = recv_req => {
					match in_req {
						Ok(req) => {
							if bypass_availability_store {
								gum::debug!(
									target: LOG_TARGET,
									"Skipping request to availability-store.",
								);
								let _ = req.send_response(None.into());
								Ok(())
							} else {
								match query_full_data(&mut ctx, req.payload.candidate_hash).await {
									Ok(res) => {
										let _ = req.send_response(res.into());
										Ok(())
									}
									Err(e) => {
										let _ = req.send_response(None.into());
										Err(e)
									}
								}
							}
						}
						Err(e) => Err(Error::IncomingRequest(e))
					}
				}
				output = state.ongoing_recoveries.select_next_some() => {
					let mut res = Ok(());
					if let Some((candidate_hash, result)) = output {
						if let Err(ref e) = result {
							res = Err(Error::Recovery(e.clone()));
						}

						if let Ok(recovery) = CachedRecovery::try_from(result) {
							state.availability_lru.insert(candidate_hash, recovery);
						}
					}

					res
				}
			};

			// Only bubble up fatal errors, but log all of them.
			if let Err(e) = res {
				log_error(Err(e))?;
			}
		}
	}
}

// A simple thread pool implementation using `spawn_blocking` threads.
struct ThreadPoolBuilder;

const MAX_THREADS: NonZeroUsize = match NonZeroUsize::new(4) {
	Some(max_threads) => max_threads,
	None => panic!("MAX_THREADS must be non-zero"),
};

impl ThreadPoolBuilder {
	// Creates a pool of `size` workers, where 1 <= `size` <= `MAX_THREADS`.
	//
	// Each worker is created by `spawn_blocking` and takes the receiver side of a channel
	// while all of the senders are returned to the caller. Each worker runs `erasure_task_thread`
	// that polls the `Receiver` for an `ErasureTask` which is expected to be CPU intensive. The
	// larger the input (more or larger chunks/availability data), the more CPU cycles will be
	// spent.
	//
	// For example, for 32KB PoVs, we'd expect re-encode to eat as much as 90ms and 500ms for
	// 2.5MiB.
	//
	// After executing such a task, the worker sends the response via a provided `oneshot` sender.
	//
	// The caller is responsible for routing work to the workers.
	#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
	pub fn build<Context>(
		size: NonZeroUsize,
		metrics: Metrics,
		ctx: &mut Context,
	) -> Vec<futures::channel::mpsc::Sender<ErasureTask>> {
		// At least 1 task, at most `MAX_THREADS.
		let size = std::cmp::min(size, MAX_THREADS);
		let mut senders = Vec::new();

		for index in 0..size.into() {
			let (tx, rx) = futures::channel::mpsc::channel(8);
			senders.push(tx);

			if let Err(e) = ctx
				.spawn_blocking("erasure-task", Box::pin(erasure_task_thread(metrics.clone(), rx)))
			{
				gum::warn!(
					target: LOG_TARGET,
					err = ?e,
					index,
					"Failed to spawn a erasure task",
				);
			}
		}
		senders
	}
}

// Handles CPU intensive operation on a dedicated blocking thread.
async fn erasure_task_thread(
	metrics: Metrics,
	mut ingress: futures::channel::mpsc::Receiver<ErasureTask>,
) {
	loop {
		match ingress.next().await {
			Some(ErasureTask::Reconstruct(n_validators, chunks, sender)) => {
				let _ = sender.send(polkadot_erasure_coding::reconstruct_v1(
					n_validators,
					chunks.iter().map(|(c_index, chunk)| {
						(
							&chunk[..],
							usize::try_from(c_index.0)
								.expect("usize is at least u32 bytes on all modern targets."),
						)
					}),
				));
			},
			Some(ErasureTask::Reencode(n_validators, root, available_data, sender)) => {
				let metrics = metrics.clone();

				let maybe_data = if reconstructed_data_matches_root(
					n_validators,
					&root,
					&available_data,
					&metrics,
				) {
					Some(available_data)
				} else {
					None
				};

				let _ = sender.send(maybe_data);
			},
			None => {
				gum::trace!(
					target: LOG_TARGET,
					"Erasure task channel closed. Node shutting down ?",
				);
				break
			},
		}

		// In benchmarks this is a very hot loop not yielding at all.
		// To update CPU metrics for the task we need to yield.
		#[cfg(feature = "subsystem-benchmarks")]
		tokio::task::yield_now().await;
	}
}

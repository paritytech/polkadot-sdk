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
	collections::{HashMap, VecDeque},
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
use schnellru::{ByLength, LruMap};
use task::{FetchChunks, FetchChunksParams, FetchFull, FetchFullParams};

use fatality::Nested;
use polkadot_erasure_coding::{
	branch_hash, branches, obtain_chunks_v1, recovery_threshold, Error as ErasureEncodingError,
};
use task::{RecoveryParams, RecoveryStrategy, RecoveryTask};

use polkadot_node_network_protocol::{
	request_response::{v1 as request_v1, IncomingRequestReceiver},
	UnifiedReputationChange as Rep,
};
use polkadot_node_primitives::{AvailableData, ErasureChunk};
use polkadot_node_subsystem::{
	errors::RecoveryError,
	jaeger,
	messages::{AvailabilityRecoveryMessage, AvailabilityStoreMessage},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem,
	SubsystemContext, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_util::request_session_info;
use polkadot_primitives::{
	BlakeTwo256, BlockNumber, CandidateHash, CandidateReceipt, GroupIndex, Hash, HashT,
	SessionIndex, SessionInfo, ValidatorIndex,
};

mod error;
mod futures_undead;
mod metrics;
mod task;
pub use metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::availability-recovery";

// Size of the LRU cache where we keep recovered data.
const LRU_SIZE: u32 = 16;

const COST_INVALID_REQUEST: Rep = Rep::CostMajor("Peer sent unparsable request");

/// PoV size limit in bytes for which prefer fetching from backers.
const SMALL_POV_LIMIT: usize = 128 * 1024;

#[derive(Clone, PartialEq)]
/// The strategy we use to recover the PoV.
pub enum RecoveryStrategyKind {
	/// We always try the backing group first, then fallback to validator chunks.
	BackersFirstAlways,
	/// We try the backing group first if PoV size is lower than specified, then fallback to
	/// validator chunks.
	BackersFirstIfSizeLower(usize),
	/// We always recover using validator chunks.
	ChunksAlways,
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
}

#[derive(Clone, PartialEq, Debug)]
/// The type of check to perform after available data was recovered.
pub enum PostRecoveryCheck {
	/// Reencode the data and check erasure root. For validators.
	Reencode,
	/// Only check the pov hash. For collators only.
	PovHash,
}

/// Expensive erasure coding computations that we want to run on a blocking thread.
pub enum ErasureTask {
	/// Reconstructs `AvailableData` from chunks given `n_validators`.
	Reconstruct(
		usize,
		HashMap<ValidatorIndex, ErasureChunk>,
		oneshot::Sender<Result<AvailableData, ErasureEncodingError>>,
	),
	/// Re-encode `AvailableData` into erasure chunks in order to verify the provided root hash of
	/// the Merkle tree.
	Reencode(usize, Hash, AvailableData, oneshot::Sender<Option<AvailableData>>),
}

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
					validator_index = ?chunk.index,
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
			validator_index = ?chunk.index,
			"Merkle proof mismatch"
		);
		return false
	}
	true
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
	remote: RemoteHandle<Result<AvailableData, RecoveryError>>,
	awaiting: Vec<oneshot::Sender<Result<AvailableData, RecoveryError>>>,
}

impl Future for RecoveryHandle {
	type Output = Option<(CandidateHash, Result<AvailableData, RecoveryError>)>;

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
	fn into_result(self) -> Result<AvailableData, RecoveryError> {
		match self {
			Self::Valid(d) => Ok(d),
			Self::Invalid => Err(RecoveryError::Invalid),
		}
	}
}

impl TryFrom<Result<AvailableData, RecoveryError>> for CachedRecovery {
	type Error = ();
	fn try_from(o: Result<AvailableData, RecoveryError>) -> Result<CachedRecovery, Self::Error> {
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
}

impl Default for State {
	fn default() -> Self {
		Self {
			ongoing_recoveries: FuturesUnordered::new(),
			live_block: (0, Hash::default()),
			availability_lru: LruMap::new(ByLength::new(LRU_SIZE)),
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
async fn handle_signal(state: &mut State, signal: OverseerSignal) -> SubsystemResult<bool> {
	match signal {
		OverseerSignal::Conclude => Ok(true),
		OverseerSignal::ActiveLeaves(ActiveLeavesUpdate { activated, .. }) => {
			// if activated is non-empty, set state.live_block to the highest block in `activated`
			if let Some(activated) = activated {
				if activated.number > state.live_block.0 {
					state.live_block = (activated.number, activated.hash)
				}
			}

			Ok(false)
		},
		OverseerSignal::BlockFinalized(_, _) => Ok(false),
	}
}

/// Machinery around launching recovery tasks into the background.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn launch_recovery_task<Context>(
	state: &mut State,
	ctx: &mut Context,
	session_info: SessionInfo,
	receipt: CandidateReceipt,
	response_sender: oneshot::Sender<Result<AvailableData, RecoveryError>>,
	metrics: &Metrics,
	recovery_strategies: VecDeque<Box<dyn RecoveryStrategy<<Context as SubsystemContext>::Sender>>>,
	bypass_availability_store: bool,
	post_recovery_check: PostRecoveryCheck,
) -> error::Result<()> {
	let candidate_hash = receipt.hash();
	let params = RecoveryParams {
		validator_authority_keys: session_info.discovery_keys.clone(),
		n_validators: session_info.validators.len(),
		threshold: recovery_threshold(session_info.validators.len())?,
		candidate_hash,
		erasure_root: receipt.descriptor.erasure_root,
		metrics: metrics.clone(),
		bypass_availability_store,
		post_recovery_check,
		pov_hash: receipt.descriptor.pov_hash,
	};

	let recovery_task = RecoveryTask::new(ctx.sender().clone(), params, recovery_strategies);

	let (remote, remote_handle) = recovery_task.run().remote_handle();

	state.ongoing_recoveries.push(RecoveryHandle {
		candidate_hash,
		remote: remote_handle,
		awaiting: vec![response_sender],
	});

	if let Err(e) = ctx.spawn("recovery-task", Box::pin(remote)) {
		gum::warn!(
			target: LOG_TARGET,
			err = ?e,
			"Failed to spawn a recovery task",
		);
	}

	Ok(())
}

/// Handles an availability recovery request.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn handle_recover<Context>(
	state: &mut State,
	ctx: &mut Context,
	receipt: CandidateReceipt,
	session_index: SessionIndex,
	backing_group: Option<GroupIndex>,
	response_sender: oneshot::Sender<Result<AvailableData, RecoveryError>>,
	metrics: &Metrics,
	erasure_task_tx: futures::channel::mpsc::Sender<ErasureTask>,
	recovery_strategy_kind: RecoveryStrategyKind,
	bypass_availability_store: bool,
	post_recovery_check: PostRecoveryCheck,
) -> error::Result<()> {
	let candidate_hash = receipt.hash();

	let span = jaeger::Span::new(candidate_hash, "availability-recovery")
		.with_stage(jaeger::Stage::AvailabilityRecovery);

	if let Some(result) =
		state.availability_lru.get(&candidate_hash).cloned().map(|v| v.into_result())
	{
		if let Err(e) = response_sender.send(result) {
			gum::warn!(
				target: LOG_TARGET,
				err = ?e,
				"Error responding with an availability recovery result",
			);
		}
		return Ok(())
	}

	if let Some(i) =
		state.ongoing_recoveries.iter_mut().find(|i| i.candidate_hash == candidate_hash)
	{
		i.awaiting.push(response_sender);
		return Ok(())
	}

	let _span = span.child("not-cached");
	let session_info = request_session_info(state.live_block.1, session_index, ctx.sender())
		.await
		.await
		.map_err(error::Error::CanceledSessionInfo)??;

	let _span = span.child("session-info-ctx-received");
	match session_info {
		Some(session_info) => {
			let mut recovery_strategies: VecDeque<
				Box<dyn RecoveryStrategy<<Context as SubsystemContext>::Sender>>,
			> = VecDeque::with_capacity(2);

			if let Some(backing_group) = backing_group {
				if let Some(backing_validators) = session_info.validator_groups.get(backing_group) {
					let mut small_pov_size = true;

					if let RecoveryStrategyKind::BackersFirstIfSizeLower(small_pov_limit) =
						recovery_strategy_kind
					{
						// Get our own chunk size to get an estimate of the PoV size.
						let chunk_size: Result<Option<usize>, error::Error> =
							query_chunk_size(ctx, candidate_hash).await;
						if let Ok(Some(chunk_size)) = chunk_size {
							let pov_size_estimate =
								chunk_size.saturating_mul(session_info.validators.len()) / 3;
							small_pov_size = pov_size_estimate < small_pov_limit;

							gum::trace!(
								target: LOG_TARGET,
								?candidate_hash,
								pov_size_estimate,
								small_pov_limit,
								enabled = small_pov_size,
								"Prefer fetch from backing group",
							);
						} else {
							// we have a POV limit but were not able to query the chunk size, so
							// don't use the backing group.
							small_pov_size = false;
						}
					};

					match (&recovery_strategy_kind, small_pov_size) {
						(RecoveryStrategyKind::BackersFirstAlways, _) |
						(RecoveryStrategyKind::BackersFirstIfSizeLower(_), true) => recovery_strategies.push_back(
							Box::new(FetchFull::new(FetchFullParams {
								validators: backing_validators.to_vec(),
								erasure_task_tx: erasure_task_tx.clone(),
							})),
						),
						_ => {},
					};
				}
			}

			recovery_strategies.push_back(Box::new(FetchChunks::new(FetchChunksParams {
				n_validators: session_info.validators.len(),
				erasure_task_tx,
			})));

			launch_recovery_task(
				state,
				ctx,
				session_info,
				receipt,
				response_sender,
				metrics,
				recovery_strategies,
				bypass_availability_store,
				post_recovery_check,
			)
			.await
		},
		None => {
			gum::warn!(target: LOG_TARGET, "SessionInfo is `None` at {:?}", state.live_block);
			response_sender
				.send(Err(RecoveryError::Unavailable))
				.map_err(|_| error::Error::CanceledResponseSender)?;
			Ok(())
		},
	}
}

/// Queries a chunk from av-store.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn query_full_data<Context>(
	ctx: &mut Context,
	candidate_hash: CandidateHash,
) -> error::Result<Option<AvailableData>> {
	let (tx, rx) = oneshot::channel();
	ctx.send_message(AvailabilityStoreMessage::QueryAvailableData(candidate_hash, tx))
		.await;

	rx.await.map_err(error::Error::CanceledQueryFullData)
}

/// Queries a chunk from av-store.
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
async fn query_chunk_size<Context>(
	ctx: &mut Context,
	candidate_hash: CandidateHash,
) -> error::Result<Option<usize>> {
	let (tx, rx) = oneshot::channel();
	ctx.send_message(AvailabilityStoreMessage::QueryChunkSize(candidate_hash, tx))
		.await;

	rx.await.map_err(error::Error::CanceledQueryFullData)
}

#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
impl AvailabilityRecoverySubsystem {
	/// Create a new instance of `AvailabilityRecoverySubsystem` suitable for collator nodes,
	/// which never requests the `AvailabilityStoreSubsystem` subsystem and only checks the POV hash
	/// instead of reencoding the available data.
	pub fn for_collator(
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind: RecoveryStrategyKind::BackersFirstIfSizeLower(SMALL_POV_LIMIT),
			bypass_availability_store: true,
			post_recovery_check: PostRecoveryCheck::PovHash,
			req_receiver,
			metrics,
		}
	}

	/// Create a new instance of `AvailabilityRecoverySubsystem` which starts with a fast path to
	/// request data from backers.
	pub fn with_fast_path(
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind: RecoveryStrategyKind::BackersFirstAlways,
			bypass_availability_store: false,
			post_recovery_check: PostRecoveryCheck::Reencode,
			req_receiver,
			metrics,
		}
	}

	/// Create a new instance of `AvailabilityRecoverySubsystem` which requests only chunks
	pub fn with_chunks_only(
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind: RecoveryStrategyKind::ChunksAlways,
			bypass_availability_store: false,
			post_recovery_check: PostRecoveryCheck::Reencode,
			req_receiver,
			metrics,
		}
	}

	/// Create a new instance of `AvailabilityRecoverySubsystem` which requests chunks if PoV is
	/// above a threshold.
	pub fn with_chunks_if_pov_large(
		req_receiver: IncomingRequestReceiver<request_v1::AvailableDataFetchingRequest>,
		metrics: Metrics,
	) -> Self {
		Self {
			recovery_strategy_kind: RecoveryStrategyKind::BackersFirstIfSizeLower(SMALL_POV_LIMIT),
			bypass_availability_store: false,
			post_recovery_check: PostRecoveryCheck::Reencode,
			req_receiver,
			metrics,
		}
	}

	/// Starts the inner subsystem loop.
	pub async fn run<Context>(self, mut ctx: Context) -> SubsystemResult<()> {
		let mut state = State::default();
		let Self {
			mut req_receiver,
			metrics,
			recovery_strategy_kind,
			bypass_availability_store,
			post_recovery_check,
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
			futures::select! {
				erasure_task = erasure_task_rx.next() => {
					match erasure_task {
						Some(task) => {
							let send_result = to_pool
								.next()
								.expect("Pool size is `NonZeroUsize`; qed")
								.send(task)
								.await
								.map_err(|_| RecoveryError::ChannelClosed);

							if let Err(err) = send_result {
								gum::warn!(
									target: LOG_TARGET,
									?err,
									"Failed to send erasure coding task",
								);
							}
						},
						None => {
							gum::debug!(
								target: LOG_TARGET,
								"Erasure task channel closed",
							);

							return Err(SubsystemError::with_origin("availability-recovery", RecoveryError::ChannelClosed))
						}
					}
				}
				v = ctx.recv().fuse() => {
					match v? {
						FromOrchestra::Signal(signal) => if handle_signal(
							&mut state,
							signal,
						).await? {
							gum::debug!(target: LOG_TARGET, "subsystem concluded");
							return Ok(());
						}
						FromOrchestra::Communication { msg } => {
							match msg {
								AvailabilityRecoveryMessage::RecoverAvailableData(
									receipt,
									session_index,
									maybe_backing_group,
									response_sender,
								) => {
									if let Err(e) = handle_recover(
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
										post_recovery_check.clone()
									).await {
										gum::warn!(
											target: LOG_TARGET,
											err = ?e,
											"Error handling a recovery request",
										);
									}
								}
							}
						}
					}
				}
				in_req = recv_req => {
					match in_req.into_nested().map_err(|fatal| SubsystemError::with_origin("availability-recovery", fatal))? {
						Ok(req) => {
							if bypass_availability_store {
								gum::debug!(
									target: LOG_TARGET,
									"Skipping request to availability-store.",
								);
								let _ = req.send_response(None.into());
								continue
							}
							match query_full_data(&mut ctx, req.payload.candidate_hash).await {
								Ok(res) => {
									let _ = req.send_response(res.into());
								}
								Err(e) => {
									gum::debug!(
										target: LOG_TARGET,
										err = ?e,
										"Failed to query available data.",
									);

									let _ = req.send_response(None.into());
								}
							}
						}
						Err(jfyi) => {
							gum::debug!(
								target: LOG_TARGET,
								error = ?jfyi,
								"Decoding incoming request failed"
							);
							continue
						}
					}
				}
				output = state.ongoing_recoveries.select_next_some() => {
					if let Some((candidate_hash, result)) = output {
						if let Ok(recovery) = CachedRecovery::try_from(result) {
							state.availability_lru.insert(candidate_hash, recovery);
						}
					}
				}
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
					chunks.values().map(|c| (&c.chunk[..], c.index.0 as usize)),
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

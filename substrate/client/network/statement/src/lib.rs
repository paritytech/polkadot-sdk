// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Statement handling to plug on top of the network service.
//!
//! Usage:
//!
//! - Use [`StatementHandlerPrototype::new`] to create a prototype.
//! - Pass the `NonDefaultSetConfig` returned from [`StatementHandlerPrototype::new`] to the network
//!   configuration as an extra peers set.
//! - Use [`StatementHandlerPrototype::build`] then [`StatementHandler::run`] to obtain a
//! `Future` that processes statements.

use crate::config::*;

use codec::{Compact, Decode, Encode, MaxEncodedLen};
#[cfg(any(test, feature = "test-helpers"))]
use futures::future::pending;
use futures::{future::FusedFuture, prelude::*, stream::FuturesUnordered};
use prometheus_endpoint::{
	prometheus, register, Counter, Gauge, Histogram, HistogramOpts, PrometheusError, Registry, U64,
};
use sc_network::{
	config::{NonReservedPeerMode, SetConfig},
	error, multiaddr,
	peer_store::PeerStoreProvider,
	service::{
		traits::{NotificationEvent, NotificationService, ValidationResult},
		NotificationMetrics,
	},
	types::ProtocolName,
	utils::{interval, LruHashSet},
	NetworkBackend, NetworkEventStream, NetworkPeers, ObservedRole,
};
use sc_network_sync::{SyncEvent, SyncEventStream};
use sc_network_types::PeerId;
use sp_runtime::traits::Block as BlockT;
use sp_statement_store::{
	FilterDecision, Hash, Statement, StatementSource, StatementStore, SubmitResult,
};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
	iter,
	num::NonZeroUsize,
	pin::Pin,
	sync::{Arc, RwLock},
};
use tokio::time::timeout;
pub mod config;

/// A set of statements.
pub type Statements = Vec<Statement>;

mod rep {
	use sc_network::ReputationChange as Rep;
	/// Reputation change when a peer sends us any statement.
	///
	/// This forces node to verify it, thus the negative value here. Once statement is verified,
	/// reputation change should be refunded with `ANY_STATEMENT_REFUND`
	pub const ANY_STATEMENT: Rep = Rep::new(-(1 << 4), "Any statement");
	/// Reputation change when a peer sends us any statement that is not invalid.
	pub const ANY_STATEMENT_REFUND: Rep = Rep::new(1 << 4, "Any statement (refund)");
	/// Reputation change when a peer sends us an statement that we didn't know about.
	pub const GOOD_STATEMENT: Rep = Rep::new(1 << 8, "Good statement");
	/// Reputation change when a peer sends us an invalid statement.
	pub const INVALID_STATEMENT: Rep = Rep::new(-(1 << 12), "Invalid statement");
	/// Reputation change when a peer sends us a duplicate statement.
	pub const DUPLICATE_STATEMENT: Rep = Rep::new(-(1 << 7), "Duplicate statement");
}

const LOG_TARGET: &str = "statement-gossip";
/// Maximim time we wait for sending a notification to a peer.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Interval for sending statement batches during initial sync to new peers.
const INITIAL_SYNC_BURST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[cfg_attr(any(test, feature = "test-helpers"), doc(hidden))]
#[cfg_attr(any(test, feature = "test-helpers"), allow(dead_code))]
#[derive(Debug)]
pub struct Metrics {
	propagated_statements: Counter<U64>,
	known_statements_received: Counter<U64>,
	skipped_oversized_statements: Counter<U64>,
	propagated_statements_chunks: Histogram,
	pending_statements: Gauge<U64>,
	ignored_statements: Counter<U64>,
}

impl Metrics {
	fn register(r: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			propagated_statements: register(
				Counter::new(
					"substrate_sync_propagated_statements",
					"Number of statements propagated to at least one peer",
				)?,
				r,
			)?,
			known_statements_received: register(
				Counter::new(
					"substrate_sync_known_statement_received",
					"Number of statements received via gossiping that were already in the statement store",
				)?,
				r,
			)?,
			skipped_oversized_statements: register(
				Counter::new(
					"substrate_sync_skipped_oversized_statements",
					"Number of oversized statements that were skipped to be gossiped",
				)?,
				r,
			)?,
			propagated_statements_chunks: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_sync_propagated_statements_chunks",
						"Distribution of chunk sizes when propagating statements",
					).buckets(prometheus::exponential_buckets(1.0, 2.0, 14)?),
				)?,
				r,
			)?,
			pending_statements: register(
				Gauge::new(
					"substrate_sync_pending_statement_validations",
					"Number of pending statement validations",
				)?,
				r,
			)?,
			ignored_statements: register(
				Counter::new(
					"substrate_sync_ignored_statements",
					"Number of statements ignored due to exceeding MAX_PENDING_STATEMENTS limit",
				)?,
				r,
			)?,
		})
	}
}

/// Prototype for a [`StatementHandler`].
pub struct StatementHandlerPrototype {
	protocol_name: ProtocolName,
	notification_service: Box<dyn NotificationService>,
}

impl StatementHandlerPrototype {
	/// Create a new instance.
	pub fn new<
		Hash: AsRef<[u8]>,
		Block: BlockT,
		Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
	>(
		genesis_hash: Hash,
		fork_id: Option<&str>,
		metrics: NotificationMetrics,
		peer_store_handle: Arc<dyn PeerStoreProvider>,
	) -> (Self, Net::NotificationProtocolConfig) {
		let genesis_hash = genesis_hash.as_ref();
		let protocol_name = if let Some(fork_id) = fork_id {
			format!("/{}/{}/statement/1", array_bytes::bytes2hex("", genesis_hash), fork_id)
		} else {
			format!("/{}/statement/1", array_bytes::bytes2hex("", genesis_hash))
		};
		let (config, notification_service) = Net::notification_config(
			protocol_name.clone().into(),
			Vec::new(),
			MAX_STATEMENT_NOTIFICATION_SIZE,
			None,
			SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Deny,
			},
			metrics,
			peer_store_handle,
		);

		(Self { protocol_name: protocol_name.into(), notification_service }, config)
	}

	/// Turns the prototype into the actual handler.
	///
	/// Important: the statements handler is initially disabled and doesn't gossip statements.
	/// Gossiping is enabled when major syncing is done.
	pub fn build<
		N: NetworkPeers + NetworkEventStream,
		S: SyncEventStream + sp_consensus::SyncOracle,
	>(
		self,
		network: N,
		sync: S,
		statement_store: Arc<dyn StatementStore>,
		metrics_registry: Option<&Registry>,
		executor: impl Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send,
		mut num_submission_workers: usize,
	) -> error::Result<StatementHandler<N, S>> {
		let sync_event_stream = sync.event_stream("statement-handler-sync");

		// Channel for submitting statements to the store (validation worker).
		let (submit_queue_sender, submit_queue_receiver) =
			async_channel::bounded::<(Hash, Statement)>(MAX_PENDING_STATEMENTS);

		// Channel for on_statements requests from main loop to worker processing statements
		let (statements_queue_sender, statements_queue_receiver) =
			async_channel::bounded::<OnStatementsRequest>(MAX_PENDING_STATEMENTS);

		// Channel for worker events back to main loop.
		let (results_queue_sender, results_queue_receiver) =
			async_channel::bounded::<WorkerEvent>(MAX_PENDING_STATEMENTS);

		// Shared state for pending statements.
		let pending_state = Arc::new(RwLock::new(PendingState::new()));

		// Create metrics early so we can share with worker.
		let metrics = if let Some(r) = metrics_registry {
			Some(Arc::new(Metrics::register(r)?))
		} else {
			None
		};

		if num_submission_workers == 0 {
			log::warn!(
				target: LOG_TARGET,
				"num_submission_workers is 0, defaulting to 1"
			);
			num_submission_workers = 1;
		}

		// Spawn the statement store submission workers.
		// These workers validate statements and send results directly to the main loop.
		for _ in 0..num_submission_workers {
			let store = statement_store.clone();
			let validation_results_sender = results_queue_sender.clone();
			let mut submit_queue_receiver = submit_queue_receiver.clone();
			executor(
				async move {
					loop {
						let task: Option<(Hash, Statement)> = submit_queue_receiver.next().await;
						match task {
							None => return,
							Some((hash, statement)) => {
								let result = store.submit(statement, StatementSource::Network);
								// Send result directly to main loop.
								let event = WorkerEvent::StatementResult(StatementProcessResult {
									hash,
									result: Some(result),
								});
								if validation_results_sender.send(event).await.is_err() {
									log::debug!(
										target: LOG_TARGET,
										"Error sending validation result, receiver dropped"
									);
									return;
								}
							},
						}
					}
				}
				.boxed(),
			);
		}

		// Create shared peers map.
		let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));

		// Spawn the statements processing workers.
		for _ in 0..num_submission_workers {
			let worker_pending_state = pending_state.clone();
			let worker_statement_store = statement_store.clone();
			let worker_peers = peers.clone();
			let worker_metrics = metrics.clone();
			let worker_statements_queue_receiver = statements_queue_receiver.clone();
			let worker_submit_queue_sender = submit_queue_sender.clone();
			let worker_results_queue_sender = results_queue_sender.clone();
			executor(
				Self::run_on_statements_worker(
					worker_statements_queue_receiver,
					worker_submit_queue_sender,
					worker_results_queue_sender,
					worker_pending_state,
					worker_statement_store,
					worker_peers,
					worker_metrics,
				)
				.boxed(),
			);
		}

		let handler = StatementHandler {
			protocol_name: self.protocol_name,
			notification_service: self.notification_service,
			propagate_timeout: (Box::pin(interval(PROPAGATE_TIMEOUT))
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network,
			sync,
			sync_event_stream: sync_event_stream.fuse(),
			peers,
			statement_store,
			metrics,
			initial_sync_timeout: Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};

		Ok(handler)
	}

	/// Worker that processes statements received from the network.
	pub async fn run_on_statements_worker(
		mut receiver: async_channel::Receiver<OnStatementsRequest>,
		submit_queue_sender: async_channel::Sender<(Hash, Statement)>,
		event_sender: async_channel::Sender<WorkerEvent>,
		shared_state: Arc<RwLock<PendingState>>,
		statement_store: Arc<dyn StatementStore>,
		shared_peers: PeersState,
		metrics: Option<Arc<Metrics>>,
	) {
		loop {
			match receiver.next().await {
				None => return,
				Some(OnStatementsRequest { who, notification }) => {
					Self::process_on_statements(
						who,
						notification,
						&shared_state,
						&statement_store,
						&submit_queue_sender,
						&event_sender,
						&shared_peers,
						&metrics,
					)
					.await;
				},
			}
		}
	}

	/// Process statements from a peer (runs in worker thread).
	/// Decodes the notification, filters statements, and queues them for validation.
	async fn process_on_statements(
		who: PeerId,
		notification: Vec<u8>,
		shared_state: &Arc<RwLock<PendingState>>,
		statement_store: &Arc<dyn StatementStore>,
		submit_queue_sender: &async_channel::Sender<(Hash, Statement)>,
		event_sender: &async_channel::Sender<WorkerEvent>,
		shared_peers: &PeersState,
		metrics: &Option<Arc<Metrics>>,
	) {
		// Decode the notification
		let Ok(statements) = <Statements as Decode>::decode(&mut notification.as_ref()) else {
			log::debug!(target: LOG_TARGET, "Worker: Failed to decode statement list from {who}");
			return;
		};

		log::trace!(target: LOG_TARGET, "Worker processing {} statements from {}", statements.len(), who);

		// Track aggregated reputation change for this peer
		let mut aggregated_reputation: i32 = 0;

		for s in statements {
			let hash = s.hash();

			// Mark statement as known for this peer.
			{
				let Ok(mut guard) = shared_peers.write() else {
					log::error!(target: LOG_TARGET, "shared_peers lock poisoned");
					break;
				};
				// We can't punish peers for sending us statements we already know from them,
				// because there might be a race between us sending them the statement and them
				// sending it to us. So we just skip duplicates from the same peer
				if !guard
					.get_mut(&who)
					.map(|peer| peer.known_statements.insert(hash))
					.unwrap_or(true)
				{
					continue;
				}
			};

			// Check if we already received this from the same peer while pending validation.
			{
				let Ok(state) = shared_state.read() else {
					log::error!(target: LOG_TARGET, "shared_state lock poisoned");
					break;
				};
				if state
					.pending_statements_peers
					.get(&hash)
					.map(|peers| peers.contains(&who))
					.unwrap_or(false)
				{
					log::trace!(
						target: LOG_TARGET,
						"Worker: Already received the statement from the same peer {who} while pending.",
					);
					aggregated_reputation =
						aggregated_reputation.saturating_add(rep::DUPLICATE_STATEMENT.value);
					continue;
				}
			}

			// Skip if we already have this statement (from another peer).
			if statement_store.has_statement(&hash) {
				log::trace!(
					target: LOG_TARGET,
					"Worker: Already have statement in store (from another peer), skipping.",
				);
				if let Some(metrics) = metrics {
					metrics.known_statements_received.inc();
				}
				continue;
			}

			// Queue ANY_STATEMENT reputation report for new statements
			aggregated_reputation = aggregated_reputation.saturating_add(rep::ANY_STATEMENT.value);

			// Acquire write lock to update pending state.
			let Ok(mut state) = shared_state.write() else {
				log::error!(target: LOG_TARGET, "shared_state lock poisoned");
				break;
			};

			// Check pending limit.
			if state.pending_statements_peers.len() > MAX_PENDING_STATEMENTS {
				log::debug!(
					target: LOG_TARGET,
					"Worker: Ignoring statement, exceeded MAX_PENDING_STATEMENTS limit",
				);
				if let Some(metrics) = metrics {
					metrics.ignored_statements.inc();
				}
				break;
			}

			match state.pending_statements_peers.entry(hash) {
				Entry::Vacant(entry) => match submit_queue_sender.try_send((hash, s)) {
					Ok(()) => {
						entry.insert(HashSet::from_iter([who]));
					},
					Err(async_channel::TrySendError::Full(_)) => {
						log::debug!(
							target: LOG_TARGET,
							"Worker: Dropped statement because validation channel is full",
						);
					},
					Err(async_channel::TrySendError::Closed(_)) => {
						log::trace!(
							target: LOG_TARGET,
							"Worker: Dropped statement because validation channel is closed",
						);
					},
				},
				Entry::Occupied(mut entry) =>
					if !entry.get_mut().insert(who) {
						//  We might have raced with another worker thread adding the same peer.
						aggregated_reputation =
							aggregated_reputation.saturating_add(rep::DUPLICATE_STATEMENT.value);
					},
			}
		}

		// Send aggregated reputation change if any
		if aggregated_reputation != 0 {
			let _ = event_sender
				.send(WorkerEvent::ReputationChange(ReputationChange {
					peer: who,
					change: sc_network::ReputationChange::new(
						aggregated_reputation,
						"Statement batch",
					),
				}))
				.await;
		}
	}
}

/// Shared peers map type alias for clarity.
pub type PeersState = Arc<RwLock<HashMap<PeerId, Peer>>>;

/// Handler for statements. Call [`StatementHandler::run`] to start the processing.
pub struct StatementHandler<
	N: NetworkPeers + NetworkEventStream,
	S: SyncEventStream + sp_consensus::SyncOracle,
> {
	protocol_name: ProtocolName,
	/// Interval at which we call `propagate_statements`.
	propagate_timeout: stream::Fuse<Pin<Box<dyn Stream<Item = ()> + Send>>>,
	/// Shared state for pending statements accessed by main loop and worker threads that process
	/// statements coming from the network.
	pending_state: Arc<RwLock<PendingState>>,
	/// Receiver for events from worker threads, about the results of statement processing.
	pub results_queue_receiver: async_channel::Receiver<WorkerEvent>,
	/// Sender for on_statements requests to worker threads that does the initial decoding and
	/// filtering.
	pub statements_queue_sender: async_channel::Sender<OnStatementsRequest>,
	/// Network service to use to send messages and manage peers.
	network: N,
	/// Syncing service.
	sync: S,
	/// Receiver for syncing-related events.
	sync_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = SyncEvent> + Send>>>,
	/// Notification service.
	notification_service: Box<dyn NotificationService>,
	/// All connected peers (shared with worker thread for direct known_statements updates).
	peers: PeersState,
	statement_store: Arc<dyn StatementStore>,
	/// Prometheus metrics (shared with worker thread).
	metrics: Option<Arc<Metrics>>,
	/// Timeout for sending next statement batch during initial sync.
	initial_sync_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Pending initial syncs per peer.
	pending_initial_syncs: HashMap<PeerId, PendingInitialSync>,
	/// Queue for round-robin processing of initial syncs.
	initial_sync_peer_queue: VecDeque<PeerId>,
	/// Pending statement send operations, polled by the main loop.
	pending_sends: PendingSends,
}

/// Peer information
#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
#[derive(Debug)]
pub struct Peer {
	/// Holds a set of statements known to this peer.
	known_statements: LruHashSet<Hash>,
	role: ObservedRole,
}

/// Tracks pending initial sync state for a peer (hashes only, statements fetched on-demand).
struct PendingInitialSync {
	hashes: Vec<Hash>,
}

/// Message sent from main loop to on_statements worker thread.
#[cfg_attr(any(test, feature = "test-helpers"), derive(Debug, Clone))]
pub struct OnStatementsRequest {
	/// The peer that sent the statements.
	pub who: PeerId,
	/// Raw notification bytes to decode in worker thread.
	pub notification: Vec<u8>,
}

/// Result from processing a statement in the worker thread.
#[derive(Debug)]
pub struct StatementProcessResult {
	/// Hash of the statement that was processed.
	pub hash: Hash,
	/// Result of the statement submission.
	pub result: Option<SubmitResult>,
}

/// Reputation change to be applied by the main loop.
#[derive(Debug)]
pub struct ReputationChange {
	/// The peer to report.
	pub peer: PeerId,
	/// The reputation change to apply.
	pub change: sc_network::ReputationChange,
}

/// Result of a pending statement send operation.
struct PendingSendResult {
	/// The peer the notification was sent to.
	peer: PeerId,
	/// Number of statements in the chunk.
	count: usize,
	/// Result of the send operation (Ok if successful, Err with error or timeout).
	result: Result<Result<(), sc_network::error::Error>, tokio::time::error::Elapsed>,
}

/// Type alias for the pending sends future collection, this is a list of in-flight sends to peers.
type PendingSends =
	FuturesUnordered<Pin<Box<dyn Future<Output = PendingSendResult> + Send + 'static>>>;

/// Events sent from workers back to main loop.
#[derive(Debug)]
pub enum WorkerEvent {
	/// A statement was processed and completed validation.
	StatementResult(StatementProcessResult),
	/// A reputation change should be applied.
	ReputationChange(ReputationChange),
}

/// Shared state for pending statements, protected by RwLock.
#[derive(Debug)]
pub struct PendingState {
	/// As multiple peers can send us the same statement, we group
	/// these peers using the statement hash while the statement is
	/// imported. This prevents that we import the same statement
	/// multiple times concurrently.
	pub pending_statements_peers: HashMap<Hash, HashSet<PeerId>>,
}

impl PendingState {
	/// Create a new empty shared pending state.
	pub fn new() -> Self {
		Self { pending_statements_peers: HashMap::new() }
	}
}

/// Result of finding a sendable chunk of statements.
enum ChunkResult {
	/// Found a chunk that fits. Contains the end index (exclusive).
	Send(usize),
	/// First statement is oversized, skip it.
	SkipOversized,
}

/// Result of sending a chunk of statements.
enum SendChunkResult {
	/// Successfully sent a chunk of N statements.
	Sent(usize),
	/// First statement was oversized and skipped.
	Skipped,
	/// Nothing to send.
	Empty,
	/// Send failed.
	Failed,
}

/// Returns the maximum payload size for statement notifications.
///
/// This reserves space for encoding the length of the vector (Compact<u32>),
/// ensuring the final encoded message fits within MAX_STATEMENT_NOTIFICATION_SIZE.
fn max_statement_payload_size() -> usize {
	MAX_STATEMENT_NOTIFICATION_SIZE as usize - Compact::<u32>::max_encoded_len()
}

/// Find the largest chunk of statements starting from the beginning that fits
/// within MAX_STATEMENT_NOTIFICATION_SIZE.
///
/// Uses an incremental approach: adds statements one by one until the limit is reached.
/// This is efficient because we only compute sizes for statements we'll actually send
/// in this chunk, rather than computing sizes for all statements upfront.
fn find_sendable_chunk(statements: &[&Statement]) -> ChunkResult {
	if statements.is_empty() {
		return ChunkResult::Send(0);
	}
	let max_size = max_statement_payload_size();

	// Incrementally add statements until we exceed the limit.
	// This is efficient because we only compute sizes for statements in this chunk.
	// accumulated_size is the sum of encoded sizes of all statements so far (without vec
	// overhead).
	let mut accumulated_size = 0;
	let mut count = 0usize;

	for stmt in &statements[0..] {
		let stmt_size = stmt.encoded_size();
		let new_count = count + 1;
		// Compact encoding overhead for the new count
		let new_total = accumulated_size + stmt_size;
		if new_total > max_size {
			break;
		}

		accumulated_size += stmt_size;
		count = new_count;
	}

	// If we couldn't fit even a single statement, skip it.
	if count == 0 {
		ChunkResult::SkipOversized
	} else {
		ChunkResult::Send(count)
	}
}

impl Peer {
	/// Create a new peer for testing/benchmarking purposes.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_for_testing(known_statements: LruHashSet<Hash>, role: ObservedRole) -> Self {
		Self { known_statements, role }
	}
}

impl<N, S> StatementHandler<N, S>
where
	N: NetworkPeers + NetworkEventStream,
	S: SyncEventStream + sp_consensus::SyncOracle,
{
	/// Create a new `StatementHandler` for testing/benchmarking purposes.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_for_testing(
		protocol_name: ProtocolName,
		notification_service: Box<dyn NotificationService>,
		propagate_timeout: stream::Fuse<Pin<Box<dyn Stream<Item = ()> + Send>>>,
		network: N,
		sync: S,
		sync_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = SyncEvent> + Send>>>,
		peers: PeersState,
		statement_store: Arc<dyn StatementStore>,
		statements_queue_sender: async_channel::Sender<OnStatementsRequest>,
		results_queue_receiver: async_channel::Receiver<WorkerEvent>,
		pending_state: Arc<RwLock<PendingState>>,
	) -> Self {
		Self {
			protocol_name,
			notification_service,
			propagate_timeout,
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network,
			sync,
			sync_event_stream,
			peers,
			statement_store,
			metrics: None,
			initial_sync_timeout: Box::pin(pending().fuse()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		}
	}

	/// Get the shared pending state for testing/benchmarking.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn pending_state(&self) -> &Arc<RwLock<PendingState>> {
		&self.pending_state
	}

	/// Turns the [`StatementHandler`] into a future that should run forever and not be
	/// interrupted.
	pub async fn run(mut self) {
		loop {
			futures::select_biased! {
				// Poll pending sends first (highest priority for completing in-flight work)
				send_result = self.pending_sends.select_next_some() => {
					self.handle_send_result(send_result);
				},
				_ = self.propagate_timeout.next() => {
					self.propagate_statements();
					if let Some(metrics) = self.metrics.as_ref() {
						if let Ok(state) = self.pending_state.read() {
							metrics.pending_statements.set(state.pending_statements_peers.len() as u64);
						} else {
							log::error!(target: LOG_TARGET, "pending_state lock poisoned");
						}
					}
				},
				event = self.results_queue_receiver.next().fuse() => {
					match event {
						Some(WorkerEvent::StatementResult(StatementProcessResult { hash, result })) => {
							// Update shared state and handle import result.
							let Ok(mut state) = self.pending_state.write() else {
								log::error!(target: LOG_TARGET, "pending_state lock poisoned");
								continue;
							};
							let peers = state.pending_statements_peers.remove(&hash);
							drop(state);
							if let Some(peers) = peers {
								if let Some(result) = result {
									peers.into_iter().for_each(|p| self.on_handle_statement_import(p, &result));
								}
							} else {
								log::warn!(target: LOG_TARGET, "Inconsistent state, no peers for pending statement!");
							}
						},
						Some(WorkerEvent::ReputationChange(ReputationChange { peer, change })) => {
							self.network.report_peer(peer, change);
						},
						None => {
							// Worker channel closed, shutting down.
							log::debug!(target: LOG_TARGET, "Worker event channel closed");
							return;
						},
					}
				},
				sync_event = self.sync_event_stream.next() => {
					if let Some(sync_event) = sync_event {
						self.handle_sync_event(sync_event);
					} else {
						// Syncing has seemingly closed. Closing as well.
						return;
					}
				}
				event = self.notification_service.next_event().fuse() => {
					if let Some(event) = event {
						self.handle_notification_event(event).await
					} else {
						// `Notifications` has seemingly closed. Closing as well.
						return
					}
				}
				_ = &mut self.initial_sync_timeout => {
					self.process_initial_sync_burst().await;
					self.initial_sync_timeout =
						Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse());
				},
			}
		}
	}

	/// Send a single chunk of statements to a peer.
	/// Returns a future that calls `send_async_notification`.
	async fn send_statement_chunk<'a>(
		&'a mut self,
		peer: &'a PeerId,
		statements: &'a [&'a Statement],
	) -> SendChunkResult {
		match find_sendable_chunk(statements) {
			ChunkResult::Send(0) => SendChunkResult::Empty,
			ChunkResult::Send(chunk_end) => {
				let chunk = &statements[..chunk_end];
				if let Err(e) = timeout(
					SEND_TIMEOUT,
					self.notification_service.send_async_notification(peer, chunk.encode()),
				)
				.await
				{
					log::debug!(target: LOG_TARGET, "Failed to send notification to {peer}: {e:?}");
					return SendChunkResult::Failed;
				}
				log::trace!(target: LOG_TARGET, "Sent {} statements to {}", chunk.len(), peer);
				self.metrics.as_ref().map(|metrics| {
					metrics.propagated_statements.inc_by(chunk.len() as u64);
					metrics.propagated_statements_chunks.observe(chunk.len() as f64);
				});
				SendChunkResult::Sent(chunk_end)
			},
			ChunkResult::SkipOversized => {
				log::warn!(target: LOG_TARGET, "Statement too large, skipping");
				self.metrics.as_ref().map(|metrics| {
					metrics.skipped_oversized_statements.inc();
				});
				SendChunkResult::Skipped
			},
		}
	}

	fn handle_sync_event(&mut self, event: SyncEvent) {
		match event {
			SyncEvent::PeerConnected(remote) => {
				let addr = iter::once(multiaddr::Protocol::P2p(remote.into()))
					.collect::<multiaddr::Multiaddr>();
				let result = self.network.add_peers_to_reserved_set(
					self.protocol_name.clone(),
					iter::once(addr).collect(),
				);
				if let Err(err) = result {
					log::error!(target: LOG_TARGET, "Add reserved peer failed: {}", err);
				}
			},
			SyncEvent::PeerDisconnected(remote) => {
				let result = self.network.remove_peers_from_reserved_set(
					self.protocol_name.clone(),
					iter::once(remote).collect(),
				);
				if let Err(err) = result {
					log::error!(target: LOG_TARGET, "Failed to remove reserved peer: {err}");
				}
			},
		}
	}

	async fn handle_notification_event(&mut self, event: NotificationEvent) {
		match event {
			NotificationEvent::ValidateInboundSubstream { peer, handshake, result_tx, .. } => {
				// only accept peers whose role can be determined
				let result = self
					.network
					.peer_role(peer, handshake)
					.map_or(ValidationResult::Reject, |_| ValidationResult::Accept);
				let _ = result_tx.send(result);
			},
			NotificationEvent::NotificationStreamOpened { peer, handshake, .. } => {
				let Some(role) = self.network.peer_role(peer, handshake) else {
					log::debug!(target: LOG_TARGET, "role for {peer} couldn't be determined");
					return
				};
				{
					let Ok(mut guard) = self.peers.write() else {
						log::error!(target: LOG_TARGET, "peers lock poisoned");
						return;
					};
					let _was_in = guard.insert(
						peer,
						Peer {
							known_statements: LruHashSet::new(
								NonZeroUsize::new(MAX_KNOWN_STATEMENTS)
									.expect("Constant is nonzero"),
							),
							role,
						},
					);
					debug_assert!(_was_in.is_none());
				}

				if !self.sync.is_major_syncing() && !role.is_light() {
					let hashes = self.statement_store.statement_hashes();
					if !hashes.is_empty() {
						self.pending_initial_syncs.insert(peer, PendingInitialSync { hashes });
						self.initial_sync_peer_queue.push_back(peer);
					}
				}
			},
			NotificationEvent::NotificationStreamClosed { peer } => {
				let Ok(mut guard) = self.peers.write() else {
					log::error!(target: LOG_TARGET, "peers lock poisoned");
					return;
				};
				{
					let _peer = guard.remove(&peer);
					debug_assert!(_peer.is_some());
				}
				self.pending_initial_syncs.remove(&peer);
				self.initial_sync_peer_queue.retain(|p| *p != peer);
			},
			NotificationEvent::NotificationReceived { peer, notification } => {
				// Accept statements only when node is not major syncing
				if self.sync.is_major_syncing() {
					log::trace!(
						target: LOG_TARGET,
						"{peer}: Ignoring statements while major syncing or offline"
					);
					return
				}

				// Send raw notification to worker for decoding and processing
				if let Err(e) = self.statements_queue_sender.try_send(OnStatementsRequest {
					who: peer,
					notification: notification.to_vec(),
				}) {
					log::debug!(
						target: LOG_TARGET,
						"Failed to send notification to worker: {:?}",
						e
					);
				}
			},
		}
	}

	fn on_handle_statement_import(&mut self, who: PeerId, import: &SubmitResult) {
		match import {
			SubmitResult::New => self.network.report_peer(who, rep::GOOD_STATEMENT),
			SubmitResult::Known => self.network.report_peer(who, rep::ANY_STATEMENT_REFUND),
			SubmitResult::KnownExpired => {},
			SubmitResult::Rejected(_) => {},
			SubmitResult::Invalid(_) => self.network.report_peer(who, rep::INVALID_STATEMENT),
			SubmitResult::InternalError(_) => {},
		}
	}

	/// Propagate one statement.
	/// Queues send futures to `pending_sends`, polled by the main loop.
	pub fn propagate_statement(&mut self, hash: &Hash) {
		// Accept statements only when node is not major syncing
		if self.sync.is_major_syncing() {
			return
		}

		log::debug!(target: LOG_TARGET, "Propagating statement [{:?}]", hash);
		if let Ok(Some(statement)) = self.statement_store.statement(hash) {
			self.do_propagate_statements(&[(*hash, statement)]);
		}
	}

	/// Propagate the given `statements` to the given `peer`.
	///
	/// Internally filters `statements` to only send unknown statements to the peer.
	/// Send futures are queued to `pending_sends` and polled by the main loop.
	fn send_statements_to_peer(&mut self, who: &PeerId, statements: &[(Hash, Statement)]) {
		let Ok(mut peers) = self.peers.write() else {
			log::error!(target: LOG_TARGET, "peers lock poisoned");
			return;
		};
		let Some(peer) = peers.get_mut(who) else {
			return;
		};

		// Never send statements to light nodes
		if peer.role.is_light() {
			log::trace!(target: LOG_TARGET, "{who} is a light node, skipping propagation");
			return
		}

		let to_send: Vec<_> = statements
			.iter()
			.filter_map(|(hash, stmt)| peer.known_statements.insert(*hash).then(|| stmt))
			.collect();
		drop(peers);

		log::trace!(target: LOG_TARGET, "We have {} statements that the peer doesn't know about", to_send.len());

		if to_send.is_empty() {
			return
		}

		self.send_statements_in_chunks(who, &to_send);
	}

	/// Send statements to a peer in chunks, respecting the maximum notification size.
	/// Chunks are queued to `pending_sends` and will be polled by the main loop.
	fn send_statements_in_chunks(&mut self, who: &PeerId, statements: &[&Statement]) {
		// Pre-compute all chunks with their sizes (encoded data, statement count)
		let mut chunks: Vec<(Vec<u8>, usize)> = Vec::new();
		let mut offset = 0;
		while offset < statements.len() {
			match find_sendable_chunk(&statements[offset..]) {
				ChunkResult::Send(0) => break,
				ChunkResult::Send(chunk_end) => {
					let chunk = &statements[offset..offset + chunk_end];
					chunks.push((chunk.encode(), chunk_end));
					offset += chunk_end;
				},
				ChunkResult::SkipOversized => {
					log::warn!(target: LOG_TARGET, "Statement too large, skipping");
					self.metrics.as_ref().map(|metrics| {
						metrics.skipped_oversized_statements.inc();
					});
					offset += 1;
				},
			}
		}

		if chunks.is_empty() {
			return;
		}

		// Queue futures for all chunks to be polled by the main loop
		for (encoded, count) in chunks {
			let Some(message_sink) = self.notification_service.message_sink(who) else {
				log::debug!(target: LOG_TARGET, "Failed to get message sink for peer {who}");
				return;
			};
			let peer = *who;
			self.pending_sends.push(Box::pin(async move {
				let result =
					timeout(SEND_TIMEOUT, message_sink.send_async_notification(encoded)).await;
				PendingSendResult { peer, count, result }
			}));
		}
	}

	/// Handle the result of a completed send operation.
	fn handle_send_result(&mut self, send_result: PendingSendResult) {
		let PendingSendResult { peer, count, result } = send_result;
		match result {
			Ok(Ok(())) => {
				log::trace!(target: LOG_TARGET, "Sent {} statements to {}", count, peer);
				self.metrics.as_ref().map(|metrics| {
					metrics.propagated_statements.inc_by(count as u64);
					metrics.propagated_statements_chunks.observe(count as f64);
				});
			},
			Ok(Err(e)) => {
				log::debug!(target: LOG_TARGET, "Failed to send notification to {peer}: {e:?}");
			},
			Err(_) => {
				log::debug!(target: LOG_TARGET, "Send to {peer} timed out");
			},
		}
	}

	/// Queue statement sends to all peers. Futures are polled by the main loop.
	fn do_propagate_statements(&mut self, statements: &[(Hash, Statement)]) {
		let Ok(guard) = self.peers.read() else {
			log::error!(target: LOG_TARGET, "peers lock poisoned");
			return;
		};
		let peers: Vec<_> = guard.keys().copied().collect();
		drop(guard);
		log::debug!(target: LOG_TARGET, "Propagating {} statements for {} peers", statements.len(), peers.len());
		for who in peers {
			log::trace!(target: LOG_TARGET, "Start propagating statements for {}", who);
			self.send_statements_to_peer(&who, statements);
		}

		log::trace!(target: LOG_TARGET, "Statements queued for propagation to all peers");
	}

	/// Call when we must propagate ready statements
	/// to peers.
	/// Queues send futures to `pending_sends`, polled by the main loop.
	fn propagate_statements(&mut self) {
		// Send out statements only when node is not major syncing
		if self.sync.is_major_syncing() {
			return
		}

		let Ok(statements) = self.statement_store.take_recent_statements() else { return };
		if !statements.is_empty() {
			self.do_propagate_statements(&statements);
		}
	}

	/// Process one batch of initial sync for the next peer in the queue (round-robin).
	async fn process_initial_sync_burst(&mut self) {
		if self.sync.is_major_syncing() {
			return;
		}

		let Some(peer_id) = self.initial_sync_peer_queue.pop_front() else {
			return;
		};

		let Entry::Occupied(mut entry) = self.pending_initial_syncs.entry(peer_id) else {
			return;
		};

		if entry.get().hashes.is_empty() {
			entry.remove();
			return;
		}

		// Fetch statements up to max_statement_payload_size (reserves space for vec encoding)
		let max_size = max_statement_payload_size();
		let mut accumulated_size = 0;
		let (statements, processed) = match self.statement_store.statements_by_hashes(
			&entry.get().hashes,
			&mut |_hash, encoded, _stmt| {
				if accumulated_size > 0 && accumulated_size + encoded.len() > max_size {
					return FilterDecision::Abort
				}
				accumulated_size += encoded.len();
				FilterDecision::Take
			},
		) {
			Ok(r) => r,
			Err(e) => {
				log::debug!(target: LOG_TARGET, "Failed to fetch statements for initial sync: {e:?}");
				entry.remove();
				return;
			},
		};

		// Drain processed hashes and check if more remain
		entry.get_mut().hashes.drain(..processed);
		let has_more = !entry.get().hashes.is_empty();
		drop(entry);

		// Send statements (already sized to fit in one message)
		let to_send: Vec<_> = statements.iter().map(|(_, stmt)| stmt).collect();
		match self.send_statement_chunk(&peer_id, &to_send).await {
			SendChunkResult::Failed => {
				self.pending_initial_syncs.remove(&peer_id);
				return;
			},
			SendChunkResult::Sent(sent) => {
				debug_assert_eq!(to_send.len(), sent);
				// Mark statements as known
				if let Ok(mut guard) = self.peers.write() {
					if let Some(peer) = guard.get_mut(&peer_id) {
						for (hash, _) in &statements {
							peer.known_statements.insert(*hash);
						}
					}
				} else {
					log::error!(target: LOG_TARGET, "peers lock poisoned");
				}
			},
			SendChunkResult::Empty | SendChunkResult::Skipped => {},
		}

		// Re-queue if more hashes remain
		if has_more {
			self.initial_sync_peer_queue.push_back(peer_id);
		} else {
			self.pending_initial_syncs.remove(&peer_id);
		}
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use std::sync::Mutex;

	#[derive(Clone)]
	struct TestNetwork {
		reported_peers: Arc<Mutex<Vec<(PeerId, sc_network::ReputationChange)>>>,
		peer_roles: Arc<Mutex<HashMap<PeerId, ObservedRole>>>,
	}

	impl TestNetwork {
		fn new() -> Self {
			Self {
				reported_peers: Arc::new(Mutex::new(Vec::new())),
				peer_roles: Arc::new(Mutex::new(HashMap::new())),
			}
		}

		fn get_reports(&self) -> Vec<(PeerId, sc_network::ReputationChange)> {
			self.reported_peers.lock().unwrap().clone()
		}

		fn set_peer_role(&self, peer: PeerId, role: ObservedRole) {
			self.peer_roles.lock().unwrap().insert(peer, role);
		}
	}

	#[async_trait::async_trait]
	impl NetworkPeers for TestNetwork {
		fn set_authorized_peers(&self, _: std::collections::HashSet<PeerId>) {
			unimplemented!()
		}

		fn set_authorized_only(&self, _: bool) {
			unimplemented!()
		}

		fn add_known_address(&self, _: PeerId, _: sc_network::Multiaddr) {
			unimplemented!()
		}

		fn report_peer(&self, peer_id: PeerId, cost_benefit: sc_network::ReputationChange) {
			self.reported_peers.lock().unwrap().push((peer_id, cost_benefit));
		}

		fn peer_reputation(&self, _: &PeerId) -> i32 {
			unimplemented!()
		}

		fn disconnect_peer(&self, _: PeerId, _: sc_network::ProtocolName) {
			unimplemented!()
		}

		fn accept_unreserved_peers(&self) {
			unimplemented!()
		}

		fn deny_unreserved_peers(&self) {
			unimplemented!()
		}

		fn add_reserved_peer(
			&self,
			_: sc_network::config::MultiaddrWithPeerId,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn remove_reserved_peer(&self, _: PeerId) {
			unimplemented!()
		}

		fn set_reserved_peers(
			&self,
			_: sc_network::ProtocolName,
			_: std::collections::HashSet<sc_network::Multiaddr>,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn add_peers_to_reserved_set(
			&self,
			_: sc_network::ProtocolName,
			_: std::collections::HashSet<sc_network::Multiaddr>,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn remove_peers_from_reserved_set(
			&self,
			_: sc_network::ProtocolName,
			_: Vec<PeerId>,
		) -> Result<(), String> {
			unimplemented!()
		}

		fn sync_num_connected(&self) -> usize {
			unimplemented!()
		}

		fn peer_role(&self, peer: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
			self.peer_roles.lock().unwrap().get(&peer).copied()
		}

		async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
			unimplemented!();
		}
	}

	struct TestSync {}

	impl SyncEventStream for TestSync {
		fn event_stream(
			&self,
			_name: &'static str,
		) -> Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>> {
			unimplemented!()
		}
	}

	impl sp_consensus::SyncOracle for TestSync {
		fn is_major_syncing(&self) -> bool {
			false
		}

		fn is_offline(&self) -> bool {
			unimplemented!()
		}
	}

	impl NetworkEventStream for TestNetwork {
		fn event_stream(
			&self,
			_name: &'static str,
		) -> Pin<Box<dyn Stream<Item = sc_network::Event> + Send>> {
			unimplemented!()
		}
	}

	/// A test message sink for sending notifications to a specific peer.
	struct TestMessageSink {
		peer: PeerId,
		sent_notifications: Arc<Mutex<Vec<(PeerId, Vec<u8>)>>>,
		notification_sender: Option<tokio::sync::mpsc::Sender<(PeerId, Vec<u8>)>>,
	}

	#[async_trait::async_trait]
	impl sc_network::service::traits::MessageSink for TestMessageSink {
		fn send_sync_notification(&self, notification: Vec<u8>) {
			self.sent_notifications.lock().unwrap().push((self.peer, notification.clone()));
			if let Some(ref sender) = self.notification_sender {
				let _ = sender.try_send((self.peer, notification));
			}
		}

		async fn send_async_notification(
			&self,
			notification: Vec<u8>,
		) -> Result<(), sc_network::error::Error> {
			self.sent_notifications.lock().unwrap().push((self.peer, notification.clone()));
			if let Some(ref sender) = self.notification_sender {
				sender
					.send((self.peer, notification))
					.await
					.map_err(|_| sc_network::error::Error::ChannelClosed)?;
			}
			Ok(())
		}
	}

	/// A unified test notification service that supports:
	/// - Synchronous notification collection via `get_sent_notifications()`
	/// - Optional event injection via `next_event()`
	/// - Optional bounded channel for backpressure simulation
	#[derive(Debug)]
	struct TestNotificationService {
		/// Synchronous storage for sent notifications (always available)
		sent_notifications: Arc<Mutex<Vec<(PeerId, Vec<u8>)>>>,
		/// Optional receiver for injected events (peer connect/disconnect, incoming statements).
		/// Uses tokio::sync::Mutex to allow holding the lock across await points.
		/// None for simple tests and clones; Some(...) for tests with event injection.
		event_receiver: Option<tokio::sync::mpsc::UnboundedReceiver<NotificationEvent>>,
		/// Optional bounded channel sender for async notifications (simulates backpressure)
		notification_sender: Option<tokio::sync::mpsc::Sender<(PeerId, Vec<u8>)>>,
	}

	impl Clone for TestNotificationService {
		fn clone(&self) -> Self {
			Self {
				sent_notifications: self.sent_notifications.clone(),
				event_receiver: None, // Clones don't receive events
				notification_sender: self.notification_sender.clone(),
			}
		}
	}

	impl TestNotificationService {
		/// Create a simple test notification service without event injection.
		fn new() -> Self {
			Self {
				sent_notifications: Arc::new(Mutex::new(Vec::new())),
				event_receiver: None,
				notification_sender: None,
			}
		}

		/// Create a test notification service with event injection and bounded channel
		/// for backpressure simulation.
		fn with_event_injection(
			channel_capacity: usize,
		) -> (
			Self,
			tokio::sync::mpsc::UnboundedSender<NotificationEvent>,
			tokio::sync::mpsc::Receiver<(PeerId, Vec<u8>)>,
		) {
			let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
			let (notification_sender, notification_receiver) =
				tokio::sync::mpsc::channel(channel_capacity);
			(
				Self {
					sent_notifications: Arc::new(Mutex::new(Vec::new())),
					event_receiver: Some(event_receiver),
					notification_sender: Some(notification_sender),
				},
				event_sender,
				notification_receiver,
			)
		}

		fn get_sent_notifications(&self) -> Vec<(PeerId, Vec<u8>)> {
			self.sent_notifications.lock().unwrap().clone()
		}
	}

	#[async_trait::async_trait]
	impl NotificationService for TestNotificationService {
		async fn open_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
			unimplemented!()
		}

		async fn close_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
			unimplemented!()
		}

		fn send_sync_notification(&mut self, peer: &PeerId, notification: Vec<u8>) {
			self.sent_notifications.lock().unwrap().push((*peer, notification.clone()));
			// Also send to channel if available (ignore errors for sync send)
			if let Some(ref sender) = self.notification_sender {
				let _ = sender.try_send((*peer, notification));
			}
		}

		async fn send_async_notification(
			&mut self,
			peer: &PeerId,
			notification: Vec<u8>,
		) -> Result<(), sc_network::error::Error> {
			self.sent_notifications.lock().unwrap().push((*peer, notification.clone()));
			// Also send to channel if available
			if let Some(ref sender) = self.notification_sender {
				sender
					.send((*peer, notification))
					.await
					.map_err(|_| sc_network::error::Error::ChannelClosed)?;
			}
			Ok(())
		}

		async fn set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		fn try_set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		async fn next_event(&mut self) -> Option<NotificationEvent> {
			match &mut self.event_receiver {
				Some(receiver) => receiver.recv().await,
				None => None,
			}
		}

		fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
			unimplemented!()
		}

		fn protocol(&self) -> &sc_network::types::ProtocolName {
			unimplemented!()
		}

		fn message_sink(
			&self,
			peer: &PeerId,
		) -> Option<Box<dyn sc_network::service::traits::MessageSink>> {
			Some(Box::new(TestMessageSink {
				peer: *peer,
				sent_notifications: self.sent_notifications.clone(),
				notification_sender: self.notification_sender.clone(),
			}))
		}
	}

	#[derive(Clone)]
	struct TestStatementStore {
		statements: Arc<Mutex<HashMap<sp_statement_store::Hash, sp_statement_store::Statement>>>,
		recent_statements:
			Arc<Mutex<HashMap<sp_statement_store::Hash, sp_statement_store::Statement>>>,
	}

	impl TestStatementStore {
		fn new() -> Self {
			Self { statements: Default::default(), recent_statements: Default::default() }
		}
	}

	impl StatementStore for TestStatementStore {
		fn statements(
			&self,
		) -> sp_statement_store::Result<
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
		> {
			Ok(self.statements.lock().unwrap().iter().map(|(h, s)| (*h, s.clone())).collect())
		}

		fn take_recent_statements(
			&self,
		) -> sp_statement_store::Result<
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
		> {
			Ok(self.recent_statements.lock().unwrap().drain().collect())
		}

		fn statement(
			&self,
			_hash: &sp_statement_store::Hash,
		) -> sp_statement_store::Result<Option<sp_statement_store::Statement>> {
			unimplemented!()
		}

		fn has_statement(&self, hash: &sp_statement_store::Hash) -> bool {
			self.statements.lock().unwrap().contains_key(hash)
		}

		fn statement_hashes(&self) -> Vec<sp_statement_store::Hash> {
			self.statements.lock().unwrap().keys().cloned().collect()
		}

		fn statements_by_hashes(
			&self,
			hashes: &[sp_statement_store::Hash],
			filter: &mut dyn FnMut(
				&sp_statement_store::Hash,
				&[u8],
				&sp_statement_store::Statement,
			) -> FilterDecision,
		) -> sp_statement_store::Result<(
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
			usize,
		)> {
			let statements = self.statements.lock().unwrap();
			let mut result = Vec::new();
			let mut processed = 0;
			for hash in hashes {
				let Some(stmt) = statements.get(hash) else {
					processed += 1;
					continue
				};
				let encoded = stmt.encode();
				match filter(hash, &encoded, stmt) {
					FilterDecision::Skip => {
						processed += 1;
					},
					FilterDecision::Take => {
						processed += 1;
						result.push((*hash, stmt.clone()));
					},
					FilterDecision::Abort => break,
				}
			}
			Ok((result, processed))
		}

		fn broadcasts(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_clear(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn broadcasts_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn posted_clear_stmt(
			&self,
			_match_all_topics: &[sp_statement_store::Topic],
			_dest: [u8; 32],
		) -> sp_statement_store::Result<Vec<Vec<u8>>> {
			unimplemented!()
		}

		fn submit(
			&self,
			statement: sp_statement_store::Statement,
			_source: sp_statement_store::StatementSource,
		) -> sp_statement_store::SubmitResult {
			let hash = statement.hash();
			let mut statements = self.statements.lock().unwrap();
			if statements.contains_key(&hash) {
				SubmitResult::Known
			} else {
				statements.insert(hash, statement);
				SubmitResult::New
			}
		}

		fn remove(&self, _hash: &sp_statement_store::Hash) -> sp_statement_store::Result<()> {
			unimplemented!()
		}

		fn remove_by(&self, _who: [u8; 32]) -> sp_statement_store::Result<()> {
			unimplemented!()
		}
	}

	fn build_handler() -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
		async_channel::Receiver<OnStatementsRequest>,
		async_channel::Sender<WorkerEvent>,
	) {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, statements_queue_receiver) = async_channel::bounded(100);
		let (results_queue_sender, results_queue_receiver) = async_channel::bounded(100);
		let pending_state = Arc::new(RwLock::new(PendingState::new()));
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let peer_id = PeerId::random();
		let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));
		peers.write().unwrap().insert(
			peer_id,
			Peer {
				known_statements: LruHashSet::new(NonZeroUsize::new(100).unwrap()),
				role: ObservedRole::Full,
			},
		);

		let handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync {},
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};
		(
			handler,
			statement_store,
			network,
			notification_service,
			statements_queue_receiver,
			results_queue_sender,
		)
	}

	#[tokio::test]
	async fn test_notification_is_sent_to_worker() {
		let (
			mut handler,
			_statement_store,
			_network,
			_notification_service,
			statements_queue_receiver,
			_results_queue_sender,
		) = build_handler();

		let mut statement1 = Statement::new();
		statement1.set_plain_data(b"statement1".to_vec());

		let mut statement2 = Statement::new();
		statement2.set_plain_data(b"statement2".to_vec());

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();
		let notification = vec![statement1, statement2].encode();

		// Simulate receiving a notification
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: notification.clone().into(),
			})
			.await;

		// The notification should be sent to the worker
		let request = statements_queue_receiver.try_recv();
		let request = request.expect("Expected a request to be sent to worker");
		assert_eq!(request.who, peer_id);
		assert_eq!(request.notification, notification);

		let no_more = statements_queue_receiver.try_recv();
		assert!(no_more.is_err(), "Expected only one request to be queued");
	}

	#[tokio::test]
	async fn test_worker_event_reputation_change_is_applied() {
		let (
			handler,
			_statement_store,
			network,
			_notification_service,
			_statements_queue_receiver,
			results_queue_sender,
		) = build_handler();

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		// Simulate worker sending a reputation change event
		results_queue_sender
			.send(WorkerEvent::ReputationChange(ReputationChange {
				peer: peer_id,
				change: rep::ANY_STATEMENT,
			}))
			.await
			.unwrap();

		// Process the event in the handler
		let event = handler.results_queue_receiver.try_recv().unwrap();
		match event {
			WorkerEvent::ReputationChange(ReputationChange { peer, change }) => {
				handler.network.report_peer(peer, change);
			},
			_ => panic!("Expected ReputationChange event"),
		}

		let reports = network.get_reports();
		assert_eq!(
			reports,
			vec![(peer_id, rep::ANY_STATEMENT)],
			"Expected ANY_STATEMENT reputation change"
		);
	}

	#[tokio::test]
	async fn test_shared_peers_mark_known_directly() {
		let (
			handler,
			_statement_store,
			_network,
			_notification_service,
			_statements_queue_receiver,
			_results_queue_sender,
		) = build_handler();

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();
		let hash = [1u8; 32];

		// Simulate worker marking statement as known directly via shared peers
		{
			let mut peers = handler.peers.write().unwrap();
			if let Some(peer) = peers.get_mut(&peer_id) {
				peer.known_statements.insert(hash);
			}
		}

		// Verify the hash is now known (insert returns false if already present)
		assert!(
			!handler
				.peers
				.write()
				.unwrap()
				.get_mut(&peer_id)
				.unwrap()
				.known_statements
				.insert(hash),
			"Hash should be known after direct update (insert returns false for existing)"
		);
	}

	#[tokio::test]
	async fn test_splits_large_batches_into_smaller_chunks() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_statements_queue_receiver,
			_results_queue_sender,
		) = build_handler();

		let num_statements = 30;
		let statement_size = 100 * 1024; // 100KB per statement
		for i in 0..num_statements {
			let mut statement = Statement::new();
			let mut data = vec![0u8; statement_size];
			data[0] = i as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		handler.propagate_statements();

		// Drive pending sends to completion
		while let Some(result) = handler.pending_sends.next().await {
			handler.handle_send_result(result);
		}

		let sent = notification_service.get_sent_notifications();
		let mut total_statements_sent = 0;
		assert!(
			sent.len() == 3,
			"Expected batch to be split into 3 chunks, but got {} chunks",
			sent.len()
		);
		for (_peer, notification) in sent.iter() {
			assert!(
				notification.len() <= MAX_STATEMENT_NOTIFICATION_SIZE as usize,
				"Notification size {} exceeds limit {}",
				notification.len(),
				MAX_STATEMENT_NOTIFICATION_SIZE
			);
			if let Ok(stmts) = <Statements as Decode>::decode(&mut notification.as_slice()) {
				total_statements_sent += stmts.len();
			}
		}

		assert_eq!(
			total_statements_sent, num_statements,
			"Expected all {} statements to be sent, but only {} were sent",
			num_statements, total_statements_sent
		);
	}

	#[tokio::test]
	async fn test_skips_only_oversized_statements() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_statements_queue_receiver,
			_results_queue_sender,
		) = build_handler();

		let mut statement1 = Statement::new();
		statement1.set_plain_data(vec![1u8; 100]);
		let hash1 = statement1.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash1, statement1.clone());

		let mut oversized1 = Statement::new();
		oversized1.set_plain_data(vec![2u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize * 100]);
		let hash_oversized1 = oversized1.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_oversized1, oversized1);

		let mut statement2 = Statement::new();
		statement2.set_plain_data(vec![3u8; 100]);
		let hash2 = statement2.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash2, statement2.clone());

		let mut oversized2 = Statement::new();
		oversized2.set_plain_data(vec![4u8; MAX_STATEMENT_NOTIFICATION_SIZE as usize]);
		let hash_oversized2 = oversized2.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_oversized2, oversized2);

		let mut statement3 = Statement::new();
		statement3.set_plain_data(vec![5u8; 100]);
		let hash3 = statement3.hash();
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash3, statement3.clone());

		handler.propagate_statements();

		// Drive pending sends to completion
		while let Some(result) = handler.pending_sends.next().await {
			handler.handle_send_result(result);
		}

		let sent = notification_service.get_sent_notifications();

		let mut sent_hashes = sent
			.iter()
			.flat_map(|(_peer, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect::<Vec<_>>();
		sent_hashes.sort();
		let mut expected_hashes = vec![hash1, hash2, hash3];
		expected_hashes.sort();
		assert_eq!(sent_hashes, expected_hashes, "Only small statements should be sent");
	}

	fn build_handler_no_peers() -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
	) {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, _statements_queue_receiver) = async_channel::bounded(100);
		let (_results_queue_sender, results_queue_receiver) = async_channel::bounded(100);
		let pending_state = Arc::new(RwLock::new(PendingState::new()));
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();

		let handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync {},
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers: Arc::new(RwLock::new(HashMap::new())),
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};
		(handler, statement_store, network, notification_service)
	}

	/// Build a handler configured for testing the run loop with event injection
	/// and bounded notification channels.
	fn build_handler_for_run_loop(
		num_peers: usize,
		channel_capacity: usize,
	) -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		tokio::sync::mpsc::UnboundedSender<NotificationEvent>,
		tokio::sync::mpsc::Receiver<(PeerId, Vec<u8>)>,
		async_channel::Sender<WorkerEvent>,
		async_channel::Receiver<OnStatementsRequest>,
		Vec<PeerId>,
	) {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, statements_queue_receiver) = async_channel::bounded(1000);
		let (results_queue_sender, results_queue_receiver) = async_channel::bounded(1000);
		let network = TestNetwork::new();
		let (notification_service, event_sender, notification_receiver) =
			TestNotificationService::with_event_injection(channel_capacity);

		// Create peers
		let mut peers_map = HashMap::new();
		let mut peer_ids = Vec::new();
		for _ in 0..num_peers {
			let peer_id = PeerId::random();
			peers_map.insert(
				peer_id,
				Peer {
					known_statements: LruHashSet::new(NonZeroUsize::new(10000).unwrap()),
					role: ObservedRole::Full,
				},
			);
			network.set_peer_role(peer_id, ObservedRole::Full);
			peer_ids.push(peer_id);
		}
		let peers: PeersState = Arc::new(RwLock::new(peers_map));

		// Use tokio interval stream that works with tokio's paused time for testing
		let propagate_timeout = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
			std::time::Duration::from_secs(1),
		))
		.map(|_| ());

		let handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service),
			propagate_timeout: (Box::pin(propagate_timeout)
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state: Arc::new(RwLock::new(PendingState::new())),
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync {},
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			initial_sync_timeout: Box::pin(
				tokio::time::sleep(std::time::Duration::from_millis(100)).fuse(),
			),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};
		(
			handler,
			statement_store,
			network,
			event_sender,
			notification_receiver,
			results_queue_sender,
			statements_queue_receiver,
			peer_ids,
		)
	}

	#[tokio::test]
	async fn test_initial_sync_burst_single_peer() {
		let (mut handler, statement_store, network, notification_service) =
			build_handler_no_peers();

		// Create 20MB of statements (200 statements x 100KB each)
		// Using 100KB ensures ~10 statements per 1MB batch, requiring ~20 bursts
		let num_statements = 200;
		let statement_size = 100 * 1024; // 100KB per statement
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut statement = Statement::new();
			let mut data = vec![0u8; statement_size];
			// Use multiple bytes for uniqueness since we have >255 statements
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.statements.lock().unwrap().insert(hash, statement);
		}

		// Setup peer and simulate connection
		let peer_id = PeerId::random();
		network.set_peer_role(peer_id, ObservedRole::Full);

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Verify peer was added and initial sync was queued
		assert!(handler.peers.read().unwrap().contains_key(&peer_id));
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(handler.initial_sync_peer_queue.len(), 1);

		// Process bursts until all statements are sent
		let mut burst_count = 0;
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst().await;
			burst_count += 1;
			// Safety limit
			assert!(burst_count <= 300, "Too many bursts, possible infinite loop");
		}

		// Verify multiple bursts were needed
		// With 200 statements x 100KB each and ~1MB per batch, we expect many bursts
		assert!(
			burst_count >= 10,
			"Expected multiple bursts for 200 statements of 100KB each, got {}",
			burst_count
		);

		// Verify all statements were sent
		let sent = notification_service.get_sent_notifications();
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(peer, notification)| {
				assert_eq!(*peer, peer_id);
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();
		expected_hashes.sort();

		assert_eq!(
			sent_hashes.len(),
			expected_hashes.len(),
			"Expected {} statements to be sent, got {}",
			expected_hashes.len(),
			sent_hashes.len()
		);
		assert_eq!(sent_hashes, expected_hashes, "All statements should be sent");

		// Verify cleanup
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn test_initial_sync_burst_multiple_peers_round_robin() {
		let (mut handler, statement_store, network, notification_service) =
			build_handler_no_peers();

		// Create 20MB of statements (200 statements x 100KB each)
		let num_statements = 200;
		let statement_size = 100 * 1024; // 100KB per statement
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut statement = Statement::new();
			let mut data = vec![0u8; statement_size];
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			statement.set_plain_data(data);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.statements.lock().unwrap().insert(hash, statement);
		}

		// Setup 3 peers and simulate connections
		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let peer3 = PeerId::random();
		network.set_peer_role(peer1, ObservedRole::Full);
		network.set_peer_role(peer2, ObservedRole::Full);
		network.set_peer_role(peer3, ObservedRole::Full);

		// Connect peers
		for peer in [peer1, peer2, peer3] {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamOpened {
					peer,
					direction: sc_network::service::traits::Direction::Inbound,
					handshake: vec![],
					negotiated_fallback: None,
				})
				.await;
		}

		// Verify all peers were added and initial syncs were queued
		assert_eq!(handler.peers.read().unwrap().len(), 3);
		assert_eq!(handler.pending_initial_syncs.len(), 3);
		assert_eq!(handler.initial_sync_peer_queue.len(), 3);

		// Track which peer was processed on each burst for round-robin verification
		let mut peer_burst_order = Vec::new();
		let mut burst_count = 0;

		while !handler.pending_initial_syncs.is_empty() {
			// Record which peer will be processed next
			if let Some(&next_peer) = handler.initial_sync_peer_queue.front() {
				peer_burst_order.push(next_peer);
			}
			handler.process_initial_sync_burst().await;
			burst_count += 1;
			// Safety limit
			assert!(burst_count <= 500, "Too many bursts, possible infinite loop");
		}

		// Verify multiple bursts were needed
		// With 3 peers and many bursts per peer, we expect many bursts total
		assert!(
			burst_count >= 30,
			"Expected many bursts for 3 peers with 200 statements each, got {}",
			burst_count
		);

		// Verify round-robin pattern in first 9 bursts (3 peers x 3 rounds)
		assert!(peer_burst_order.len() >= 9, "Expected at least 9 bursts");
		// First round
		assert_eq!(peer_burst_order[0], peer1, "First burst should be peer1");
		assert_eq!(peer_burst_order[1], peer2, "Second burst should be peer2");
		assert_eq!(peer_burst_order[2], peer3, "Third burst should be peer3");
		// Second round
		assert_eq!(peer_burst_order[3], peer1, "Fourth burst should be peer1");
		assert_eq!(peer_burst_order[4], peer2, "Fifth burst should be peer2");
		assert_eq!(peer_burst_order[5], peer3, "Sixth burst should be peer3");

		// Verify all peers received all statements
		let sent = notification_service.get_sent_notifications();
		let mut peer1_hashes: Vec<_> = sent
			.iter()
			.filter(|(peer, _)| *peer == peer1)
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		let mut peer2_hashes: Vec<_> = sent
			.iter()
			.filter(|(peer, _)| *peer == peer2)
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		let mut peer3_hashes: Vec<_> = sent
			.iter()
			.filter(|(peer, _)| *peer == peer3)
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();

		peer1_hashes.sort();
		peer2_hashes.sort();
		peer3_hashes.sort();
		expected_hashes.sort();

		assert_eq!(peer1_hashes, expected_hashes, "Peer1 should receive all statements");
		assert_eq!(peer2_hashes, expected_hashes, "Peer2 should receive all statements");
		assert_eq!(peer3_hashes, expected_hashes, "Peer3 should receive all statements");

		// Verify cleanup
		assert!(handler.pending_initial_syncs.is_empty());
		assert!(handler.initial_sync_peer_queue.is_empty());
	}

	#[tokio::test]
	async fn test_send_statements_in_chunks_exact_max_size() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_statements_queue_receiver,
			_results_queue_sender,
		) = build_handler();

		// Calculate the data sizes so that 100 statements together exactly fill max_size.
		// This tests that all 100 statements fit in a single notification.
		//
		// The limit check in find_sendable_chunk is:
		//   max_size = MAX_STATEMENT_NOTIFICATION_SIZE - Compact::<u32>::max_encoded_len()
		//
		// Statement encoding (encodes as Vec<Field>):
		// - Compact<u32> for number of fields (1 byte for value 1)
		// - Field::Data discriminant (1 byte, value 8)
		// - Compact<u32> for the data length (2 bytes for small data)
		// So per-statement overhead = 1 + 1 + 2 = 4 bytes
		let max_size = MAX_STATEMENT_NOTIFICATION_SIZE as usize - Compact::<u32>::max_encoded_len();
		let num_statements: usize = 100;
		let per_statement_overhead = 1 + 1 + 2; // Vec<Field> length + discriminant + Compact data length
		let total_overhead = per_statement_overhead * num_statements;
		let total_data_size = max_size - total_overhead;
		let per_statement_data_size = total_data_size / num_statements;
		let remainder = total_data_size % num_statements;

		let mut expected_hashes = Vec::with_capacity(num_statements);
		let mut total_encoded_size = 0;

		for i in 0..num_statements {
			let mut statement = Statement::new();
			// Distribute remainder across first `remainder` statements to exactly fill max_size
			let extra = if i < remainder { 1 } else { 0 };
			let mut data = vec![42u8; per_statement_data_size + extra];
			// Make each statement unique by modifying the first few bytes
			data[0] = i as u8;
			data[1] = (i >> 8) as u8;
			statement.set_plain_data(data);

			total_encoded_size += statement.encoded_size();

			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		// Verify our calculation: total encoded size should be <= max_size
		assert!(
			total_encoded_size == max_size,
			"Total encoded size {} should be <= max_size {}",
			total_encoded_size,
			max_size
		);

		handler.propagate_statements();

		// Drive pending sends to completion
		while let Some(result) = handler.pending_sends.next().await {
			handler.handle_send_result(result);
		}

		let sent = notification_service.get_sent_notifications();

		// All statements should fit in a single chunk
		assert_eq!(
			sent.len(),
			1,
			"Expected 1 notification for all {} statements, but got {}",
			num_statements,
			sent.len()
		);

		let (_peer, notification) = &sent[0];
		assert!(
			notification.len() <= MAX_STATEMENT_NOTIFICATION_SIZE as usize,
			"Notification size {} exceeds limit {}",
			notification.len(),
			MAX_STATEMENT_NOTIFICATION_SIZE
		);

		let decoded = <Statements as Decode>::decode(&mut notification.as_slice()).unwrap();
		assert_eq!(
			decoded.len(),
			num_statements,
			"Expected {} statements in the notification",
			num_statements
		);

		// Verify all statements were sent (order may differ due to HashMap iteration)
		let mut received_hashes: Vec<_> = decoded.iter().map(|s| s.hash()).collect();
		expected_hashes.sort();
		received_hashes.sort();
		assert_eq!(expected_hashes, received_hashes, "All statement hashes should match");
	}

	#[tokio::test]
	async fn test_initial_sync_burst_size_limit_consistency() {
		// This test verifies that process_initial_sync_burst and find_sendable_chunk
		// use the same size limit (max_statement_payload_size).
		//
		// Previously there was a bug where the filter in process_initial_sync_burst used
		// MAX_STATEMENT_NOTIFICATION_SIZE, but find_sendable_chunk reserved extra space
		// for Compact::<u32>::max_encoded_len(). This caused a debug_assert failure when
		// statements fit the filter but not find_sendable_chunk.
		//
		// With the fix, both use max_statement_payload_size(), so the filter will reject
		// statements that wouldn't fit in find_sendable_chunk.
		let (mut handler, statement_store, network, notification_service) =
			build_handler_no_peers();

		let payload_limit = max_statement_payload_size();

		// Create first statement that's just over half the payload limit
		let first_stmt_data_size = payload_limit / 2 + 10;
		let mut stmt1 = Statement::new();
		stmt1.set_plain_data(vec![1u8; first_stmt_data_size]);
		let stmt1_encoded_size = stmt1.encoded_size();

		// Create second statement that, combined with the first, exceeds the payload limit.
		// This means the filter will only accept the first statement.
		let remaining = payload_limit.saturating_sub(stmt1_encoded_size);
		let target_stmt2_encoded = remaining + 3; // 3 bytes over limit when combined
		let stmt2_data_size = target_stmt2_encoded.saturating_sub(4); // ~4 bytes encoding overhead
		let mut stmt2 = Statement::new();
		stmt2.set_plain_data(vec![2u8; stmt2_data_size]);
		let stmt2_encoded_size = stmt2.encoded_size();

		let total_encoded = stmt1_encoded_size + stmt2_encoded_size;

		// Verify our setup: total exceeds payload limit
		assert!(
			total_encoded > payload_limit,
			"Total {} should exceed payload_limit {} so filter rejects second statement",
			total_encoded,
			payload_limit
		);

		let hash1 = stmt1.hash();
		let hash2 = stmt2.hash();
		statement_store.statements.lock().unwrap().insert(hash1, stmt1);
		statement_store.statements.lock().unwrap().insert(hash2, stmt2);

		// Setup peer and simulate connection
		let peer_id = PeerId::random();
		network.set_peer_role(peer_id, ObservedRole::Full);

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Verify initial sync was queued with both hashes
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(handler.pending_initial_syncs.get(&peer_id).unwrap().hashes.len(), 2);

		// Process first burst - should send only one statement (the other doesn't fit)
		handler.process_initial_sync_burst().await;

		// With the fix, the filter and find_sendable_chunk use the same limit,
		// so no assertion failure occurs. Only one statement is fetched and sent.
		let sent = notification_service.get_sent_notifications();
		assert_eq!(sent.len(), 1, "First burst should send one notification");

		let decoded = <Statements as Decode>::decode(&mut sent[0].1.as_slice()).unwrap();
		assert_eq!(decoded.len(), 1, "First notification should contain one statement");

		// Verify one of the two statements was sent (order is non-deterministic due to HashMap)
		let sent_hash = decoded[0].hash();
		assert!(
			sent_hash == hash1 || sent_hash == hash2,
			"Sent statement should be one of the two created"
		);

		// Second statement should still be pending
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(handler.pending_initial_syncs.get(&peer_id).unwrap().hashes.len(), 1);

		// Process second burst - should send the remaining statement
		handler.process_initial_sync_burst().await;

		let sent = notification_service.get_sent_notifications();
		assert_eq!(sent.len(), 2, "Second burst should send another notification");

		// Both statements should now be sent
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();
		let mut expected_hashes = vec![hash1, hash2];
		expected_hashes.sort();
		assert_eq!(sent_hashes, expected_hashes, "Both statements should be sent");

		// No more pending
		assert!(!handler.pending_initial_syncs.contains_key(&peer_id));
	}

	#[tokio::test(start_paused = true)]
	async fn test_run_loop_propagates_recent_statements_to_peers() {
		// Setup: 3 peers, 600 statements of ~2KB each (~1.2MB total, exceeds 1MB limit)
		let (
			handler,
			statement_store,
			_network,
			_event_sender,
			mut notification_receiver,
			_results_queue_sender,
			_statements_queue_receiver,
			peer_ids,
		) = build_handler_for_run_loop(3, 100);

		assert_eq!(peer_ids.len(), 3);

		// Add 600 statements to recent_statements (~2KB each = ~1.2MB total)
		let num_statements = 600;
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut stmt = Statement::new();
			let mut data = vec![0u8; 2048]; // ~2KB each
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			stmt.set_plain_data(data);
			let hash = stmt.hash();
			expected_hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, stmt);
		}

		// Spawn run loop
		let handle = tokio::spawn(handler.run());

		// Advance time to trigger propagate_timeout (fires at 1 second intervals)
		tokio::time::advance(std::time::Duration::from_secs(2)).await;
		tokio::task::yield_now().await;

		// Collect sent notifications (time-boxed)
		let mut received: Vec<(PeerId, Vec<u8>)> = Vec::new();
		let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
		while tokio::time::Instant::now() < deadline {
			tokio::task::yield_now().await;
			match tokio::time::timeout(
				std::time::Duration::from_millis(100),
				notification_receiver.recv(),
			)
			.await
			{
				Ok(Some(notif)) => received.push(notif),
				Ok(None) => break, // Channel closed
				Err(_) => break,   // Timeout
			}
		}

		// Abort the run loop
		handle.abort();

		// Verify: Decode all notifications and collect statement hashes per peer
		let mut hashes_per_peer: HashMap<PeerId, HashSet<Hash>> = HashMap::new();
		for (peer, notification) in &received {
			if let Ok(statements) = <Statements as Decode>::decode(&mut notification.as_slice()) {
				hashes_per_peer
					.entry(*peer)
					.or_insert_with(HashSet::new)
					.extend(statements.iter().map(|s| s.hash()));
			}
		}

		// Each peer should have received all 600 statements
		for peer_id in &peer_ids {
			let peer_hashes =
				hashes_per_peer.get(peer_id).expect("Peer should have received notifications");
			assert_eq!(
				peer_hashes.len(),
				num_statements,
				"Peer {:?} should have received all {} statements, got {}",
				peer_id,
				num_statements,
				peer_hashes.len()
			);
		}

		// Verify chunking occurred (should have multiple notifications per peer due to 1MB limit)
		let notifications_per_peer: HashMap<PeerId, usize> =
			received.iter().fold(HashMap::new(), |mut acc, (peer, _)| {
				*acc.entry(*peer).or_insert(0) += 1;
				acc
			});

		for peer_id in &peer_ids {
			let count = notifications_per_peer.get(peer_id).unwrap_or(&0);
			assert!(
				*count > 1,
				"Peer {:?} should have received multiple chunks due to 1MB limit, got {}",
				peer_id,
				count
			);
		}
	}

	#[tokio::test(start_paused = true)]
	async fn test_run_loop_validates_statements_and_updates_reputation() {
		let (
			handler,
			statement_store,
			network,
			event_sender,
			_notification_receiver,
			results_queue_sender,
			statements_queue_receiver,
			peer_ids,
		) = build_handler_for_run_loop(1, 100);

		let peer = peer_ids[0];

		// Create 10 statements (~2KB each)
		let num_statements = 10;
		let statements: Vec<Statement> = (0..num_statements)
			.map(|i| {
				let mut stmt = Statement::new();
				let mut data = vec![0u8; 2048];
				data[0] = i as u8;
				stmt.set_plain_data(data);
				stmt
			})
			.collect();

		// Get pending_state from handler to share with mock worker
		let pending_state = handler.pending_state.clone();

		// Spawn a mock worker that emulates the real on_statements_worker behavior
		let worker_store = statement_store.clone();
		let worker_handle = tokio::spawn(async move {
			loop {
				match statements_queue_receiver.recv().await {
					Ok(request) => {
						let who = request.who;
						// Decode the notification to get statements
						if let Ok(stmts) =
							<Statements as Decode>::decode(&mut request.notification.as_slice())
						{
							let mut aggregated_reputation: i32 = 0;
							for stmt in stmts {
								let hash = stmt.hash();

								// Add ANY_STATEMENT penalty for each new statement
								aggregated_reputation =
									aggregated_reputation.saturating_add(rep::ANY_STATEMENT.value);

								// Update pending_state to track this peer for this statement
								{
									let mut state = pending_state.write().unwrap();
									state
										.pending_statements_peers
										.entry(hash)
										.or_insert_with(HashSet::new)
										.insert(who);
								}

								let result = worker_store.submit(stmt, StatementSource::Network);
								let event = WorkerEvent::StatementResult(StatementProcessResult {
									hash,
									result: Some(result),
								});
								if results_queue_sender.send(event).await.is_err() {
									return;
								}
							}

							// Send aggregated reputation change
							if aggregated_reputation != 0 {
								let event = WorkerEvent::ReputationChange(ReputationChange {
									peer: who,
									change: sc_network::ReputationChange::new(
										aggregated_reputation,
										"Any statement",
									),
								});
								if results_queue_sender.send(event).await.is_err() {
									return;
								}
							}
						}
					},
					Err(_) => return,
				}
			}
		});

		// Spawn run loop
		let handle = tokio::spawn(handler.run());

		// Yield to let run loop and worker start
		tokio::task::yield_now().await;

		// Inject NotificationReceived event with encoded statements
		event_sender
			.send(NotificationEvent::NotificationReceived {
				peer,
				notification: statements.encode().into(),
			})
			.unwrap();

		// Allow time for statements to be processed through the validation pipeline:
		// 1. Run loop receives NotificationReceived event
		// 2. Statements are queued for validation
		// 3. Worker processes queue and submits to store
		// 4. Store returns SubmitResult::New
		// 5. Run loop receives completion and updates reputation
		tokio::time::advance(std::time::Duration::from_millis(200)).await;
		tokio::task::yield_now().await;

		// Abort both the run loop and worker
		handle.abort();
		worker_handle.abort();

		// Verify reputation changes
		let reports = network.get_reports();

		// The worker sends an aggregated ANY_STATEMENT report (all statements combined)
		// and individual GOOD_STATEMENT reports for each new statement.
		// Total reputation impact: num_statements * ANY_STATEMENT + num_statements * GOOD_STATEMENT

		// Check aggregated ANY_STATEMENT report (aggregated value = num_statements * -16)
		let expected_any_statement_total = num_statements as i32 * rep::ANY_STATEMENT.value;
		let any_statement_report = reports
			.iter()
			.find(|(p, rep)| *p == peer && rep.value == expected_any_statement_total);
		assert!(
			any_statement_report.is_some(),
			"Expected aggregated ANY_STATEMENT report with value {}, reports: {:?}",
			expected_any_statement_total,
			reports
		);

		// Check individual GOOD_STATEMENT reports
		let good_statement_count = reports
			.iter()
			.filter(|(p, rep)| *p == peer && rep.value == rep::GOOD_STATEMENT.value)
			.count();
		assert_eq!(
			good_statement_count, num_statements,
			"Expected {} GOOD_STATEMENT reports, got {}",
			num_statements, good_statement_count
		);

		// Verify statements were actually stored
		let stored_count = statement_store.statements.lock().unwrap().len();
		assert_eq!(
			stored_count, num_statements,
			"Expected {} statements in store, got {}",
			num_statements, stored_count
		);
	}

	#[tokio::test(start_paused = true)]
	async fn test_run_loop_sends_statements_to_new_peer() {
		// Start with no peers, but pre-populate statement store
		let (
			handler,
			statement_store,
			network,
			event_sender,
			mut notification_receiver,
			_results_queue_sender,
			_statements_queue_receiver,
			_peer_ids,
		) = build_handler_for_run_loop(0, 100);

		// Add 600 statements to store (main statements for initial sync)
		let num_statements = 600;
		let mut expected_hashes = Vec::new();
		for i in 0..num_statements {
			let mut stmt = Statement::new();
			let mut data = vec![0u8; 2048]; // ~2KB each
			data[0] = (i % 256) as u8;
			data[1] = (i / 256) as u8;
			stmt.set_plain_data(data);
			let hash = stmt.hash();
			expected_hashes.push(hash);
			statement_store.statements.lock().unwrap().insert(hash, stmt);
		}

		// Spawn run loop
		let handle = tokio::spawn(handler.run());

		// Yield to let run loop start
		tokio::task::yield_now().await;

		// Inject peer connection event
		let new_peer = PeerId::random();
		network.set_peer_role(new_peer, ObservedRole::Full);
		event_sender
			.send(NotificationEvent::NotificationStreamOpened {
				peer: new_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.unwrap();

		// Advance time to trigger initial_sync_timeout (100ms intervals)
		// With 600 statements of ~2KB each, we need multiple bursts
		// Allow enough time for all bursts to complete
		for _ in 0..100 {
			tokio::time::advance(std::time::Duration::from_millis(100)).await;
			tokio::task::yield_now().await;
		}

		// Collect sent notifications
		let mut received: Vec<(PeerId, Vec<u8>)> = Vec::new();
		loop {
			match notification_receiver.try_recv() {
				Ok(notif) => received.push(notif),
				Err(_) => break,
			}
		}

		// Abort the run loop
		handle.abort();

		// Verify: Decode all notifications and collect statement hashes for the new peer
		let mut received_hashes: HashSet<Hash> = HashSet::new();
		for (peer, notification) in &received {
			if *peer == new_peer {
				if let Ok(statements) = <Statements as Decode>::decode(&mut notification.as_slice())
				{
					received_hashes.extend(statements.iter().map(|s| s.hash()));
				}
			}
		}

		// New peer should have received all 600 statements
		assert_eq!(
			received_hashes.len(),
			num_statements,
			"New peer should have received all {} statements, got {}",
			num_statements,
			received_hashes.len()
		);

		// Verify chunking occurred (multiple notifications due to 1MB limit)
		let notifications_for_new_peer = received.iter().filter(|(p, _)| *p == new_peer).count();
		assert!(
			notifications_for_new_peer > 1,
			"New peer should have received multiple chunks due to 1MB limit, got {}",
			notifications_for_new_peer
		);
	}
}

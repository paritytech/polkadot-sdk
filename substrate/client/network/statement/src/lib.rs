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

mod affinity;

use crate::config::*;

use affinity::AffinityFilter;
use codec::{Compact, Decode, Encode, MaxEncodedLen};
#[cfg(any(test, feature = "test-helpers"))]
use futures::future::pending;
use futures::{channel::oneshot, future::FusedFuture, prelude::*, stream::FuturesUnordered};
use governor::{
	clock::DefaultClock,
	state::{InMemoryState, NotKeyed},
	Quota, RateLimiter,
};
use prometheus_endpoint::{
	exponential_buckets, register, Counter, Gauge, Histogram, HistogramOpts, PrometheusError,
	Registry, U64,
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
	NetworkBackend, NetworkEventStream, NetworkPeers,
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
	num::{NonZeroU32, NonZeroUsize},
	pin::Pin,
	sync::{Arc, RwLock},
	time::Instant,
};
use tokio::time::timeout;
pub mod config;

/// A set of statements.
pub type Statements = Vec<Statement>;

/// The protocol version that was negotiated with a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerProtocolVersion {
	/// V1: messages are encoded as `Vec<Statement>` (the legacy format).
	V1,
	/// V2: messages are encoded as `StatementMessage` enum (supports topic affinity).
	V2,
}

impl PeerProtocolVersion {
	/// Returns the encoding envelope overhead for this protocol version.
	fn envelope_overhead(&self) -> usize {
		match self {
			PeerProtocolVersion::V1 => V1_ENVELOPE_OVERHEAD,
			PeerProtocolVersion::V2 => V2_ENVELOPE_OVERHEAD,
		}
	}
}

#[derive(Debug, Encode, Decode)]
enum StatementMessage {
	#[codec(index = 0)]
	Statements(Vec<Statement>),
	/// Bloom filter bytes representing the topics this peer is interested in.
	#[codec(index = 1)]
	ExplicitTopicAffinity(AffinityFilter),
}

/// Codec variant index for `StatementMessage::Statements`, kept in sync with `#[codec(index)]`.
const STATEMENTS_VARIANT_INDEX: u8 = 0;

impl StatementMessage {
	/// Encode a slice of statement references as a `StatementMessage::Statements`
	/// without cloning the statements.
	fn encode_statement_refs(statements: &[&Statement]) -> Vec<u8> {
		let mut out = Vec::new();
		STATEMENTS_VARIANT_INDEX.encode_to(&mut out);
		statements.encode_to(&mut out);
		out
	}
}

/// Future resolving to statement import result.
pub type StatementImportFuture = oneshot::Receiver<SubmitResult>;

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
	/// Reputation change when a peer floods us with statements.
	pub const STATEMENT_FLOODING: Rep = Rep::new_fatal("Statement flooding");
	/// Reputation change when a peer sends us a message we can't decode.
	pub const BAD_MESSAGE: Rep = Rep::new(-(1 << 12), "Bad statement message");
}

const LOG_TARGET: &str = "statement-gossip";
/// V2 statement protocol suffix, work in progress protocol with topic affinity and other
/// improvements, may have breaking changes before stabilization.
const STATEMENT_PROTOCOL_V2: &str = "statement/2";
/// V1 statement protocol suffix, current stable protocol, no breaking changes will be made to it.
const STATEMENT_PROTOCOL_V1: &str = "statement/1";
/// Maximum time we wait for sending a notification to a peer.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Interval for sending statement batches during initial sync to new peers.
const INITIAL_SYNC_BURST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
/// Maximum number of statements to propagate per cycle remaining statements are buffered.
const MAX_STATEMENTS_PER_PROPAGATION: usize = 256;
/// Maximum number of in-flight send futures before skipping propagation.
const MAX_PENDING_SENDS: usize = 4096;
/// Interval for processing pending topic affinity changes from peers.
const PENDING_AFFINITIES_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

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
	peers_connected: Gauge<U64>,
	statements_received: Counter<U64>,
	bytes_sent_total: Counter<U64>,
	bytes_received_total: Counter<U64>,
	sent_latency_seconds: Histogram,
	initial_sync_statements_sent: Counter<U64>,
	initial_sync_bursts_total: Counter<U64>,
	initial_sync_peers_active: Gauge<U64>,
	initial_sync_duration_seconds: Histogram,
	statement_flooding_detected: Counter<U64>,
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
					)
					.buckets(exponential_buckets(1.0, 2.0, 14)?),
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
			peers_connected: register(
				Gauge::new(
					"substrate_sync_statement_peers_connected",
					"Number of peers connected using the statement protocol",
				)?,
				r,
			)?,
			statements_received: register(
				Counter::new(
					"substrate_sync_statements_received",
					"Total number of statements received from peers",
				)?,
				r,
			)?,
			bytes_sent_total: register(
				Counter::new(
					"substrate_sync_statement_bytes_sent_total",
					"Total bytes sent for statement protocol messages",
				)?,
				r,
			)?,
			bytes_received_total: register(
				Counter::new(
					"substrate_sync_statement_bytes_received_total",
					"Total bytes received for statement protocol messages",
				)?,
				r,
			)?,
			sent_latency_seconds: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_sync_statement_sent_latency_seconds",
						"Time to send statement messages to peers",
					)
					// Buckets from 1μs to ~1s covering microsecond to millisecond range.
					.buckets(vec![0.000_001, 0.000_01, 0.000_1, 0.001, 0.01, 0.1, 1.0]),
				)?,
				r,
			)?,
			initial_sync_statements_sent: register(
				Counter::new(
					"substrate_sync_initial_sync_statements_sent",
					"Total statements sent during initial sync bursts to newly connected peers",
				)?,
				r,
			)?,
			initial_sync_bursts_total: register(
				Counter::new(
					"substrate_sync_initial_sync_bursts_total",
					"Total number of initial sync burst rounds processed",
				)?,
				r,
			)?,
			initial_sync_peers_active: register(
				Gauge::new(
					"substrate_sync_initial_sync_peers_active",
					"Number of peers currently being synced via initial sync",
				)?,
				r,
			)?,
			initial_sync_duration_seconds: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_sync_initial_sync_duration_seconds",
						"Per-peer total duration of initial sync from start to completion",
					)
					.buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]),
				)?,
				r,
			)?,
			statement_flooding_detected: register(
				Counter::new(
					"substrate_sync_statement_flooding_detected",
					"Number of peers disconnected for exceeding statement rate limits",
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
		let hex = array_bytes::bytes2hex("", genesis_hash);
		let (protocol_name, fallback_name) = if let Some(fork_id) = fork_id {
			(
				format!("/{hex}/{fork_id}/{STATEMENT_PROTOCOL_V2}"),
				format!("/{hex}/{fork_id}/{STATEMENT_PROTOCOL_V1}"),
			)
		} else {
			(format!("/{hex}/{STATEMENT_PROTOCOL_V2}"), format!("/{hex}/{STATEMENT_PROTOCOL_V1}"))
		};
		let (config, notification_service) = Net::notification_config(
			protocol_name.clone().into(),
			vec![fallback_name.into()],
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
		statements_per_second: u32,
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

		let statements_per_second = match NonZeroU32::new(statements_per_second) {
			Some(rate) => rate,
			None => {
				log::warn!(
					target: LOG_TARGET,
					"statements_per_second is 0, defaulting to {}",
					DEFAULT_STATEMENTS_PER_SECOND
				);
				NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
					.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero")
			},
		};

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
			statements_per_second,
			metrics,
			initial_sync_timeout: Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse()),
			pending_affinities_timeout: Box::pin(
				tokio::time::sleep(PENDING_AFFINITIES_INTERVAL).fuse(),
			),
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
	#[cfg_attr(any(test, feature = "test-helpers"), allow(dead_code))]
	pub(crate) async fn process_on_statements(
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
			let _ = event_sender
				.send(WorkerEvent::ReputationChange(ReputationChange {
					peer: who,
					change: rep::INVALID_STATEMENT,
				}))
				.await;
			return;
		};

		log::trace!(target: LOG_TARGET, "Worker processing {} statements from {}", statements.len(), who);

		let is_flooding = {
			let Ok(guard) = shared_peers.read() else {
				log::error!(target: LOG_TARGET, "shared_peers lock poisoned");
				return;
			};
			guard
				.get(&who)
				.map_or(false, |peer| peer.rate_limiter.is_flooding(statements.len()))
		};
		if is_flooding {
			log::warn!(
				target: LOG_TARGET,
				"Peer {} exceeded statement rate limit ({} statements in batch). Disconnecting.",
				who,
				statements.len(),
			);
			if let Some(metrics) = metrics {
				metrics.statement_flooding_detected.inc();
			}
			let _ = event_sender.send(WorkerEvent::PeerFlooding { peer: who }).await;
			return;
		}

		// Track aggregated reputation change for this peer.
		let mut aggregated_reputation: i32 = 0;

		let statements_with_hashes: Vec<(Hash, Statement)> = statements
			.into_iter()
			.map(|s| {
				let h = s.hash();
				(h, s)
			})
			.collect();

		// Filter by known_statements
		// We can't punish peers for sending us statements we already know from them,
		// because there might be a race between us sending them the statement and them
		// sending it to us. So we just skip duplicates from the same peer.
		let after_known_filter: Vec<(Hash, Statement)> = {
			let Ok(mut guard) = shared_peers.write() else {
				log::error!(target: LOG_TARGET, "shared_peers lock poisoned");
				return;
			};
			let mut result = Vec::with_capacity(statements_with_hashes.len());
			if let Some(peer) = guard.get_mut(&who) {
				for (hash, stmt) in statements_with_hashes {
					if peer.known_statements.insert(hash) {
						result.push((hash, stmt));
					}
				}
			} else {
				// Peer not in map (disconnected?), process all statements.
				result = statements_with_hashes;
			}
			result
		};

		let after_pending_filter: Vec<(Hash, Statement)> = {
			let Ok(state) = shared_state.read() else {
				log::error!(target: LOG_TARGET, "shared_state lock poisoned");
				return;
			};
			after_known_filter
				.into_iter()
				.filter(|(hash, _)| {
					if state
						.pending_statements_peers
						.get(hash)
						.map(|peers| peers.contains(&who))
						.unwrap_or(false)
					{
						log::trace!(
							target: LOG_TARGET,
							"Worker: Already received the statement from the same peer {who} while pending.",
						);
						aggregated_reputation =
							aggregated_reputation.saturating_add(rep::DUPLICATE_STATEMENT.value);
						false
					} else {
						true
					}
				})
				.collect()
		};

		// Filter by statement store if we already have this statement (from another peer)
		let to_submit: Vec<(Hash, Statement)> = after_pending_filter
			.into_iter()
			.filter(|(hash, _)| {
				if statement_store.has_statement(hash) {
					log::trace!(
						target: LOG_TARGET,
						"Worker: Already have statement in store (from another peer), skipping.",
					);
					if let Some(metrics) = metrics {
						metrics.known_statements_received.inc();
					}
					false
				} else {
					aggregated_reputation =
						aggregated_reputation.saturating_add(rep::ANY_STATEMENT.value);
					true
				}
			})
			.collect();

		// Submit to validation queue.
		if !to_submit.is_empty() {
			let Ok(mut state) = shared_state.write() else {
				log::error!(target: LOG_TARGET, "shared_state lock poisoned");
				return;
			};

			for (hash, s) in to_submit {
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
					Entry::Occupied(mut entry) => {
						//  We might have raced with another worker thread adding the same peer.
						if !entry.get_mut().insert(who) {
							aggregated_reputation = aggregated_reputation
								.saturating_add(rep::DUPLICATE_STATEMENT.value);
						}
					},
				}
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
	/// Maximum statements per second per peer.
	statements_per_second: NonZeroU32,
	/// Prometheus metrics (shared with worker thread).
	metrics: Option<Arc<Metrics>>,
	/// Timeout for sending next statement batch during initial sync.
	initial_sync_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Timeout for processing pending topic affinity changes.
	pending_affinities_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Pending initial syncs per peer.
	pending_initial_syncs: HashMap<PeerId, PendingInitialSync>,
	/// Queue for round-robin processing of initial syncs.
	initial_sync_peer_queue: VecDeque<PeerId>,
	/// Pending statement send operations, polled by the main loop.
	pending_sends: PendingSends,
}

/// Per-peer rate limiter using a token bucket algorithm.
///
/// The token bucket allows short bursts up to the per-second limit while enforcing
/// the average rate over time.
#[derive(Debug)]
struct PeerRateLimiter {
	limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl PeerRateLimiter {
	fn new(statements_per_second: NonZeroU32, burst: NonZeroU32) -> Self {
		let quota = Quota::per_second(statements_per_second).allow_burst(burst);
		Self { limiter: RateLimiter::direct(quota) }
	}

	/// Check if receiving `count` statements would exceed the rate limit.
	fn is_flooding(&self, count: usize) -> bool {
		if count > u32::MAX as usize {
			return true;
		}

		let Some(n) = NonZeroU32::new(count as u32) else {
			return false;
		};
		!matches!(self.limiter.check_n(n), Ok(Ok(())))
	}
}

/// Peer information
#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
#[derive(Debug)]
pub struct Peer {
	/// Holds a set of statements known to this peer.
	known_statements: LruHashSet<Hash>,
	/// Rate limiter for statement flooding protection.
	rate_limiter: PeerRateLimiter,
	/// Protocol version negotiated with this peer.
	protocol_version: PeerProtocolVersion,
	/// Topic affinity filter received from a v2 peer.
	/// When set, only statements matching this filter should be propagated to the peer.
	topic_affinity: Option<AffinityFilter>,
	/// Whether this peer is a light client.
	/// Light clients on V2 must set topic affinity before receiving statements.
	is_light: bool,
	/// A pending topic affinity filter waiting to be scheduled for initial sync.
	/// Set when a new `ExplicitTopicAffinity` arrives; consumed by the main loop
	/// once any in-progress initial sync for this peer completes.
	pending_topic_affinity: Option<AffinityFilter>,
}

/// Tracks pending initial sync state for a peer (hashes only, statements fetched on-demand).
struct PendingInitialSync {
	hashes: Vec<Hash>,
	started_at: Instant,
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
	/// A peer exceeded the statement rate limit and should be disconnected.
	PeerFlooding { peer: PeerId },
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

/// Encoding overhead for V1: just the `Compact<u32>` vec length prefix (max 5 bytes).
const V1_ENVELOPE_OVERHEAD: usize = 5;

/// Encoding overhead for V2: 1 byte enum discriminant + `Compact<u32>` vec length prefix.
const V2_ENVELOPE_OVERHEAD: usize = 1 + V1_ENVELOPE_OVERHEAD;

/// Returns the maximum payload size for statement notifications given the
/// protocol envelope overhead.
fn max_statement_payload_size(envelope_overhead: usize) -> usize {
	debug_assert_eq!(
		V1_ENVELOPE_OVERHEAD,
		Compact::<u32>::max_encoded_len(),
		"V1_ENVELOPE_OVERHEAD must equal Compact::<u32>::max_encoded_len()"
	);
	MAX_STATEMENT_NOTIFICATION_SIZE as usize - envelope_overhead
}

/// Find the largest chunk of statements starting from the beginning that fits
/// within MAX_STATEMENT_NOTIFICATION_SIZE minus the given `envelope_overhead`.
///
/// Uses an incremental approach: adds statements one by one until the limit is reached.
/// This is efficient because we only compute sizes for statements we'll actually send
/// in this chunk, rather than computing sizes for all statements upfront.
fn find_sendable_chunk(statements: &[&Statement], envelope_overhead: usize) -> ChunkResult {
	if statements.is_empty() {
		return ChunkResult::Send(0);
	}
	let max_size = max_statement_payload_size(envelope_overhead);

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
	pub fn new_for_testing(
		known_statements: LruHashSet<Hash>,
		statements_per_second: NonZeroU32,
		burst: NonZeroU32,
	) -> Self {
		Self {
			known_statements,
			rate_limiter: PeerRateLimiter::new(statements_per_second, burst),
			protocol_version: PeerProtocolVersion::V1,
			topic_affinity: None,
			is_light: false,
			pending_topic_affinity: None,
		}
	}

	/// Whether this peer is ready to receive statements.
	///
	/// Light V2 peers must set their topic affinity before receiving any statements.
	fn can_receive(&self) -> bool {
		!(self.is_light &&
			self.protocol_version == PeerProtocolVersion::V2 &&
			self.topic_affinity.is_none())
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
		statements_per_second: NonZeroU32,
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
			statements_per_second,
			metrics: None,
			initial_sync_timeout: Box::pin(pending().fuse()),
			pending_affinities_timeout: Box::pin(pending().fuse()),
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

	#[cfg(test)]
	fn on_statements(&mut self, who: PeerId, statements: Vec<Statement>) {
		let is_flooding = {
			let Ok(guard) = self.peers.read() else {
				return;
			};
			guard
				.get(&who)
				.map_or(false, |peer| peer.rate_limiter.is_flooding(statements.len()))
		};
		if is_flooding {
			self.network.report_peer(who, rep::STATEMENT_FLOODING);
			self.network.disconnect_peer(who, self.protocol_name.clone());
			{
				let Ok(mut guard) = self.peers.write() else {
					return;
				};
				guard.remove(&who);
			}
			self.pending_initial_syncs.remove(&who);
			self.initial_sync_peer_queue.retain(|p| *p != who);
		}
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
						Some(WorkerEvent::PeerFlooding { peer }) => {
							self.network.report_peer(peer, rep::STATEMENT_FLOODING);
							self.network.disconnect_peer(peer, self.protocol_name.clone());
							{
								let Ok(mut guard) = self.peers.write() else {
									log::error!(target: LOG_TARGET, "peers lock poisoned");
									continue;
								};
								guard.remove(&peer);
							}
							self.pending_initial_syncs.remove(&peer);
							self.initial_sync_peer_queue.retain(|p| *p != peer);
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
				_ = &mut self.pending_affinities_timeout => {
					self.process_pending_affinities();
					self.pending_affinities_timeout =
						Box::pin(tokio::time::sleep(PENDING_AFFINITIES_INTERVAL).fuse());
				},
			}
		}
	}

	/// Send a single chunk of statements to a peer.
	///
	/// Encodes the chunk according to the peer's protocol version:
	/// - V1: raw `Vec<Statement>` encoding
	/// - V2: `StatementMessage::Statements(...)` encoding
	async fn send_statement_chunk(
		&mut self,
		peer: &PeerId,
		statements: &[&Statement],
	) -> SendChunkResult {
		let peer_version = {
			let Ok(guard) = self.peers.read() else {
				log::error!(target: LOG_TARGET, "peers lock poisoned");
				return SendChunkResult::Failed;
			};
			let Some(peer_data) = guard.get(peer) else {
				log::error!(target: LOG_TARGET, "Peer {peer} not found in peers map during send_statement_chunk");
				return SendChunkResult::Failed;
			};
			peer_data.protocol_version
		};
		let envelope_overhead = peer_version.envelope_overhead();
		match find_sendable_chunk(statements, envelope_overhead) {
			ChunkResult::Send(0) => SendChunkResult::Empty,
			ChunkResult::Send(chunk_end) => {
				let chunk = &statements[..chunk_end];
				let encoded = match peer_version {
					PeerProtocolVersion::V1 => chunk.encode(),
					PeerProtocolVersion::V2 => StatementMessage::encode_statement_refs(chunk),
				};
				let bytes_to_send = encoded.len() as u64;

				let sent_latency_timer =
					self.metrics.as_ref().map(|m| m.sent_latency_seconds.start_timer());
				let send_result = timeout(
					SEND_TIMEOUT,
					self.notification_service.send_async_notification(peer, encoded),
				)
				.await;
				drop(sent_latency_timer);

				if let Err(e) = send_result {
					log::debug!(target: LOG_TARGET, "Failed to send notification to {peer}: {e:?}");
					return SendChunkResult::Failed;
				}

				log::trace!(target: LOG_TARGET, "Sent {} statements to {}", chunk.len(), peer);
				self.metrics.as_ref().map(|metrics| {
					metrics.propagated_statements.inc_by(chunk.len() as u64);
					metrics.bytes_sent_total.inc_by(bytes_to_send);
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
				// Only accept peers whose role can be determined
				let result = self
					.network
					.peer_role(peer, handshake)
					.map_or(ValidationResult::Reject, |_| ValidationResult::Accept);
				let _ = result_tx.send(result);
			},
			NotificationEvent::NotificationStreamOpened {
				peer,
				negotiated_fallback,
				handshake,
				..
			} => {
				// If negotiated_fallback is Some, the peer connected on a fallback protocol
				// (v1). If None, the peer connected on the main protocol (v2).
				let protocol_version = if negotiated_fallback.is_some() {
					PeerProtocolVersion::V1
				} else {
					PeerProtocolVersion::V2
				};
				let Some(peer_role) = self.network.peer_role(peer, handshake) else {
					log::debug!(
						target: LOG_TARGET,
						"Peer {peer} connected but role could not be determined, ignoring"
					);
					return;
				};
				let is_light = peer_role.is_light();
				log::debug!(
					target: LOG_TARGET,
					"Peer {peer} connected with statement protocol {protocol_version:?}, role={peer_role:?}"
				);
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
							rate_limiter: PeerRateLimiter::new(
								self.statements_per_second,
								NonZeroU32::new(
									self.statements_per_second.get() *
										config::STATEMENTS_BURST_COEFFICIENT,
								)
								.expect("burst capacity is nonzero"),
							),
							protocol_version,
							topic_affinity: None,
							is_light,
							pending_topic_affinity: None,
						},
					);
					debug_assert!(_was_in.is_none());
					self.metrics.as_ref().map(|metrics| {
						metrics.peers_connected.set(guard.len() as u64);
					});
				}

				// Light V2 peers must set topic affinity before receiving statements.
				// All other peers get initial sync immediately.
				if self.peers.read().ok().as_deref().and_then(|g| g.get(&peer)).map_or(false, |p| p.can_receive()) {
					self.schedule_initial_sync_for_peer(peer);
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
				if let Some(pending) = self.pending_initial_syncs.remove(&peer) {
					self.metrics.as_ref().map(|metrics| {
						metrics.initial_sync_peers_active.dec();
						metrics
							.initial_sync_duration_seconds
							.observe(pending.started_at.elapsed().as_secs_f64());
					});
				}
				self.initial_sync_peer_queue.retain(|p| *p != peer);
				self.metrics.as_ref().map(|metrics| {
					metrics.peers_connected.set(guard.len() as u64);
				});
			},
			NotificationEvent::NotificationReceived { peer, notification } => {
				let bytes_received = notification.len() as u64;
				self.metrics.as_ref().map(|metrics| {
					metrics.bytes_received_total.inc_by(bytes_received);
					metrics.statements_received.inc();
				});

				// Accept statements only when node is not major syncing
				if self.sync.is_major_syncing() {
					log::trace!(
						target: LOG_TARGET,
						"{peer}: Ignoring statements while major syncing or offline"
					);
					return;
				}

				// Get peer's protocol version to dispatch correctly
				let protocol_version = self
					.peers
					.read()
					.ok()
					.as_deref()
					.and_then(|g| g.get(&peer).map(|p| p.protocol_version));

				let Some(protocol_version) = protocol_version else {
					log::error!(
						target: LOG_TARGET,
						"Received notification from unknown peer {peer}"
					);
					return;
				};

				match protocol_version {
					PeerProtocolVersion::V1 => {
						// V1: forward raw bytes directly to worker
						if let Err(e) = self.statements_queue_sender.try_send(
							OnStatementsRequest { who: peer, notification: notification.to_vec() },
						) {
							log::debug!(
								target: LOG_TARGET,
								"Failed to send notification to worker: {:?}",
								e
							);
						}
					},
					PeerProtocolVersion::V2 => {
						// V2: decode StatementMessage, handle ExplicitTopicAffinity inline,
						// re-encode Statements as V1 for the worker
						match StatementMessage::decode(&mut notification.as_ref()) {
							Ok(StatementMessage::Statements(statements)) => {
								let v1_encoded = statements.encode();
								if let Err(e) = self.statements_queue_sender.try_send(
									OnStatementsRequest { who: peer, notification: v1_encoded },
								) {
									log::debug!(
										target: LOG_TARGET,
										"Failed to send notification to worker: {:?}",
										e
									);
								}
							},
							Ok(StatementMessage::ExplicitTopicAffinity(filter)) => {
								let Ok(mut guard) = self.peers.write() else {
									log::error!(target: LOG_TARGET, "peers lock poisoned");
									return;
								};
								if let Some(peer_data) = guard.get_mut(&peer) {
									if peer_data.rate_limiter.is_flooding(1) {
										log::debug!(
											target: LOG_TARGET,
											"Rate-limiting ExplicitTopicAffinity from {peer}"
										);
										self.network.report_peer(peer, rep::BAD_MESSAGE);
									} else {
										log::debug!(
											target: LOG_TARGET,
											"Received topic affinity filter from {peer}"
										);
										peer_data.pending_topic_affinity = Some(filter);
									}
								}
							},
							Err(_) => {
								log::debug!(
									target: LOG_TARGET,
									"Failed to decode v2 statement message from {peer}"
								);
								self.network.report_peer(peer, rep::BAD_MESSAGE);
							},
						}
					},
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
			return;
		}

		log::debug!(target: LOG_TARGET, "Propagating statement [{:?}]", hash);
		if let Ok(Some(statement)) = self.statement_store.statement(hash) {
			self.do_propagate_statements(&[(*hash, statement)]);
		}
	}

	/// Propagate the given `statements` to the given `peer`.
	///
	/// Internally filters `statements` to only send unknown statements to the peer.
	/// For v2 peers with a topic affinity filter, also filters by topic match.
	/// Send futures are queued to `pending_sends` and polled by the main loop.
	fn send_statements_to_peer(&mut self, who: &PeerId, statements: &[(Hash, Statement)]) {
		let Ok(mut peers) = self.peers.write() else {
			log::error!(target: LOG_TARGET, "peers lock poisoned");
			return;
		};
		let Some(peer) = peers.get_mut(who) else {
			return;
		};

		if !peer.can_receive() {
			return;
		}

		let protocol_version = peer.protocol_version;
		let to_send: Vec<_> = statements
			.iter()
			.filter_map(|(hash, stmt)| {
				if peer.known_statements.contains(hash) {
					return None;
				}
				// For v2 peers with topic affinity, filter by topic match.
				// Don't mark filtered statements as known so they can be retried
				// when the peer's affinity changes.
				if peer.topic_affinity.as_ref().is_some_and(|a| !a.matches_statement(stmt)) {
					return None;
				}
				peer.known_statements.insert(*hash);
				Some(stmt)
			})
			.collect();
		drop(peers);

		log::trace!(target: LOG_TARGET, "We have {} statements that the peer doesn't know about", to_send.len());

		if to_send.is_empty() {
			return;
		}

		self.send_statements_in_chunks(who, &to_send, protocol_version);
	}

	/// Send statements to a peer in chunks, respecting the maximum notification size.
	/// Chunks are queued to `pending_sends` and will be polled by the main loop.
	fn send_statements_in_chunks(
		&mut self,
		who: &PeerId,
		statements: &[&Statement],
		protocol_version: PeerProtocolVersion,
	) {
		let envelope_overhead = protocol_version.envelope_overhead();
		// Pre-compute all chunks with their sizes (encoded data, statement count)
		let mut chunks: Vec<(Vec<u8>, usize)> = Vec::new();
		let mut offset = 0;
		while offset < statements.len() {
			match find_sendable_chunk(&statements[offset..], envelope_overhead) {
				ChunkResult::Send(0) => break,
				ChunkResult::Send(chunk_end) => {
					let chunk = &statements[offset..offset + chunk_end];
					let encoded = match protocol_version {
						PeerProtocolVersion::V1 => chunk.encode(),
						PeerProtocolVersion::V2 => StatementMessage::encode_statement_refs(chunk),
					};
					chunks.push((encoded, chunk_end));
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

	/// Drive all queued `pending_sends` to completion.
	///
	/// Unit tests call this after `propagate_statements()` or similar so that
	/// the futures pushed onto `pending_sends` are actually polled without
	/// needing the main run loop.
	#[cfg(any(test, feature = "test-helpers"))]
	pub async fn flush_pending_sends(&mut self) {
		use futures::StreamExt;
		while let Some(result) = self.pending_sends.next().await {
			self.handle_send_result(result);
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
			return;
		}

		if self.pending_sends.len() > MAX_PENDING_SENDS {
			log::debug!(
				target: LOG_TARGET,
				"Skipping propagation, {} sends still pending",
				self.pending_sends.len()
			);
			return;
		}

		let statements =
			match self.statement_store.take_recent_statements(MAX_STATEMENTS_PER_PROPAGATION) {
				Ok(s) if !s.is_empty() => s,
				_ => return,
			};

		self.do_propagate_statements(&statements);
	}

	/// Schedule an initial sync for a peer, sending all known statements.
	///
	/// This is called both when a new peer connects and when a peer's topic
	/// affinity changes (so that newly-matching statements get sent).
	/// If the peer already has a pending initial sync, it is replaced.
	fn schedule_initial_sync_for_peer(&mut self, peer: PeerId) {
		// If there's already a pending sync, clean it up first.
		if let Some(pending) = self.pending_initial_syncs.remove(&peer) {
			self.record_initial_sync_completion(pending.started_at);
			self.initial_sync_peer_queue.retain(|p| *p != peer);
		}
		let hashes = self.statement_store.statement_hashes();
		// Clear known statements so that all statements are redelivered when
		// explicit affinity changes, this is necessary because light nodes change
		// their affinity without disconnecting, and we want them to receive all matching
		// statements, so they can deliver them to their active subscriptions.
		if let Ok(mut guard) = self.peers.write() {
			if let Some(peer_data) = guard.get_mut(&peer) {
				peer_data.known_statements.clear();
			}
		}
		if !hashes.is_empty() {
			self.pending_initial_syncs
				.insert(peer, PendingInitialSync { hashes, started_at: Instant::now() });
			self.initial_sync_peer_queue.push_back(peer);
			self.metrics.as_ref().map(|metrics| {
				metrics.initial_sync_peers_active.inc();
			});
		}
	}

	/// Process pending topic affinity changes for peers that have no active initial sync.
	///
	/// When a peer sends `ExplicitTopicAffinity`, we defer the expensive
	/// `schedule_initial_sync_for_peer` call. This method applies the pending affinity
	/// and schedules the sync once the peer's current sync (if any) has completed.
	fn process_pending_affinities(&mut self) {
		let ready_peers: Vec<PeerId> = {
			let Ok(guard) = self.peers.read() else {
				log::error!(target: LOG_TARGET, "peers lock poisoned");
				return;
			};
			guard
				.iter()
				.filter(|(peer_id, peer_data)| {
					peer_data.pending_topic_affinity.is_some() &&
						!self.pending_initial_syncs.contains_key(*peer_id)
				})
				.map(|(peer_id, _)| *peer_id)
				.collect()
		};

		for peer_id in ready_peers {
			if let Ok(mut guard) = self.peers.write() {
				if let Some(peer_data) = guard.get_mut(&peer_id) {
					peer_data.topic_affinity = peer_data.pending_topic_affinity.take();
				}
			}
			self.schedule_initial_sync_for_peer(peer_id);
		}
	}

	/// Record initial sync completion metrics for a peer being removed.
	fn record_initial_sync_completion(&self, started_at: Instant) {
		self.metrics.as_ref().map(|metrics| {
			metrics.initial_sync_peers_active.dec();
			metrics
				.initial_sync_duration_seconds
				.observe(started_at.elapsed().as_secs_f64());
		});
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

		self.metrics.as_ref().map(|metrics| {
			metrics.initial_sync_bursts_total.inc();
		});

		if entry.get().hashes.is_empty() {
			let started_at = entry.get().started_at;
			entry.remove();
			self.record_initial_sync_completion(started_at);
			return;
		}

		// Fetch statements up to max_statement_payload_size, skipping statements the peer
		// already knows or that don't match its topic affinity directly in the callback.
		// This avoids materializing non-matching statements and lets each batch carry more
		// useful data.
		let (envelope_overhead, topic_affinity): (usize, Option<AffinityFilter>) = {
			let peers_guard = match self.peers.read() {
				Ok(g) => g,
				Err(_) => {
					log::error!(target: LOG_TARGET, "peers lock poisoned");
					entry.remove();
					return;
				},
			};
			let Some(peer_data) = peers_guard.get(&peer_id) else {
				log::error!(target: LOG_TARGET, "Peer {peer_id} has pending initial sync but is not in peers map");
				entry.remove();
				return;
			};
			(peer_data.protocol_version.envelope_overhead(), peer_data.topic_affinity.clone())
		};
		let peers_clone: PeersState = self.peers.clone();
		let max_size = max_statement_payload_size(envelope_overhead);
		let mut accumulated_size = 0;
		let (statements, processed) = match self.statement_store.statements_by_hashes(
			&entry.get().hashes,
			&mut |hash, encoded, stmt| {
				// Skip statements the peer already knows or that don't match its topic
				// affinity. This avoids materializing non-matching statements and lets
				// each batch carry more useful data.
				let peer_knows = peers_clone
					.read()
					.ok()
					.and_then(|g: std::sync::RwLockReadGuard<'_, HashMap<PeerId, Peer>>| {
						g.get(&peer_id).map(|p| p.known_statements.contains(hash))
					})
					.unwrap_or(false);
				if peer_knows {
					return FilterDecision::Skip;
				}
				if topic_affinity.as_ref().is_some_and(|a: &AffinityFilter| !a.matches_statement(stmt)) {
					return FilterDecision::Skip;
				}
				if accumulated_size > 0 && accumulated_size + encoded.len() > max_size {
					return FilterDecision::Abort;
				}
				accumulated_size += encoded.len();
				FilterDecision::Take
			},
		) {
			Ok(r) => r,
			Err(e) => {
				log::debug!(target: LOG_TARGET, "Failed to fetch statements for initial sync: {e:?}");
				let started_at = entry.get().started_at;
				entry.remove();
				self.record_initial_sync_completion(started_at);
				return;
			},
		};

		// Drain processed hashes and check if more remain
		entry.get_mut().hashes.drain(..processed);
		let has_more = !entry.get().hashes.is_empty();
		drop(entry);

		let send_stmts: Vec<_> = statements.iter().map(|(_, stmt)| stmt).collect();
		match self.send_statement_chunk(&peer_id, &send_stmts).await {
			SendChunkResult::Failed => {
				if let Some(pending) = self.pending_initial_syncs.remove(&peer_id) {
					self.record_initial_sync_completion(pending.started_at);
				}
				return;
			},
			SendChunkResult::Sent(sent) => {
				debug_assert_eq!(send_stmts.len(), sent);
				self.metrics.as_ref().map(|metrics| {
					metrics.initial_sync_statements_sent.inc_by(sent as u64);
				});
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
			if let Some(pending) = self.pending_initial_syncs.remove(&peer_id) {
				self.record_initial_sync_completion(pending.started_at);
			}
		}
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use std::sync::{
		atomic::{AtomicBool, Ordering},
		Mutex,
	};

	/// Default seed used for bloom filters in tests.
	const BLOOM_SEED: u128 = 0x5EED_5EED_5EED_5EED;

	#[derive(Clone)]
	struct TestNetwork {
		reported_peers: Arc<Mutex<Vec<(PeerId, sc_network::ReputationChange)>>>,
		disconnected_peers: Arc<Mutex<Vec<PeerId>>>,
		/// Role to return from `peer_role`. Default: `Full`.
		default_role: sc_network::ObservedRole,
	}

	impl TestNetwork {
		fn new() -> Self {
			Self {
				reported_peers: Arc::new(Mutex::new(Vec::new())),
				disconnected_peers: Arc::new(Mutex::new(Vec::new())),
				default_role: sc_network::ObservedRole::Full,
			}
		}

		fn new_light() -> Self {
			Self {
				reported_peers: Arc::new(Mutex::new(Vec::new())),
				disconnected_peers: Arc::new(Mutex::new(Vec::new())),
				default_role: sc_network::ObservedRole::Light,
			}
		}

		fn get_reports(&self) -> Vec<(PeerId, sc_network::ReputationChange)> {
			self.reported_peers.lock().unwrap().clone()
		}

		fn get_disconnected_peers(&self) -> Vec<PeerId> {
			self.disconnected_peers.lock().unwrap().clone()
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

		fn disconnect_peer(&self, peer: PeerId, _: sc_network::ProtocolName) {
			self.disconnected_peers.lock().unwrap().push(peer);
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

		fn peer_role(&self, _: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
			Some(self.default_role)
		}

		async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
			unimplemented!();
		}
	}

	#[derive(Clone)]
	struct TestSync {
		major_syncing: Arc<AtomicBool>,
	}

	impl TestSync {
		fn new() -> Self {
			Self { major_syncing: Arc::new(AtomicBool::new(false)) }
		}
	}

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
			self.major_syncing.load(Ordering::Relaxed)
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

		fn clear_sent_notifications(&self) {
			self.sent_notifications.lock().unwrap().clear();
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
			max: usize,
		) -> sp_statement_store::Result<
			Vec<(sp_statement_store::Hash, sp_statement_store::Statement)>,
		> {
			let mut recent = self.recent_statements.lock().unwrap();
			if max >= recent.len() {
				return Ok(recent.drain().collect());
			}
			let keys: Vec<_> = recent.keys().copied().take(max).collect();
			Ok(keys.into_iter().filter_map(|k| recent.remove(&k).map(|v| (k, v))).collect())
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
					continue;
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

	fn build_handler(
		num_peers: usize,
	) -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
		async_channel::Receiver<OnStatementsRequest>,
		async_channel::Sender<WorkerEvent>,
		Vec<PeerId>,
	) {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, statements_queue_receiver) = async_channel::bounded(100);
		let (results_queue_sender, results_queue_receiver) = async_channel::bounded(100);
		let pending_state = Arc::new(RwLock::new(PendingState::new()));
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));
		let mut peer_ids = Vec::new();
		for _ in 0..num_peers {
			let peer_id = PeerId::random();
			peers.write().unwrap().insert(
				peer_id,
				Peer {
					known_statements: LruHashSet::new(NonZeroUsize::new(1000).unwrap()),
					rate_limiter: PeerRateLimiter::new(
						NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
							.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
						NonZeroU32::new(
							DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
						)
						.expect("burst capacity is nonzero"),
					),
					protocol_version: PeerProtocolVersion::V1,
					topic_affinity: None,
					is_light: false,
					pending_topic_affinity: None,
				},
			);
			peer_ids.push(peer_id);
		}

		let handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
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
			peer_ids,
		)
	}

	fn get_peer_hashes(sent: &[(PeerId, Vec<u8>)], peer: PeerId) -> Vec<sp_statement_store::Hash> {
		sent.iter()
			.filter(|(p, _)| *p == peer)
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect()
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
			_peer_ids,
		) = build_handler(1);

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
			_peer_ids,
		) = build_handler(1);

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
			_peer_ids,
		) = build_handler(1);

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
			_peer_ids,
		) = build_handler(1);

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
			_peer_ids,
		) = build_handler(1);

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
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers: Arc::new(RwLock::new(HashMap::new())),
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};
		(handler, statement_store, network, notification_service)
	}

	/// Like `build_handler_no_peers` but the network mock returns `Light` for peer roles.
	fn build_handler_no_peers_light() -> (
		StatementHandler<TestNetwork, TestSync>,
		TestStatementStore,
		TestNetwork,
		TestNotificationService,
	) {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, _statements_queue_receiver) = async_channel::bounded(100);
		let (_results_queue_sender, results_queue_receiver) = async_channel::bounded(100);
		let pending_state = Arc::new(RwLock::new(PendingState::new()));
		let network = TestNetwork::new_light();
		let notification_service = TestNotificationService::new();

		let handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state,
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers: Arc::new(RwLock::new(HashMap::new())),
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
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
					rate_limiter: PeerRateLimiter::new(
						NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
							.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
						NonZeroU32::new(
							DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
						)
						.expect("burst capacity is nonzero"),
					),
					protocol_version: PeerProtocolVersion::V1,
					topic_affinity: None,
					is_light: false,
					pending_topic_affinity: None,
				},
			);
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
			sync: TestSync::new(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			metrics: None,
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			initial_sync_timeout: Box::pin(
				tokio::time::sleep(std::time::Duration::from_millis(100)).fuse(),
			),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
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
		let (mut handler, statement_store, _network, notification_service, _, _, _) = build_handler(0);

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

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
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
		let (mut handler, statement_store, _network, notification_service, _, _, _) = build_handler(0);

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

		// Connect peers
		for peer in [peer1, peer2, peer3] {
			handler
				.handle_notification_event(NotificationEvent::NotificationStreamOpened {
					peer,
					direction: sc_network::service::traits::Direction::Inbound,
					handshake: vec![],
					negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
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
		let mut peer1_hashes = get_peer_hashes(&sent, peer1);
		let mut peer2_hashes = get_peer_hashes(&sent, peer2);
		let mut peer3_hashes = get_peer_hashes(&sent, peer3);

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
			_peer_ids,
		) = build_handler(1);

		// Calculate the data sizes so that 100 statements together exactly fill max_size.
		// This tests that all 100 statements fit in a single notification.
		//
		// The limit check in find_sendable_chunk is:
		//   max_size = MAX_STATEMENT_NOTIFICATION_SIZE - Compact::<u32>::max_encoded_len()
		//
		// Statement encoding (encodes as Vec<Field>):
		// - Compact<u32> for number of fields (1 byte for value 2: expiry + data)
		// - Field::Expiry discriminant (1 byte, value 2)
		// - u64 expiry value (8 bytes)
		// - Field::Data discriminant (1 byte, value 8)
		// - Compact<u32> for the data length (2 bytes for small data)
		// So per-statement overhead = 1 + 1 + 8 + 1 + 2 = 13 bytes
		let max_size = MAX_STATEMENT_NOTIFICATION_SIZE as usize - Compact::<u32>::max_encoded_len();
		let num_statements: usize = 100;
		let per_statement_overhead = 1 + 1 + 8 + 1 + 2; // Vec<Field> length + expiry field + data discriminant + Compact data length
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
		let (mut handler, statement_store, _network, notification_service, _, _, _) = build_handler(0);

		// This peer connects as V1 (see negotiated_fallback below).
		let payload_limit = max_statement_payload_size(V1_ENVELOPE_OVERHEAD);

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

		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
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

	#[tokio::test]
	async fn test_peer_disconnected_on_flooding() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_results_sender,
			_peer_ids,
		) = build_handler(1);

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		let mut flood_statements = Vec::new();
		for i in 0..600_000 {
			let mut statement = Statement::new();
			statement.set_plain_data(vec![i as u8, (i >> 8) as u8, (i >> 16) as u8]);
			flood_statements.push(statement);
		}

		handler.on_statements(peer_id, flood_statements);

		let reports = network.get_reports();
		assert!(
			reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Expected STATEMENT_FLOODING reputation change, but got: {:?}",
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Expected peer {} to be disconnected, but it wasn't. Disconnected peers: {:?}",
			peer_id,
			disconnected
		);

		// Verify peer state was cleaned up
		assert!(
			!handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should be removed from peers map"
		);
		assert!(
			!handler.pending_initial_syncs.contains_key(&peer_id),
			"Peer should be removed from pending_initial_syncs"
		);
		assert!(
			!handler.initial_sync_peer_queue.contains(&peer_id),
			"Peer should be removed from initial_sync_peer_queue"
		);
	}

	#[tokio::test]
	async fn test_legitimate_traffic_not_flagged() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_results_sender,
			_peer_ids,
		) = build_handler(1);

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		let start = std::time::Instant::now();
		let duration = std::time::Duration::from_secs(5);
		let mut counter = 0u32;

		while start.elapsed() < duration {
			let mut statements = Vec::new();
			for i in 0..5_000 {
				let mut statement = Statement::new();
				statement.set_plain_data(vec![
					counter as u8,
					(counter >> 8) as u8,
					(counter >> 16) as u8,
					i as u8,
				]);
				statements.push(statement);
				counter = counter.wrapping_add(1);
			}

			handler.on_statements(peer_id, statements);

			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
		}

		let reports = network.get_reports();
		assert!(
			!reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Legitimate traffic should not trigger flooding detection. Reports: {:?}",
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			!disconnected.contains(&peer_id),
			"Legitimate traffic should not cause disconnection. Disconnected peers: {:?}",
			disconnected
		);

		assert!(
			handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should still be connected"
		);
	}

	#[tokio::test]
	async fn test_just_over_rate_limit_triggers_flooding() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_results_sender,
			_peer_ids,
		) = build_handler(1);

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		let mut statements = Vec::new();
		for i in 0..260_000 {
			let mut statement = Statement::new();
			statement.set_plain_data(vec![
				i as u8,
				(i >> 8) as u8,
				(i >> 16) as u8,
				(i >> 24) as u8,
			]);
			statements.push(statement);
		}

		handler.on_statements(peer_id, statements);

		let reports = network.get_reports();
		let expected_burst = DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT;
		assert!(
			reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"Sending 260,000 statements should trigger flooding (burst limit: {}). Reports: {:?}",
			expected_burst,
			reports
		);

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Peer should be disconnected after exceeding rate limit. Disconnected: {:?}",
			disconnected
		);

		assert!(
			!handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should be removed from peers map"
		);
	}

	#[tokio::test]
	async fn test_burst_of_250k_statements_allowed() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_results_sender,
			_peer_ids,
		) = build_handler(1);

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		let mut statements = Vec::new();
		for i in 0..250_000 {
			let mut statement = Statement::new();
			statement.set_plain_data(vec![
				i as u8,
				(i >> 8) as u8,
				(i >> 16) as u8,
				(i >> 24) as u8,
			]);
			statements.push(statement);
		}

		handler.on_statements(peer_id, statements);

		let reports = network.get_reports();
		assert!(
			!reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING),
			"250k burst should be allowed (burst = rate × 5). Reports: {:?}",
			reports
		);

		assert!(
			handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should still be connected after 250k burst"
		);
	}

	#[tokio::test]
	async fn test_sustained_rate_above_limit_triggers_flooding() {
		let (
			mut handler,
			_statement_store,
			network,
			_notification_service,
			_queue_receiver,
			_results_sender,
			_peer_ids,
		) = build_handler(1);

		let peer_id = *handler.peers.read().unwrap().keys().next().unwrap();

		let mut counter = 0u32;

		let start = std::time::Instant::now();
		let duration = std::time::Duration::from_secs(5);

		let mut flooding_detected = false;
		while start.elapsed() < duration {
			let mut statements = Vec::new();
			for i in 0..30_000 {
				let mut statement = Statement::new();
				statement.set_plain_data(vec![
					counter as u8,
					(counter >> 8) as u8,
					(counter >> 16) as u8,
					i as u8,
				]);
				statements.push(statement);
				counter = counter.wrapping_add(1);
			}

			handler.on_statements(peer_id, statements);

			// Check if flooding was detected
			let reports = network.get_reports();
			if reports
				.iter()
				.any(|(id, rep)| *id == peer_id && *rep == rep::STATEMENT_FLOODING)
			{
				flooding_detected = true;
				break;
			}

			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
		}

		assert!(flooding_detected, "Sustained rate of 300k/sec should trigger flooding");

		let disconnected = network.get_disconnected_peers();
		assert!(
			disconnected.contains(&peer_id),
			"Peer should be disconnected after sustained high rate. Disconnected: {:?}",
			disconnected
		);

		assert!(
			!handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should be removed from peers map"
		);
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
		// With 600 statements and a batch limit of 256, we need 3 ticks (256+256+88)
		tokio::time::advance(std::time::Duration::from_secs(4)).await;
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
			_network,
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

		// Inject peer connection event (V1 peer so that the test can decode with V1 format)
		let new_peer = PeerId::random();
		event_sender
			.send(NotificationEvent::NotificationStreamOpened {
				peer: new_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
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
		while let Ok(notif) = notification_receiver.try_recv() {
			received.push(notif);
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

	/// Helper to set up shared state for direct `process_on_statements` testing
	/// without going through the full handler/run loop.
	fn build_worker_test_env(
		num_peers: usize,
		queue_capacity: usize,
	) -> (
		PeersState,
		Arc<RwLock<PendingState>>,
		Arc<TestStatementStore>,
		async_channel::Sender<(Hash, Statement)>,
		async_channel::Receiver<(Hash, Statement)>,
		async_channel::Sender<WorkerEvent>,
		async_channel::Receiver<WorkerEvent>,
		Vec<PeerId>,
	) {
		let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));
		let mut peer_ids = Vec::new();
		for _ in 0..num_peers {
			let peer_id = PeerId::random();
			peer_ids.push(peer_id);
			peers.write().unwrap().insert(
				peer_id,
				Peer {
					known_statements: LruHashSet::new(NonZeroUsize::new(10_000).unwrap()),
					rate_limiter: PeerRateLimiter::new(
						NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
							.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
						NonZeroU32::new(
							DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
						)
						.expect("burst capacity is nonzero"),
					),
					protocol_version: PeerProtocolVersion::V1,
					topic_affinity: None,
					is_light: false,
					pending_topic_affinity: None,
				},
			);
		}

		let pending_state = Arc::new(RwLock::new(PendingState::new()));
		let statement_store = Arc::new(TestStatementStore::new());
		let (submit_sender, submit_receiver) = async_channel::bounded(queue_capacity);
		let (event_sender, event_receiver) = async_channel::bounded(queue_capacity);

		(
			peers,
			pending_state,
			statement_store,
			submit_sender,
			submit_receiver,
			event_sender,
			event_receiver,
			peer_ids,
		)
	}

	fn make_statement(index: usize) -> Statement {
		let mut stmt = Statement::new();
		let mut data = vec![0u8; 128];
		data[..8].copy_from_slice(&(index as u64).to_le_bytes());
		stmt.set_plain_data(data);
		stmt
	}

	/// Multiple workers process the same statement from different peers concurrently
	#[tokio::test]
	async fn test_concurrent_workers_same_statement_different_peers() {
		let num_peers = 20;
		let (
			peers,
			pending_state,
			statement_store,
			submit_sender,
			submit_receiver,
			event_sender,
			event_receiver,
			peer_ids,
		) = build_worker_test_env(num_peers, 1000);

		let statement = make_statement(42);
		let expected_hash = statement.hash();
		let notification = vec![statement].encode();

		let mut handles = Vec::new();
		for peer_id in &peer_ids {
			let who = *peer_id;
			let notif = notification.clone();
			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sq = submit_sender.clone();
			let es = event_sender.clone();
			let sp = peers.clone();

			handles.push(tokio::spawn(async move {
				StatementHandlerPrototype::process_on_statements(
					who, notif, &ss, &store, &sq, &es, &sp, &None,
				)
				.await;
			}));
		}

		for h in handles {
			h.await.unwrap();
		}

		// Only one submission should reach the validation queue
		let mut submitted = Vec::new();
		while let Ok((hash, _stmt)) = submit_receiver.try_recv() {
			submitted.push(hash);
		}
		assert_eq!(
			submitted.len(),
			1,
			"Expected exactly 1 submission to validation queue, got {}",
			submitted.len()
		);
		assert_eq!(submitted[0], expected_hash);

		let state = pending_state.read().unwrap();
		let recorded_peers = state.pending_statements_peers.get(&expected_hash).unwrap();
		assert_eq!(
			recorded_peers.len(),
			num_peers,
			"Expected all {} peers recorded in pending_statements_peers, got {}",
			num_peers,
			recorded_peers.len()
		);
		for peer_id in &peer_ids {
			assert!(
				recorded_peers.contains(peer_id),
				"Peer {} should be in pending_statements_peers",
				peer_id
			);
		}

		drop(event_sender);
		let mut reputation_events = Vec::new();
		while let Ok(event) = event_receiver.try_recv() {
			if let WorkerEvent::ReputationChange(rc) = event {
				reputation_events.push(rc);
			}
		}

		assert!(!reputation_events.is_empty(), "Expected at least some reputation change events");
	}

	/// Multiple workers process distinct statements from the same peer concurrently
	#[tokio::test]
	async fn test_concurrent_workers_distinct_statements_same_peer() {
		let num_workers = 10;
		let statements_per_worker = 50;
		let (
			peers,
			pending_state,
			statement_store,
			submit_sender,
			submit_receiver,
			event_sender,
			_event_receiver,
			peer_ids,
		) = build_worker_test_env(1, 10_000);

		let peer_id = peer_ids[0];

		let mut handles = Vec::new();
		let mut all_expected_hashes = HashSet::new();
		for worker_idx in 0..num_workers {
			let mut statements = Vec::new();
			for stmt_idx in 0..statements_per_worker {
				let stmt = make_statement(worker_idx * 1000 + stmt_idx);
				all_expected_hashes.insert(stmt.hash());
				statements.push(stmt);
			}
			let notification = statements.encode();

			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sq = submit_sender.clone();
			let es = event_sender.clone();
			let sp = peers.clone();

			handles.push(tokio::spawn(async move {
				StatementHandlerPrototype::process_on_statements(
					peer_id,
					notification,
					&ss,
					&store,
					&sq,
					&es,
					&sp,
					&None,
				)
				.await;
			}));
		}

		for h in handles {
			h.await.unwrap();
		}

		// All unique statements should be submitted to the validation queue.
		let mut submitted_hashes = HashSet::new();
		while let Ok((hash, _stmt)) = submit_receiver.try_recv() {
			submitted_hashes.insert(hash);
		}
		assert_eq!(
			submitted_hashes.len(),
			all_expected_hashes.len(),
			"Expected {} unique submissions, got {}",
			all_expected_hashes.len(),
			submitted_hashes.len()
		);
		assert_eq!(submitted_hashes, all_expected_hashes);

		let state = pending_state.read().unwrap();
		for hash in &all_expected_hashes {
			assert!(
				state.pending_statements_peers.contains_key(hash),
				"Hash should be in pending_statements_peers"
			);
		}
	}

	/// Many workers, many peers, overlapping statements
	/// N peers each send batches containing some overlapping and some unique statements
	/// Verifies that:
	/// 1. Each unique statement is submitted exactly once
	/// 2. No panics or deadlocks occur under contention
	/// 3. All peers are properly recorded in pending_statements_peers
	#[tokio::test]
	async fn test_concurrent_workers_overlapping_statements_many_peers() {
		let num_peers = 10;
		let statements_per_peer = 30;
		let overlap_count = 10; // First N statements are shared across all peers
		let (
			peers,
			pending_state,
			statement_store,
			submit_sender,
			submit_receiver,
			event_sender,
			_event_receiver,
			peer_ids,
		) = build_worker_test_env(num_peers, 10_000);

		let shared_statements: Vec<Statement> =
			(0..overlap_count).map(|i| make_statement(i)).collect();
		let shared_hashes: HashSet<Hash> = shared_statements.iter().map(|s| s.hash()).collect();

		let mut all_expected_hashes = shared_hashes.clone();

		// Each peer gets shared statements + unique ones
		let mut handles = Vec::new();
		for (peer_idx, peer_id) in peer_ids.iter().enumerate() {
			let mut batch = shared_statements.clone();
			for stmt_idx in 0..(statements_per_peer - overlap_count) {
				let stmt = make_statement(10_000 + peer_idx * 1000 + stmt_idx);
				all_expected_hashes.insert(stmt.hash());
				batch.push(stmt);
			}
			let notification = batch.encode();

			let who = *peer_id;
			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sq = submit_sender.clone();
			let es = event_sender.clone();
			let sp = peers.clone();

			handles.push(tokio::spawn(async move {
				StatementHandlerPrototype::process_on_statements(
					who,
					notification,
					&ss,
					&store,
					&sq,
					&es,
					&sp,
					&None,
				)
				.await;
			}));
		}

		for h in handles {
			h.await.unwrap();
		}

		let mut submitted_hashes = HashSet::new();
		let mut submission_counts: HashMap<Hash, usize> = HashMap::new();
		while let Ok((hash, _stmt)) = submit_receiver.try_recv() {
			submitted_hashes.insert(hash);
			*submission_counts.entry(hash).or_insert(0) += 1;
		}

		assert_eq!(
			submitted_hashes.len(),
			all_expected_hashes.len(),
			"Expected {} unique submissions, got {}. Missing: {:?}",
			all_expected_hashes.len(),
			submitted_hashes.len(),
			all_expected_hashes.difference(&submitted_hashes).collect::<Vec<_>>()
		);

		for hash in &shared_hashes {
			assert_eq!(
				submission_counts[hash], 1,
				"Shared statement should be submitted exactly once, got {}",
				submission_counts[hash]
			);
		}

		// Shared hashes have multiple peers
		let state = pending_state.read().unwrap();
		for hash in &shared_hashes {
			let recorded = state.pending_statements_peers.get(hash).unwrap();
			assert!(
				recorded.len() >= 2,
				"Shared hash should have at least 2 peers recorded, got {}",
				recorded.len()
			);
		}
	}

	/// Verifies that workers handle `TrySendError::Full` gracefully without panicking
	/// or corrupting shared state
	#[tokio::test]
	async fn test_concurrent_workers_submit_queue_full() {
		let num_peers = 5;
		let queue_capacity = 2;
		let (
			peers,
			pending_state,
			statement_store,
			_submit_sender,
			_submit_receiver,
			event_sender,
			_event_receiver,
			peer_ids,
		) = build_worker_test_env(num_peers, 1000);

		// Override with a small submit queue
		let (small_submit_sender, small_submit_receiver) = async_channel::bounded(queue_capacity);

		let statements_per_peer = 50;
		let mut handles = Vec::new();
		for (peer_idx, peer_id) in peer_ids.iter().enumerate() {
			let mut batch = Vec::new();
			for stmt_idx in 0..statements_per_peer {
				batch.push(make_statement(peer_idx * 1000 + stmt_idx));
			}
			let notification = batch.encode();

			let who = *peer_id;
			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sq = small_submit_sender.clone();
			let es = event_sender.clone();
			let sp = peers.clone();

			handles.push(tokio::spawn(async move {
				StatementHandlerPrototype::process_on_statements(
					who,
					notification,
					&ss,
					&store,
					&sq,
					&es,
					&sp,
					&None,
				)
				.await;
			}));
		}

		for h in handles {
			h.await.unwrap();
		}

		let mut submitted = 0;
		while let Ok(_) = small_submit_receiver.try_recv() {
			submitted += 1;
		}
		assert!(submitted > 0, "At least some statements should have been submitted");
		assert!(
			submitted <= queue_capacity + num_peers,
			"Submitted count ({}) should be bounded by queue capacity ({}) plus concurrent senders ({})",
			submitted,
			queue_capacity,
			num_peers
		);

		// Key here no panics, no deadlocks, shared state is consistent
		// pending_state should only contain entries for statements that were successfully submitted
		let state = pending_state.read().unwrap();
		assert!(
			state.pending_statements_peers.len() <= submitted,
			"Pending entries ({}) should not exceed submitted count ({})",
			state.pending_statements_peers.len(),
			submitted
		);
	}

	/// Multiple worker tasks race to consume from the same channel and process overlapping
	/// statements
	#[tokio::test]
	async fn test_concurrent_run_on_statements_workers() {
		let num_workers = 4;
		let num_peers = 8;
		let statements_per_peer = 20;
		let overlap_count = 5;
		let (
			peers,
			pending_state,
			statement_store,
			submit_sender,
			submit_receiver,
			event_sender,
			event_receiver,
			peer_ids,
		) = build_worker_test_env(num_peers, 10_000);

		let (input_sender, input_receiver) = async_channel::bounded::<OnStatementsRequest>(1000);

		let mut worker_handles = Vec::new();
		for _ in 0..num_workers {
			let recv = input_receiver.clone();
			let sq = submit_sender.clone();
			let es = event_sender.clone();
			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sp = peers.clone();

			worker_handles.push(tokio::spawn(StatementHandlerPrototype::run_on_statements_worker(
				recv, sq, es, ss, store, sp, None,
			)));
		}

		let shared_statements: Vec<Statement> =
			(0..overlap_count).map(|i| make_statement(i)).collect();
		let shared_hashes: HashSet<Hash> = shared_statements.iter().map(|s| s.hash()).collect();
		let mut all_expected_hashes = shared_hashes.clone();

		for (peer_idx, peer_id) in peer_ids.iter().enumerate() {
			let mut batch = shared_statements.clone();
			for stmt_idx in 0..(statements_per_peer - overlap_count) {
				let stmt = make_statement(10_000 + peer_idx * 1000 + stmt_idx);
				all_expected_hashes.insert(stmt.hash());
				batch.push(stmt);
			}
			let notification = batch.encode();
			input_sender
				.send(OnStatementsRequest { who: *peer_id, notification })
				.await
				.unwrap();
		}

		drop(input_sender);

		for h in worker_handles {
			h.await.unwrap();
		}

		drop(submit_sender);
		let mut submitted_hashes = HashSet::new();
		let mut submission_counts: HashMap<Hash, usize> = HashMap::new();
		while let Ok((hash, _stmt)) = submit_receiver.try_recv() {
			submitted_hashes.insert(hash);
			*submission_counts.entry(hash).or_insert(0) += 1;
		}

		assert_eq!(
			submitted_hashes.len(),
			all_expected_hashes.len(),
			"Expected {} unique submissions, got {}",
			all_expected_hashes.len(),
			submitted_hashes.len()
		);

		// Shared statements submitted exactly
		for hash in &shared_hashes {
			assert_eq!(
				submission_counts[hash], 1,
				"Shared statement should be submitted exactly once"
			);
		}

		let state = pending_state.read().unwrap();
		for hash in &shared_hashes {
			let recorded = state.pending_statements_peers.get(hash).unwrap();
			assert!(
				recorded.len() >= 2,
				"Shared hash should have multiple peers, got {}",
				recorded.len()
			);
		}

		drop(event_sender);
		let mut rep_count = 0;
		while let Ok(_) = event_receiver.try_recv() {
			rep_count += 1;
		}
		assert!(
			rep_count >= num_peers,
			"Expected at least {} reputation events, got {}",
			num_peers,
			rep_count
		);
	}

	/// Stress test: High contention scenario with many peers sending many statements
	/// to verify no deadlocks occur under heavy load.
	#[tokio::test]
	async fn test_high_contention_no_deadlock() {
		let num_peers = 50;
		let statements_per_peer = 100;
		let (
			peers,
			pending_state,
			statement_store,
			submit_sender,
			_submit_receiver,
			event_sender,
			_event_receiver,
			peer_ids,
		) = build_worker_test_env(num_peers, 100_000);

		let mut handles = Vec::new();
		for (peer_idx, peer_id) in peer_ids.iter().enumerate() {
			let mut batch = Vec::new();
			for stmt_idx in 0..statements_per_peer {
				batch.push(make_statement(peer_idx * 10_000 + stmt_idx));
			}
			let notification = batch.encode();

			let who = *peer_id;
			let ss = pending_state.clone();
			let store = statement_store.clone() as Arc<dyn StatementStore>;
			let sq = submit_sender.clone();
			let es = event_sender.clone();
			let sp = peers.clone();

			handles.push(tokio::spawn(async move {
				StatementHandlerPrototype::process_on_statements(
					who,
					notification,
					&ss,
					&store,
					&sq,
					&es,
					&sp,
					&None,
				)
				.await;
			}));
		}

		let all_done = futures::future::join_all(handles);
		let result: Result<Vec<Result<(), tokio::task::JoinError>>, _> =
			tokio::time::timeout(std::time::Duration::from_secs(30), all_done).await;
		assert!(result.is_ok());

		for r in result.unwrap() {
			r.unwrap();
		}

		let state = pending_state.read().unwrap();
		let expected_total = num_peers * statements_per_peer;
		assert_eq!(
			state.pending_statements_peers.len(),
			expected_total,
			"Expected {} pending entries, got {}",
			expected_total,
			state.pending_statements_peers.len()
		);
	}

	#[tokio::test]
	async fn test_v2_peer_detected_when_no_fallback() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// No negotiated_fallback means the peer connected on the main protocol (v2).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		assert_eq!(
			handler.peers.read().unwrap().get(&peer_id).unwrap().protocol_version,
			PeerProtocolVersion::V2,
			"Peer should be detected as v2 when no fallback is negotiated"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_detected_when_fallback_negotiated() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// negotiated_fallback is Some means the peer fell back to v1.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		assert_eq!(
			handler.peers.read().unwrap().get(&peer_id).unwrap().protocol_version,
			PeerProtocolVersion::V1,
			"Peer should be detected as v1 when fallback is negotiated"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_decodes_raw_statements() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();
		let (queue_sender, queue_receiver) =
			async_channel::bounded::<OnStatementsRequest>(10);
		handler.statements_queue_sender = queue_sender;

		// Connect peer as v1 (with fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// V1 peer sends raw Vec<Statement>.
		let mut statement = Statement::new();
		statement.set_plain_data(b"v1 statement".to_vec());
		let hash = statement.hash();
		let raw_encoded = vec![statement].encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: raw_encoded.clone().into(),
			})
			.await;

		let req = queue_receiver.try_recv().unwrap();
		assert_eq!(req.who, peer_id);
		let received_stmts: Vec<Statement> =
			Decode::decode(&mut req.notification.as_slice()).unwrap();
		assert_eq!(received_stmts.len(), 1);
		assert_eq!(
			received_stmts[0].hash(),
			hash,
			"V1 peer's raw statement should be forwarded correctly"
		);
	}

	#[tokio::test]
	async fn test_v2_peer_decodes_statement_message() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();
		let (queue_sender, queue_receiver) =
			async_channel::bounded::<OnStatementsRequest>(10);
		handler.statements_queue_sender = queue_sender;

		// Connect peer as v2 (no fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// V2 peer sends StatementMessage::Statements.
		let mut statement = Statement::new();
		statement.set_plain_data(b"v2 statement".to_vec());
		let hash = statement.hash();
		let msg = StatementMessage::Statements(vec![statement]);
		let encoded = msg.encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		let req = queue_receiver.try_recv().unwrap();
		assert_eq!(req.who, peer_id);
		let received_stmts: Vec<Statement> =
			Decode::decode(&mut req.notification.as_slice()).unwrap();
		assert_eq!(received_stmts.len(), 1);
		assert_eq!(
			received_stmts[0].hash(),
			hash,
			"V2 peer's StatementMessage should be forwarded correctly"
		);
	}

	#[tokio::test]
	async fn test_v2_peer_topic_affinity_stored() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		assert!(
			handler.peers.read().unwrap().get(&peer_id).unwrap().topic_affinity.is_none(),
			"Topic affinity should be None initially"
		);

		// Send ExplicitTopicAffinity message.
		let topic: [u8; 32] = [0xAA; 32];
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();

		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		let peers_guard = handler.peers.read().unwrap();
		let peer_data = peers_guard.get(&peer_id).unwrap();
		assert!(
			peer_data.topic_affinity.is_some(),
			"Topic affinity should be set after receiving ExplicitTopicAffinity"
		);
		// The filter should match the topic we inserted.
		assert!(
			peer_data.topic_affinity.as_ref().unwrap().contains(&topic),
			"Stored affinity filter should match the topic"
		);
	}

	#[tokio::test]
	async fn test_topic_affinity_filters_propagation() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Set up topic affinity: peer is interested in topic 0xAA only.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Create statements: one matching, one not matching, one with no topics.
		let mut stmt_matching = Statement::new();
		stmt_matching.set_plain_data(b"matching".to_vec());
		stmt_matching.set_topic(0, topic_aa.into());
		let hash_matching = stmt_matching.hash();

		let mut stmt_not_matching = Statement::new();
		stmt_not_matching.set_plain_data(b"not matching".to_vec());
		stmt_not_matching.set_topic(0, topic_bb.into());
		let hash_not_matching = stmt_not_matching.hash();

		let mut stmt_no_topic = Statement::new();
		stmt_no_topic.set_plain_data(b"no topic".to_vec());
		let hash_no_topic = stmt_no_topic.hash();

		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_matching, stmt_matching);
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_not_matching, stmt_not_matching);
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_no_topic, stmt_no_topic);

		handler.propagate_statements();
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let mut sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				// V2 peer gets StatementMessage encoding.
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		sent_hashes.sort();

		// Matching and no-topic statements should be sent; non-matching should be filtered.
		assert!(
			sent_hashes.contains(&hash_matching),
			"Statement matching topic affinity should be propagated"
		);
		assert!(
			sent_hashes.contains(&hash_no_topic),
			"Statement with no topics should be propagated (broadcast)"
		);
		assert!(
			!sent_hashes.contains(&hash_not_matching),
			"Statement NOT matching topic affinity should be filtered out"
		);
	}

	#[tokio::test]
	async fn test_v1_peer_no_topic_filtering() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as v1 (with fallback).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// V1 peers have no topic affinity - all statements should be propagated.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let mut stmt_with_topic = Statement::new();
		stmt_with_topic.set_plain_data(b"with topic".to_vec());
		stmt_with_topic.set_topic(0, topic_aa.into());
		let hash_with_topic = stmt_with_topic.hash();

		let mut stmt_no_topic = Statement::new();
		stmt_no_topic.set_plain_data(b"no topic".to_vec());
		let hash_no_topic = stmt_no_topic.hash();

		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_with_topic, stmt_with_topic);
		statement_store
			.recent_statements
			.lock()
			.unwrap()
			.insert(hash_no_topic, stmt_no_topic);

		handler.propagate_statements();
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: Vec<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				<Statements as Decode>::decode(&mut notification.as_slice()).unwrap()
			})
			.map(|s| s.hash())
			.collect();

		assert_eq!(
			sent_hashes.len(),
			2,
			"V1 peer should receive all statements regardless of topics"
		);
		assert!(sent_hashes.contains(&hash_with_topic));
		assert!(sent_hashes.contains(&hash_no_topic));
	}

	#[tokio::test]
	async fn test_affinity_change_triggers_resync() {
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers_light();

		let peer_id = PeerId::random();

		// Add statements with different topics to the store.
		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];

		let mut stmt_aa = Statement::new();
		stmt_aa.set_plain_data(b"stmt_aa".to_vec());
		stmt_aa.set_topic(0, topic_aa.into());
		let hash_aa = stmt_aa.hash();

		let mut stmt_bb = Statement::new();
		stmt_bb.set_plain_data(b"stmt_bb".to_vec());
		stmt_bb.set_topic(0, topic_bb.into());
		let hash_bb = stmt_bb.hash();

		let mut stmt_no_topic = Statement::new();
		stmt_no_topic.set_plain_data(b"no topic".to_vec());
		let hash_no_topic = stmt_no_topic.hash();

		statement_store.statements.lock().unwrap().insert(hash_aa, stmt_aa);
		statement_store.statements.lock().unwrap().insert(hash_bb, stmt_bb);
		statement_store.statements.lock().unwrap().insert(hash_no_topic, stmt_no_topic);

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Light V2 peers should NOT get initial sync on connect (must set affinity first).
		assert!(
			!handler.pending_initial_syncs.contains_key(&peer_id),
			"Light V2 peer should NOT have initial sync scheduled on connect"
		);

		// Set topic affinity to topic_aa — this triggers the first initial sync.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be scheduled after setting affinity"
		);

		// Drain initial sync — only stmt_aa and stmt_no_topic should be sent.
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(sent_hashes.contains(&hash_aa), "stmt_aa should be sent (matches affinity)");
		assert!(
			sent_hashes.contains(&hash_no_topic),
			"stmt_no_topic should be sent (broadcast, no topic)"
		);
		assert!(!sent_hashes.contains(&hash_bb), "stmt_bb should NOT be sent (filtered)");

		// Now change affinity to topic_bb — triggers re-sync.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_bb);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be re-scheduled after affinity change"
		);

		notification_service.clear_sent_notifications();
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst().await;
		}

		let sent_after_bb = notification_service.get_sent_notifications();
		let sent_hashes_bb: HashSet<_> = sent_after_bb
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		// stmt_bb was previously filtered and should now be sent.
		assert!(
			sent_hashes_bb.contains(&hash_bb),
			"stmt_bb should now be sent after affinity changed to topic_bb"
		);
		// Known statements are redelivered on affinity change.
		assert!(
			sent_hashes_bb.contains(&hash_no_topic),
			"stmt_no_topic should be re-sent (known_statements cleared on affinity change)"
		);
	}

	#[tokio::test]
	async fn test_affinity_change_sends_previously_filtered_statements() {
		// This tests the scenario where:
		// 1. Peer connects and immediately sets affinity (before initial sync).
		// 2. Statements not matching the initial affinity are NOT marked as known.
		// 3. When affinity changes to include those topics, they ARE sent.
		let (mut handler, statement_store, _network, notification_service) =
			build_handler_no_peers_light();

		let peer_id = PeerId::random();

		let topic_aa: [u8; 32] = [0xAA; 32];
		let topic_bb: [u8; 32] = [0xBB; 32];

		let mut stmt_aa = Statement::new();
		stmt_aa.set_plain_data(b"stmt_aa".to_vec());
		stmt_aa.set_topic(0, topic_aa.into());
		let hash_aa = stmt_aa.hash();

		let mut stmt_bb = Statement::new();
		stmt_bb.set_plain_data(b"stmt_bb".to_vec());
		stmt_bb.set_topic(0, topic_bb.into());
		let hash_bb = stmt_bb.hash();

		statement_store.statements.lock().unwrap().insert(hash_aa, stmt_aa.clone());
		statement_store.statements.lock().unwrap().insert(hash_bb, stmt_bb.clone());

		// Also put them in recent_statements so propagate_statements can find them.
		statement_store.recent_statements.lock().unwrap().insert(hash_aa, stmt_aa);
		statement_store.recent_statements.lock().unwrap().insert(hash_bb, stmt_bb);

		// Connect peer as v2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Immediately set affinity to topic_aa BEFORE any initial sync runs.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Drain initial sync — should only send stmt_aa (matches affinity).
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(sent_hashes.contains(&hash_aa), "stmt_aa should be sent (matches affinity)");
		assert!(
			!sent_hashes.contains(&hash_bb),
			"stmt_bb should NOT be sent (filtered by affinity)"
		);

		// Now propagate_statements — stmt_bb should be filtered by affinity and NOT marked as
		// known.
		handler.propagate_statements();
		handler.flush_pending_sends().await;

		// Verify stmt_bb was NOT marked as known (the bug fix).
		let peers_guard = handler.peers.read().unwrap();
		let peer = peers_guard.get(&peer_id).unwrap();
		assert!(
			!peer.known_statements.contains(&hash_bb),
			"stmt_bb should NOT be in known_statements (filtered by affinity)"
		);
		assert!(peer.known_statements.contains(&hash_aa), "stmt_aa should be in known_statements");
		drop(peers_guard);

		// Now change affinity to include topic_bb.
		let mut filter = AffinityFilter::new(BLOOM_SEED, 0.01, 100);
		filter.insert(&topic_aa);
		filter.insert(&topic_bb);
		let msg = StatementMessage::ExplicitTopicAffinity(filter);
		let encoded = msg.encode();

		notification_service.clear_sent_notifications();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: encoded.into(),
			})
			.await;

		// Affinity is deferred; process it.
		handler.process_pending_affinities();

		// Drain re-sync — stmt_bb should now be sent.
		while handler.pending_initial_syncs.contains_key(&peer_id) {
			handler.process_initial_sync_burst().await;
		}

		let sent = notification_service.get_sent_notifications();
		let sent_hashes: HashSet<_> = sent
			.iter()
			.flat_map(|(_, notification)| {
				match StatementMessage::decode(&mut notification.as_slice()).unwrap() {
					StatementMessage::Statements(stmts) => stmts,
					_ => panic!("Expected StatementMessage::Statements"),
				}
			})
			.map(|s| s.hash())
			.collect();
		assert!(
			sent_hashes.contains(&hash_bb),
			"stmt_bb should now be sent after affinity expanded to include topic_bb"
		);
		// stmt_aa is also redelivered on affinity change.
		assert!(
			sent_hashes.contains(&hash_aa),
			"stmt_aa should be re-sent (known_statements cleared on affinity change)"
		);
	}

	#[test]
	fn test_encode_statement_refs_matches_derive_encoding() {
		let mut stmt1 = Statement::new();
		stmt1.set_plain_data(b"first".to_vec());
		let mut stmt2 = Statement::new();
		stmt2.set_plain_data(b"second".to_vec());

		let refs: Vec<&Statement> = vec![&stmt1, &stmt2];
		let hand_rolled = StatementMessage::encode_statement_refs(&refs);
		let derive_encoded = StatementMessage::Statements(vec![stmt1, stmt2]).encode();

		assert_eq!(
			hand_rolled, derive_encoded,
			"encode_statement_refs must produce identical bytes to derive Encode"
		);
	}

	#[test]
	fn test_encode_statement_refs_empty() {
		let refs: Vec<&Statement> = vec![];
		let hand_rolled = StatementMessage::encode_statement_refs(&refs);
		let derive_encoded = StatementMessage::Statements(vec![]).encode();

		assert_eq!(hand_rolled, derive_encoded);
	}

	#[test]
	fn test_can_receive_all_combinations() {
		let make_peer = |is_light: bool, version: PeerProtocolVersion, has_affinity: bool| {
			let topic_affinity = has_affinity.then(|| AffinityFilter::new(BLOOM_SEED, 0.01, 10));
			Peer {
				known_statements: LruHashSet::new(NonZeroUsize::new(10).unwrap()),
				rate_limiter: PeerRateLimiter::new(
					NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).expect("nonzero"),
					NonZeroU32::new(
						DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
					)
					.expect("nonzero"),
				),
				protocol_version: version,
				topic_affinity,
				is_light,
				pending_topic_affinity: None,
			}
		};

		// Full node, V1, no affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V1, false).can_receive());
		// Full node, V2, no affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V2, false).can_receive());
		// Light, V1, no affinity → can receive (V1 doesn't gate)
		assert!(make_peer(true, PeerProtocolVersion::V1, false).can_receive());
		// Light, V2, no affinity → CANNOT receive (must set affinity first)
		assert!(!make_peer(true, PeerProtocolVersion::V2, false).can_receive());
		// Light, V2, with affinity → can receive
		assert!(make_peer(true, PeerProtocolVersion::V2, true).can_receive());
		// Full node, V2, with affinity → can receive
		assert!(make_peer(false, PeerProtocolVersion::V2, true).can_receive());
	}

	#[tokio::test]
	async fn test_send_chunk_v1_vs_v2_encoding() {
		let (mut handler, _statement_store, _network, notification_service) =
			build_handler_no_peers();

		let v1_peer = PeerId::random();
		let v2_peer = PeerId::random();

		// Connect V1 peer.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: v1_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Connect V2 peer.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: v2_peer,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		let mut stmt = Statement::new();
		stmt.set_plain_data(b"encoding test".to_vec());

		// Send to V1 peer.
		notification_service.clear_sent_notifications();
		handler.send_statement_chunk(&v1_peer, &[&stmt]).await;
		let v1_sent = notification_service.get_sent_notifications();
		assert_eq!(v1_sent.len(), 1);
		let v1_bytes = &v1_sent[0].1;
		// V1 encoding is raw Vec<Statement>.
		let decoded_v1 = <Statements as Decode>::decode(&mut v1_bytes.as_slice())
			.expect("V1 peer should receive raw Vec<Statement> encoding");
		assert_eq!(decoded_v1.len(), 1);

		// Send to V2 peer.
		notification_service.clear_sent_notifications();
		handler.send_statement_chunk(&v2_peer, &[&stmt]).await;
		let v2_sent = notification_service.get_sent_notifications();
		assert_eq!(v2_sent.len(), 1);
		let v2_bytes = &v2_sent[0].1;
		// V2 encoding is StatementMessage::Statements.
		let decoded_v2 = StatementMessage::decode(&mut v2_bytes.as_slice())
			.expect("V2 peer should receive StatementMessage encoding");
		match decoded_v2 {
			StatementMessage::Statements(stmts) => assert_eq!(stmts.len(), 1),
			_ => panic!("Expected StatementMessage::Statements for V2 peer"),
		}

		// Verify the two encodings are different (V2 has an extra enum discriminant byte).
		assert_ne!(v1_bytes, v2_bytes, "V1 and V2 encodings should differ");
	}

	#[tokio::test]
	async fn test_schedule_initial_sync_replaces_existing() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Add some statements to the store.
		let mut stmt1 = Statement::new();
		stmt1.set_plain_data(b"stmt1".to_vec());
		let hash1 = stmt1.hash();
		statement_store.statements.lock().unwrap().insert(hash1, stmt1);

		// Connect peer as V1.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: Some(format!("/{STATEMENT_PROTOCOL_V1}").into()),
			})
			.await;

		// Should have initial sync scheduled.
		assert!(handler.pending_initial_syncs.contains_key(&peer_id));
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"Peer should appear exactly once in the queue"
		);

		// Add another statement and re-schedule.
		let mut stmt2 = Statement::new();
		stmt2.set_plain_data(b"stmt2".to_vec());
		let hash2 = stmt2.hash();
		statement_store.statements.lock().unwrap().insert(hash2, stmt2);

		handler.schedule_initial_sync_for_peer(peer_id);

		// Peer should still appear exactly once in the queue (no duplicates).
		assert_eq!(
			handler.initial_sync_peer_queue.iter().filter(|p| **p == peer_id).count(),
			1,
			"Peer should NOT be duplicated in the queue after re-schedule"
		);
		// The new sync should contain both hashes.
		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		assert!(pending.hashes.contains(&hash1));
		assert!(pending.hashes.contains(&hash2));
	}

	#[tokio::test]
	async fn test_initial_sync_queued_during_major_sync_processed_after() {
		let statement_store = TestStatementStore::new();
		let (statements_queue_sender, _statements_queue_receiver) = async_channel::bounded(100);
		let (_results_queue_sender, results_queue_receiver) = async_channel::bounded(100);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let sync = TestSync::new();
		// Set major syncing to true.
		sync.major_syncing.store(true, Ordering::Relaxed);

		let mut handler = StatementHandler {
			protocol_name: format!("/{STATEMENT_PROTOCOL_V1}").into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_state: Arc::new(RwLock::new(PendingState::new())),
			results_queue_receiver,
			statements_queue_sender,
			network: network.clone(),
			sync: sync.clone(),
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers: Arc::new(RwLock::new(HashMap::new())),
			statement_store: Arc::new(statement_store.clone()),
			statements_per_second: NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND)
				.expect("DEFAULT_STATEMENTS_PER_SECOND is nonzero"),
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_affinities_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
			pending_sends: FuturesUnordered::new(),
		};

		// Add a statement so there's something to sync.
		let mut stmt = Statement::new();
		stmt.set_plain_data(b"during major sync".to_vec());
		let hash = stmt.hash();
		statement_store.statements.lock().unwrap().insert(hash, stmt);

		// Add a peer manually.
		let peer_id = PeerId::random();
		handler.peers.write().unwrap().insert(
			peer_id,
			Peer::new_for_testing(
				LruHashSet::new(NonZeroUsize::new(100).unwrap()),
				NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).unwrap(),
				NonZeroU32::new(
					DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
				)
				.unwrap(),
			),
		);

		// Scheduling during major sync should queue the peer.
		handler.schedule_initial_sync_for_peer(peer_id);

		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Initial sync should be queued even during major sync"
		);
		assert_eq!(handler.initial_sync_peer_queue.len(), 1);

		// But burst processing should be a no-op while major syncing.
		handler.process_initial_sync_burst().await;
		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Pending sync should remain untouched during major sync"
		);

		// Once major sync completes, burst processing should proceed.
		sync.major_syncing.store(false, Ordering::Relaxed);
		handler.process_initial_sync_burst().await;
		assert!(
			handler.initial_sync_peer_queue.is_empty(),
			"Peer should have been processed after major sync ended"
		);
	}

	#[tokio::test]
	async fn test_schedule_initial_sync_resends_all_matching() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Add statements to the store.
		let mut stmt1 = Statement::new();
		stmt1.set_plain_data(b"known".to_vec());
		let hash1 = stmt1.hash();
		let mut stmt2 = Statement::new();
		stmt2.set_plain_data(b"unknown".to_vec());
		let hash2 = stmt2.hash();

		statement_store.statements.lock().unwrap().insert(hash1, stmt1);
		statement_store.statements.lock().unwrap().insert(hash2, stmt2);

		// Add peer manually with hash1 already known.
		let mut known = LruHashSet::new(NonZeroUsize::new(100).unwrap());
		known.insert(hash1);
		handler.peers.write().unwrap().insert(
			peer_id,
			Peer {
				known_statements: known,
				rate_limiter: PeerRateLimiter::new(
					NonZeroU32::new(DEFAULT_STATEMENTS_PER_SECOND).unwrap(),
					NonZeroU32::new(
						DEFAULT_STATEMENTS_PER_SECOND * config::STATEMENTS_BURST_COEFFICIENT,
					)
					.unwrap(),
				),
				protocol_version: PeerProtocolVersion::V1,
				topic_affinity: None,
				is_light: false,
				pending_topic_affinity: None,
			},
		);

		handler.schedule_initial_sync_for_peer(peer_id);

		let pending = handler.pending_initial_syncs.get(&peer_id).unwrap();
		// all hashes are included for redelivery.
		assert!(
			pending.hashes.contains(&hash1),
			"Previously known hash should be included after affinity change"
		);
		assert!(pending.hashes.contains(&hash2), "Unknown hash should be included in initial sync");
		// known_statements should have been cleared.
		let peers_guard = handler.peers.read().unwrap();
		let peer_data = peers_guard.get(&peer_id).unwrap();
		assert!(
			!peer_data.known_statements.contains(&hash1),
			"known_statements should be cleared after schedule_initial_sync_for_peer"
		);
	}

	#[tokio::test]
	async fn test_malformed_v2_message_does_not_panic() {
		let (mut handler, _statement_store, _network, _notification_service) =
			build_handler_no_peers();

		let peer_id = PeerId::random();

		// Connect peer as V2.
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Send garbage data — should not panic, just log debug.
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: vec![0xFF, 0xFE, 0xFD].into(),
			})
			.await;

		// Send V1-encoded data to V2 peer — also should not panic.
		let mut stmt = Statement::new();
		stmt.set_plain_data(b"v1 encoded".to_vec());
		let v1_encoded = vec![stmt].encode();
		handler
			.handle_notification_event(NotificationEvent::NotificationReceived {
				peer: peer_id,
				notification: v1_encoded.into(),
			})
			.await;

		// If we got here without panic, the test passes.
		assert!(
			handler.peers.read().unwrap().contains_key(&peer_id),
			"Peer should still be connected"
		);
	}

	#[test]
	fn test_find_sendable_chunk_v2_overhead() {
		let v1_max = max_statement_payload_size(V1_ENVELOPE_OVERHEAD);
		let v2_max = max_statement_payload_size(V2_ENVELOPE_OVERHEAD);

		// V2 has strictly less payload space than V1.
		assert!(
			v2_max < v1_max,
			"V2 payload capacity ({v2_max}) should be less than V1 ({v1_max})"
		);
		assert_eq!(v1_max - v2_max, 1, "V2 overhead is exactly 1 byte more than V1");

		// Create enough statements to fill V1 but not V2.
		let stmts: Vec<Statement> = (0..1000)
			.map(|i| {
				let mut s = Statement::new();
				s.set_plain_data(format!("stmt-{i}").into_bytes());
				s
			})
			.collect();
		let refs: Vec<&Statement> = stmts.iter().collect();

		let v1_chunk = find_sendable_chunk(&refs, V1_ENVELOPE_OVERHEAD);
		let v2_chunk = find_sendable_chunk(&refs, V2_ENVELOPE_OVERHEAD);

		// V2 should fit the same or fewer statements.
		let v1_count = match v1_chunk {
			ChunkResult::Send(n) => n,
			_ => panic!("Expected Send for V1"),
		};
		let v2_count = match v2_chunk {
			ChunkResult::Send(n) => n,
			_ => panic!("Expected Send for V2"),
		};
		assert!(
			v2_count <= v1_count,
			"V2 ({v2_count}) should fit at most as many statements as V1 ({v1_count})"
		);
	}

	#[tokio::test]
	async fn test_full_node_v2_gets_initial_sync_immediately() {
		let (mut handler, statement_store, _network, _notification_service) =
			build_handler_no_peers();

		// Add a statement so there's something to sync.
		let mut stmt = Statement::new();
		stmt.set_plain_data(b"full node v2".to_vec());
		let hash = stmt.hash();
		statement_store.statements.lock().unwrap().insert(hash, stmt);

		let peer_id = PeerId::random();

		// Connect as full-node V2 (no fallback, network returns Full role).
		handler
			.handle_notification_event(NotificationEvent::NotificationStreamOpened {
				peer: peer_id,
				direction: sc_network::service::traits::Direction::Inbound,
				handshake: vec![],
				negotiated_fallback: None,
			})
			.await;

		// Full-node V2 peer should get initial sync immediately (not gated).
		assert!(
			handler.pending_initial_syncs.contains_key(&peer_id),
			"Full-node V2 peer should have initial sync scheduled immediately"
		);
		assert_eq!(
			handler.peers.read().unwrap().get(&peer_id).unwrap().protocol_version,
			PeerProtocolVersion::V2
		);
		assert!(!handler.peers.read().unwrap().get(&peer_id).unwrap().is_light);
	}

	#[tokio::test]
	async fn test_propagation_reaches_all_connected_peers() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_queue_receiver,
			_results_queue_sender,
			peer_ids,
		) = build_handler(5);

		// Insert 3 statements into recent_statements for propagation
		let mut expected_hashes = Vec::new();
		for i in 0..3u8 {
			let mut statement = Statement::new();
			statement.set_plain_data(vec![i; 100]);
			let hash = statement.hash();
			expected_hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}
		expected_hashes.sort();

		handler.propagate_statements();
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		// Verify each peer received all 3 statements
		for peer_id in &peer_ids {
			let mut received_hashes = get_peer_hashes(&sent, *peer_id);
			received_hashes.sort();

			assert_eq!(
				received_hashes, expected_hashes,
				"Peer {peer_id} should have received all 3 statements"
			);
		}

		// Recent statements should be drained
		assert!(statement_store.recent_statements.lock().unwrap().is_empty());
	}

	#[tokio::test]
	async fn test_known_statement_filtering_per_peer() {
		let (
			mut handler,
			statement_store,
			_network,
			notification_service,
			_queue_receiver,
			_results_queue_sender,
			peer_ids,
		) = build_handler(3);

		let peer_a = peer_ids[0];
		let peer_b = peer_ids[1];
		let peer_c = peer_ids[2];

		// Create 5 statements
		let mut hashes = Vec::new();
		for i in 0..5u8 {
			let mut statement = Statement::new();
			statement.set_plain_data(vec![i; 100]);
			let hash = statement.hash();
			hashes.push(hash);
			statement_store.recent_statements.lock().unwrap().insert(hash, statement);
		}

		// Pre-populate known_statements: peer_a knows s1,s2; peer_b knows s3; peer_c knows none
		{
			let mut peers_guard = handler.peers.write().unwrap();
			peers_guard.get_mut(&peer_a).unwrap().known_statements.insert(hashes[0]);
			peers_guard.get_mut(&peer_a).unwrap().known_statements.insert(hashes[1]);
			peers_guard.get_mut(&peer_b).unwrap().known_statements.insert(hashes[2]);
		}

		handler.propagate_statements();
		handler.flush_pending_sends().await;

		let sent = notification_service.get_sent_notifications();

		let peer_a_hashes = get_peer_hashes(&sent, peer_a);
		let peer_b_hashes = get_peer_hashes(&sent, peer_b);
		let peer_c_hashes = get_peer_hashes(&sent, peer_c);

		// peer_a already knows s1,s2 → should only get s3,s4,s5
		assert_eq!(peer_a_hashes.len(), 3, "peer_a should get 3 statements");
		assert!(!peer_a_hashes.contains(&hashes[0]), "peer_a already knows s1");
		assert!(!peer_a_hashes.contains(&hashes[1]), "peer_a already knows s2");
		assert!(peer_a_hashes.contains(&hashes[2]));
		assert!(peer_a_hashes.contains(&hashes[3]));
		assert!(peer_a_hashes.contains(&hashes[4]));

		// peer_b already knows s3 → should get s1,s2,s4,s5
		assert_eq!(peer_b_hashes.len(), 4, "peer_b should get 4 statements");
		assert!(!peer_b_hashes.contains(&hashes[2]), "peer_b already knows s3");
		assert!(peer_b_hashes.contains(&hashes[0]));
		assert!(peer_b_hashes.contains(&hashes[1]));
		assert!(peer_b_hashes.contains(&hashes[3]));
		assert!(peer_b_hashes.contains(&hashes[4]));

		// peer_c knows nothing → should get all 5
		let mut sorted_peer_c: Vec<_> = peer_c_hashes.into_iter().collect();
		sorted_peer_c.sort();
		let mut all_hashes = hashes.clone();
		all_hashes.sort();
		assert_eq!(sorted_peer_c, all_hashes, "peer_c should get all 5 statements");
	}
}

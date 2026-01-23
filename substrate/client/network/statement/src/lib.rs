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
use futures::{channel::oneshot, future::FusedFuture, prelude::*, stream::FuturesUnordered};
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
	sync::Arc,
};
use tokio::time::timeout;

pub mod config;

/// A set of statements.
pub type Statements = Vec<Statement>;
/// Future resolving to batch statement import results.
pub type StatementBatchImportFuture = oneshot::Receiver<Vec<(Hash, SubmitResult)>>;

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

struct Metrics {
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
	) -> error::Result<StatementHandler<N, S>> {
		let sync_event_stream = sync.event_stream("statement-handler-sync");
		let (queue_sender, mut queue_receiver) = async_channel::bounded(MAX_PENDING_STATEMENTS);

		let store = statement_store.clone();
		executor(
			async move {
				loop {
					let task: Option<(Vec<Statement>, oneshot::Sender<Vec<(Hash, SubmitResult)>>)> =
						queue_receiver.next().await;
					match task {
						None => return,
						Some((statements, completion)) => {
							let results = store.submit_batch(statements, StatementSource::Network);
							if completion.send(results).is_err() {
								log::debug!(
									target: LOG_TARGET,
									"Error sending batch validation completion"
								);
							}
						},
					}
				}
			}
			.boxed(),
		);

		let handler = StatementHandler {
			protocol_name: self.protocol_name,
			notification_service: self.notification_service,
			propagate_timeout: (Box::pin(interval(PROPAGATE_TIMEOUT))
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			network,
			sync,
			sync_event_stream: sync_event_stream.fuse(),
			peers: HashMap::new(),
			statement_store,
			queue_sender,
			metrics: if let Some(r) = metrics_registry {
				Some(Metrics::register(r)?)
			} else {
				None
			},
			initial_sync_timeout: Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
		};

		Ok(handler)
	}
}

/// Handler for statements. Call [`StatementHandler::run`] to start the processing.
pub struct StatementHandler<
	N: NetworkPeers + NetworkEventStream,
	S: SyncEventStream + sp_consensus::SyncOracle,
> {
	protocol_name: ProtocolName,
	/// Interval at which we call `propagate_statements`.
	propagate_timeout: stream::Fuse<Pin<Box<dyn Stream<Item = ()> + Send>>>,
	/// Pending batch validation tasks.
	pending_statements:
		FuturesUnordered<Pin<Box<dyn Future<Output = Vec<(Hash, SubmitResult)>> + Send>>>,
	/// As multiple peers can send us the same statement, we group
	/// these peers using the statement hash while the statement is
	/// imported. This prevents that we import the same statement
	/// multiple times concurrently.
	pending_statements_peers: HashMap<Hash, HashSet<PeerId>>,
	/// Network service to use to send messages and manage peers.
	network: N,
	/// Syncing service.
	sync: S,
	/// Receiver for syncing-related events.
	sync_event_stream: stream::Fuse<Pin<Box<dyn Stream<Item = SyncEvent> + Send>>>,
	/// Notification service.
	notification_service: Box<dyn NotificationService>,
	// All connected peers
	peers: HashMap<PeerId, Peer>,
	statement_store: Arc<dyn StatementStore>,
	queue_sender:
		async_channel::Sender<(Vec<Statement>, oneshot::Sender<Vec<(Hash, SubmitResult)>>)>,
	/// Prometheus metrics.
	metrics: Option<Metrics>,
	/// Timeout for sending next statement batch during initial sync.
	initial_sync_timeout: Pin<Box<dyn FusedFuture<Output = ()> + Send>>,
	/// Pending initial syncs per peer.
	pending_initial_syncs: HashMap<PeerId, PendingInitialSync>,
	/// Queue for round-robin processing of initial syncs.
	initial_sync_peer_queue: VecDeque<PeerId>,
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
		peers: HashMap<PeerId, Peer>,
		statement_store: Arc<dyn StatementStore>,
		queue_sender: async_channel::Sender<(
			Vec<Statement>,
			oneshot::Sender<Vec<(Hash, SubmitResult)>>,
		)>,
	) -> Self {
		Self {
			protocol_name,
			notification_service,
			propagate_timeout,
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			network,
			sync,
			sync_event_stream,
			peers,
			statement_store,
			queue_sender,
			metrics: None,
			initial_sync_timeout: Box::pin(tokio::time::sleep(INITIAL_SYNC_BURST_INTERVAL).fuse()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
		}
	}

	/// Get mutable access to pending statements for testing/benchmarking.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn pending_statements_mut(
		&mut self,
	) -> &mut FuturesUnordered<Pin<Box<dyn Future<Output = Vec<(Hash, SubmitResult)>> + Send>>> {
		&mut self.pending_statements
	}

	/// Turns the [`StatementHandler`] into a future that should run forever and not be
	/// interrupted.
	pub async fn run(mut self) {
		loop {
			futures::select_biased! {
				_ = self.propagate_timeout.next() => {
					self.propagate_statements().await;
					self.metrics.as_ref().map(|metrics| {
						metrics.pending_statements.set(self.pending_statements.len() as u64);
					});
				},
				results = self.pending_statements.select_next_some() => {
					for (hash, result) in results {
						if let Some(peers) = self.pending_statements_peers.remove(&hash) {
							peers.into_iter().for_each(|p| self.on_handle_statement_import(p, &result));
						} else {
							log::warn!(target: LOG_TARGET, "Inconsistent state, no peers for pending statement!");
						}
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
	async fn send_statement_chunk(
		&mut self,
		peer: &PeerId,
		statements: &[&Statement],
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

				let _was_in = self.peers.insert(
					peer,
					Peer {
						known_statements: LruHashSet::new(
							NonZeroUsize::new(MAX_KNOWN_STATEMENTS).expect("Constant is nonzero"),
						),
						role,
					},
				);
				debug_assert!(_was_in.is_none());

				if !self.sync.is_major_syncing() && !role.is_light() {
					let hashes = self.statement_store.statement_hashes();
					if !hashes.is_empty() {
						self.pending_initial_syncs.insert(peer, PendingInitialSync { hashes });
						self.initial_sync_peer_queue.push_back(peer);
					}
				}
			},
			NotificationEvent::NotificationStreamClosed { peer } => {
				let _peer = self.peers.remove(&peer);
				debug_assert!(_peer.is_some());
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

				if let Ok(statements) = <Statements as Decode>::decode(&mut notification.as_ref()) {
					self.on_statements(peer, statements);
				} else {
					log::debug!(target: LOG_TARGET, "Failed to decode statement list from {peer}");
				}
			},
		}
	}

	/// Called when peer sends us new statements
	#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
	pub fn on_statements(&mut self, who: PeerId, statements: Statements) {
		log::trace!(target: LOG_TARGET, "Received {} statements from {}", statements.len(), who);

		let Some(ref mut peer) = self.peers.get_mut(&who) else {
			return;
		};

		// Filter statements and collect those that need validation
		let mut to_validate = Vec::new();
		let mut statements_left = statements.len() as u64;

		for s in statements {
			if self.pending_statements.len() + to_validate.len() > MAX_PENDING_STATEMENTS {
				log::debug!(
					target: LOG_TARGET,
					"Ignoring {} statements that exceed `MAX_PENDING_STATEMENTS`({}) limit",
					statements_left,
					MAX_PENDING_STATEMENTS,
				);
				self.metrics.as_ref().map(|metrics| {
					metrics.ignored_statements.inc_by(statements_left);
				});
				break;
			}

			let hash = s.hash();
			peer.known_statements.insert(hash);
			statements_left -= 1;
			self.network.report_peer(who, rep::ANY_STATEMENT);

			// Skip statements that are already in the store
			if self.statement_store.has_statement(&hash) {
				self.metrics.as_ref().map(|metrics| {
					metrics.known_statements_received.inc();
				});

				if let Some(peers) = self.pending_statements_peers.get(&hash) {
					if peers.contains(&who) {
						log::trace!(
							target: LOG_TARGET,
							"Already received the statement from the same peer {who}.",
						);
						self.network.report_peer(who, rep::DUPLICATE_STATEMENT);
					}
				}
				continue;
			}

			// Track pending statement
			match self.pending_statements_peers.entry(hash) {
				Entry::Vacant(entry) => {
					entry.insert(HashSet::from_iter([who]));
					to_validate.push(s);
				},
				Entry::Occupied(mut entry) => {
					if !entry.get_mut().insert(who) {
						// Already received this from the same peer.
						self.network.report_peer(who, rep::DUPLICATE_STATEMENT);
					}
				},
			}
		}

		// Submit statements in batches
		for batch in to_validate.chunks(BATCH_SIZE) {
			let (completion_sender, completion_receiver) = oneshot::channel();
			match self.queue_sender.try_send((batch.to_vec(), completion_sender)) {
				Ok(()) => {
					self.pending_statements.push(
						async move { completion_receiver.await.unwrap_or_else(|_| Vec::new()) }
							.boxed(),
					);
				},
				Err(err) => {
					log::debug!(
						target: LOG_TARGET,
						"Dropped batch of {} statements because of the error with validation channel: {err}",
						batch.len(),
					);
					// Remove from pending as they won't be validated
					for s in batch {
						self.pending_statements_peers.remove(&s.hash());
					}
				},
			}
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
	pub async fn propagate_statement(&mut self, hash: &Hash) {
		// Accept statements only when node is not major syncing
		if self.sync.is_major_syncing() {
			return
		}

		log::debug!(target: LOG_TARGET, "Propagating statement [{:?}]", hash);
		if let Ok(Some(statement)) = self.statement_store.statement(hash) {
			self.do_propagate_statements(&[(*hash, statement)]).await;
		}
	}

	/// Propagate the given `statements` to the given `peer`.
	///
	/// Internally filters `statements` to only send unknown statements to the peer.
	async fn send_statements_to_peer(&mut self, who: &PeerId, statements: &[(Hash, Statement)]) {
		let Some(peer) = self.peers.get_mut(who) else {
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

		log::trace!(target: LOG_TARGET, "We have {} statements that the peer doesn't know about", to_send.len());

		if to_send.is_empty() {
			return
		}

		self.send_statements_in_chunks(who, &to_send).await;
	}

	/// Send statements to a peer in chunks, respecting the maximum notification size.
	async fn send_statements_in_chunks(&mut self, who: &PeerId, statements: &[&Statement]) {
		let mut offset = 0;
		while offset < statements.len() {
			match self.send_statement_chunk(who, &statements[offset..]).await {
				SendChunkResult::Sent(chunk_end) => {
					offset += chunk_end;
				},
				SendChunkResult::Skipped => {
					offset += 1;
				},
				SendChunkResult::Empty | SendChunkResult::Failed => return,
			}
		}
	}

	async fn do_propagate_statements(&mut self, statements: &[(Hash, Statement)]) {
		log::debug!(target: LOG_TARGET, "Propagating {} statements for {} peers", statements.len(), self.peers.len());
		let peers: Vec<_> = self.peers.keys().copied().collect();
		for who in peers {
			log::trace!(target: LOG_TARGET, "Start propagating statements for {}", who);
			self.send_statements_to_peer(&who, statements).await;
		}
		log::trace!(target: LOG_TARGET, "Statements propagated to all peers");
	}

	/// Call when we must propagate ready statements to peers.
	async fn propagate_statements(&mut self) {
		// Send out statements only when node is not major syncing
		if self.sync.is_major_syncing() {
			return
		}

		let Ok(statements) = self.statement_store.take_recent_statements() else { return };
		if !statements.is_empty() {
			self.do_propagate_statements(&statements).await;
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
				if let Some(peer) = self.peers.get_mut(&peer_id) {
					for (hash, _) in &statements {
						peer.known_statements.insert(*hash);
					}
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

	#[derive(Debug, Clone)]
	struct TestNotificationService {
		sent_notifications: Arc<Mutex<Vec<(PeerId, Vec<u8>)>>>,
	}

	impl TestNotificationService {
		fn new() -> Self {
			Self { sent_notifications: Arc::new(Mutex::new(Vec::new())) }
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
			self.sent_notifications.lock().unwrap().push((*peer, notification));
		}

		async fn send_async_notification(
			&mut self,
			peer: &PeerId,
			notification: Vec<u8>,
		) -> Result<(), sc_network::error::Error> {
			self.sent_notifications.lock().unwrap().push((*peer, notification));
			Ok(())
		}

		async fn set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		fn try_set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
			unimplemented!()
		}

		async fn next_event(&mut self) -> Option<sc_network::service::traits::NotificationEvent> {
			None
		}

		fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
			unimplemented!()
		}

		fn protocol(&self) -> &sc_network::types::ProtocolName {
			unimplemented!()
		}

		fn message_sink(
			&self,
			_peer: &PeerId,
		) -> Option<Box<dyn sc_network::service::traits::MessageSink>> {
			unimplemented!()
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
			_statement: sp_statement_store::Statement,
			_source: sp_statement_store::StatementSource,
		) -> sp_statement_store::SubmitResult {
			unimplemented!()
		}

		fn submit_batch(
			&self,
			statements: Vec<Statement>,
			_source: StatementSource,
		) -> Vec<(Hash, SubmitResult)> {
			statements
				.into_iter()
				.map(|s| {
					let hash = s.hash();
					self.statements.lock().unwrap().insert(hash, s);
					(hash, SubmitResult::New)
				})
				.collect()
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
		async_channel::Receiver<(Vec<Statement>, oneshot::Sender<Vec<(Hash, SubmitResult)>>)>,
	) {
		let statement_store = TestStatementStore::new();
		let (queue_sender, queue_receiver) = async_channel::bounded(2);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();
		let peer_id = PeerId::random();
		let mut peers = HashMap::new();
		peers.insert(
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
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			network: network.clone(),
			sync: TestSync {},
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers,
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
		};
		(handler, statement_store, network, notification_service, queue_receiver)
	}

	#[tokio::test]
	async fn test_skips_processing_statements_that_already_in_store() {
		let (mut handler, statement_store, _network, _notification_service, queue_receiver) =
			build_handler();

		let mut statement1 = Statement::new();
		statement1.set_plain_data(b"statement1".to_vec());
		let hash1 = statement1.hash();

		statement_store.statements.lock().unwrap().insert(hash1, statement1.clone());

		let mut statement2 = Statement::new();
		statement2.set_plain_data(b"statement2".to_vec());
		let hash2 = statement2.hash();

		let peer_id = *handler.peers.keys().next().unwrap();

		handler.on_statements(peer_id, vec![statement1, statement2]);

		let to_submit = queue_receiver.try_recv();
		let batch = to_submit.unwrap().0;
		assert_eq!(batch.len(), 1, "Expected only one statement in batch");
		assert_eq!(batch[0].hash(), hash2, "Expected only statement2 to be queued");

		let no_more = queue_receiver.try_recv();
		assert!(no_more.is_err(), "Expected only one batch to be queued");
	}

	#[tokio::test]
	async fn test_reports_for_duplicate_statements() {
		let (mut handler, statement_store, network, _notification_service, queue_receiver) =
			build_handler();

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut statement1 = Statement::new();
		statement1.set_plain_data(b"statement1".to_vec());

		handler.on_statements(peer_id, vec![statement1.clone()]);
		{
			// Manually process statements batch submission
			let (batch, _) = queue_receiver.try_recv().unwrap();
			for s in batch {
				let _ = statement_store.statements.lock().unwrap().insert(s.hash(), s);
			}
			handler.network.report_peer(peer_id, rep::ANY_STATEMENT_REFUND);
		}

		handler.on_statements(peer_id, vec![statement1]);

		let reports = network.get_reports();
		assert_eq!(
			reports,
			vec![
				(peer_id, rep::ANY_STATEMENT),        // Report for first statement
				(peer_id, rep::ANY_STATEMENT_REFUND), // Refund for first statement
				(peer_id, rep::DUPLICATE_STATEMENT)   // Report for duplicate statement
			],
			"Expected ANY_STATEMENT, ANY_STATEMENT_REFUND, DUPLICATE_STATEMENT reputation change, but got: {:?}",
			reports
		);
	}

	#[tokio::test]
	async fn test_splits_large_batches_into_smaller_chunks() {
		let (mut handler, statement_store, _network, notification_service, _queue_receiver) =
			build_handler();

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

		handler.propagate_statements().await;

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
		let (mut handler, statement_store, _network, notification_service, _queue_receiver) =
			build_handler();

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

		handler.propagate_statements().await;

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
		let (queue_sender, _queue_receiver) = async_channel::bounded(2);
		let network = TestNetwork::new();
		let notification_service = TestNotificationService::new();

		let handler = StatementHandler {
			protocol_name: "/statement/1".into(),
			notification_service: Box::new(notification_service.clone()),
			propagate_timeout: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = ()> + Send>>)
				.fuse(),
			pending_statements: FuturesUnordered::new(),
			pending_statements_peers: HashMap::new(),
			network: network.clone(),
			sync: TestSync {},
			sync_event_stream: (Box::pin(futures::stream::pending())
				as Pin<Box<dyn Stream<Item = sc_network_sync::types::SyncEvent> + Send>>)
				.fuse(),
			peers: HashMap::new(),
			statement_store: Arc::new(statement_store.clone()),
			queue_sender,
			metrics: None,
			initial_sync_timeout: Box::pin(futures::future::pending()),
			pending_initial_syncs: HashMap::new(),
			initial_sync_peer_queue: VecDeque::new(),
		};
		(handler, statement_store, network, notification_service)
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
		assert!(handler.peers.contains_key(&peer_id));
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
		assert_eq!(handler.peers.len(), 3);
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
		let (mut handler, statement_store, _network, notification_service, _queue_receiver) =
			build_handler();

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

		handler.propagate_statements().await;

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
}

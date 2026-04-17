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

use codec::{Decode, Encode};
use futures::{channel::oneshot, prelude::*, stream::FuturesUnordered, FutureExt};
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
	NetworkBackend, NetworkEventStream, NetworkPeers,
};
use sc_network_common::role::ObservedRole;
use sc_network_sync::{SyncEvent, SyncEventStream};
use sc_network_types::PeerId;
use sp_runtime::traits::Block as BlockT;
use sp_statement_store::{
	Hash, NetworkPriority, Statement, StatementSource, StatementStore, SubmitResult,
};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	iter,
	num::NonZeroUsize,
	pin::Pin,
	sync::Arc,
};
use tokio::time::timeout;

pub mod config;

/// A set of statements.
pub type Statements = Vec<Statement>;
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
	pub const GOOD_STATEMENT: Rep = Rep::new(1 << 7, "Good statement");
	/// Reputation change when a peer sends us a bad statement.
	pub const BAD_STATEMENT: Rep = Rep::new(-(1 << 12), "Bad statement");
	/// Reputation change when a peer sends us a duplicate statement.
	pub const DUPLICATE_STATEMENT: Rep = Rep::new(-(1 << 7), "Duplicate statement");
	/// Reputation change when a peer sends us particularly useful statement
	pub const EXCELLENT_STATEMENT: Rep = Rep::new(1 << 8, "High priority statement");
}

const LOG_TARGET: &str = "statement-gossip";
/// Maximim time we wait for sending a notification to a peer.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
					let task: Option<(Statement, oneshot::Sender<SubmitResult>)> =
						queue_receiver.next().await;
					match task {
						None => return,
						Some((statement, completion)) => {
							let result = store.submit(statement, StatementSource::Network);
							if completion.send(result).is_err() {
								log::debug!(
									target: LOG_TARGET,
									"Error sending validation completion"
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
	/// Pending statements verification tasks.
	pending_statements:
		FuturesUnordered<Pin<Box<dyn Future<Output = (Hash, Option<SubmitResult>)> + Send>>>,
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
	queue_sender: async_channel::Sender<(Statement, oneshot::Sender<SubmitResult>)>,
	/// Prometheus metrics.
	metrics: Option<Metrics>,
}

/// Peer information
#[derive(Debug)]
struct Peer {
	/// Holds a set of statements known to this peer.
	known_statements: LruHashSet<Hash>,
	role: ObservedRole,
}

impl<N, S> StatementHandler<N, S>
where
	N: NetworkPeers + NetworkEventStream,
	S: SyncEventStream + sp_consensus::SyncOracle,
{
	/// Turns the [`StatementHandler`] into a future that should run forever and not be
	/// interrupted.
	pub async fn run(mut self) {
		loop {
			futures::select! {
				_ = self.propagate_timeout.next() => {
					self.propagate_statements().await;
					self.metrics.as_ref().map(|metrics| {
						metrics.pending_statements.set(self.pending_statements.len() as u64);
					});
				},
				(hash, result) = self.pending_statements.select_next_some() => {
					if let Some(peers) = self.pending_statements_peers.remove(&hash) {
						if let Some(result) = result {
							peers.into_iter().for_each(|p| self.on_handle_statement_import(p, &result));
						}
					} else {
						log::warn!(target: LOG_TARGET, "Inconsistent state, no peers for pending statement!");
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
						self.handle_notification_event(event)
					} else {
						// `Notifications` has seemingly closed. Closing as well.
						return
					}
				}
			}
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

	fn handle_notification_event(&mut self, event: NotificationEvent) {
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
			},
			NotificationEvent::NotificationStreamClosed { peer } => {
				let _peer = self.peers.remove(&peer);
				debug_assert!(_peer.is_some());
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
	fn on_statements(&mut self, who: PeerId, statements: Statements) {
		log::trace!(target: LOG_TARGET, "Received {} statements from {}", statements.len(), who);
		if let Some(ref mut peer) = self.peers.get_mut(&who) {
			let mut statements_left = statements.len() as u64;
			for s in statements {
				if self.pending_statements.len() > MAX_PENDING_STATEMENTS {
					log::debug!(
						target: LOG_TARGET,
						"Ignoring {} statements that exceed `MAX_PENDING_STATEMENTS`({}) limit",
						statements_left,
						MAX_PENDING_STATEMENTS,
					);
					self.metrics.as_ref().map(|metrics| {
						metrics.ignored_statements.inc_by(statements_left);
					});
					break
				}

				let hash = s.hash();
				peer.known_statements.insert(hash);

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

				self.network.report_peer(who, rep::ANY_STATEMENT);

				match self.pending_statements_peers.entry(hash) {
					Entry::Vacant(entry) => {
						let (completion_sender, completion_receiver) = oneshot::channel();
						match self.queue_sender.try_send((s, completion_sender)) {
							Ok(()) => {
								self.pending_statements.push(
									async move {
										let res = completion_receiver.await;
										(hash, res.ok())
									}
									.boxed(),
								);
								entry.insert(HashSet::from_iter([who]));
							},
							Err(async_channel::TrySendError::Full(_)) => {
								log::debug!(
									target: LOG_TARGET,
									"Dropped statement because validation channel is full",
								);
							},
							Err(async_channel::TrySendError::Closed(_)) => {
								log::trace!(
									target: LOG_TARGET,
									"Dropped statement because validation channel is closed",
								);
							},
						}
					},
					Entry::Occupied(mut entry) => {
						if !entry.get_mut().insert(who) {
							// Already received this from the same peer.
							self.network.report_peer(who, rep::DUPLICATE_STATEMENT);
						}
					},
				}

				statements_left -= 1;
			}
		}
	}

	fn on_handle_statement_import(&mut self, who: PeerId, import: &SubmitResult) {
		match import {
			SubmitResult::New(NetworkPriority::High) =>
				self.network.report_peer(who, rep::EXCELLENT_STATEMENT),
			SubmitResult::New(NetworkPriority::Low) =>
				self.network.report_peer(who, rep::GOOD_STATEMENT),
			SubmitResult::Known => self.network.report_peer(who, rep::ANY_STATEMENT_REFUND),
			SubmitResult::KnownExpired => {},
			SubmitResult::Ignored => {},
			SubmitResult::Bad(_) => self.network.report_peer(who, rep::BAD_STATEMENT),
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

	async fn do_propagate_statements(&mut self, statements: &[(Hash, Statement)]) {
		log::debug!(target: LOG_TARGET, "Propagating {} statements for {} peers", statements.len(), self.peers.len());
		for (who, peer) in self.peers.iter_mut() {
			log::trace!(target: LOG_TARGET, "Start propagating statements for {}", who);

			// never send statements to light nodes
			if peer.role.is_light() {
				log::trace!(target: LOG_TARGET, "{} is a light node, skipping propagation", who);
				continue
			}

			let to_send = statements
				.iter()
				.filter_map(|(hash, stmt)| peer.known_statements.insert(*hash).then(|| stmt))
				.collect::<Vec<_>>();
			log::trace!(target: LOG_TARGET, "We have {} statements that the peer doesn't know about", to_send.len());

			let mut offset = 0;
			while offset < to_send.len() {
				// Try to send as many statements as possible in one notification
				let mut current_end = to_send.len();
				log::trace!(target: LOG_TARGET, "Looking for better chunk size");

				loop {
					let chunk = &to_send[offset..current_end];
					let encoded_size = chunk.encoded_size();
					log::trace!(target: LOG_TARGET, "Chunk: {} statements, {} KB", chunk.len(), encoded_size / 1024);

					// If chunk fits, send it
					if encoded_size <= MAX_STATEMENT_NOTIFICATION_SIZE as usize {
						if let Err(e) = timeout(
							SEND_TIMEOUT,
							self.notification_service.send_async_notification(who, chunk.encode()),
						)
						.await
						{
							log::debug!(target: LOG_TARGET, "Failed to send notification to {}, peer disconnected, skipping further batches: {:?}", who, e);
							offset = to_send.len();
							break;
						}
						offset = current_end;
						log::trace!(target: LOG_TARGET, "Sent {} statements ({} KB) to {}, {} left", chunk.len(), encoded_size / 1024, who, to_send.len() - offset);
						self.metrics.as_ref().map(|metrics| {
							metrics.propagated_statements.inc_by(chunk.len() as u64);
							metrics.propagated_statements_chunks.observe(chunk.len() as f64);
						});
						break;
					}

					// Size exceeded - split the chunk
					let split_factor =
						(encoded_size / MAX_STATEMENT_NOTIFICATION_SIZE as usize) + 1;
					let mut new_chunk_size = (current_end - offset) / split_factor;

					// Single statement is too large
					if new_chunk_size == 0 {
						if chunk.len() == 1 {
							log::warn!(target: LOG_TARGET, "Statement too large ({} KB), skipping", encoded_size / 1024);
							self.metrics.as_ref().map(|metrics| {
								metrics.skipped_oversized_statements.inc();
							});
							offset = current_end;
							break;
						}
						// Don't skip more than one statement at once
						new_chunk_size = 1;
					}

					// Reduce chunk size and try again
					current_end = offset + new_chunk_size;
				}
			}
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
}

#[cfg(test)]
mod tests {

	use super::*;
	use std::sync::Mutex;

	#[derive(Clone)]
	struct TestNetwork {
		reported_peers: Arc<Mutex<Vec<(PeerId, sc_network::ReputationChange)>>>,
	}

	impl TestNetwork {
		fn new() -> Self {
			Self { reported_peers: Arc::new(Mutex::new(Vec::new())) }
		}

		fn get_reports(&self) -> Vec<(PeerId, sc_network::ReputationChange)> {
			self.reported_peers.lock().unwrap().clone()
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

		fn peer_role(&self, _: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
			unimplemented!()
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
		async_channel::Receiver<(Statement, oneshot::Sender<SubmitResult>)>,
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
		};
		(handler, statement_store, network, notification_service, queue_receiver)
	}

	#[test]
	fn test_skips_processing_statements_that_already_in_store() {
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
		assert_eq!(to_submit.unwrap().0.hash(), hash2, "Expected only statement2 to be queued");

		let no_more = queue_receiver.try_recv();
		assert!(no_more.is_err(), "Expected only one statement to be queued");
	}

	#[test]
	fn test_reports_for_duplicate_statements() {
		let (mut handler, statement_store, network, _notification_service, queue_receiver) =
			build_handler();

		let peer_id = *handler.peers.keys().next().unwrap();

		let mut statement1 = Statement::new();
		statement1.set_plain_data(b"statement1".to_vec());

		handler.on_statements(peer_id, vec![statement1.clone()]);
		{
			// Manually process statements submission
			let (s, _) = queue_receiver.try_recv().unwrap();
			let _ = statement_store.statements.lock().unwrap().insert(s.hash(), s);
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
}

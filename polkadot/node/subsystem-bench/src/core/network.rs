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
//!
//! Implements network emulation and interfaces to control and specialize
//! network peer behaviour.

use super::{
	configuration::{TestAuthorities, TestConfiguration},
	environment::TestEnvironmentDependencies,
	*,
};
use colored::Colorize;
use futures::{
	channel::{mpsc, oneshot},
	future::FusedFuture,
	lock::Mutex,
	stream::FuturesUnordered,
};
use net_protocol::{
	request_response::{OutgoingRequest, Requests},
	VersionedValidationProtocol,
};
use parity_scale_codec::Encode;
use polkadot_primitives::AuthorityDiscoveryId;
use prometheus_endpoint::U64;
use rand::{seq::SliceRandom, thread_rng};
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	RequestFailure,
};
use sc_service::SpawnTaskHandle;
use std::{
	collections::HashMap,
	ops::DerefMut,
	pin::Pin,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};

use polkadot_node_network_protocol::{
	self as net_protocol,
	peer_set::{ProtocolVersion, ValidationVersion},
	v1 as protocol_v1, v2 as protocol_v2, vstaging as protocol_vstaging, OurView, PeerId,
	UnifiedReputationChange as Rep, Versioned, View,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

use futures::{Future, FutureExt, Stream, StreamExt};
// An emulated node egress traffic rate_limiter.
#[derive(Debug)]
pub struct RateLimit {
	// How often we refill credits in buckets
	tick_rate: usize,
	// Total ticks
	total_ticks: usize,
	// Max refill per tick
	max_refill: usize,
	// Available credit. We allow for bursts over 1/tick_rate of `cps` budget, but we
	// account it by negative credit.
	credits: isize,
	// When last refilled.
	last_refill: Instant,
}

impl RateLimit {
	// Create a new `RateLimit` from a `cps` (credits per second) budget and
	// `tick_rate`.
	pub fn new(tick_rate: usize, cps: usize) -> Self {
		// Compute how much refill for each tick
		let max_refill = cps / tick_rate;
		RateLimit {
			tick_rate,
			total_ticks: 0,
			max_refill,
			// A fresh start
			credits: max_refill as isize,
			last_refill: Instant::now(),
		}
	}

	pub async fn refill(&mut self) {
		// If this is called to early, we need to sleep until next tick.
		let now = Instant::now();
		let next_tick_delta =
			(self.last_refill + Duration::from_millis(1000 / self.tick_rate as u64)) - now;

		// Sleep until next tick.
		if !next_tick_delta.is_zero() {
			gum::trace!(target: LOG_TARGET, "need to sleep {}ms", next_tick_delta.as_millis());
			tokio::time::sleep(next_tick_delta).await;
		}

		self.total_ticks += 1;
		self.credits += self.max_refill as isize;
		self.last_refill = Instant::now();
	}

	// Reap credits from the bucket.
	// Blocks if credits budged goes negative during call.
	pub async fn reap(&mut self, amount: usize) {
		self.credits -= amount as isize;

		if self.credits >= 0 {
			return
		}

		while self.credits < 0 {
			gum::trace!(target: LOG_TARGET, "Before refill: {:?}", &self);
			self.refill().await;
			gum::trace!(target: LOG_TARGET, "After refill: {:?}", &self);
		}
	}
}

/// A wrapper for both gossip and request/response protocols along with the destination
/// peer(`AuthorityDiscoveryId``).
pub enum PeerMessage {
	/// A gossip message
	Message(AuthorityDiscoveryId, VersionedValidationProtocol),
	/// A request originating from our node
	RequestFromNode(AuthorityDiscoveryId, Requests),
	/// A request originating from an emultated peer
	RequestFromPeer(IncomingRequest),
}

impl PeerMessage {
	/// Returns the size of the encoded message or request
	pub fn size(&self) -> usize {
		match &self {
			PeerMessage::Message(_peer_id, Versioned::V2(message)) => message.encoded_size(),
			PeerMessage::Message(_peer_id, Versioned::V1(message)) => message.encoded_size(),
			PeerMessage::Message(_peer_id, Versioned::VStaging(message)) => message.encoded_size(),
			PeerMessage::RequestFromNode(_peer_id, incoming) => request_size(incoming),
			PeerMessage::RequestFromPeer(request) => request.payload.encoded_size(),
		}
	}

	/// Returns the destination peer from the message
	pub fn peer(&self) -> Option<&AuthorityDiscoveryId> {
		match &self {
			PeerMessage::Message(peer_id, _) | PeerMessage::RequestFromNode(peer_id, _) =>
				Some(peer_id),
			_ => None,
		}
	}
}

/// A network interface of the node under test.
/// TODO(soon): Implement latency and connection errors here, instead of doing it on the peers.
pub struct NetworkInterface {
	// Used to receive traffic from the `network-bridge-tx` subsystem.
	// The network action is forwarded via `network` to the relevant peer(s).
	// from_netowork_bridge: Receiver<NetworkAction>,
	// Sender for subsystems.
	bridge_to_interface_sender: UnboundedSender<PeerMessage>,
	// A sender to forward actions to the network bridge subsystem.
	interface_to_bridge_sender: UnboundedSender<PeerMessage>,
}

// Wraps the receiving side of a interface to bridge channel. It is a required
// parameter of the `network-bridge` mock.
pub struct NetworkInterfaceReceiver(pub UnboundedReceiver<PeerMessage>);

struct ProxiedRequest {
	sender: Option<oneshot::Sender<OutgoingResponse>>,
	receiver: oneshot::Receiver<OutgoingResponse>,
}

struct ProxiedResponse {
	pub sender: oneshot::Sender<OutgoingResponse>,
	pub result: Result<Vec<u8>, RequestFailure>,
}

use std::task::Poll;

impl Future for ProxiedRequest {
	// The sender and result.
	type Output = ProxiedResponse;

	fn poll(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Self::Output> {
		match self.receiver.poll_unpin(cx) {
			Poll::Pending => Poll::Pending,
			Poll::Ready(response) => Poll::Ready(ProxiedResponse {
				sender: self.sender.take().expect("sender already used"),
				result: response
					.expect("Response is always succesfully received.")
					.result
					.map_err(|_| RequestFailure::Refused),
			}),
		}
	}
}

impl NetworkInterface {
	/// Create a new `NetworkInterface`
	pub fn new(
		spawn_task_handle: SpawnTaskHandle,
		mut network: NetworkEmulatorHandle,
		bandiwdth_bps: usize,
		mut from_network: UnboundedReceiver<PeerMessage>,
	) -> (NetworkInterface, NetworkInterfaceReceiver) {
		let mut rx_limiter = RateLimit::new(10, bandiwdth_bps);
		// We need to share the transimit limiter as we handle incoming request/response on rx
		// thread.
		let mut tx_limiter = Arc::new(Mutex::new(RateLimit::new(10, bandiwdth_bps)));
		let mut proxied_requests = FuturesUnordered::new();

		// The sender (`egress_tx`) is
		let (bridge_to_interface_sender, mut bridge_to_interface_receiver) =
			mpsc::unbounded::<PeerMessage>();

		// Channel for forwarding actions to the bridge.
		let (interface_to_bridge_sender, interface_to_bridge_receiver) =
			mpsc::unbounded::<PeerMessage>();

		let mut rx_network = network.clone();
		let mut tx_network = network;

		let rx_task_bridge_sender = interface_to_bridge_sender.clone();
		let rx_task_tx_limiter = tx_limiter.clone();
		let tx_task_tx_limiter = tx_limiter;

		// Spawn the network interface task.
		let rx_task = async move {
			loop {
				let mut from_network = from_network.next().fuse();
				futures::select! {
					maybe_peer_message = from_network => {
						if let Some(peer_message) = maybe_peer_message {
							let size = peer_message.size();
							// TODO (maybe): Implement limiter as part of something like `RateLimitedChannel`.
							rx_limiter.reap(size).await;
							rx_network.inc_received(size);

							// We act as an incoming request proxy, so we'll craft a new request and wait for
							// the answer from subsystem, and only then, after ratelimiting we send back a response
							// to the peer
							if let PeerMessage::RequestFromPeer(request) = peer_message {
								let (response_sender, response_receiver) = oneshot::channel();
								// Create a new `IncomingRequest` that we forward to the network bridge.
								let new_request = IncomingRequest {payload: request.payload, peer: request.peer, pending_response: response_sender};

								proxied_requests.push(ProxiedRequest {sender: Some(request.pending_response), receiver: response_receiver});
								// Send the new message to network bridge rx
								rx_task_bridge_sender
									.unbounded_send(PeerMessage::RequestFromPeer(new_request))
									.expect("network bridge subsystem is alive");
								continue
							}

							// Forward the message to the bridge.
							rx_task_bridge_sender
								.unbounded_send(peer_message)
								.expect("network bridge subsystem is alive");
						} else {
							gum::info!(target: LOG_TARGET, "Uplink channel closed, network interface task exiting");
							break
						}
					},
					proxied_request = proxied_requests.next() => {
						if let Some(proxied_request) = proxied_request {
							match proxied_request.result {
								Ok(result) => {
									let bytes = result.encoded_size();
									gum::trace!(target: LOG_TARGET, size = bytes, "proxied request completed");

									rx_task_tx_limiter.lock().await.reap(bytes).await;
									rx_network.inc_sent(bytes);

									proxied_request.sender.send(
										OutgoingResponse {
											reputation_changes: Vec::new(),
											result: Ok(result),
											sent_feedback: None
										}
									).expect("network is alive");
								}
								Err(e) => {
									gum::warn!(target: LOG_TARGET, "Node req/response failure: {:?}", e)
								}
							}
						} else {
							gum::debug!(target: LOG_TARGET, "No more active proxied requests");
							// break
						}
					}
				}
			}
		}
		.boxed();

		let tx_task = async move {
			loop {
				if let Some(peer_message) = bridge_to_interface_receiver.next().await {
					let size = peer_message.size();
					tx_task_tx_limiter.lock().await.reap(size).await;
					let dst_peer = peer_message
						.peer()
						.expect(
							"Node always needs to specify destination peer when sending a message",
						)
						.clone();
					tx_network.submit_peer_message(&dst_peer, peer_message);
					tx_network.inc_sent(size);
				} else {
					gum::info!(target: LOG_TARGET, "Downlink channel closed, network interface task exiting");
					break
				}
			}
		}
		.boxed();

		spawn_task_handle.spawn("network-interface-rx", "test-environment", rx_task);
		spawn_task_handle.spawn("network-interface-tx", "test-environment", tx_task);

		(
			Self { bridge_to_interface_sender, interface_to_bridge_sender },
			NetworkInterfaceReceiver(interface_to_bridge_receiver),
		)
	}

	/// Get a sender that can be used by a subsystem to send network actions to the network.
	pub fn subsystem_sender(&self) -> UnboundedSender<PeerMessage> {
		self.bridge_to_interface_sender.clone()
	}

	/// Get a sender that can be used by the Network to send network actions to the network.
	pub fn network_sender(&self) -> UnboundedSender<PeerMessage> {
		self.interface_to_bridge_sender.clone()
	}
}

/// An emulated peer network interface.
#[derive(Debug)]
pub struct PeerNetworkInterface {
	/// Receive rate limiter.
	rx_limiter: RateLimit,
	/// Transmit rate limiter
	tx_limiter: RateLimit,
	/// Network receive queue.
	/// This is paired to a `Sender` in `NetworkEmulator`.
	/// The network interface task will forward messages/requests from the node over
	/// this channel.
	rx_queue: UnboundedReceiver<NetworkAction>,
	/// Network send queue.
	/// Paired to the `rx_queue` receiver on the `NetworkInterface`.
	tx_queue: UnboundedSender<NetworkAction>,
}

/// A handle send messages and actions to an emulated peer.
#[derive(Clone)]
pub struct EmulatedPeerHandle {
	/// Send messages to the peer emulator task
	messages_tx: UnboundedSender<PeerMessage>,
	/// Send actions to the peer emulator task
	actions_tx: UnboundedSender<NetworkAction>,
}

impl EmulatedPeerHandle {
	/// Send a message to the peer.
	pub fn send_message(&self, message: PeerMessage) {
		let _ = self
			.messages_tx
			.unbounded_send(message)
			.expect("Sending message to the peer never fails");
	}

	/// Send a message to the peer.
	pub fn send_action(&self, action: NetworkAction) {
		let _ = self
			.actions_tx
			.unbounded_send(action)
			.expect("Sending action to the peer never fails");
	}
}

/// A network peer emulator. Receives `PeerMessages` and `NetworkActions`. Tipically
/// these actions send a message to the node under test.
pub struct EmulatedPeer {
	to_node: UnboundedSender<PeerMessage>,
	tx_limiter: RateLimit,
	rx_limiter: RateLimit,
}

impl EmulatedPeer {
	pub async fn send_message(&mut self, message: PeerMessage) {
		self.tx_limiter.reap(message.size()).await;
		let _ = self.to_node.unbounded_send(message).expect("Sending to the node never fails");
	}

	pub fn rx_limiter(&mut self) -> &mut RateLimit {
		&mut self.rx_limiter
	}

	pub fn tx_limiter(&mut self) -> &mut RateLimit {
		&mut self.tx_limiter
	}
}

/// Interceptor pattern for handling messages.
pub trait HandlePeerMessage {
	// Returns `None` if the message was handled, or the `message`
	// otherwise.
	fn handle(
		&self,
		message: PeerMessage,
		node_sender: &mut UnboundedSender<PeerMessage>,
	) -> Option<PeerMessage>;
}

impl<T> HandlePeerMessage for Arc<T>
where
	T: HandlePeerMessage,
{
	fn handle(
		&self,
		message: PeerMessage,
		node_sender: &mut UnboundedSender<PeerMessage>,
	) -> Option<PeerMessage> {
		self.as_ref().handle(message, node_sender)
	}
}

async fn emulated_peer_loop(
	handlers: Vec<Arc<dyn HandlePeerMessage + Sync + Send>>,
	stats: Arc<PeerEmulatorStats>,
	mut emulated_peer: EmulatedPeer,
	messages_rx: UnboundedReceiver<PeerMessage>,
	actions_rx: UnboundedReceiver<NetworkAction>,
	mut to_network_interface: UnboundedSender<PeerMessage>,
) {
	let mut proxied_requests = FuturesUnordered::new();
	let mut messages_rx = messages_rx.fuse();
	let mut actions_rx = actions_rx.fuse();
	loop {
		futures::select! {
			maybe_peer_message = messages_rx.next() => {
				if let Some(peer_message) = maybe_peer_message {
					let size = peer_message.size();
					emulated_peer.rx_limiter().reap(size).await;
					stats.inc_received(size);

					let mut message = Some(peer_message);
					for handler in handlers.iter() {
						// The check below guarantees that message is always `Some`.
						message = handler.handle(message.unwrap(), &mut to_network_interface);
						if message.is_none() {
							break
						}
					}
					if let Some(message) = message {
						panic!("Emulated message from peer {:?} not handled", message.peer());
					}
				} else {
					gum::debug!(target: LOG_TARGET, "Downlink channel closed, peer task exiting");
					break
				}
			},
			maybe_action = actions_rx.next() => {
				if let Some(action) = maybe_action {
					// We proxy any request being sent to the node to limit bandwidth as we
					// do in the `NetworkInterface` task.
					if let PeerMessage::RequestFromPeer(request) = action.message {
						let (response_sender, response_receiver) = oneshot::channel();
						// Create a new `IncomingRequest` that we forward to the network interface.
						let new_request = IncomingRequest {payload: request.payload, peer: request.peer, pending_response: response_sender};

						proxied_requests.push(ProxiedRequest {sender: Some(request.pending_response), receiver: response_receiver});

						emulated_peer.send_message(PeerMessage::RequestFromPeer(new_request)).await;
						continue
					}

					emulated_peer.send_message(action.message).await;
				} else {
					gum::debug!(target: LOG_TARGET, "Action channel closed, peer task exiting");
					break
				}
			},
			proxied_request = proxied_requests.next() => {
				if let Some(proxied_request) = proxied_request {
					match proxied_request.result {
						Ok(result) => {
							let bytes = result.encoded_size();
							gum::trace!(target: LOG_TARGET, size = bytes, "Peer proxied request completed");

							emulated_peer.rx_limiter().reap(bytes).await;
							stats.inc_received(bytes);

							proxied_request.sender.send(
								OutgoingResponse {
									reputation_changes: Vec::new(),
									result: Ok(result),
									sent_feedback: None
								}
							).expect("network is alive");
						}
						Err(e) => {
							gum::warn!(target: LOG_TARGET, "Node req/response failure: {:?}", e)
						}
					}
				}
			}
		}
	}
}

pub fn new_peer(
	bandwidth: usize,
	spawn_task_handle: SpawnTaskHandle,
	handlers: Vec<Arc<dyn HandlePeerMessage + Sync + Send>>,
	stats: Arc<PeerEmulatorStats>,
	mut to_network_interface: UnboundedSender<PeerMessage>,
) -> EmulatedPeerHandle {
	let (messages_tx, mut messages_rx) = mpsc::unbounded::<PeerMessage>();
	let (actions_tx, mut actions_rx) = mpsc::unbounded::<NetworkAction>();

	let rx_limiter = RateLimit::new(10, bandwidth);
	let tx_limiter = RateLimit::new(10, bandwidth);
	let mut emulated_peer =
		EmulatedPeer { rx_limiter, tx_limiter, to_node: to_network_interface.clone() };

	spawn_task_handle.clone().spawn(
		"peer-emulator",
		"test-environment",
		emulated_peer_loop(
			handlers,
			stats,
			emulated_peer,
			messages_rx,
			actions_rx,
			to_network_interface,
		)
		.boxed(),
	);

	EmulatedPeerHandle { messages_tx, actions_tx }
}

/// A network action to be completed by an emulator task.
pub struct NetworkAction {
	/// The message to be sent by the peer.
	pub message: PeerMessage,
	/// Peer which should run the action.
	pub peer: AuthorityDiscoveryId,
}

/// Book keeping of sent and received bytes.
pub struct PeerEmulatorStats {
	metrics: Metrics,
	peer_index: usize,
}

impl PeerEmulatorStats {
	pub(crate) fn new(peer_index: usize, metrics: Metrics) -> Self {
		Self { metrics, peer_index }
	}

	pub fn inc_sent(&self, bytes: usize) {
		self.metrics.on_peer_sent(self.peer_index, bytes);
	}

	pub fn inc_received(&self, bytes: usize) {
		self.metrics.on_peer_received(self.peer_index, bytes);
	}

	pub fn sent(&self) -> usize {
		self.metrics
			.peer_total_sent
			.get_metric_with_label_values(&[&format!("node{}", self.peer_index)])
			.expect("Metric exists")
			.get() as usize
	}

	pub fn received(&self) -> usize {
		self.metrics
			.peer_total_received
			.get_metric_with_label_values(&[&format!("node{}", self.peer_index)])
			.expect("Metric exists")
			.get() as usize
	}
}

/// The state of a peer on the emulated network.
#[derive(Clone)]
enum Peer {
	Connected(EmulatedPeerHandle),
	Disconnected(EmulatedPeerHandle),
}

impl Peer {
	pub fn disconnect(&mut self) {
		let new_self = match self {
			Peer::Connected(peer) => Peer::Disconnected(peer.clone()),
			_ => return,
		};
		*self = new_self;
	}

	pub fn is_connected(&self) -> bool {
		matches!(self, Peer::Connected(_))
	}

	pub fn handle(&self) -> &EmulatedPeerHandle {
		match self {
			Peer::Connected(ref emulator) => emulator,
			Peer::Disconnected(ref emulator) => emulator,
		}
	}
	pub fn handle_mut(&mut self) -> &mut EmulatedPeerHandle {
		match self {
			Peer::Connected(ref mut emulator) => emulator,
			Peer::Disconnected(ref mut emulator) => emulator,
		}
	}
}

/// A ha emulated network implementation.
#[derive(Clone)]
pub struct NetworkEmulatorHandle {
	// Per peer network emulation.
	peers: Vec<Peer>,
	/// Per peer stats.
	stats: Vec<Arc<PeerEmulatorStats>>,
	/// Each emulated peer is a validator.
	validator_authority_ids: HashMap<AuthorityDiscoveryId, usize>,
}

/// Create a new emulated network based on `config`.
/// Each emulated peer will run the specified `handlers` to process incoming messages.
pub fn new_network(
	config: &TestConfiguration,
	dependencies: &TestEnvironmentDependencies,
	authorities: &TestAuthorities,
	handlers: Vec<Arc<dyn HandlePeerMessage + Sync + Send>>,
) -> (NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver) {
	let n_peers = config.n_validators;
	gum::info!(target: LOG_TARGET, "{}",format!("Initializing emulation for a {} peer network.", n_peers).bright_blue());
	gum::info!(target: LOG_TARGET, "{}",format!("connectivity {}%, error {}%", config.connectivity, config.error).bright_black());

	let metrics =
		Metrics::new(&dependencies.registry).expect("Metrics always register succesfully");
	let mut validator_authority_id_mapping = HashMap::new();

	// Create the channel from `peer` to `NetworkInterface` .
	let (to_network_interface, from_network) = mpsc::unbounded();

	// Create a `PeerEmulator` for each peer.
	let (stats, mut peers): (_, Vec<_>) = (0..n_peers)
		.zip(authorities.validator_authority_id.clone().into_iter())
		.map(|(peer_index, authority_id)| {
			validator_authority_id_mapping.insert(authority_id, peer_index);
			let stats = Arc::new(PeerEmulatorStats::new(peer_index, metrics.clone()));
			(
				stats.clone(),
				Peer::Connected(new_peer(
					config.peer_bandwidth,
					dependencies.task_manager.spawn_handle(),
					handlers.clone(),
					stats,
					to_network_interface.clone(),
				)),
			)
		})
		.unzip();

	let connected_count = config.n_validators as f64 / (100.0 / config.connectivity as f64);

	let (_connected, to_disconnect) =
		peers.partial_shuffle(&mut thread_rng(), connected_count as usize);

	for peer in to_disconnect {
		peer.disconnect();
	}

	gum::info!(target: LOG_TARGET, "{}",format!("Network created, connected validator count {}", connected_count).bright_black());

	let handle = NetworkEmulatorHandle {
		peers,
		stats,
		validator_authority_ids: validator_authority_id_mapping,
	};

	// Finally create the `NetworkInterface` with the `from_network` receiver.
	let (network_interface, network_interface_receiver) = NetworkInterface::new(
		dependencies.task_manager.spawn_handle(),
		handle.clone(),
		config.bandwidth,
		from_network,
	);

	(handle, network_interface, network_interface_receiver)
}

impl NetworkEmulatorHandle {
	pub fn is_peer_connected(&self, peer: &AuthorityDiscoveryId) -> bool {
		self.peer(peer).is_connected()
	}

	/// Forward `message`` to an emulated `peer`.
	/// Panics if peer is not connected.
	pub fn submit_peer_message(&self, peer: &AuthorityDiscoveryId, message: PeerMessage) {
		let peer = self.peer(peer);
		assert!(peer.is_connected(), "forward message only for connected peers.");
		peer.handle().send_message(message);
	}

	/// Run a `NetworkAction` in the context of an emulated peer.
	pub fn submit_peer_action(&mut self, action: NetworkAction) {
		let dst_peer = self.peer(&action.peer);

		if !dst_peer.is_connected() {
			gum::warn!(target: LOG_TARGET, "Attempted to send data from a disconnected peer, operation ignored");
			return
		}

		dst_peer.handle().send_action(action);
	}

	// Returns the sent/received stats for `peer_index`.
	pub fn peer_stats(&self, peer_index: usize) -> Arc<PeerEmulatorStats> {
		self.stats[peer_index].clone()
	}

	// Helper to get peer index by `AuthorityDiscoveryId`
	fn peer_index(&self, peer: &AuthorityDiscoveryId) -> usize {
		*self
			.validator_authority_ids
			.get(peer)
			.expect("all test authorities are valid; qed")
	}

	// Return the Peer entry for a given `AuthorityDiscoveryId`.
	fn peer(&self, peer: &AuthorityDiscoveryId) -> &Peer {
		&self.peers[self.peer_index(peer)]
	}

	// Returns the sent/received stats for `peer`.
	pub fn peer_stats_by_id(&mut self, peer: &AuthorityDiscoveryId) -> Arc<PeerEmulatorStats> {
		let peer_index = self.peer_index(peer);

		self.stats[peer_index].clone()
	}

	// Increment bytes sent by our node (the node that contains the subsystem under test)
	pub fn inc_sent(&self, bytes: usize) {
		// Our node is always peer 0.
		self.peer_stats(0).inc_sent(bytes);
	}

	// Increment bytes received by our node (the node that contains the subsystem under test)
	pub fn inc_received(&self, bytes: usize) {
		// Our node is always peer 0.
		self.peer_stats(0).inc_received(bytes);
	}
}

use polkadot_node_subsystem_util::metrics::prometheus::{
	self, CounterVec, Opts, PrometheusError, Registry,
};

/// Emulated network metrics.
#[derive(Clone)]
pub(crate) struct Metrics {
	/// Number of bytes sent per peer.
	peer_total_sent: CounterVec<U64>,
	/// Number of received sent per peer.
	peer_total_received: CounterVec<U64>,
}

impl Metrics {
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			peer_total_sent: prometheus::register(
				CounterVec::new(
					Opts::new(
						"subsystem_benchmark_network_peer_total_bytes_sent",
						"Total number of bytes a peer has sent.",
					),
					&["peer"],
				)?,
				registry,
			)?,
			peer_total_received: prometheus::register(
				CounterVec::new(
					Opts::new(
						"subsystem_benchmark_network_peer_total_bytes_received",
						"Total number of bytes a peer has received.",
					),
					&["peer"],
				)?,
				registry,
			)?,
		})
	}

	/// Increment total sent for a peer.
	pub fn on_peer_sent(&self, peer_index: usize, bytes: usize) {
		self.peer_total_sent
			.with_label_values(vec![format!("node{}", peer_index).as_str()].as_slice())
			.inc_by(bytes as u64);
	}

	/// Increment total receioved for a peer.
	pub fn on_peer_received(&self, peer_index: usize, bytes: usize) {
		self.peer_total_received
			.with_label_values(vec![format!("node{}", peer_index).as_str()].as_slice())
			.inc_by(bytes as u64);
	}
}

#[cfg(test)]
mod tests {
	use std::time::Instant;

	use super::RateLimit;

	#[tokio::test]
	async fn test_expected_rate() {
		let tick_rate = 200;
		let budget = 1_000_000;
		// rate must not exceeed 100 credits per second
		let mut rate_limiter = RateLimit::new(tick_rate, budget);
		let mut total_sent = 0usize;
		let start = Instant::now();

		let mut reap_amount = 0;
		while rate_limiter.total_ticks < tick_rate {
			reap_amount += 1;
			reap_amount = reap_amount % 100;

			rate_limiter.reap(reap_amount).await;
			total_sent += reap_amount;
		}

		let end = Instant::now();

		println!("duration: {}", (end - start).as_millis());

		// Allow up to `budget/max_refill` error tolerance
		let lower_bound = budget as u128 * ((end - start).as_millis() / 1000u128);
		let upper_bound = budget as u128 *
			((end - start).as_millis() / 1000u128 + rate_limiter.max_refill as u128);
		assert!(total_sent as u128 >= lower_bound);
		assert!(total_sent as u128 <= upper_bound);
	}
}

/// A helper to determine the request payload size.
pub fn request_size(request: &Requests) -> usize {
	match request {
		Requests::ChunkFetchingV1(outgoing_request) => outgoing_request.payload.encoded_size(),
		Requests::AvailableDataFetchingV1(outgoing_request) =>
			outgoing_request.payload.encoded_size(),
		_ => unimplemented!("received an unexpected request"),
	}
}

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

//! Implements network emulation and interfaces to control and specialize
//! network peer behaviour.

//	     [TestEnvironment]
// 	  [NetworkEmulatorHandle]
// 			    ||
//   +-------+--||--+-------+
//   |       |      |       |
//  Peer1	Peer2  Peer3  Peer4
//    \      |	    |	    /
//     \     |      |	   /
//      \    |      |    /
//       \   |      |   /
//        \  |      |  /
//     [Network Interface]
//               |
//    [Emulated Network Bridge]
//               |
//     Subsystems under test

use crate::core::{
	configuration::{random_latency, TestAuthorities, TestConfiguration},
	environment::TestEnvironmentDependencies,
	NODE_UNDER_TEST,
};
use colored::Colorize;
use futures::{
	channel::{
		mpsc,
		mpsc::{UnboundedReceiver, UnboundedSender},
		oneshot,
	},
	lock::Mutex,
	stream::FuturesUnordered,
	Future, FutureExt, StreamExt,
};
use itertools::Itertools;
use net_protocol::{
	peer_set::{ProtocolVersion, ValidationVersion},
	request_response::{Recipient, Requests, ResponseSender},
	ObservedRole, VersionedValidationProtocol,
};
use parity_scale_codec::Encode;
use polkadot_node_network_protocol::{self as net_protocol, Versioned};
use polkadot_node_subsystem_types::messages::{ApprovalDistributionMessage, NetworkBridgeEvent};
use polkadot_node_subsystem_util::metrics::prometheus::{
	self, CounterVec, Opts, PrometheusError, Registry,
};
use polkadot_overseer::AllMessages;
use polkadot_primitives::AuthorityDiscoveryId;
use prometheus_endpoint::U64;
use rand::{seq::SliceRandom, thread_rng};
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	PeerId, RequestFailure,
};
use sc_service::SpawnTaskHandle;
use std::{
	collections::HashMap,
	sync::Arc,
	task::Poll,
	time::{Duration, Instant},
};

const LOG_TARGET: &str = "subsystem-bench::network";

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
pub enum NetworkMessage {
	/// A gossip message from peer to node.
	MessageFromPeer(PeerId, VersionedValidationProtocol),
	/// A gossip message from node to a peer.
	MessageFromNode(AuthorityDiscoveryId, VersionedValidationProtocol),
	/// A request originating from our node
	RequestFromNode(AuthorityDiscoveryId, Requests),
	/// A request originating from an emultated peer
	RequestFromPeer(IncomingRequest),
}

impl NetworkMessage {
	/// Returns the size of the encoded message or request
	pub fn size(&self) -> usize {
		match &self {
			NetworkMessage::MessageFromPeer(_, Versioned::V2(message)) => message.encoded_size(),
			NetworkMessage::MessageFromPeer(_, Versioned::V1(message)) => message.encoded_size(),
			NetworkMessage::MessageFromPeer(_, Versioned::V3(message)) => message.encoded_size(),
			NetworkMessage::MessageFromNode(_peer_id, Versioned::V2(message)) =>
				message.encoded_size(),
			NetworkMessage::MessageFromNode(_peer_id, Versioned::V1(message)) =>
				message.encoded_size(),
			NetworkMessage::MessageFromNode(_peer_id, Versioned::V3(message)) =>
				message.encoded_size(),
			NetworkMessage::RequestFromNode(_peer_id, incoming) => incoming.size(),
			NetworkMessage::RequestFromPeer(request) => request.payload.encoded_size(),
		}
	}

	/// Returns the destination peer from the message or `None` if it originates from a peer.
	pub fn peer(&self) -> Option<&AuthorityDiscoveryId> {
		match &self {
			NetworkMessage::MessageFromNode(peer_id, _) |
			NetworkMessage::RequestFromNode(peer_id, _) => Some(peer_id),
			_ => None,
		}
	}
}

/// A network interface of the node under test.
pub struct NetworkInterface {
	// Sender for subsystems.
	bridge_to_interface_sender: UnboundedSender<NetworkMessage>,
}

// Wraps the receiving side of a interface to bridge channel. It is a required
// parameter of the `network-bridge` mock.
pub struct NetworkInterfaceReceiver(pub UnboundedReceiver<NetworkMessage>);

struct ProxiedRequest {
	sender: Option<oneshot::Sender<OutgoingResponse>>,
	receiver: oneshot::Receiver<OutgoingResponse>,
}

struct ProxiedResponse {
	pub sender: oneshot::Sender<OutgoingResponse>,
	pub result: Result<Vec<u8>, RequestFailure>,
}

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
		network: NetworkEmulatorHandle,
		bandwidth_bps: usize,
		mut from_network: UnboundedReceiver<NetworkMessage>,
	) -> (NetworkInterface, NetworkInterfaceReceiver) {
		let rx_limiter = Arc::new(Mutex::new(RateLimit::new(10, bandwidth_bps)));
		let tx_limiter = Arc::new(Mutex::new(RateLimit::new(10, bandwidth_bps)));

		// Channel for receiving messages from the network bridge subsystem.
		let (bridge_to_interface_sender, mut bridge_to_interface_receiver) =
			mpsc::unbounded::<NetworkMessage>();

		// Channel for forwarding messages to the network bridge subsystem.
		let (interface_to_bridge_sender, interface_to_bridge_receiver) =
			mpsc::unbounded::<NetworkMessage>();

		let rx_network = network.clone();
		let tx_network = network;

		let rx_task_bridge_sender = interface_to_bridge_sender.clone();

		let task_rx_limiter = rx_limiter.clone();
		let task_tx_limiter = tx_limiter.clone();

		// A task that forwards messages from emulated peers to the node (emulated network bridge).
		let rx_task = async move {
			let mut proxied_requests = FuturesUnordered::new();

			loop {
				let mut from_network = from_network.next().fuse();
				futures::select! {
					maybe_peer_message = from_network => {
						if let Some(peer_message) = maybe_peer_message {
							let size = peer_message.size();
							task_rx_limiter.lock().await.reap(size).await;
							rx_network.inc_received(size);

							// To be able to apply the configured bandwidth limits for responses being sent
							// over channels, we need to implement a simple proxy that allows this loop
							// to receive the response and enforce the configured bandwidth before
							// sending it to the original recipient.
							if let NetworkMessage::RequestFromPeer(request) = peer_message {
								let (response_sender, response_receiver) = oneshot::channel();

								// Create a new `IncomingRequest` that we forward to the network bridge.
								let new_request = IncomingRequest {payload: request.payload, peer: request.peer, pending_response: response_sender};
								proxied_requests.push(ProxiedRequest {sender: Some(request.pending_response), receiver: response_receiver});

								// Send the new message to network bridge subsystem.
								rx_task_bridge_sender
									.unbounded_send(NetworkMessage::RequestFromPeer(new_request))
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

									// Enforce bandwidth based on the response the node has sent.
									// TODO: Fix the stall of RX when TX lock() takes a while to refill
									// the token bucket. Good idea would be to create a task for each request.
									task_tx_limiter.lock().await.reap(bytes).await;
									rx_network.inc_sent(bytes);

									// Forward the response to original recipient.
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

		let task_spawn_handle = spawn_task_handle.clone();
		let task_rx_limiter = rx_limiter.clone();
		let task_tx_limiter = tx_limiter.clone();

		// A task that forwards messages from the node to emulated peers.
		let tx_task = async move {
			// Wrap it in an `Arc` to avoid `clone()` the inner data as we need to share it across
			// many send tasks.
			let tx_network = Arc::new(tx_network);

			loop {
				if let Some(peer_message) = bridge_to_interface_receiver.next().await {
					let size = peer_message.size();
					// Ensure bandwidth used is limited.
					task_tx_limiter.lock().await.reap(size).await;

					match peer_message {
						NetworkMessage::MessageFromNode(peer, message) =>
							tx_network.send_message_to_peer(&peer, message),
						NetworkMessage::RequestFromNode(peer, request) => {
							// Send request through a proxy so we can account and limit bandwidth
							// usage for the node.
							let send_task = Self::proxy_send_request(
								peer.clone(),
								request,
								tx_network.clone(),
								task_rx_limiter.clone(),
							)
							.boxed();

							task_spawn_handle.spawn("request-proxy", "test-environment", send_task);
						},
						_ => panic!(
							"Unexpected network message received from emulated network bridge"
						),
					}

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
			Self { bridge_to_interface_sender },
			NetworkInterfaceReceiver(interface_to_bridge_receiver),
		)
	}

	/// Get a sender that can be used by a subsystem to send network actions to the network.
	pub fn subsystem_sender(&self) -> UnboundedSender<NetworkMessage> {
		self.bridge_to_interface_sender.clone()
	}

	/// Helper method that proxies a request from node to peer and implements rate limiting and
	/// accounting.
	async fn proxy_send_request(
		peer: AuthorityDiscoveryId,
		mut request: Requests,
		tx_network: Arc<NetworkEmulatorHandle>,
		task_rx_limiter: Arc<Mutex<RateLimit>>,
	) {
		let (proxy_sender, proxy_receiver) = oneshot::channel();

		// Modify the request response sender so we can intercept the answer
		let sender = request.swap_response_sender(proxy_sender);

		// Send the modified request to the peer.
		tx_network.send_request_to_peer(&peer, request);

		// Wait for answer (intercept the response).
		match proxy_receiver.await {
			Err(_) => {
				panic!("Emulated peer hangup");
			},
			Ok(Err(err)) => {
				sender.send(Err(err)).expect("Oneshot send always works.");
			},
			Ok(Ok((response, protocol_name))) => {
				let response_size = response.encoded_size();
				task_rx_limiter.lock().await.reap(response_size).await;
				tx_network.inc_received(response_size);

				// Send the response to the original request sender.
				if sender.send(Ok((response, protocol_name))).is_err() {
					gum::warn!(target: LOG_TARGET, response_size, "response oneshot canceled by node")
				}
			},
		};
	}
}

/// A handle for controlling an emulated peer.
#[derive(Clone)]
pub struct EmulatedPeerHandle {
	/// Send messages to be processed by the peer.
	messages_tx: UnboundedSender<NetworkMessage>,
	/// Send actions to be performed by the peer.
	actions_tx: UnboundedSender<NetworkMessage>,
	peer_id: PeerId,
}

impl EmulatedPeerHandle {
	/// Receive and process a message from the node.
	pub fn receive(&self, message: NetworkMessage) {
		self.messages_tx.unbounded_send(message).expect("Peer message channel hangup");
	}

	/// Send a message to the node.
	pub fn send_message(&self, message: VersionedValidationProtocol) {
		self.actions_tx
			.unbounded_send(NetworkMessage::MessageFromPeer(self.peer_id, message))
			.expect("Peer action channel hangup");
	}

	/// Send a `request` to the node.
	pub fn send_request(&self, request: IncomingRequest) {
		self.actions_tx
			.unbounded_send(NetworkMessage::RequestFromPeer(request))
			.expect("Peer action channel hangup");
	}
}

// A network peer emulator.
struct EmulatedPeer {
	spawn_handle: SpawnTaskHandle,
	to_node: UnboundedSender<NetworkMessage>,
	tx_limiter: RateLimit,
	rx_limiter: RateLimit,
	latency_ms: usize,
}

impl EmulatedPeer {
	/// Send a message to the node.
	pub async fn send_message(&mut self, message: NetworkMessage) {
		self.tx_limiter.reap(message.size()).await;

		if self.latency_ms == 0 {
			self.to_node.unbounded_send(message).expect("Sending to the node never fails");
		} else {
			let to_node = self.to_node.clone();
			let latency_ms = std::time::Duration::from_millis(self.latency_ms as u64);

			// Emulate RTT latency
			self.spawn_handle
				.spawn("peer-latency-emulator", "test-environment", async move {
					tokio::time::sleep(latency_ms).await;
					to_node.unbounded_send(message).expect("Sending to the node never fails");
				});
		}
	}

	/// Returns the rx bandwidth limiter.
	pub fn rx_limiter(&mut self) -> &mut RateLimit {
		&mut self.rx_limiter
	}
}

/// Interceptor pattern for handling messages.
pub trait HandleNetworkMessage {
	/// Returns `None` if the message was handled, or the `message`
	/// otherwise.
	///
	/// `node_sender` allows sending of messages to the node in response
	/// to the handled message.
	fn handle(
		&self,
		message: NetworkMessage,
		node_sender: &mut UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage>;
}

impl<T> HandleNetworkMessage for Arc<T>
where
	T: HandleNetworkMessage,
{
	fn handle(
		&self,
		message: NetworkMessage,
		node_sender: &mut UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		self.as_ref().handle(message, node_sender)
	}
}

// This loop is responsible for handling of messages/requests between the peer and the node.
async fn emulated_peer_loop(
	handlers: Vec<Arc<dyn HandleNetworkMessage + Sync + Send>>,
	stats: Arc<PeerEmulatorStats>,
	mut emulated_peer: EmulatedPeer,
	messages_rx: UnboundedReceiver<NetworkMessage>,
	actions_rx: UnboundedReceiver<NetworkMessage>,
	mut to_network_interface: UnboundedSender<NetworkMessage>,
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

					// Try all handlers until the message gets processed.
					// Panic if the message is not consumed.
					for handler in handlers.iter() {
						// The check below guarantees that message is always `Some`: we are still
						// inside the loop.
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
				match maybe_action {
					// We proxy any request being sent to the node to limit bandwidth as we
					// do in the `NetworkInterface` task.
					Some(NetworkMessage::RequestFromPeer(request)) => {
						let (response_sender, response_receiver) = oneshot::channel();
						// Create a new `IncomingRequest` that we forward to the network interface.
						let new_request = IncomingRequest {payload: request.payload, peer: request.peer, pending_response: response_sender};

						proxied_requests.push(ProxiedRequest {sender: Some(request.pending_response), receiver: response_receiver});

						emulated_peer.send_message(NetworkMessage::RequestFromPeer(new_request)).await;
					},
					Some(message) => emulated_peer.send_message(message).await,
					None => {
						gum::debug!(target: LOG_TARGET, "Action channel closed, peer task exiting");
						break
					}
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

/// Creates a new peer emulator task and returns a handle to it.
pub fn new_peer(
	bandwidth: usize,
	spawn_task_handle: SpawnTaskHandle,
	handlers: Vec<Arc<dyn HandleNetworkMessage + Sync + Send>>,
	stats: Arc<PeerEmulatorStats>,
	to_network_interface: UnboundedSender<NetworkMessage>,
	latency_ms: usize,
	peer_id: PeerId,
) -> EmulatedPeerHandle {
	let (messages_tx, messages_rx) = mpsc::unbounded::<NetworkMessage>();
	let (actions_tx, actions_rx) = mpsc::unbounded::<NetworkMessage>();

	let rx_limiter = RateLimit::new(10, bandwidth);
	let tx_limiter = RateLimit::new(10, bandwidth);
	let emulated_peer = EmulatedPeer {
		spawn_handle: spawn_task_handle.clone(),
		rx_limiter,
		tx_limiter,
		to_node: to_network_interface.clone(),
		latency_ms,
	};

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

	EmulatedPeerHandle { messages_tx, actions_tx, peer_id }
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

impl NetworkEmulatorHandle {
	/// Generates peer_connected messages for all peers in `test_authorities`
	pub fn generate_peer_connected(&self) -> Vec<AllMessages> {
		self.peers
			.iter()
			.filter(|peer| peer.is_connected())
			.map(|peer| {
				let network = NetworkBridgeEvent::PeerConnected(
					peer.handle().peer_id,
					ObservedRole::Full,
					ProtocolVersion::from(ValidationVersion::V3),
					None,
				);

				AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(
					network,
				))
			})
			.collect_vec()
	}
}

/// Create a new emulated network based on `config`.
/// Each emulated peer will run the specified `handlers` to process incoming messages.
pub fn new_network(
	config: &TestConfiguration,
	dependencies: &TestEnvironmentDependencies,
	authorities: &TestAuthorities,
	handlers: Vec<Arc<dyn HandleNetworkMessage + Sync + Send>>,
) -> (NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver) {
	let n_peers = config.n_validators;
	gum::info!(target: LOG_TARGET, "{}",format!("Initializing emulation for a {} peer network.", n_peers).bright_blue());
	gum::info!(target: LOG_TARGET, "{}",format!("connectivity {}%, latency {:?}", config.connectivity, config.latency).bright_black());

	let metrics =
		Metrics::new(&dependencies.registry).expect("Metrics always register succesfully");
	let mut validator_authority_id_mapping = HashMap::new();

	// Create the channel from `peer` to `NetworkInterface` .
	let (to_network_interface, from_network) = mpsc::unbounded();

	// Create a `PeerEmulator` for each peer.
	let (stats, mut peers): (_, Vec<_>) = (0..n_peers)
		.zip(authorities.validator_authority_id.clone())
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
					random_latency(config.latency.as_ref()),
					*authorities.peer_ids.get(peer_index).unwrap(),
				)),
			)
		})
		.unzip();

	let connected_count = config.connected_count();

	let mut peers_indicies = (0..n_peers).collect_vec();
	let (_connected, to_disconnect) =
		peers_indicies.partial_shuffle(&mut thread_rng(), connected_count);

	// Node under test is always mark as disconnected.
	peers[NODE_UNDER_TEST as usize].disconnect();
	for peer in to_disconnect.iter().skip(1) {
		peers[*peer].disconnect();
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

/// Errors that can happen when sending data to emulated peers.
#[derive(Clone, Debug)]
pub enum EmulatedPeerError {
	NotConnected,
}

impl NetworkEmulatorHandle {
	/// Returns true if the emulated peer is connected to the node under test.
	pub fn is_peer_connected(&self, peer: &AuthorityDiscoveryId) -> bool {
		self.peer(peer).is_connected()
	}

	/// Forward notification `message` to an emulated `peer`.
	/// Panics if peer is not connected.
	pub fn send_message_to_peer(
		&self,
		peer_id: &AuthorityDiscoveryId,
		message: VersionedValidationProtocol,
	) {
		let peer = self.peer(peer_id);
		assert!(peer.is_connected(), "forward message only for connected peers.");
		peer.handle().receive(NetworkMessage::MessageFromNode(peer_id.clone(), message));
	}

	/// Forward a `request`` to an emulated `peer`.
	/// Panics if peer is not connected.
	pub fn send_request_to_peer(&self, peer_id: &AuthorityDiscoveryId, request: Requests) {
		let peer = self.peer(peer_id);
		assert!(peer.is_connected(), "forward request only for connected peers.");
		peer.handle().receive(NetworkMessage::RequestFromNode(peer_id.clone(), request));
	}

	/// Send a message from a peer to the node.
	pub fn send_message_from_peer(
		&self,
		from_peer: &AuthorityDiscoveryId,
		message: VersionedValidationProtocol,
	) -> Result<(), EmulatedPeerError> {
		let dst_peer = self.peer(from_peer);

		if !dst_peer.is_connected() {
			gum::warn!(target: LOG_TARGET, "Attempted to send message from a peer not connected to our node, operation ignored");
			return Err(EmulatedPeerError::NotConnected)
		}

		dst_peer.handle().send_message(message);
		Ok(())
	}

	/// Send a request from a peer to the node.
	pub fn send_request_from_peer(
		&self,
		from_peer: &AuthorityDiscoveryId,
		request: IncomingRequest,
	) -> Result<(), EmulatedPeerError> {
		let dst_peer = self.peer(from_peer);

		if !dst_peer.is_connected() {
			gum::warn!(target: LOG_TARGET, "Attempted to send request from a peer not connected to our node, operation ignored");
			return Err(EmulatedPeerError::NotConnected)
		}

		dst_peer.handle().send_request(request);
		Ok(())
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

// Helper trait for low level access to `Requests` variants.
pub trait RequestExt {
	/// Get the authority id if any from the request.
	fn authority_id(&self) -> Option<&AuthorityDiscoveryId>;
	/// Consume self and return the response sender.
	fn into_response_sender(self) -> ResponseSender;
	/// Allows to change the `ResponseSender` in place.
	fn swap_response_sender(&mut self, new_sender: ResponseSender) -> ResponseSender;
	/// Returns the size in bytes of the request payload.
	fn size(&self) -> usize;
}

impl RequestExt for Requests {
	fn authority_id(&self) -> Option<&AuthorityDiscoveryId> {
		match self {
			Requests::ChunkFetchingV1(request) => {
				if let Recipient::Authority(authority_id) = &request.peer {
					Some(authority_id)
				} else {
					None
				}
			},
			Requests::AvailableDataFetchingV1(request) => {
				if let Recipient::Authority(authority_id) = &request.peer {
					Some(authority_id)
				} else {
					None
				}
			},
			request => {
				unimplemented!("RequestAuthority not implemented for {:?}", request)
			},
		}
	}

	fn into_response_sender(self) -> ResponseSender {
		match self {
			Requests::ChunkFetchingV1(outgoing_request) => outgoing_request.pending_response,
			Requests::AvailableDataFetchingV1(outgoing_request) =>
				outgoing_request.pending_response,
			_ => unimplemented!("unsupported request type"),
		}
	}

	/// Swaps the `ResponseSender` and returns the previous value.
	fn swap_response_sender(&mut self, new_sender: ResponseSender) -> ResponseSender {
		match self {
			Requests::ChunkFetchingV1(outgoing_request) =>
				std::mem::replace(&mut outgoing_request.pending_response, new_sender),
			Requests::AvailableDataFetchingV1(outgoing_request) =>
				std::mem::replace(&mut outgoing_request.pending_response, new_sender),
			_ => unimplemented!("unsupported request type"),
		}
	}

	/// Returns the size in bytes of the request payload.
	fn size(&self) -> usize {
		match self {
			Requests::ChunkFetchingV1(outgoing_request) => outgoing_request.payload.encoded_size(),
			Requests::AvailableDataFetchingV1(outgoing_request) =>
				outgoing_request.payload.encoded_size(),
			_ => unimplemented!("received an unexpected request"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::RateLimit;
	use std::time::Instant;

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
			reap_amount %= 100;

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

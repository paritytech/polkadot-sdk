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
use parity_scale_codec::Encode;

use colored::Colorize;
use futures::channel::mpsc;
use net_protocol::VersionedValidationProtocol;
use polkadot_primitives::AuthorityDiscoveryId;
use prometheus_endpoint::U64;
use rand::{seq::SliceRandom, thread_rng};
use sc_network::request_responses::IncomingRequest;
use sc_service::SpawnTaskHandle;
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::{Duration, Instant}, ops::DerefMut,
};

use polkadot_node_network_protocol::{
	self as net_protocol,
	
	peer_set::{ProtocolVersion, ValidationVersion},
	v1 as protocol_v1, v2 as protocol_v2, vstaging as protocol_vstaging, OurView, PeerId,
	UnifiedReputationChange as Rep, Versioned, View,
};

use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver};

use futures::{
	Future, FutureExt, Stream, StreamExt,
};
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

// A wrapper for both gossip and request/response protocols along with the destination peer(`AuthorityDiscoveryId``).
pub enum PeerMessage {
	Message(AuthorityDiscoveryId, VersionedValidationProtocol),
	Request(AuthorityDiscoveryId, IncomingRequest)
}

impl PeerMessage {
	/// Returns the size of the encoded message or request
	pub fn size(&self) -> usize {
		match &self {
			PeerMessage::Message(_peer_id, Versioned::V1(message)) => {
				message.encoded_size()
			},
			PeerMessage::Request(_peer_id, incoming) => {
				incoming.payload.encoded_size()
			}
		}
	}
}

/// A network interface of the node under test.
pub struct NetworkInterface {
	// The network we are connected to.
	network: Option<NetworkEmulatorHandle>,
	// Used to receive traffic from the network and implement `rx_limiter` limits.
	// from_network: Receiver<NetworkAction>,
	// `tx_limiter` enforces the configured bandwidth.
	// Sender for network peers.
	network_to_interface_sender: UnboundedSender<PeerMessage>,
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

impl NetworkInterface {
	/// Create a new `NetworkInterface`
	pub fn new(
		spawn_task_handle: SpawnTaskHandle,
		mut network: NetworkEmulator,
		bandiwdth_bps: usize,
	) -> (NetworkInterface, NetworkInterfaceReceiver) {
		let mut rx_limiter: RateLimit = RateLimit::new(10, bandiwdth_bps);
		let mut tx_limiter: RateLimit = RateLimit::new(10, bandiwdth_bps);

		// A sender (`ingress_tx`) clone will be owned by each `PeerEmulator`, such that it can send
		// messages to the node. THe receiver will be polled in the network interface task.
		let (network_to_interface_sender, network_to_interface_receiver) = mpsc::unbounded::<PeerMessage>();
		// The sender (`egress_tx`) is
		let (bridge_to_interface_sender, bridge_to_interface_receiver) = mpsc::unbounded::<PeerMessage>();

		// Channel for forwarding actions to the bridge.
		let (interface_to_bridge_sender, interface_to_bridge_receiver) = mpsc::unbounded::<PeerMessage>();

		// Spawn the network interface task.
		let task = async move {
			let mut network_to_interface_receiver = network_to_interface_receiver.fuse();
			let mut bridge_to_interface_receiver = bridge_to_interface_receiver.fuse();

			loop {
				// TODO (maybe): split into separate rx/tx tasks.
				futures::select! {
					maybe_peer_message = network_to_interface_receiver.next() => {
						if let Some(peer_message) = maybe_peer_message {
							rx_limiter.reap(peer_message.size()).await;

							// Forward the message to the bridge.
							interface_to_bridge_sender.unbounded_send(peer_message).expect("network bridge subsystem is alive");
						} else {
							gum::info!(target: LOG_TARGET, "Uplink channel closed, network interface task exiting");
							break
						}
					},
					maybe_peer_message = bridge_to_interface_receiver.next() => {
						if let Some(peer_message) = maybe_action_from_subsystem {
							tx_limiter.reap(peer_message.size()).await;
							
							// Forward action to the destination network peer.
							network.submit_peer_action(peer, action);
						} else {
							gum::info!(target: LOG_TARGET, "Downlink channel closed, network interface task exiting");
							break
						}
					}

				}
			}
		}
		.boxed();

		(
			Self {
				network: None,
				network_to_interface_sender,
				bridge_to_interface_sender,
				interface_to_bridge_sender,
			},
			NetworkInterfaceReceiver(interface_to_bridge_receiver),
		)
	}

	/// Connect the interface to a network.
	pub fn connect(&mut self, network: NetworkEmulator) {
		self.network = Some(network);
	}

	/// Get a sender that can be used by a subsystem to send network actions to the network.
	pub fn subsystem_sender(&self) -> UnboundedSender<PeerMessage> {
		self.bridge_to_interface_sender.clone()
	}

	/// Get a sender that can be used by the Network to send network actions to the network.
	pub fn network_sender(&self) -> UnboundedSender<PeerMessage> {
		self.network_to_interface_sender.clone()
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

//
// A network peer emulator. It spawns a task that accepts `NetworkActions` and
// executes them with a configurable delay and bandwidth constraints. Tipically
// these actions wrap a future that performs a channel send to the subsystem(s) under test.
#[derive(Clone)]
struct EmulatedPeerHandle {
	// Send messages to the peer emulator task
	messages_tx: UnboundedSender<PeerMessage>,
	// Send actions to the peer emulator task
	actions_tx: UnboundedSender<NetworkAction>,
}

/// Interceptor pattern for handling messages.
pub trait HandlePeerMessage {
	// Returns `None` if the message was handled, or the `message`
	// otherwise.
	fn handle(&self, message: PeerMessage, node_sender: &mut UnboundedSender<PeerMessage>) -> Option<PeerMessage>;
}

impl HandlePeerMessage for Arc<T> 
where T: HandlePeerMessage {
	fn handle(&self, message: PeerMessage, node_sender: &mut UnboundedSender<PeerMessage>) -> Option<PeerMessage> {
		self.as_ref().handle(message, node_sender)
	}
}

pub fn new_peer(
	bandwidth: usize,
	spawn_task_handle: SpawnTaskHandle,
	handlers: Vec<Box<dyn HandlePeerMessage>>,
	stats: Arc<PeerEmulatorStats>,
	network_interface: &NetworkInterface,
) -> Self {
	let (messages_tx, mut messages_rx) = mpsc::unbounded::<PeerMessage>();
	let (actions_tx, mut actions_rx) = mpsc::unbounded::<NetworkAction>();

	// We'll use this to send messages from this peer to the node under test (peer 0)
	let to_node = network_interface.network_sender();
	
	spawn_task_handle
		.clone()
		.spawn("peer-emulator", "test-environment", async move {
			let mut rx_limiter = RateLimit::new(10, bandwidth);
			let mut tx_limiter = RateLimit::new(10, bandwidth);
			let mut messages_rx = messages_rx.fuse();
			let mut actions_rx = actions_rx.fuse();
			
			assert!(message.is_none(), "A peer will process all received messages");

			// TODO: implement latency and error.
			loop {
				futures::select! {
					maybe_peer_message = messages_rx.next() => {
						if let Some(peer_message) = maybe_peer_message {
							let size = peer_message.size();
							rx_limiter.reap(size).await;
							stats.inc_received(size);

							let message = Some(message);
							for handler in handlers.iter() {
								message = handler.handle(message, &mut to_node);
								if message.is_none() {
									break
								}
							}
						} else {
							gum::trace!(target: LOG_TARGET, "Downlink channel closed, peer task exiting");
							break
						}
					},
					maybe_action = actions_rx.next() => {
						if let Some(action) = maybe_action {
							match action.kind {
								NetworkActionKind::SendMessage(message) => {
									let size = message.size();
									tx_limiter.reap(size).await;
									stats.inc_sent(size);
									to_node.unbounded_send(message);
								}
							}
						} else {
							gum::trace!(target: LOG_TARGET, "Action channel closed, peer task exiting");
							break
						}
					},
				}
			}
		});

	EmulatedPeerHandle { messages_tx, actions_tx }
}

/// Types of actions that an emulated peer can run.
pub enum NetworkActionKind {
	/// Send a message to node under test (peer 0)
	SendMessage(PeerMessage)
}

/// A network action to be completed by an emulator task.
pub struct NetworkAction {
	/// The action type
	pub kind: NetworkActionKind,
	// Peer which should run the action.
	pub peer: AuthorityDiscoveryId,
}

unsafe impl Send for NetworkAction {}

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
	Connected(PeerEmulator),
	Disconnected(PeerEmulator),
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
		if let Peer::Connected(_) = self {
			true
		} else {
			false
		}
	}

	pub fn emulator(&mut self) -> &mut PeerEmulator {
		match self {
			Peer::Connected(ref mut emulator) => emulator,
			Peer::Disconnected(ref mut emulator) => emulator,
		}
	}
}

/// An emulated network implementation. Can be cloned 
#[derive(Clone)]
pub struct NetworkEmulatorHandle {
	// Per peer network emulation.
	peers: Vec<Peer>,
	/// Per peer stats.
	stats: Vec<Arc<PeerEmulatorStats>>,
	/// Each emulated peer is a validator.
	validator_authority_ids: HashMap<AuthorityDiscoveryId, usize>,
}

pub fn new_network(
	config: &TestConfiguration,
	dependencies: &TestEnvironmentDependencies,
	authorities: &TestAuthorities,
	handlers: Vec<Box<dyn HandlePeerMessage>>,
	network_interface: &NetworkInterface,
) -> NetworkEmulatorHandle {
	let n_peers = config.n_validators;
	gum::info!(target: LOG_TARGET, "{}",format!("Initializing emulation for a {} peer network.", n_peers).bright_blue());
	gum::info!(target: LOG_TARGET, "{}",format!("connectivity {}%, error {}%", config.connectivity, config.error).bright_black());

	let metrics =
		Metrics::new(&dependencies.registry).expect("Metrics always register succesfully");
	let mut validator_authority_id_mapping = HashMap::new();

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
					handlers,
					stats,
					network_interface,
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

	NetworkEmulatorHandle { peers, stats, validator_authority_ids: validator_authority_id_mapping }
}


impl NetworkEmulatorHandle {
	pub fn is_peer_connected(&self, peer: &AuthorityDiscoveryId) -> bool {
		self.peer(peer).is_connected()
	}

	/// Forward `message`` to an emulated `peer``.
	/// Panics if peer is not connected.
	pub fn forward_message(&self, peer: &AuthorityDiscoveryId, message: PeerMessage) {
		assert(!self.peer(peer).is_connected(), "forward message only for connected peers.");


	}

	/// Run some code in the context of an emulated
	pub fn submit_peer_action(&mut self, peer: AuthorityDiscoveryId, action: NetworkAction) {
		let index = self
			.validator_authority_ids
			.get(&peer)
			.expect("all test authorities are valid; qed");

		let peer = self.peers.get_mut(*index).expect("We just retrieved the index above; qed");

		// Only actions of size 0 are allowed on disconnected peers.
		// Typically this are delayed error response sends.
		if action.size() > 0 && !peer.is_connected() {
			gum::warn!(target: LOG_TARGET, peer_index = index, "Attempted to send data from a disconnected peer, operation ignored");
			return
		}

		peer.emulator().send(action);
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

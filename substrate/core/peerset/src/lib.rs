// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Peer Set Manager (PSM). Contains the strategy for choosing which nodes the network should be
//! connected to.

mod peersstate;

use std::{collections::{HashSet, HashMap}, collections::VecDeque, time::Instant};
use futures::{prelude::*, channel::mpsc};
use libp2p::PeerId;
use log::{debug, error, trace};
use serde_json::json;
use std::{pin::Pin, task::Context, task::Poll};

/// We don't accept nodes whose reputation is under this value.
const BANNED_THRESHOLD: i32 = 82 * (i32::min_value() / 100);
/// Reputation change for a node when we get disconnected from it.
const DISCONNECT_REPUTATION_CHANGE: i32 = -10;
/// Reserved peers group ID
const RESERVED_NODES: &'static str = "reserved";

#[derive(Debug)]
enum Action {
	AddReservedPeer(PeerId),
	RemoveReservedPeer(PeerId),
	SetReservedOnly(bool),
	ReportPeer(PeerId, i32),
	SetPriorityGroup(String, HashSet<PeerId>),
	AddToPriorityGroup(String, PeerId),
	RemoveFromPriorityGroup(String, PeerId),
}

/// Shared handle to the peer set manager (PSM). Distributed around the code.
#[derive(Debug, Clone)]
pub struct PeersetHandle {
	tx: mpsc::UnboundedSender<Action>,
}

impl PeersetHandle {
	/// Adds a new reserved peer. The peerset will make an effort to always remain connected to
	/// this peer.
	///
	/// Has no effect if the node was already a reserved peer.
	///
	/// > **Note**: Keep in mind that the networking has to know an address for this node,
	/// >			otherwise it will not be able to connect to it.
	pub fn add_reserved_peer(&self, peer_id: PeerId) {
		let _ = self.tx.unbounded_send(Action::AddReservedPeer(peer_id));
	}

	/// Remove a previously-added reserved peer.
	///
	/// Has no effect if the node was not a reserved peer.
	pub fn remove_reserved_peer(&self, peer_id: PeerId) {
		let _ = self.tx.unbounded_send(Action::RemoveReservedPeer(peer_id));
	}

	/// Sets whether or not the peerset only has connections .
	pub fn set_reserved_only(&self, reserved: bool) {
		let _ = self.tx.unbounded_send(Action::SetReservedOnly(reserved));
	}

	/// Reports an adjustment to the reputation of the given peer.
	pub fn report_peer(&self, peer_id: PeerId, score_diff: i32) {
		let _ = self.tx.unbounded_send(Action::ReportPeer(peer_id, score_diff));
	}

	/// Modify a priority group.
	pub fn set_priority_group(&self, group_id: String, peers: HashSet<PeerId>) {
		let _ = self.tx.unbounded_send(Action::SetPriorityGroup(group_id, peers));
	}

	/// Add a peer to a priority group.
	pub fn add_to_priority_group(&self, group_id: String, peer_id: PeerId) {
		let _ = self.tx.unbounded_send(Action::AddToPriorityGroup(group_id, peer_id));
	}

	/// Remove a peer from a priority group.
	pub fn remove_from_priority_group(&self, group_id: String, peer_id: PeerId) {
		let _ = self.tx.unbounded_send(Action::RemoveFromPriorityGroup(group_id, peer_id));
	}
}

/// Message that can be sent by the peer set manager (PSM).
#[derive(Debug, PartialEq)]
pub enum Message {
	/// Request to open a connection to the given peer. From the point of view of the PSM, we are
	/// immediately connected.
	Connect(PeerId),

	/// Drop the connection to the given peer, or cancel the connection attempt after a `Connect`.
	Drop(PeerId),

	/// Equivalent to `Connect` for the peer corresponding to this incoming index.
	Accept(IncomingIndex),

	/// Equivalent to `Drop` for the peer corresponding to this incoming index.
	Reject(IncomingIndex),
}

/// Opaque identifier for an incoming connection. Allocated by the network.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct IncomingIndex(pub u64);

impl From<u64> for IncomingIndex {
	fn from(val: u64) -> IncomingIndex {
		IncomingIndex(val)
	}
}

/// Configuration to pass when creating the peer set manager.
#[derive(Debug)]
pub struct PeersetConfig {
	/// Maximum number of ingoing links to peers.
	pub in_peers: u32,

	/// Maximum number of outgoing links to peers.
	pub out_peers: u32,

	/// List of bootstrap nodes to initialize the peer with.
	///
	/// > **Note**: Keep in mind that the networking has to know an address for these nodes,
	/// >			otherwise it will not be able to connect to them.
	pub bootnodes: Vec<PeerId>,

	/// If true, we only accept reserved nodes.
	pub reserved_only: bool,

	/// List of nodes that we should always be connected to.
	///
	/// > **Note**: Keep in mind that the networking has to know an address for these nodes,
	/// >			otherwise it will not be able to connect to them.
	pub reserved_nodes: Vec<PeerId>,
}

/// Side of the peer set manager owned by the network. In other words, the "receiving" side.
///
/// Implements the `Stream` trait and can be polled for messages. The `Stream` never ends and never
/// errors.
#[derive(Debug)]
pub struct Peerset {
	data: peersstate::PeersState,
	/// If true, we only accept reserved nodes.
	reserved_only: bool,
	/// Receiver for messages from the `PeersetHandle` and from `tx`.
	rx: mpsc::UnboundedReceiver<Action>,
	/// Sending side of `rx`.
	tx: mpsc::UnboundedSender<Action>,
	/// Queue of messages to be emitted when the `Peerset` is polled.
	message_queue: VecDeque<Message>,
	/// When the `Peerset` was created.
	created: Instant,
	/// Last time when we updated the reputations of connected nodes.
	latest_time_update: Instant,
}

impl Peerset {
	/// Builds a new peerset from the given configuration.
	pub fn from_config(config: PeersetConfig) -> (Peerset, PeersetHandle) {
		let (tx, rx) = mpsc::unbounded();

		let handle = PeersetHandle {
			tx: tx.clone(),
		};

		let mut peerset = Peerset {
			data: peersstate::PeersState::new(config.in_peers, config.out_peers, config.reserved_only),
			tx,
			rx,
			reserved_only: config.reserved_only,
			message_queue: VecDeque::new(),
			created: Instant::now(),
			latest_time_update: Instant::now(),
		};

		peerset.data.set_priority_group(RESERVED_NODES, config.reserved_nodes.into_iter().collect());
		for peer_id in config.bootnodes {
			if let peersstate::Peer::Unknown(entry) = peerset.data.peer(&peer_id) {
				entry.discover();
			} else {
				debug!(target: "peerset", "Duplicate bootnode in config: {:?}", peer_id);
			}
		}

		peerset.alloc_slots();
		(peerset, handle)
	}

	fn on_add_reserved_peer(&mut self, peer_id: PeerId) {
		let mut reserved = self.data.get_priority_group(RESERVED_NODES).unwrap_or_default();
		reserved.insert(peer_id);
		self.data.set_priority_group(RESERVED_NODES, reserved);
		self.alloc_slots();
	}

	fn on_remove_reserved_peer(&mut self, peer_id: PeerId) {
		let mut reserved = self.data.get_priority_group(RESERVED_NODES).unwrap_or_default();
		reserved.remove(&peer_id);
		self.data.set_priority_group(RESERVED_NODES, reserved);
		match self.data.peer(&peer_id) {
			peersstate::Peer::Connected(peer) => {
				if self.reserved_only {
					peer.disconnect();
					self.message_queue.push_back(Message::Drop(peer_id));
				}
			}
			peersstate::Peer::NotConnected(_) => {},
			peersstate::Peer::Unknown(_) => {},
		}
	}

	fn on_set_reserved_only(&mut self, reserved_only: bool) {
		self.reserved_only = reserved_only;
		self.data.set_priority_only(reserved_only);

		if self.reserved_only {
			// Disconnect non-reserved nodes.
			let reserved = self.data.get_priority_group(RESERVED_NODES).unwrap_or_default();
			for peer_id in self.data.connected_peers().cloned().collect::<Vec<_>>().into_iter() {
				let peer = self.data.peer(&peer_id).into_connected()
					.expect("We are enumerating connected peers, therefore the peer is connected; qed");
				if !reserved.contains(&peer_id) {
					peer.disconnect();
					self.message_queue.push_back(Message::Drop(peer_id));
				}
			}
		} else {
			self.alloc_slots();
		}
	}

	fn on_set_priority_group(&mut self, group_id: &str, peers: HashSet<PeerId>) {
		self.data.set_priority_group(group_id, peers);
		self.alloc_slots();
	}

	fn on_add_to_priority_group(&mut self, group_id: &str, peer_id: PeerId) {
		self.data.add_to_priority_group(group_id, peer_id);
		self.alloc_slots();
	}

	fn on_remove_from_priority_group(&mut self, group_id: &str, peer_id: PeerId) {
		self.data.remove_from_priority_group(group_id, &peer_id);
		self.alloc_slots();
	}

	fn on_report_peer(&mut self, peer_id: PeerId, score_diff: i32) {
		// We want reputations to be up-to-date before adjusting them.
		self.update_time();

		match self.data.peer(&peer_id) {
			peersstate::Peer::Connected(mut peer) => {
				peer.add_reputation(score_diff);
				if peer.reputation() < BANNED_THRESHOLD {
					peer.disconnect();
					self.message_queue.push_back(Message::Drop(peer_id));
				}
			},
			peersstate::Peer::NotConnected(mut peer) => peer.add_reputation(score_diff),
			peersstate::Peer::Unknown(peer) => peer.discover().add_reputation(score_diff),
		}
	}

	/// Updates the value of `self.latest_time_update` and performs all the updates that happen
	/// over time, such as reputation increases for staying connected.
	fn update_time(&mut self) {
		// We basically do `(now - self.latest_update).as_secs()`, except that by the way we do it
		// we know that we're not going to miss seconds because of rounding to integers.
		let secs_diff = {
			let now = Instant::now();
			let elapsed_latest = self.latest_time_update - self.created;
			let elapsed_now = now - self.created;
			self.latest_time_update = now;
			elapsed_now.as_secs() - elapsed_latest.as_secs()
		};

		// For each elapsed second, move the node reputation towards zero.
		// If we multiply each second the reputation by `k` (where `k` is between 0 and 1), it
		// takes `ln(0.5) / ln(k)` seconds to reduce the reputation by half. Use this formula to
		// empirically determine a value of `k` that looks correct.
		for _ in 0..secs_diff {
			for peer in self.data.peers().cloned().collect::<Vec<_>>() {
				// We use `k = 0.98`, so we divide by `50`. With that value, it takes 34.3 seconds
				// to reduce the reputation by half.
				fn reput_tick(reput: i32) -> i32 {
					let mut diff = reput / 50;
					if diff == 0 && reput < 0 {
						diff = -1;
					} else if diff == 0 && reput > 0 {
						diff = 1;
					}
					reput.saturating_sub(diff)
				}
				match self.data.peer(&peer) {
					peersstate::Peer::Connected(mut peer) =>
						peer.set_reputation(reput_tick(peer.reputation())),
					peersstate::Peer::NotConnected(mut peer) =>
						peer.set_reputation(reput_tick(peer.reputation())),
					peersstate::Peer::Unknown(_) => unreachable!("We iterate over known peers; qed")
				}
			}
		}
	}

	/// Try to fill available out slots with nodes.
	fn alloc_slots(&mut self) {
		self.update_time();

		// Try to grab the next node to attempt to connect to.
		while let Some(next) = {
			if self.reserved_only {
				self.data.priority_not_connected_peer_from_group(RESERVED_NODES)
			} else {
				self.data.priority_not_connected_peer()
			}
		} {
			match next.try_outgoing() {
				Ok(conn) => self.message_queue.push_back(Message::Connect(conn.into_peer_id())),
				Err(_) => break,	// No more slots available.
			}
		}

		loop {
			if self.reserved_only {
				break
			}

			// Try to grab the next node to attempt to connect to.
			let next = match self.data.highest_not_connected_peer() {
				Some(p) => p,
				None => break,	// No known node to add.
			};

			// Don't connect to nodes with an abysmal reputation.
			if next.reputation() < BANNED_THRESHOLD {
				break;
			}

			match next.try_outgoing() {
				Ok(conn) => self.message_queue.push_back(Message::Connect(conn.into_peer_id())),
				Err(_) => break,	// No more slots available.
			}
		}
	}

	/// Indicate that we received an incoming connection. Must be answered either with
	/// a corresponding `Accept` or `Reject`, except if we were already connected to this peer.
	///
	/// Note that this mechanism is orthogonal to `Connect`/`Drop`. Accepting an incoming
	/// connection implicitly means `Connect`, but incoming connections aren't cancelled by
	/// `dropped`.
	///
	// Implementation note: because of concurrency issues, it is possible that we push a `Connect`
	// message to the output channel with a `PeerId`, and that `incoming` gets called with the same
	// `PeerId` before that message has been read by the user. In this situation we must not answer.
	pub fn incoming(&mut self, peer_id: PeerId, index: IncomingIndex) {
		trace!(target: "peerset", "Incoming {:?}", peer_id);
		self.update_time();

		let not_connected = match self.data.peer(&peer_id) {
			// If we're already connected, don't answer, as the docs mention.
			peersstate::Peer::Connected(_) => return,
			peersstate::Peer::NotConnected(entry) => entry,
			peersstate::Peer::Unknown(entry) => entry.discover(),
		};

		if not_connected.reputation() < BANNED_THRESHOLD {
			self.message_queue.push_back(Message::Reject(index));
			return
		}

		match not_connected.try_accept_incoming() {
			Ok(_) => self.message_queue.push_back(Message::Accept(index)),
			Err(_) => self.message_queue.push_back(Message::Reject(index)),
		}
	}

	/// Indicate that we dropped an active connection with a peer, or that we failed to connect.
	///
	/// Must only be called after the PSM has either generated a `Connect` message with this
	/// `PeerId`, or accepted an incoming connection with this `PeerId`.
	pub fn dropped(&mut self, peer_id: PeerId) {
		trace!(target: "peerset", "Dropping {:?}", peer_id);

		// We want reputations to be up-to-date before adjusting them.
		self.update_time();

		match self.data.peer(&peer_id) {
			peersstate::Peer::Connected(mut entry) => {
				// Decrease the node's reputation so that we don't try it again and again and again.
				entry.add_reputation(DISCONNECT_REPUTATION_CHANGE);
				entry.disconnect();
			}
			peersstate::Peer::NotConnected(_) | peersstate::Peer::Unknown(_) =>
				error!(target: "peerset", "Received dropped() for non-connected node"),
		}

		self.alloc_slots();
	}

	/// Adds discovered peer ids to the PSM.
	///
	/// > **Note**: There is no equivalent "expired" message, meaning that it is the responsibility
	/// >			of the PSM to remove `PeerId`s that fail to dial too often.
	pub fn discovered<I: IntoIterator<Item = PeerId>>(&mut self, peer_ids: I) {
		let mut discovered_any = false;

		for peer_id in peer_ids {
			if let peersstate::Peer::Unknown(entry) = self.data.peer(&peer_id) {
				entry.discover();
				discovered_any = true;
			}
		}

		if discovered_any {
			self.alloc_slots();
		}
	}

	/// Reports an adjustment to the reputation of the given peer.
	pub fn report_peer(&mut self, peer_id: PeerId, score_diff: i32) {
		// We don't immediately perform the adjustments in order to have state consistency. We
		// don't want the reporting here to take priority over messages sent using the
		// `PeersetHandle`.
		let _ = self.tx.unbounded_send(Action::ReportPeer(peer_id, score_diff));
	}

	/// Produces a JSON object containing the state of the peerset manager, for debugging purposes.
	pub fn debug_info(&mut self) -> serde_json::Value {
		self.update_time();

		json!({
			"nodes": self.data.peers().cloned().collect::<Vec<_>>().into_iter().map(|peer_id| {
				let state = match self.data.peer(&peer_id) {
					peersstate::Peer::Connected(entry) => json!({
						"connected": true,
						"reputation": entry.reputation()
					}),
					peersstate::Peer::NotConnected(entry) => json!({
						"connected": false,
						"reputation": entry.reputation()
					}),
					peersstate::Peer::Unknown(_) =>
						unreachable!("We iterate over the known peers; QED")
				};

				(peer_id.to_base58(), state)
			}).collect::<HashMap<_, _>>(),
			"reserved_only": self.reserved_only,
			"message_queue": self.message_queue.len(),
		})
	}

	/// Returns priority group by id.
	pub fn get_priority_group(&self, group_id: &str) -> Option<HashSet<PeerId>> {
		self.data.get_priority_group(group_id)
	}
}

impl Stream for Peerset {
	type Item = Message;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		loop {
			if let Some(message) = self.message_queue.pop_front() {
				return Poll::Ready(Some(message));
			}

			let action = match Stream::poll_next(Pin::new(&mut self.rx), cx) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(Some(event)) => event,
				Poll::Ready(None) => return Poll::Pending,
			};

			match action {
				Action::AddReservedPeer(peer_id) =>
					self.on_add_reserved_peer(peer_id),
				Action::RemoveReservedPeer(peer_id) =>
					self.on_remove_reserved_peer(peer_id),
				Action::SetReservedOnly(reserved) =>
					self.on_set_reserved_only(reserved),
				Action::ReportPeer(peer_id, score_diff) =>
					self.on_report_peer(peer_id, score_diff),
				Action::SetPriorityGroup(group_id, peers) =>
					self.on_set_priority_group(&group_id, peers),
				Action::AddToPriorityGroup(group_id, peer_id) =>
					self.on_add_to_priority_group(&group_id, peer_id),
				Action::RemoveFromPriorityGroup(group_id, peer_id) =>
					self.on_remove_from_priority_group(&group_id, peer_id),
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use libp2p::PeerId;
	use futures::prelude::*;
	use super::{PeersetConfig, Peerset, Message, IncomingIndex, BANNED_THRESHOLD};
	use std::{pin::Pin, task::Poll, thread, time::Duration};

	fn assert_messages(mut peerset: Peerset, messages: Vec<Message>) -> Peerset {
		for expected_message in messages {
			let (message, p) = next_message(peerset).expect("expected message");
			assert_eq!(message, expected_message);
			peerset = p;
		}
		assert!(peerset.message_queue.is_empty(), peerset.message_queue);
		peerset
	}

	fn next_message(mut peerset: Peerset) -> Result<(Message, Peerset), ()> {
		let next = futures::executor::block_on_stream(&mut peerset).next();
		let message = next.ok_or_else(|| ())?;
		Ok((message, peerset))
	}

	#[test]
	fn test_peerset_add_reserved_peer() {
		let bootnode = PeerId::random();
		let reserved_peer = PeerId::random();
		let reserved_peer2 = PeerId::random();
		let config = PeersetConfig {
			in_peers: 0,
			out_peers: 2,
			bootnodes: vec![bootnode],
			reserved_only: true,
			reserved_nodes: Vec::new(),
		};

		let (peerset, handle) = Peerset::from_config(config);
		handle.add_reserved_peer(reserved_peer.clone());
		handle.add_reserved_peer(reserved_peer2.clone());

		assert_messages(peerset, vec![
			Message::Connect(reserved_peer),
			Message::Connect(reserved_peer2)
		]);
	}

	#[test]
	fn test_peerset_incoming() {
		let bootnode = PeerId::random();
		let incoming = PeerId::random();
		let incoming2 = PeerId::random();
		let incoming3 = PeerId::random();
		let ii = IncomingIndex(1);
		let ii2 = IncomingIndex(2);
		let ii3 = IncomingIndex(3);
		let ii4 = IncomingIndex(3);
		let config = PeersetConfig {
			in_peers: 2,
			out_peers: 1,
			bootnodes: vec![bootnode.clone()],
			reserved_only: false,
			reserved_nodes: Vec::new(),
		};

		let (mut peerset, _handle) = Peerset::from_config(config);
		peerset.incoming(incoming.clone(), ii);
		peerset.incoming(incoming.clone(), ii4);
		peerset.incoming(incoming2.clone(), ii2);
		peerset.incoming(incoming3.clone(), ii3);

		assert_messages(peerset, vec![
			Message::Connect(bootnode.clone()),
			Message::Accept(ii),
			Message::Accept(ii2),
			Message::Reject(ii3),
		]);
	}

	#[test]
	fn test_peerset_discovered() {
		let bootnode = PeerId::random();
		let discovered = PeerId::random();
		let discovered2 = PeerId::random();
		let config = PeersetConfig {
			in_peers: 0,
			out_peers: 2,
			bootnodes: vec![bootnode.clone()],
			reserved_only: false,
			reserved_nodes: vec![],
		};

		let (mut peerset, _handle) = Peerset::from_config(config);
		peerset.discovered(Some(discovered.clone()));
		peerset.discovered(Some(discovered.clone()));
		peerset.discovered(Some(discovered2));

		assert_messages(peerset, vec![
			Message::Connect(bootnode),
			Message::Connect(discovered),
		]);
	}

	#[test]
	fn test_peerset_banned() {
		let (mut peerset, handle) = Peerset::from_config(PeersetConfig {
			in_peers: 25,
			out_peers: 25,
			bootnodes: vec![],
			reserved_only: false,
			reserved_nodes: vec![],
		});

		// We ban a node by setting its reputation under the threshold.
		let peer_id = PeerId::random();
		handle.report_peer(peer_id.clone(), BANNED_THRESHOLD - 1);

		let fut = futures::future::poll_fn(move |cx| {
			// We need one polling for the message to be processed.
			assert_eq!(Stream::poll_next(Pin::new(&mut peerset), cx), Poll::Pending);

			// Check that an incoming connection from that node gets refused.
			peerset.incoming(peer_id.clone(), IncomingIndex(1));
			if let Poll::Ready(msg) = Stream::poll_next(Pin::new(&mut peerset), cx) {
				assert_eq!(msg.unwrap(), Message::Reject(IncomingIndex(1)));
			} else {
				panic!()
			}

			// Wait a bit for the node's reputation to go above the threshold.
			thread::sleep(Duration::from_millis(1500));

			// Try again. This time the node should be accepted.
			peerset.incoming(peer_id.clone(), IncomingIndex(2));
			while let Poll::Ready(msg) = Stream::poll_next(Pin::new(&mut peerset), cx) {
				assert_eq!(msg.unwrap(), Message::Accept(IncomingIndex(2)));
			}

			Poll::Ready(())
		});

		futures::executor::block_on(fut);
	}
}


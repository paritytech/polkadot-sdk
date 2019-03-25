// Copyright 2019 Parity Technologies (UK) Ltd.
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

use crate::custom_proto::handler::{CustomProtoHandlerProto, CustomProtoHandlerOut, CustomProtoHandlerIn};
use crate::custom_proto::upgrade::{CustomMessage, RegisteredProtocol};
use fnv::FnvHashMap;
use futures::prelude::*;
use libp2p::core::swarm::{ConnectedPoint, NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use libp2p::core::{Multiaddr, PeerId};
use log::{debug, error, trace, warn};
use smallvec::SmallVec;
use std::{collections::hash_map::Entry, cmp, error, io, marker::PhantomData, mem, time::Duration, time::Instant};
use tokio_io::{AsyncRead, AsyncWrite};

/// Network behaviour that handles opening substreams for custom protocols with other nodes.
pub struct CustomProto<TMessage, TSubstream> {
	/// List of protocols to open with peers. Never modified.
	protocol: RegisteredProtocol<TMessage>,

	/// Receiver for instructions about who to connect to or disconnect from.
	peerset: substrate_peerset::PeersetMut,

	/// List of peers in our state.
	peers: FnvHashMap<PeerId, PeerState>,

	/// List of incoming messages we have sent to the peer set manager and that are waiting for an
	/// answer.
	incoming: SmallVec<[IncomingPeer; 6]>,

	/// We generate indices to identify incoming connections. This is the next value for the index
	/// to use when a connection is incoming.
	next_incoming_index: substrate_peerset::IncomingIndex,

	/// Events to produce from `poll()`.
	events: SmallVec<[NetworkBehaviourAction<CustomProtoHandlerIn<TMessage>, CustomProtoOut<TMessage>>; 4]>,

	/// Marker to pin the generics.
	marker: PhantomData<TSubstream>,
}

/// State of a peer we're connected to.
#[derive(Debug)]
enum PeerState {
	/// State is poisoned. This is a temporary state for a peer and we should always switch back
	/// to it later. If it is found in the wild, that means there was either a panic or a bug in
	/// the state machine code.
	Poisoned,

	/// The peer misbehaved. If the PSM wants us to connect to this node, we will add an artificial
	/// delay to the connection.
	Banned {
		/// Until when the node is banned.
		until: Instant,
	},

	/// The peerset requested that we connect to this peer. We are not connected to this node.
	PendingRequest {
		/// When to actually start dialing.
		timer: tokio_timer::Delay,
	},

	/// The peerset requested that we connect to this peer. We are currently dialing this peer.
	Requested,

	/// We are connected to this peer but the peerset refused it. This peer can still perform
	/// Kademlia queries and such, but should get disconnected in a few seconds.
	Disabled {
		/// How we are connected to this peer.
		connected_point: ConnectedPoint,
		/// If true, we still have a custom protocol open with it. It will likely get closed in
		/// a short amount of time, but we need to keep the information in order to not have a
		/// state mismatch.
		open: bool,
		/// If `Some`, the node is banned until the given `Instant`.
		banned_until: Option<Instant>,
	},

	/// We are connected to this peer but we are not opening any Substrate substream. The handler
	/// will be enabled when `timer` fires. This peer can still perform Kademlia queries and such,
	/// but should get disconnected in a few seconds.
	DisabledPendingEnable {
		/// How we are connected to this peer.
		connected_point: ConnectedPoint,
		/// If true, we still have a custom protocol open with it. It will likely get closed in
		/// a short amount of time, but we need to keep the information in order to not have a
		/// state mismatch.
		open: bool,
		/// When to enable this remote.
		timer: tokio_timer::Delay,
	},

	/// We are connected to this peer and the peerset has accepted it. The handler is in the
	/// enabled state.
	Enabled {
		/// How we are connected to this peer.
		connected_point: ConnectedPoint,
		/// If true, we have a custom protocol open with this peer.
		open: bool,
	},

	/// We are connected to this peer, and we sent an incoming message to the peerset. The handler
	/// is in initialization mode. We are waiting for the Accept or Reject from the peerset. There
	/// is a corresponding entry in `incoming`.
	Incoming {
		/// How we are connected to this peer.
		connected_point: ConnectedPoint,
	},
}

/// State of an "incoming" message sent to the peer set manager.
#[derive(Debug)]
struct IncomingPeer {
	/// Id of the node that is concerned.
	peer_id: PeerId,
	/// If true, this "incoming" still corresponds to an actual connection. If false, then the
	/// connection corresponding to it has been closed or replaced already.
	alive: bool,
	/// Id that the we sent to the peerset.
	incoming_id: substrate_peerset::IncomingIndex,
}

/// Event that can be emitted by the `CustomProto`.
#[derive(Debug)]
pub enum CustomProtoOut<TMessage> {
	/// Opened a custom protocol with the remote.
	CustomProtocolOpen {
		/// Version of the protocol that has been opened.
		version: u8,
		/// Id of the node we have opened a connection with.
		peer_id: PeerId,
		/// Endpoint used for this custom protocol.
		endpoint: ConnectedPoint,
	},

	/// Closed a custom protocol with the remote.
	CustomProtocolClosed {
		/// Id of the peer we were connected to.
		peer_id: PeerId,
		/// Reason why the substream closed. If `Ok`, then it's a graceful exit (EOF).
		result: io::Result<()>,
	},

	/// Receives a message on a custom protocol substream.
	CustomMessage {
		/// Id of the peer the message came from.
		peer_id: PeerId,
		/// Message that has been received.
		message: TMessage,
	},

	/// The substream used by the protocol is pretty large. We should print avoid sending more
	/// messages on it if possible.
	Clogged {
		/// Id of the peer which is clogged.
		peer_id: PeerId,
		/// Copy of the messages that are within the buffer, for further diagnostic.
		messages: Vec<TMessage>,
	},
}

impl<TMessage, TSubstream> CustomProto<TMessage, TSubstream> {
	/// Creates a `CustomProtos`.
	pub fn new(
		protocol: RegisteredProtocol<TMessage>,
		peerset: substrate_peerset::PeersetMut,
	) -> Self {
		CustomProto {
			protocol,
			peerset,
			peers: FnvHashMap::default(),
			incoming: SmallVec::new(),
			next_incoming_index: substrate_peerset::IncomingIndex(0),
			events: SmallVec::new(),
			marker: PhantomData,
		}
	}

	/// Disconnects the given peer if we are connected to it.
	pub fn disconnect_peer(&mut self, peer_id: &PeerId) {
		debug!(target: "sub-libp2p", "Disconnecting {:?} by request from the external API", peer_id);
		self.disconnect_peer_inner(peer_id, None);
	}

	/// Inner implementation of `disconnect_peer`. If `ban` is `Some`, we ban the node for the
	/// specific duration.
	fn disconnect_peer_inner(&mut self, peer_id: &PeerId, ban: Option<Duration>) {
		let mut entry = if let Entry::Occupied(entry) = self.peers.entry(peer_id.clone()) {
			entry
		} else {
			return
		};

		match mem::replace(entry.get_mut(), PeerState::Poisoned) {
			// We're not connected anyway.
			st @ PeerState::Disabled { .. } => *entry.into_mut() = st,
			st @ PeerState::Requested => *entry.into_mut() = st,
			st @ PeerState::PendingRequest { .. } => *entry.into_mut() = st,
			st @ PeerState::Banned { .. } => *entry.into_mut() = st,

			// DisabledPendingEnable => Disabled.
			PeerState::DisabledPendingEnable { open, connected_point, timer } => {
				debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", peer_id);
				self.peerset.dropped(peer_id);
				let banned_until = Some(if let Some(ban) = ban {
					cmp::max(timer.deadline(), Instant::now() + ban)
				} else {
					timer.deadline()
				});
				*entry.into_mut() = PeerState::Disabled { open, connected_point, banned_until }
			},

			// Enabled => Disabled.
			PeerState::Enabled { open, connected_point } => {
				debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", peer_id);
				self.peerset.dropped(peer_id);
				debug!(target: "sub-libp2p", "Handler({:?}) <= Disable", peer_id);
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: peer_id.clone(),
					event: CustomProtoHandlerIn::Disable,
				});
				let banned_until = ban.map(|dur| Instant::now() + dur);
				*entry.into_mut() = PeerState::Disabled { open, connected_point, banned_until }
			},

			// Incoming => Disabled.
			PeerState::Incoming { connected_point, .. } => {
				let inc = if let Some(inc) = self.incoming.iter_mut()
					.find(|i| i.peer_id == *entry.key() && i.alive) {
					inc
				} else {
					error!(target: "sub-libp2p", "State mismatch in libp2p: no entry in \
						incoming for incoming peer");
					return
				};

				inc.alive = false;
				debug!(target: "sub-libp2p", "Handler({:?}) <= Disable", peer_id);
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: peer_id.clone(),
					event: CustomProtoHandlerIn::Disable,
				});
				let banned_until = ban.map(|dur| Instant::now() + dur);
				*entry.into_mut() = PeerState::Disabled { open: false, connected_point, banned_until }
			},

			PeerState::Poisoned =>
				error!(target: "sub-libp2p", "State of {:?} is poisoned", peer_id),
		}
	}

	/// Returns true if we try to open protocols with the given peer.
	pub fn is_enabled(&self, peer_id: &PeerId) -> bool {
		match self.peers.get(peer_id) {
			None => false,
			Some(PeerState::Disabled { .. }) => false,
			Some(PeerState::DisabledPendingEnable { .. }) => false,
			Some(PeerState::Enabled { .. }) => true,
			Some(PeerState::Incoming { .. }) => false,
			Some(PeerState::Requested) => false,
			Some(PeerState::PendingRequest { .. }) => false,
			Some(PeerState::Banned { .. }) => false,
			Some(PeerState::Poisoned) => false,
		}
	}

	/// Returns true if we have opened a protocol with the given peer.
	pub fn is_open(&self, peer_id: &PeerId) -> bool {
		match self.peers.get(peer_id) {
			None => false,
			Some(PeerState::Disabled { open, .. }) => *open,
			Some(PeerState::DisabledPendingEnable { open, .. }) => *open,
			Some(PeerState::Enabled { open, .. }) => *open,
			Some(PeerState::Incoming { .. }) => false,
			Some(PeerState::Requested) => false,
			Some(PeerState::PendingRequest { .. }) => false,
			Some(PeerState::Banned { .. }) => false,
			Some(PeerState::Poisoned) => false,
		}
	}

	/// Sends a message to a peer.
	///
	/// Has no effect if the custom protocol is not open with the given peer.
	///
	/// Also note that even we have a valid open substream, it may in fact be already closed
	/// without us knowing, in which case the packet will not be received.
	pub fn send_packet(&mut self, target: &PeerId, message: TMessage) {
		if !self.is_open(target) {
			return;
		}

		trace!(target: "sub-libp2p", "Handler({:?}) <= Packet", target);
		self.events.push(NetworkBehaviourAction::SendEvent {
			peer_id: target.clone(),
			event: CustomProtoHandlerIn::SendCustomMessage {
				message,
			}
		});
	}

	/// Indicates to the peerset that we have discovered new addresses for a given node.
	pub fn add_discovered_node(&mut self, peer_id: &PeerId) {
		debug!(target: "sub-libp2p", "PSM <= Discovered({:?})", peer_id);
		self.peerset.discovered(peer_id.clone())
	}

	/// Function that is called when the peerset wants us to connect to a node.
	fn peerset_report_connect(&mut self, peer_id: PeerId) {
		let mut occ_entry = match self.peers.entry(peer_id) {
			Entry::Occupied(entry) => entry,
			Entry::Vacant(entry) => {
				// If there's no entry in `self.peers`, start dialing.
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Starting to connect", entry.key());
				debug!(target: "sub-libp2p", "Libp2p <= Dial {:?}", entry.key());
				self.events.push(NetworkBehaviourAction::DialPeer { peer_id: entry.key().clone() });
				entry.insert(PeerState::Requested);
				return;
			}
		};

		match mem::replace(occ_entry.get_mut(), PeerState::Poisoned) {
			PeerState::Banned { ref until } if *until > Instant::now() => {
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Will start to connect at \
					until {:?}", occ_entry.key(), until);
				*occ_entry.into_mut() = PeerState::PendingRequest {
					timer: tokio_timer::Delay::new(until.clone()),
				};
			},

			PeerState::Banned { .. } => {
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Starting to connect", occ_entry.key());
				debug!(target: "sub-libp2p", "Libp2p <= Dial {:?}", occ_entry.key());
				self.events.push(NetworkBehaviourAction::DialPeer { peer_id: occ_entry.key().clone() });
				*occ_entry.into_mut() = PeerState::Requested;
			},

			PeerState::Disabled { open, ref connected_point, banned_until: Some(ref banned) }
				if *banned > Instant::now() => {
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Has idle connection through \
					{:?} but node is banned until {:?}", occ_entry.key(), connected_point, banned);
				*occ_entry.into_mut() = PeerState::DisabledPendingEnable {
					connected_point: connected_point.clone(),
					open,
					timer: tokio_timer::Delay::new(banned.clone()),
				};
			},

			PeerState::Disabled { open, connected_point, banned_until: _ } => {
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Enabling previously-idle \
					connection through {:?}", occ_entry.key(), connected_point);
				debug!(target: "sub-libp2p", "Handler({:?}) <= Enable", occ_entry.key());
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: occ_entry.key().clone(),
					event: CustomProtoHandlerIn::Enable(connected_point.clone().into()),
				});
				*occ_entry.into_mut() = PeerState::Enabled { connected_point, open };
			},

			PeerState::Incoming { connected_point, .. } => {
				debug!(target: "sub-libp2p", "PSM => Connect({:?}): Enabling incoming \
					connection through {:?}", occ_entry.key(), connected_point);
				if let Some(inc) = self.incoming.iter_mut()
					.find(|i| i.peer_id == *occ_entry.key() && i.alive) {
					inc.alive = false;
				} else {
					error!(target: "sub-libp2p", "State mismatch in libp2p: no entry in \
						incoming for incoming peer")
				}
				debug!(target: "sub-libp2p", "Handler({:?}) <= Enable", occ_entry.key());
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: occ_entry.key().clone(),
					event: CustomProtoHandlerIn::Enable(connected_point.clone().into()),
				});
				*occ_entry.into_mut() = PeerState::Enabled { connected_point, open: false };
			},

			st @ PeerState::Enabled { .. } => {
				warn!(target: "sub-libp2p", "PSM => Connect({:?}): Already connected to this \
					peer", occ_entry.key());
				*occ_entry.into_mut() = st;
			},
			st @ PeerState::DisabledPendingEnable { .. } => {
				warn!(target: "sub-libp2p", "PSM => Connect({:?}): Already have an idle \
					connection to this peer and waiting to enable it", occ_entry.key());
				*occ_entry.into_mut() = st;
			},
			st @ PeerState::Requested { .. } | st @ PeerState::PendingRequest { .. } => {
				warn!(target: "sub-libp2p", "PSM => Connect({:?}): Received a previous \
					request for that peer", occ_entry.key());
				*occ_entry.into_mut() = st;
			},

			PeerState::Poisoned =>
				error!(target: "sub-libp2p", "State of {:?} is poisoned", occ_entry.key()),
		}
	}

	/// Function that is called when the peerset wants us to disconnect from a node.
	fn peerset_report_disconnect(&mut self, peer_id: PeerId) {
		let mut entry = match self.peers.entry(peer_id) {
			Entry::Occupied(entry) => entry,
			Entry::Vacant(entry) => {
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Node already disabled", entry.key());
				return
			}
		};

		match mem::replace(entry.get_mut(), PeerState::Poisoned) {
			st @ PeerState::Disabled { .. } | st @ PeerState::Banned { .. } => {
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Node already disabled", entry.key());
				*entry.into_mut() = st;
			},

			PeerState::DisabledPendingEnable { open, connected_point, timer } => {
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Interrupting pending \
					enable", entry.key());
				*entry.into_mut() = PeerState::Disabled {
					open,
					connected_point,
					banned_until: Some(timer.deadline()),
				};
			},

			PeerState::Enabled { open, connected_point } => {
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Disabling connection", entry.key());
				debug!(target: "sub-libp2p", "Handler({:?}) <= Disable", entry.key());
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: entry.key().clone(),
					event: CustomProtoHandlerIn::Disable,
				});
				*entry.into_mut() = PeerState::Disabled { open, connected_point, banned_until: None }
			},
			st @ PeerState::Incoming { .. } => {
				error!(target: "sub-libp2p", "PSM => Drop({:?}): Was in incoming mode",
					entry.key());
				*entry.into_mut() = st;
			},
			PeerState::Requested => {
				// We don't cancel dialing. Libp2p doesn't expose that on purpose, as other
				// sub-systems (such as the discovery mechanism) may require dialing this node as
				// well at the same time.
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Was not yet connected", entry.key());
				entry.remove();
			},
			PeerState::PendingRequest { timer } => {
				debug!(target: "sub-libp2p", "PSM => Drop({:?}): Was not yet connected", entry.key());
				*entry.into_mut() = PeerState::Banned { until: timer.deadline() }
			},

			PeerState::Poisoned =>
				error!(target: "sub-libp2p", "State of {:?} is poisoned", entry.key()),
		}
	}

	/// Function that is called when the peerset wants us to accept an incoming node.
	fn peerset_report_accept(&mut self, index: substrate_peerset::IncomingIndex) {
		let incoming = if let Some(pos) = self.incoming.iter().position(|i| i.incoming_id == index) {
			self.incoming.remove(pos)
		} else {
			error!(target: "sub-libp2p", "PSM => Accept({:?}): Invalid index", index);
			return
		};

		if !incoming.alive {
			debug!(target: "sub-libp2p", "PSM => Accept({:?}, {:?}): Obsolete incoming,
				sending back dropped", index, incoming.peer_id);
			debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", incoming.peer_id);
			self.peerset.dropped(&incoming.peer_id);
			return
		}

		let state = if let Some(state) = self.peers.get_mut(&incoming.peer_id) {
			state
		} else {
			error!(target: "sub-libp2p", "State mismatch in libp2p: no entry in peers \
				corresponding to an alive incoming");
			return
		};

		let connected_point = if let PeerState::Incoming { connected_point } = state {
			connected_point.clone()
		} else {
			error!(target: "sub-libp2p", "State mismatch in libp2p: entry in peers corresponding \
				to an alive incoming is not in incoming state");
			return
		};

		debug!(target: "sub-libp2p", "PSM => Accept({:?}, {:?}): Enabling connection \
			through {:?}", index, incoming.peer_id, connected_point);
		debug!(target: "sub-libp2p", "Handler({:?}) <= Enable", incoming.peer_id);
		self.events.push(NetworkBehaviourAction::SendEvent {
			peer_id: incoming.peer_id,
			event: CustomProtoHandlerIn::Enable(connected_point.clone().into()),
		});

		*state = PeerState::Enabled { open: false, connected_point };
	}

	/// Function that is called when the peerset wants us to reject an incoming node.
	fn peerset_report_reject(&mut self, index: substrate_peerset::IncomingIndex) {
		let incoming = if let Some(pos) = self.incoming.iter().position(|i| i.incoming_id == index) {
			self.incoming.remove(pos)
		} else {
			error!(target: "sub-libp2p", "PSM => Reject({:?}): Invalid index", index);
			return
		};

		if !incoming.alive {
			error!(target: "sub-libp2p", "PSM => Reject({:?}, {:?}): Obsolete incoming, \
				ignoring", index, incoming.peer_id);
			return
		}

		let state = if let Some(state) = self.peers.get_mut(&incoming.peer_id) {
			state
		} else {
			error!(target: "sub-libp2p", "State mismatch in libp2p: no entry in peers \
				corresponding to an alive incoming");
			return
		};

		let connected_point = if let PeerState::Incoming { connected_point } = state {
			connected_point.clone()
		} else {
			error!(target: "sub-libp2p", "State mismatch in libp2p: entry in peers corresponding \
				to an alive incoming is not in incoming state");
			return
		};

		debug!(target: "sub-libp2p", "PSM => Reject({:?}, {:?}): Rejecting connection through \
			{:?}", index, incoming.peer_id, connected_point);
		debug!(target: "sub-libp2p", "Handler({:?}) <= Disable", incoming.peer_id);
		self.events.push(NetworkBehaviourAction::SendEvent {
			peer_id: incoming.peer_id,
			event: CustomProtoHandlerIn::Disable,
		});
		*state = PeerState::Disabled { open: false, connected_point, banned_until: None };
	}
}

impl<TMessage, TSubstream> NetworkBehaviour for CustomProto<TMessage, TSubstream>
where
	TSubstream: AsyncRead + AsyncWrite,
	TMessage: CustomMessage,
{
	type ProtocolsHandler = CustomProtoHandlerProto<TMessage, TSubstream>;
	type OutEvent = CustomProtoOut<TMessage>;

	fn new_handler(&mut self) -> Self::ProtocolsHandler {
		CustomProtoHandlerProto::new(self.protocol.clone())
	}

	fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
		Vec::new()
	}

	fn inject_connected(&mut self, peer_id: PeerId, connected_point: ConnectedPoint) {
		match (self.peers.entry(peer_id), connected_point) {
			(Entry::Occupied(mut entry), connected_point) => {
				match mem::replace(entry.get_mut(), PeerState::Poisoned) {
					PeerState::Requested | PeerState::PendingRequest { .. } |
					PeerState::Banned { .. } => {
						debug!(target: "sub-libp2p", "Libp2p => Connected({:?}): Connection \
							requested by PSM (through {:?})", entry.key(), connected_point);
						debug!(target: "sub-libp2p", "Handler({:?}) <= Enable", entry.key());
						self.events.push(NetworkBehaviourAction::SendEvent {
							peer_id: entry.key().clone(),
							event: CustomProtoHandlerIn::Enable(connected_point.clone().into()),
						});
						*entry.into_mut() = PeerState::Enabled { open: false, connected_point };
					}
					st @ _ => {
						// This is a serious bug either in this state machine or in libp2p.
						error!(target: "sub-libp2p", "Received inject_connected for \
							already-connected node; state is {:?}", st);
						*entry.into_mut() = st;
						return
					}
				}
			}

			(Entry::Vacant(entry), connected_point @ ConnectedPoint::Listener { .. }) => {
				let incoming_id = self.next_incoming_index.clone();
				self.next_incoming_index.0 = match self.next_incoming_index.0.checked_add(1) {
					Some(v) => v,
					None => {
						error!(target: "sub-libp2p", "Overflow in next_incoming_index");
						return
					}
				};
				debug!(target: "sub-libp2p", "Libp2p => Connected({:?}): Incoming connection",
					entry.key());
				debug!(target: "sub-libp2p", "PSM <= Incoming({:?}, {:?}): Through {:?}",
					incoming_id, entry.key(), connected_point);
				self.peerset.incoming(entry.key().clone(), incoming_id);
				self.incoming.push(IncomingPeer {
					peer_id: entry.key().clone(),
					alive: true,
					incoming_id,
				});
				entry.insert(PeerState::Incoming { connected_point });
			}

			(Entry::Vacant(entry), connected_point) => {
				debug!(target: "sub-libp2p", "Libp2p => Connected({:?}): Requested by something \
					else than PSM, disabling", entry.key());
				debug!(target: "sub-libp2p", "Handler({:?}) <= Disable", entry.key());
				self.events.push(NetworkBehaviourAction::SendEvent {
					peer_id: entry.key().clone(),
					event: CustomProtoHandlerIn::Disable,
				});
				entry.insert(PeerState::Disabled { open: false, connected_point, banned_until: None });
			}
		}
	}

	fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
		match self.peers.remove(peer_id) {
			None | Some(PeerState::Requested) | Some(PeerState::PendingRequest { .. }) |
			Some(PeerState::Banned { .. }) =>
				// This is a serious bug either in this state machine or in libp2p.
				error!(target: "sub-libp2p", "Received inject_disconnected for non-connected \
					node {:?}", peer_id),

			Some(PeerState::Disabled { open, banned_until, .. }) => {
				debug!(target: "sub-libp2p", "Libp2p => Disconnected({:?}): Was disabled \
					(through {:?})", peer_id, endpoint);
				if let Some(until) = banned_until {
					self.peers.insert(peer_id.clone(), PeerState::Banned { until });
				}
				if open {
					debug!(target: "sub-libp2p", "External API <= Closed({:?})", peer_id);
					let event = CustomProtoOut::CustomProtocolClosed {
						peer_id: peer_id.clone(),
						result: Ok(()),
					};

					self.events.push(NetworkBehaviourAction::GenerateEvent(event));
				}
			}

			Some(PeerState::DisabledPendingEnable { open, timer, .. }) => {
				debug!(target: "sub-libp2p", "Libp2p => Disconnected({:?}): Was disabled \
					(through {:?}) but pending enable", peer_id, endpoint);
				debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", peer_id);
				self.peerset.dropped(peer_id);
				self.peers.insert(peer_id.clone(), PeerState::Banned { until: timer.deadline() });
				if open {
					debug!(target: "sub-libp2p", "External API <= Closed({:?})", peer_id);
					let event = CustomProtoOut::CustomProtocolClosed {
						peer_id: peer_id.clone(),
						result: Ok(()),
					};

					self.events.push(NetworkBehaviourAction::GenerateEvent(event));
				}
			}

			Some(PeerState::Enabled { open, .. }) => {
				debug!(target: "sub-libp2p", "Libp2p => Disconnected({:?}): Was enabled \
					(through {:?})", peer_id, endpoint);
				debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", peer_id);
				self.peerset.dropped(peer_id);

				if open {
					debug!(target: "sub-libp2p", "External API <= Closed({:?})", peer_id);
					let event = CustomProtoOut::CustomProtocolClosed {
						peer_id: peer_id.clone(),
						result: Ok(()),
					};

					self.events.push(NetworkBehaviourAction::GenerateEvent(event));
				}
			}

			// In the incoming state, we don't report "Dropped". Instead we will just ignore the
			// corresponding Accept/Reject.
			Some(PeerState::Incoming { .. }) => {
				if let Some(state) = self.incoming.iter_mut().find(|i| i.peer_id == *peer_id) {
					debug!(target: "sub-libp2p", "Libp2p => Disconnected({:?}): Was in incoming \
						mode (id {:?}, through {:?})", peer_id, state.incoming_id, endpoint);
					state.alive = false;
				} else {
					error!(target: "sub-libp2p", "State mismatch in libp2p: no entry in incoming \
						corresponding to an incoming state in peers")
				}
			}

			Some(PeerState::Poisoned) =>
				error!(target: "sub-libp2p", "State of {:?} is poisoned", peer_id),
		}
	}

	fn inject_addr_reach_failure(&mut self, peer_id: Option<&PeerId>, addr: &Multiaddr, error: &dyn error::Error) {
		trace!(target: "sub-libp2p", "Libp2p => Reach failure for {:?} through {:?}: {:?}", peer_id, addr, error);
	}

	fn inject_dial_failure(&mut self, peer_id: &PeerId) {
		if let Entry::Occupied(mut entry) = self.peers.entry(peer_id.clone()) {
			match mem::replace(entry.get_mut(), PeerState::Poisoned) {
				// The node is not in our list.
				st @ PeerState::Banned { .. } => {
					trace!(target: "sub-libp2p", "Libp2p => Dial failure for {:?}", peer_id);
					*entry.into_mut() = st;
				},

				// "Basic" situation: we failed to reach a node that the peerset requested.
				PeerState::Requested | PeerState::PendingRequest { .. } => {
					debug!(target: "sub-libp2p", "Libp2p => Dial failure for {:?}", peer_id);
					*entry.into_mut() = PeerState::Banned {
						until: Instant::now() + Duration::from_secs(5)
					};
					debug!(target: "sub-libp2p", "PSM <= Dropped({:?})", peer_id);
					self.peerset.dropped(peer_id)
				},

				// We can still get dial failures even if we are already connected to the node,
				// as an extra diagnostic for an earlier attempt.
				st @ PeerState::Disabled { .. } | st @ PeerState::Enabled { .. } |
					st @ PeerState::DisabledPendingEnable { .. } | st @ PeerState::Incoming { .. } => {
					debug!(target: "sub-libp2p", "Libp2p => Dial failure for {:?}", peer_id);
					*entry.into_mut() = st;
				},

				PeerState::Poisoned =>
					error!(target: "sub-libp2p", "State of {:?} is poisoned", peer_id),
			}

		} else {
			// The node is not in our list.
			trace!(target: "sub-libp2p", "Libp2p => Dial failure for {:?}", peer_id);
		}
	}

	fn inject_node_event(
		&mut self,
		source: PeerId,
		event: CustomProtoHandlerOut<TMessage>,
	) {
		match event {
			CustomProtoHandlerOut::CustomProtocolClosed { result } => {
				debug!(target: "sub-libp2p", "Handler({:?}) => Closed({:?})", source, result);
				match self.peers.get_mut(&source) {
					Some(PeerState::Enabled { ref mut open, .. }) if *open =>
						*open = false,
					Some(PeerState::Disabled { ref mut open, .. }) if *open =>
						*open = false,
					Some(PeerState::DisabledPendingEnable { ref mut open, .. }) if *open =>
						*open = false,
					_ => error!(target: "sub-libp2p", "State mismatch in the custom protos handler"),
				}

				debug!(target: "sub-libp2p", "External API <= Closed({:?})", source);
				let event = CustomProtoOut::CustomProtocolClosed {
					result,
					peer_id: source,
				};

				self.events.push(NetworkBehaviourAction::GenerateEvent(event));
			}

			CustomProtoHandlerOut::CustomProtocolOpen { version } => {
				debug!(target: "sub-libp2p", "Handler({:?}) => Open: version {:?}", source, version);
				let endpoint = match self.peers.get_mut(&source) {
					Some(PeerState::Enabled { ref mut open, ref connected_point }) |
					Some(PeerState::DisabledPendingEnable { ref mut open, ref connected_point, .. }) |
					Some(PeerState::Disabled { ref mut open, ref connected_point, .. }) if !*open => {
						*open = true;
						connected_point.clone()
					}
					_ => {
						error!(target: "sub-libp2p", "State mismatch in the custom protos handler");
						return
					}
				};

				debug!(target: "sub-libp2p", "External API <= Open({:?})", source);
				let event = CustomProtoOut::CustomProtocolOpen {
					version,
					peer_id: source,
					endpoint,
				};

				self.events.push(NetworkBehaviourAction::GenerateEvent(event));
			}

			CustomProtoHandlerOut::CustomMessage { message } => {
				debug_assert!(self.is_open(&source));
				trace!(target: "sub-libp2p", "Handler({:?}) => Message", source);
				trace!(target: "sub-libp2p", "External API <= Message({:?})", source);
				let event = CustomProtoOut::CustomMessage {
					peer_id: source,
					message,
				};

				self.events.push(NetworkBehaviourAction::GenerateEvent(event));
			}

			CustomProtoHandlerOut::Clogged { messages } => {
				debug_assert!(self.is_open(&source));
				trace!(target: "sub-libp2p", "Handler({:?}) => Clogged", source);
				trace!(target: "sub-libp2p", "External API <= Clogged({:?})", source);
				warn!(target: "sub-libp2p", "Queue of packets to send to {:?} is \
					pretty large", source);
				self.events.push(NetworkBehaviourAction::GenerateEvent(CustomProtoOut::Clogged {
					peer_id: source,
					messages,
				}));
			}

			// Don't do anything for non-severe errors except report them.
			CustomProtoHandlerOut::ProtocolError { is_severe, ref error } if !is_severe => {
				debug!(target: "sub-libp2p", "Handler({:?}) => Benign protocol error: {:?}",
					source, error)
			}

			CustomProtoHandlerOut::ProtocolError { error, .. } => {
				debug!(target: "sub-libp2p", "Handler({:?}) => Severe protocol error: {:?}",
					source, error);
				self.disconnect_peer_inner(&source, Some(Duration::from_secs(5)));
			}
		}
	}

	fn poll(
		&mut self,
		_params: &mut PollParameters,
	) -> Async<
		NetworkBehaviourAction<
			CustomProtoHandlerIn<TMessage>,
			Self::OutEvent,
		>,
	> {
		// Poll for instructions from the peerset.
		// Note that the peerset is a *best effort* crate, and we have to use defensive programming.
		loop {
			match self.peerset.poll() {
				Ok(Async::Ready(Some(substrate_peerset::Message::Accept(index)))) => {
					self.peerset_report_accept(index);
				}
				Ok(Async::Ready(Some(substrate_peerset::Message::Reject(index)))) => {
					self.peerset_report_reject(index);
				}
				Ok(Async::Ready(Some(substrate_peerset::Message::Connect(id)))) => {
					self.peerset_report_connect(id);
				}
				Ok(Async::Ready(Some(substrate_peerset::Message::Drop(id)))) => {
					self.peerset_report_disconnect(id);
				}
				Ok(Async::Ready(None)) => {
					error!(target: "sub-libp2p", "Peerset receiver stream has returned None");
					break;
				}
				Ok(Async::NotReady) => break,
				Err(err) => {
					error!(target: "sub-libp2p", "Peerset receiver stream has errored: {:?}", err);
					break
				}
			}
		}

		for (peer_id, peer_state) in self.peers.iter_mut() {
			match mem::replace(peer_state, PeerState::Poisoned) {
				PeerState::PendingRequest { mut timer } => {
					if let Ok(Async::NotReady) = timer.poll() {
						*peer_state = PeerState::PendingRequest { timer };
						continue;
					}

					debug!(target: "sub-libp2p", "Libp2p <= Dial {:?} now that ban has expired", peer_id);
					self.events.push(NetworkBehaviourAction::DialPeer { peer_id: peer_id.clone() });
					*peer_state = PeerState::Requested;
				}

				PeerState::DisabledPendingEnable { mut timer, connected_point, open } => {
					if let Ok(Async::NotReady) = timer.poll() {
						*peer_state = PeerState::DisabledPendingEnable { timer, connected_point, open };
						continue;
					}

					debug!(target: "sub-libp2p", "Handler({:?}) <= Enable now that ban has expired", peer_id);
					self.events.push(NetworkBehaviourAction::SendEvent {
						peer_id: peer_id.clone(),
						event: CustomProtoHandlerIn::Enable(connected_point.clone().into()),
					});
					*peer_state = PeerState::Enabled { connected_point, open };
				}

				st @ _ => *peer_state = st,
			}
		}

		if !self.events.is_empty() {
			return Async::Ready(self.events.remove(0))
		}

		Async::NotReady
	}
}

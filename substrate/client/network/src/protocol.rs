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

use crate::{
	config, error,
	peer_store::{PeerStoreHandle, PeerStoreProvider},
	protocol_controller::{self, SetId},
	service::traits::Direction,
	types::ProtocolName,
};

use codec::Encode;
use libp2p::{
	core::Endpoint,
	swarm::{
		behaviour::FromSwarm, ConnectionDenied, ConnectionId, NetworkBehaviour, PollParameters,
		THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
	},
	Multiaddr, PeerId,
};
use log::warn;

use codec::DecodeAll;
use prometheus_endpoint::Registry;
use sc_network_common::role::Roles;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::traits::Block as BlockT;

use std::{collections::HashSet, iter, task::Poll};

use notifications::{Notifications, NotificationsOut};

pub(crate) use notifications::ProtocolHandle;

pub use notifications::{
	notification_service, NotificationsSink, NotifsHandlerError, ProtocolHandlePair, Ready,
};

mod notifications;

pub mod message;

/// Maximum size used for notifications in the block announce and transaction protocols.
// Must be equal to `max(MAX_BLOCK_ANNOUNCE_SIZE, MAX_TRANSACTIONS_SIZE)`.
pub(crate) const BLOCK_ANNOUNCES_TRANSACTIONS_SUBSTREAM_SIZE: u64 = 16 * 1024 * 1024;

/// Identifier of the peerset for the block announces protocol.
const HARDCODED_PEERSETS_SYNC: SetId = SetId::from(0);

// Lock must always be taken in order declared here.
pub struct Protocol<B: BlockT> {
	/// Handles opening the unique substream and sending and receiving raw messages.
	behaviour: Notifications,
	/// List of notifications protocols that have been registered.
	notification_protocols: Vec<ProtocolName>,
	/// Handle to `PeerStore`.
	peer_store_handle: PeerStoreHandle,
	/// Streams for peers whose handshake couldn't be determined.
	bad_handshake_streams: HashSet<PeerId>,
	sync_handle: ProtocolHandle,
	_marker: std::marker::PhantomData<B>,
}

impl<B: BlockT> Protocol<B> {
	/// Create a new instance.
	pub(crate) fn new(
		roles: Roles,
		registry: &Option<Registry>,
		notification_protocols: Vec<config::NonDefaultSetConfig>,
		block_announces_protocol: config::NonDefaultSetConfig,
		peer_store_handle: PeerStoreHandle,
		protocol_controller_handles: Vec<protocol_controller::ProtocolHandle>,
		from_protocol_controllers: TracingUnboundedReceiver<protocol_controller::Message>,
	) -> error::Result<(Self, Vec<ProtocolHandle>)> {
		let (behaviour, notification_protocols, handles) = {
			let installed_protocols = iter::once(block_announces_protocol.protocol_name().clone())
				.chain(notification_protocols.iter().map(|p| p.protocol_name().clone()))
				.collect::<Vec<_>>();

			// NOTE: Block announcement protocol is still very much hardcoded into
			// `Protocol`. 	This protocol must be the first notification protocol given to
			// `Notifications`
			let (protocol_configs, handles): (Vec<_>, Vec<_>) = iter::once({
				let config = notifications::ProtocolConfig {
					name: block_announces_protocol.protocol_name().clone(),
					fallback_names: block_announces_protocol.fallback_names().cloned().collect(),
					handshake: block_announces_protocol.handshake().as_ref().unwrap().to_vec(),
					max_notification_size: block_announces_protocol.max_notification_size(),
				};

				let (handle, command_stream) =
					block_announces_protocol.take_protocol_handle().split();

				((config, handle.clone(), command_stream), handle)
			})
			.chain(notification_protocols.into_iter().map(|s| {
				let config = notifications::ProtocolConfig {
					name: s.protocol_name().clone(),
					fallback_names: s.fallback_names().cloned().collect(),
					handshake: s.handshake().as_ref().map_or(roles.encode(), |h| (*h).to_vec()),
					max_notification_size: s.max_notification_size(),
				};

				let (handle, command_stream) = s.take_protocol_handle().split();

				((config, handle.clone(), command_stream), handle)
			}))
			.unzip();

			(
				Notifications::new(
					protocol_controller_handles,
					from_protocol_controllers,
					registry,
					protocol_configs.into_iter(),
				),
				installed_protocols,
				handles,
			)
		};

		let protocol = Self {
			behaviour,
			sync_handle: handles[0].clone(),
			peer_store_handle,
			notification_protocols,
			bad_handshake_streams: HashSet::new(),
			// TODO: remove when `BlockAnnouncesHandshake` is moved away from `Protocol`
			_marker: Default::default(),
		};

		Ok((protocol, handles))
	}

	pub fn num_sync_peers(&self) -> usize {
		self.sync_handle.num_peers()
	}

	/// Returns the list of all the peers we have an open channel to.
	pub fn open_peers(&self) -> impl Iterator<Item = &PeerId> {
		self.behaviour.open_peers()
	}

	/// Disconnects the given peer if we are connected to it.
	pub fn disconnect_peer(&mut self, peer_id: &PeerId, protocol_name: ProtocolName) {
		if let Some(position) = self.notification_protocols.iter().position(|p| *p == protocol_name)
		{
			// Note: no need to remove a peer from `self.peers` if we are dealing with sync
			// protocol, because it will be done when handling
			// `NotificationsOut::CustomProtocolClosed`.
			self.behaviour.disconnect_peer(peer_id, SetId::from(position));
		} else {
			warn!(target: "sub-libp2p", "disconnect_peer() with invalid protocol name")
		}
	}

	/// Check if role is available for `peer_id` by attempt to decode the handshake to roles and if
	/// that fails, check if the role has been registered to `PeerStore`.
	fn role_available(&self, peer_id: &PeerId, handshake: &Vec<u8>) -> bool {
		match Roles::decode_all(&mut &handshake[..]) {
			Ok(_) => true,
			Err(_) => self.peer_store_handle.peer_role(&peer_id).is_some(),
		}
	}
}

/// Outcome of an incoming custom message.
#[derive(Debug)]
#[must_use]
pub enum CustomMessageOutcome {
	/// Notification protocols have been opened with a remote.
	NotificationStreamOpened {
		remote: PeerId,
		// protocol: ProtocolName,
		set_id: SetId,
		/// Direction of the stream.
		direction: Direction,
		/// See [`crate::Event::NotificationStreamOpened::negotiated_fallback`].
		negotiated_fallback: Option<ProtocolName>,
		/// Received handshake.
		received_handshake: Vec<u8>,
		/// Notification sink.
		notifications_sink: NotificationsSink,
	},
	/// The [`NotificationsSink`] of some notification protocols need an update.
	NotificationStreamReplaced {
		// Peer ID.
		remote: PeerId,
		/// Set ID.
		set_id: SetId,
		/// New notification sink.
		notifications_sink: NotificationsSink,
	},
	/// Notification protocols have been closed with a remote.
	NotificationStreamClosed {
		// Peer ID.
		remote: PeerId,
		/// Set ID.
		set_id: SetId,
	},
	/// Messages have been received on one or more notifications protocols.
	NotificationsReceived {
		// Peer ID.
		remote: PeerId,
		/// Set ID.
		set_id: SetId,
		/// Received notification.
		notification: Vec<u8>,
	},
}

impl<B: BlockT> NetworkBehaviour for Protocol<B> {
	type ConnectionHandler = <Notifications as NetworkBehaviour>::ConnectionHandler;
	type OutEvent = CustomMessageOutcome;

	fn handle_established_inbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		local_addr: &Multiaddr,
		remote_addr: &Multiaddr,
	) -> Result<THandler<Self>, ConnectionDenied> {
		self.behaviour.handle_established_inbound_connection(
			connection_id,
			peer,
			local_addr,
			remote_addr,
		)
	}

	fn handle_established_outbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		addr: &Multiaddr,
		role_override: Endpoint,
	) -> Result<THandler<Self>, ConnectionDenied> {
		self.behaviour.handle_established_outbound_connection(
			connection_id,
			peer,
			addr,
			role_override,
		)
	}

	fn handle_pending_outbound_connection(
		&mut self,
		_connection_id: ConnectionId,
		_maybe_peer: Option<PeerId>,
		_addresses: &[Multiaddr],
		_effective_role: Endpoint,
	) -> Result<Vec<Multiaddr>, ConnectionDenied> {
		// Only `Discovery::handle_pending_outbound_connection` must be returning addresses to
		// ensure that we don't return unwanted addresses.
		Ok(Vec::new())
	}

	fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
		self.behaviour.on_swarm_event(event);
	}

	fn on_connection_handler_event(
		&mut self,
		peer_id: PeerId,
		connection_id: ConnectionId,
		event: THandlerOutEvent<Self>,
	) {
		self.behaviour.on_connection_handler_event(peer_id, connection_id, event);
	}

	fn poll(
		&mut self,
		cx: &mut std::task::Context,
		params: &mut impl PollParameters,
	) -> Poll<ToSwarm<Self::OutEvent, THandlerInEvent<Self>>> {
		let event = match self.behaviour.poll(cx, params) {
			Poll::Pending => return Poll::Pending,
			Poll::Ready(ToSwarm::GenerateEvent(ev)) => ev,
			Poll::Ready(ToSwarm::Dial { opts }) => return Poll::Ready(ToSwarm::Dial { opts }),
			Poll::Ready(ToSwarm::NotifyHandler { peer_id, handler, event }) =>
				return Poll::Ready(ToSwarm::NotifyHandler { peer_id, handler, event }),
			Poll::Ready(ToSwarm::ReportObservedAddr { address, score }) =>
				return Poll::Ready(ToSwarm::ReportObservedAddr { address, score }),
			Poll::Ready(ToSwarm::CloseConnection { peer_id, connection }) =>
				return Poll::Ready(ToSwarm::CloseConnection { peer_id, connection }),
		};

		let outcome = match event {
			NotificationsOut::CustomProtocolOpen {
				peer_id,
				set_id,
				direction,
				received_handshake,
				notifications_sink,
				negotiated_fallback,
				..
			} =>
				if set_id == HARDCODED_PEERSETS_SYNC {
					let _ = self.sync_handle.report_substream_opened(
						peer_id,
						direction,
						received_handshake,
						negotiated_fallback,
						notifications_sink,
					);
					None
				} else {
					match self.role_available(&peer_id, &received_handshake) {
						true => Some(CustomMessageOutcome::NotificationStreamOpened {
							remote: peer_id,
							set_id,
							direction,
							negotiated_fallback,
							received_handshake,
							notifications_sink,
						}),
						false => {
							self.bad_handshake_streams.insert(peer_id);
							None
						},
					}
				},
			NotificationsOut::CustomProtocolReplaced { peer_id, notifications_sink, set_id } =>
				if set_id == HARDCODED_PEERSETS_SYNC {
					let _ = self
						.sync_handle
						.report_notification_sink_replaced(peer_id, notifications_sink);
					None
				} else {
					(!self.bad_handshake_streams.contains(&peer_id)).then_some(
						CustomMessageOutcome::NotificationStreamReplaced {
							remote: peer_id,
							set_id,
							notifications_sink,
						},
					)
				},
			NotificationsOut::CustomProtocolClosed { peer_id, set_id } => {
				if set_id == HARDCODED_PEERSETS_SYNC {
					let _ = self.sync_handle.report_substream_closed(peer_id);
					None
				} else {
					(!self.bad_handshake_streams.remove(&peer_id)).then_some(
						CustomMessageOutcome::NotificationStreamClosed { remote: peer_id, set_id },
					)
				}
			},
			NotificationsOut::Notification { peer_id, set_id, message } => {
				if set_id == HARDCODED_PEERSETS_SYNC {
					let _ = self
						.sync_handle
						.report_notification_received(peer_id, message.freeze().into());
					None
				} else {
					(!self.bad_handshake_streams.contains(&peer_id)).then_some(
						CustomMessageOutcome::NotificationsReceived {
							remote: peer_id,
							set_id,
							notification: message.freeze().into(),
						},
					)
				}
			},
		};

		match outcome {
			Some(event) => Poll::Ready(ToSwarm::GenerateEvent(event)),
			None => {
				cx.waker().wake_by_ref();
				Poll::Pending
			},
		}
	}
}

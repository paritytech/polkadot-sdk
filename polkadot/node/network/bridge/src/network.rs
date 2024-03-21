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

use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

use async_trait::async_trait;
use parking_lot::Mutex;

use parity_scale_codec::Encode;

use sc_network::{
	config::parse_addr, multiaddr::Multiaddr, types::ProtocolName, IfDisconnected, MessageSink,
	NetworkPeers, NetworkRequest, NetworkService, OutboundFailure, ReputationChange,
	RequestFailure,
};

use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet, ProtocolVersion, ValidationVersion},
	request_response::{OutgoingRequest, Recipient, ReqProtocolNames, Requests},
	v1 as protocol_v1, v2 as protocol_v2, v3 as protocol_v3, PeerId,
};
use polkadot_primitives::{AuthorityDiscoveryId, Block, Hash};

use crate::{metrics::Metrics, validator_discovery::AuthorityDiscovery, WireMessage};

// network bridge network abstraction log target
const LOG_TARGET: &'static str = "parachain::network-bridge-net";

// Helper function to send a validation v1 message to a list of peers.
// Messages are always sent via the main protocol, even legacy protocol messages.
pub(crate) fn send_validation_message_v1(
	peers: Vec<PeerId>,
	message: WireMessage<protocol_v1::ValidationProtocol>,
	metrics: &Metrics,
	notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) {
	gum::trace!(target: LOG_TARGET, ?peers, ?message, "Sending validation v1 message to peers",);

	send_message(
		peers,
		PeerSet::Validation,
		ValidationVersion::V1.into(),
		message,
		metrics,
		notification_sinks,
	);
}

// Helper function to send a validation v3 message to a list of peers.
// Messages are always sent via the main protocol, even legacy protocol messages.
pub(crate) fn send_validation_message_v3(
	peers: Vec<PeerId>,
	message: WireMessage<protocol_v3::ValidationProtocol>,
	metrics: &Metrics,
	notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) {
	gum::trace!(target: LOG_TARGET, ?peers, ?message, "Sending validation v3 message to peers",);

	send_message(
		peers,
		PeerSet::Validation,
		ValidationVersion::V3.into(),
		message,
		metrics,
		notification_sinks,
	);
}

// Helper function to send a validation v2 message to a list of peers.
// Messages are always sent via the main protocol, even legacy protocol messages.
pub(crate) fn send_validation_message_v2(
	peers: Vec<PeerId>,
	message: WireMessage<protocol_v2::ValidationProtocol>,
	metrics: &Metrics,
	notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) {
	send_message(
		peers,
		PeerSet::Validation,
		ValidationVersion::V2.into(),
		message,
		metrics,
		notification_sinks,
	);
}

// Helper function to send a collation v1 message to a list of peers.
// Messages are always sent via the main protocol, even legacy protocol messages.
pub(crate) fn send_collation_message_v1(
	peers: Vec<PeerId>,
	message: WireMessage<protocol_v1::CollationProtocol>,
	metrics: &Metrics,
	notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) {
	send_message(
		peers,
		PeerSet::Collation,
		CollationVersion::V1.into(),
		message,
		metrics,
		notification_sinks,
	);
}

// Helper function to send a collation v2 message to a list of peers.
// Messages are always sent via the main protocol, even legacy protocol messages.
pub(crate) fn send_collation_message_v2(
	peers: Vec<PeerId>,
	message: WireMessage<protocol_v2::CollationProtocol>,
	metrics: &Metrics,
	notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) {
	send_message(
		peers,
		PeerSet::Collation,
		CollationVersion::V2.into(),
		message,
		metrics,
		notification_sinks,
	);
}

/// Lower level function that sends a message to the network using the main protocol version.
///
/// This function is only used internally by the network-bridge, which is responsible to only send
/// messages that are compatible with the passed peer set, as that is currently not enforced by
/// this function. These are messages of type `WireMessage` parameterized on the matching type.
fn send_message<M>(
	mut peers: Vec<PeerId>,
	peer_set: PeerSet,
	version: ProtocolVersion,
	message: M,
	metrics: &super::Metrics,
	network_notification_sinks: &Arc<Mutex<HashMap<(PeerSet, PeerId), Box<dyn MessageSink>>>>,
) where
	M: Encode + Clone,
{
	if peers.is_empty() {
		return
	}

	let message = {
		let encoded = message.encode();
		metrics.on_notification_sent(peer_set, version, encoded.len(), peers.len());
		metrics.on_message(std::any::type_name::<M>());
		encoded
	};

	let notification_sinks = network_notification_sinks.lock();

	gum::trace!(
		target: LOG_TARGET,
		?peers,
		?peer_set,
		?version,
		?message,
		"Sending message to peers",
	);

	// optimization: avoid cloning the message for the last peer in the
	// list. The message payload can be quite large. If the underlying
	// network used `Bytes` this would not be necessary.
	//
	// peer may have gotten disconnect by the time `send_message()` is called
	// at which point the the sink is not available.
	let last_peer = peers.pop();
	peers.into_iter().for_each(|peer| {
		if let Some(sink) = notification_sinks.get(&(peer_set, peer)) {
			sink.send_sync_notification(message.clone());
		}
	});

	if let Some(peer) = last_peer {
		if let Some(sink) = notification_sinks.get(&(peer_set, peer)) {
			sink.send_sync_notification(message.clone());
		}
	}
}

/// An abstraction over networking for the purposes of this subsystem.
#[async_trait]
pub trait Network: Clone + Send + 'static {
	/// Ask the network to keep a substream open with these nodes and not disconnect from them
	/// until removed from the protocol's peer set.
	/// Note that `out_peers` setting has no effect on this.
	async fn set_reserved_peers(
		&mut self,
		protocol: ProtocolName,
		multiaddresses: HashSet<Multiaddr>,
	) -> Result<(), String>;

	/// Removes the peers for the protocol's peer set (both reserved and non-reserved).
	async fn remove_from_peers_set(
		&mut self,
		protocol: ProtocolName,
		peers: Vec<PeerId>,
	) -> Result<(), String>;

	/// Send a request to a remote peer.
	async fn start_request<AD: AuthorityDiscovery>(
		&self,
		authority_discovery: &mut AD,
		req: Requests,
		req_protocol_names: &ReqProtocolNames,
		if_disconnected: IfDisconnected,
	);

	/// Report a given peer as either beneficial (+) or costly (-) according to the given scalar.
	fn report_peer(&self, who: PeerId, rep: ReputationChange);

	/// Disconnect a given peer from the protocol specified without harming reputation.
	fn disconnect_peer(&self, who: PeerId, protocol: ProtocolName);

	/// Get peer role.
	fn peer_role(&self, who: PeerId, handshake: Vec<u8>) -> Option<sc_network::ObservedRole>;
}

#[async_trait]
impl Network for Arc<NetworkService<Block, Hash>> {
	async fn set_reserved_peers(
		&mut self,
		protocol: ProtocolName,
		multiaddresses: HashSet<Multiaddr>,
	) -> Result<(), String> {
		NetworkService::set_reserved_peers(&**self, protocol, multiaddresses)
	}

	async fn remove_from_peers_set(
		&mut self,
		protocol: ProtocolName,
		peers: Vec<PeerId>,
	) -> Result<(), String> {
		NetworkService::remove_peers_from_reserved_set(&**self, protocol, peers)
	}

	fn report_peer(&self, who: PeerId, rep: ReputationChange) {
		NetworkService::report_peer(&**self, who, rep);
	}

	fn disconnect_peer(&self, who: PeerId, protocol: ProtocolName) {
		NetworkService::disconnect_peer(&**self, who, protocol);
	}

	async fn start_request<AD: AuthorityDiscovery>(
		&self,
		authority_discovery: &mut AD,
		req: Requests,
		req_protocol_names: &ReqProtocolNames,
		if_disconnected: IfDisconnected,
	) {
		let (protocol, OutgoingRequest { peer, payload, pending_response, fallback_request }) =
			req.encode_request();

		let peer_id = match peer {
			Recipient::Peer(peer_id) => Some(peer_id),
			Recipient::Authority(authority) => {
				gum::trace!(
					target: LOG_TARGET,
					?authority,
					"Searching for peer id to connect to authority",
				);

				let mut found_peer_id = None;
				// Note: `get_addresses_by_authority_id` searched in a cache, and it thus expected
				// to be very quick.
				for addr in authority_discovery
					.get_addresses_by_authority_id(authority)
					.await
					.into_iter()
					.flat_map(|list| list.into_iter())
				{
					let (peer_id, addr) = match parse_addr(addr) {
						Ok(v) => v,
						Err(_) => continue,
					};
					NetworkService::add_known_address(self, peer_id, addr);
					found_peer_id = Some(peer_id);
				}
				found_peer_id
			},
		};

		let peer_id = match peer_id {
			None => {
				gum::debug!(target: LOG_TARGET, "Discovering authority failed");
				match pending_response
					.send(Err(RequestFailure::Network(OutboundFailure::DialFailure)))
				{
					Err(_) => {
						gum::debug!(target: LOG_TARGET, "Sending failed request response failed.")
					},
					Ok(_) => {},
				}
				return
			},
			Some(peer_id) => peer_id,
		};

		gum::trace!(
			target: LOG_TARGET,
			%peer_id,
			protocol = %req_protocol_names.get_name(protocol),
			fallback_protocol = ?fallback_request.as_ref().map(|(_, p)| req_protocol_names.get_name(*p)),
			?if_disconnected,
			"Starting request",
		);

		NetworkService::start_request(
			self,
			peer_id,
			req_protocol_names.get_name(protocol),
			payload,
			fallback_request.map(|(r, p)| (r, req_protocol_names.get_name(p))),
			pending_response,
			if_disconnected,
		);
	}

	fn peer_role(&self, who: PeerId, handshake: Vec<u8>) -> Option<sc_network::ObservedRole> {
		NetworkService::peer_role(self, who, handshake)
	}
}

/// We assume one `peer_id` per `authority_id`.
pub async fn get_peer_id_by_authority_id<AD: AuthorityDiscovery>(
	authority_discovery: &mut AD,
	authority: AuthorityDiscoveryId,
) -> Option<PeerId> {
	// Note: `get_addresses_by_authority_id` searched in a cache, and it thus expected
	// to be very quick.
	authority_discovery
		.get_addresses_by_authority_id(authority)
		.await
		.into_iter()
		.flat_map(|list| list.into_iter())
		.find_map(|addr| parse_addr(addr).ok().map(|(p, _)| p))
}

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

//! Mocked `network-bridge` subsystems that uses a `NetworkInterface` to access
//! the emulated network.

use crate::{
	configuration::TestAuthorities,
	network::{NetworkEmulatorHandle, NetworkInterfaceReceiver, NetworkMessage, RequestExt},
};
use futures::{channel::mpsc::UnboundedSender, FutureExt, StreamExt};
use polkadot_node_network_protocol::Versioned;
use polkadot_node_subsystem::{
	messages::NetworkBridgeTxMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::{
	messages::{ApprovalDistributionMessage, BitfieldDistributionMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use sc_network::{request_responses::ProtocolConfig, RequestFailure};

const LOG_TARGET: &str = "subsystem-bench::network-bridge";
const CHUNK_REQ_PROTOCOL_NAME_V1: &str =
	"/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff/req_chunk/1";

/// A mock of the network bridge tx subsystem.
pub struct MockNetworkBridgeTx {
	/// A network emulator handle
	network: NetworkEmulatorHandle,
	/// A channel to the network interface,
	to_network_interface: UnboundedSender<NetworkMessage>,
	/// Test authorities
	test_authorities: TestAuthorities,
}

/// A mock of the network bridge tx subsystem.
pub struct MockNetworkBridgeRx {
	/// A network interface receiver
	network_receiver: NetworkInterfaceReceiver,
	/// Chunk request sender
	chunk_request_sender: Option<ProtocolConfig>,
}

impl MockNetworkBridgeTx {
	pub fn new(
		network: NetworkEmulatorHandle,
		to_network_interface: UnboundedSender<NetworkMessage>,
		test_authorities: TestAuthorities,
	) -> MockNetworkBridgeTx {
		Self { network, to_network_interface, test_authorities }
	}
}

impl MockNetworkBridgeRx {
	pub fn new(
		network_receiver: NetworkInterfaceReceiver,
		chunk_request_sender: Option<ProtocolConfig>,
	) -> MockNetworkBridgeRx {
		Self { network_receiver, chunk_request_sender }
	}
}

#[overseer::subsystem(NetworkBridgeTx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeTx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "network-bridge-tx", future }
	}
}

#[overseer::subsystem(NetworkBridgeRx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeRx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "network-bridge-rx", future }
	}
}

#[overseer::contextbounds(NetworkBridgeTx, prefix = self::overseer)]
impl MockNetworkBridgeTx {
	async fn run<Context>(self, mut ctx: Context) {
		// Main subsystem loop.
		loop {
			let subsystem_message = ctx.recv().await.expect("Overseer never fails us");
			match subsystem_message {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					NetworkBridgeTxMessage::SendRequests(requests, _if_disconnected) => {
						for request in requests {
							gum::debug!(target: LOG_TARGET, request = ?request, "Processing request");
							let peer_id =
								request.authority_id().expect("all nodes are authorities").clone();

							if !self.network.is_peer_connected(&peer_id) {
								// Attempting to send a request to a disconnected peer.
								request
									.into_response_sender()
									.send(Err(RequestFailure::NotConnected))
									.expect("send never fails");
								continue
							}

							let peer_message =
								NetworkMessage::RequestFromNode(peer_id.clone(), request);

							let _ = self.to_network_interface.unbounded_send(peer_message);
						}
					},
					NetworkBridgeTxMessage::ReportPeer(_) => {
						// ignore rep changes
					},
					NetworkBridgeTxMessage::SendValidationMessage(peers, message) => {
						for peer in peers {
							self.to_network_interface
								.unbounded_send(NetworkMessage::MessageFromNode(
									self.test_authorities
										.peer_id_to_authority
										.get(&peer)
										.unwrap()
										.clone(),
									message.clone(),
								))
								.expect("Should not fail");
						}
					},
					_ => unimplemented!("Unexpected network bridge message"),
				},
			}
		}
	}
}

#[overseer::contextbounds(NetworkBridgeRx, prefix = self::overseer)]
impl MockNetworkBridgeRx {
	async fn run<Context>(mut self, mut ctx: Context) {
		// Main subsystem loop.
		let mut from_network_interface = self.network_receiver.0;
		loop {
			futures::select! {
				maybe_peer_message = from_network_interface.next() => {
					if let Some(message) = maybe_peer_message {
						match message {
							NetworkMessage::MessageFromPeer(peer_id, message) => match message {
								Versioned::V2(
									polkadot_node_network_protocol::v2::ValidationProtocol::BitfieldDistribution(
										bitfield,
									),
								) => {
									ctx.send_message(
										BitfieldDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(peer_id, polkadot_node_network_protocol::Versioned::V2(bitfield)))
									).await;
								},
								Versioned::V3(
									polkadot_node_network_protocol::v3::ValidationProtocol::ApprovalDistribution(msg)
								) => {
									ctx.send_message(
										ApprovalDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(peer_id, polkadot_node_network_protocol::Versioned::V3(msg)))
									).await;
								}
								_ => {
									unimplemented!("We only talk v2 network protocol")
								},
							},
							NetworkMessage::RequestFromPeer(request) => {
								if let Some(protocol) = self.chunk_request_sender.as_mut() {
									assert_eq!(&*protocol.name, CHUNK_REQ_PROTOCOL_NAME_V1);
									if let Some(inbound_queue) = protocol.inbound_queue.as_ref() {
										inbound_queue
											.send(request)
											.await
											.expect("Forwarding requests to subsystem never fails");
									}
								}
							},
							_ => {
								panic!("NetworkMessage::RequestFromNode is not expected to be received from a peer")
							}
						}
					}
				},
				subsystem_message = ctx.recv().fuse() => {
					match subsystem_message.expect("Overseer never fails us") {
						orchestra::FromOrchestra::Signal(signal) => if signal == OverseerSignal::Conclude { return },
						_ => {
							unimplemented!("Unexpected network bridge rx message")
						},
					}
				}
			}
		}
	}
}

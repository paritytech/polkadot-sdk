// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

//! Bitswap NetworkBehavior implementation for libp2p.
//!
//! Implements Bitswap 1.2.0 protocol as a proper NetworkBehavior with manual substream handling.
//! Unlike request-response protocols, Bitswap uses persistent bidirectional substreams where
//! multiple messages can be sent in both directions without closing the substream.

use crate::bitswap::schema::bitswap::{
	message::{wantlist::WantType, Block as MessageBlock, BlockPresence, BlockPresenceType},
	Message as BitswapMessage,
};

use asynchronous_codec::{Framed, FramedRead, FramedWrite};
use bytes::{Buf, BytesMut};
use cid::{self, Version};
use futures::{
	future::BoxFuture,
	prelude::*,
	stream::{FuturesUnordered, StreamExt},
};
use libp2p::{
	core::{upgrade, ConnectedPoint, Endpoint},
	swarm::{
		behaviour::ConnectionEstablished, ConnectionClosed, ConnectionDenied, ConnectionId,
		FromSwarm, NetworkBehaviour, NotifyHandler, OneShotHandler, SubstreamProtocol,
		THandlerInEvent, THandlerOutEvent, ToSwarm,
	},
	PeerId, StreamProtocol,
};
use log::{debug, error, trace, warn};
use prost::Message;
use sc_client_api::BlockBackend;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, VecDeque},
	io,
	sync::Arc,
	task::{Context, Poll},
	time::Duration,
};
use unsigned_varint::encode as varint_encode;

const LOG_TARGET: &str = "bitswap-behaviour";

/// Bitswap protocol name
const PROTOCOL_NAME: &str = "/ipfs/bitswap/1.2.0";

/// Max number of blocks per wantlist
const MAX_WANTED_BLOCKS: usize = 16;

/// Max packet size (same as substrate protocol message)
const MAX_PACKET_SIZE: usize = 16 * 1024 * 1024;

/// Prefix represents all metadata of a CID, without the actual content.
#[derive(PartialEq, Eq, Clone, Debug)]
struct Prefix {
	pub version: Version,
	pub codec: u64,
	pub mh_type: u64,
	pub mh_len: u8,
}

impl Prefix {
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut res = Vec::with_capacity(4);
		let mut buf = varint_encode::u64_buffer();
		let version = varint_encode::u64(self.version.into(), &mut buf);
		res.extend_from_slice(version);
		let mut buf = varint_encode::u64_buffer();
		let codec = varint_encode::u64(self.codec, &mut buf);
		res.extend_from_slice(codec);
		let mut buf = varint_encode::u64_buffer();
		let mh_type = varint_encode::u64(self.mh_type, &mut buf);
		res.extend_from_slice(mh_type);
		let mut buf = varint_encode::u64_buffer();
		let mh_len = varint_encode::u64(self.mh_len as u64, &mut buf);
		res.extend_from_slice(mh_len);
		res
	}
}

/// Bitswap protocol upgrade
#[derive(Debug, Clone)]
pub struct BitswapProtocol;

impl upgrade::UpgradeInfo for BitswapProtocol {
	type Info = StreamProtocol;
	type InfoIter = std::iter::Once<Self::Info>;

	fn protocol_info(&self) -> Self::InfoIter {
		std::iter::once(StreamProtocol::new(PROTOCOL_NAME))
	}
}

impl<C> upgrade::InboundUpgrade<C> for BitswapProtocol
where
	C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	type Output = C;
	type Error = io::Error;
	type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

	fn upgrade_inbound(self, socket: C, _: Self::Info) -> Self::Future {
		async move { Ok(socket) }.boxed()
	}
}

impl<C> upgrade::OutboundUpgrade<C> for BitswapProtocol
where
	C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	type Output = C;
	type Error = io::Error;
	type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

	fn upgrade_outbound(self, socket: C, _: Self::Info) -> Self::Future {
		async move { Ok(socket) }.boxed()
	}
}

/// Codec for Bitswap messages
pub struct BitswapCodec;

impl asynchronous_codec::Encoder for BitswapCodec {
	type Item<'a> = Vec<u8>;
	type Error = io::Error;

	fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
		if item.len() > MAX_PACKET_SIZE {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"Message too large",
			));
		}
		
		// Encode length prefix
		let mut buf = unsigned_varint::encode::usize_buffer();
		let len_bytes = unsigned_varint::encode::usize(item.len(), &mut buf);
		dst.extend_from_slice(len_bytes);
		dst.extend_from_slice(&item);
		Ok(())
	}
}

impl asynchronous_codec::Decoder for BitswapCodec {
	type Item = Vec<u8>;
	type Error = io::Error;

	fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
		// Try to read length prefix
		match unsigned_varint::io::read_usize(&mut src.as_ref()) {
			Ok(len) => {
				if len > MAX_PACKET_SIZE {
					return Err(io::Error::new(
						io::ErrorKind::InvalidData,
						"Message too large",
					));
				}
				
				// Calculate how many bytes the varint took
				let mut buf = unsigned_varint::encode::usize_buffer();
				let varint_bytes = unsigned_varint::encode::usize(len, &mut buf);
				let varint_len = varint_bytes.len();
				
				// Check if we have the full message
				if src.len() >= varint_len + len {
					src.advance(varint_len);
					let data = src.split_to(len).to_vec();
					Ok(Some(data))
				} else {
					Ok(None)
				}
			},
			Err(unsigned_varint::io::ReadError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
				Ok(None)
			},
			Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
		}
	}
}

/// Events emitted by the Bitswap behaviour
#[derive(Debug)]
pub enum BitswapEvent {
	/// A request was received and processed
	RequestProcessed {
		peer: PeerId,
		duration: Duration,
	},
}

/// Bitswap network behaviour
pub struct BitswapBehaviour<B: BlockT> {
	/// Blockchain client
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	/// Active connections and their substreams
	connections: HashMap<PeerId, Vec<ConnectionId>>,
	/// Pending events to emit
	events: VecDeque<ToSwarm<BitswapEvent, BitswapProtocol>>,
}

impl<B: BlockT> BitswapBehaviour<B> {
	/// Create a new Bitswap behaviour
	pub fn new(client: Arc<dyn BlockBackend<B> + Send + Sync>) -> Self {
		Self {
			client,
			connections: HashMap::new(),
			events: VecDeque::new(),
		}
	}

	/// Handle an incoming Bitswap message
	fn handle_message(&mut self, peer: &PeerId, payload: &[u8]) -> Result<Vec<u8>, BitswapError> {
		let request = BitswapMessage::decode(payload)?;

		trace!(target: LOG_TARGET, "Received request from {}: {:?}", peer, request);

		let mut response = BitswapMessage::default();

		let wantlist = match request.wantlist {
			Some(wantlist) => wantlist,
			None => {
				debug!(target: LOG_TARGET, "Unexpected bitswap message from {}", peer);
				return Err(BitswapError::InvalidWantList);
			},
		};

		if wantlist.entries.len() > MAX_WANTED_BLOCKS {
			trace!(target: LOG_TARGET, "Ignored request: too many entries");
			return Err(BitswapError::TooManyEntries);
		}

		for entry in wantlist.entries {
			let cid = match cid::Cid::read_bytes(entry.block.as_slice()) {
				Ok(cid) => cid,
				Err(e) => {
					trace!(target: LOG_TARGET, "Bad CID {:?}: {:?}", entry.block, e);
					continue;
				},
			};

			if cid.version() != cid::Version::V1 ||
				cid.hash().code() != u64::from(cid::multihash::Code::Blake2b256) ||
				cid.hash().size() != 32
			{
				debug!(target: LOG_TARGET, "Ignoring unsupported CID {}: {}", peer, cid);
				continue;
			}

			let mut hash = B::Hash::default();
			hash.as_mut().copy_from_slice(&cid.hash().digest()[0..32]);
			let transaction = match self.client.indexed_transaction(hash) {
				Ok(ex) => ex,
				Err(e) => {
					error!(target: LOG_TARGET, "Error retrieving transaction {}: {}", hash, e);
					None
				},
			};

			match transaction {
				Some(transaction) => {
					trace!(target: LOG_TARGET, "Found CID {:?}, hash {:?}", cid, hash);

					if entry.want_type == WantType::Block as i32 {
						let prefix = Prefix {
							version: cid.version(),
							codec: cid.codec(),
							mh_type: cid.hash().code(),
							mh_len: cid.hash().size(),
						};
						response
							.payload
							.push(MessageBlock { prefix: prefix.to_bytes(), data: transaction });
					} else {
						response.block_presences.push(BlockPresence {
							r#type: BlockPresenceType::Have as i32,
							cid: cid.to_bytes(),
						});
					}
				},
				None => {
					trace!(target: LOG_TARGET, "Missing CID {:?}, hash {:?}", cid, hash);

					if entry.send_dont_have {
						response.block_presences.push(BlockPresence {
							r#type: BlockPresenceType::DontHave as i32,
							cid: cid.to_bytes(),
						});
					}
				},
			}
		}

		Ok(response.encode_to_vec())
	}
}

impl<B: BlockT> NetworkBehaviour for BitswapBehaviour<B> {
	type ConnectionHandler = BitswapHandler;
	type ToSwarm = BitswapEvent;

	fn handle_established_inbound_connection(
		&mut self,
		_connection_id: ConnectionId,
		_peer: PeerId,
		_local_addr: &libp2p::Multiaddr,
		_remote_addr: &libp2p::Multiaddr,
	) -> Result<libp2p::swarm::THandler<Self>, ConnectionDenied> {
		Ok(BitswapHandler::new())
	}

	fn handle_established_outbound_connection(
		&mut self,
		_connection_id: ConnectionId,
		_peer: PeerId,
		_addr: &libp2p::Multiaddr,
		_role_override: Endpoint,
		_port_use: libp2p::core::transport::PortUse,
	) -> Result<libp2p::swarm::THandler<Self>, ConnectionDenied> {
		Ok(BitswapHandler::new())
	}

	fn on_swarm_event(&mut self, event: FromSwarm) {
		match event {
			FromSwarm::ConnectionEstablished(ConnectionEstablished {
				peer_id,
				connection_id,
				..
			}) => {
				self.connections.entry(peer_id).or_default().push(connection_id);
				debug!(target: LOG_TARGET, "Connection established with {}", peer_id);
			},
			FromSwarm::ConnectionClosed(ConnectionClosed {
				peer_id,
				connection_id,
				..
			}) => {
				if let Some(connections) = self.connections.get_mut(&peer_id) {
					connections.retain(|&id| id != connection_id);
					if connections.is_empty() {
						self.connections.remove(&peer_id);
					}
				}
				debug!(target: LOG_TARGET, "Connection closed with {}", peer_id);
			},
			_ => {},
		}
	}

	fn on_connection_handler_event(
		&mut self,
		peer_id: PeerId,
		_connection_id: ConnectionId,
		event: THandlerOutEvent<Self>,
	) {
		match event {
			InnerMessage::Rx { msg } => {
				let start = std::time::Instant::now();
				match self.handle_message(&peer_id, &msg) {
					Ok(response) => {
						let duration = start.elapsed();
						trace!(
							target: LOG_TARGET,
							"Processed bitswap request from {} in {:?}",
							peer_id,
							duration
						);
						
						// Send response back
						self.events.push_back(ToSwarm::NotifyHandler {
							peer_id,
							handler: libp2p::swarm::NotifyHandler::Any,
							event: InnerMessage::Tx { msg: response },
						});
						
						self.events.push_back(ToSwarm::GenerateEvent(BitswapEvent::RequestProcessed {
							peer: peer_id,
							duration,
						}));
					},
					Err(e) => {
						error!(
							target: LOG_TARGET,
							"Failed to process bitswap request from {}: {:?}",
							peer_id,
							e
						);
					},
				}
			},
			InnerMessage::Tx { .. } => {
				// Outbound message sent, nothing to do
			},
		}
	}

	fn poll(
		&mut self,
		_cx: &mut Context<'_>,
	) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
		if let Some(event) = self.events.pop_front() {
			return Poll::Ready(event);
		}
		Poll::Pending
	}
}

/// Messages sent between the behaviour and handlers
#[derive(Debug)]
pub enum InnerMessage {
	/// Received message
	Rx { msg: Vec<u8> },
	/// Message to send
	Tx { msg: Vec<u8> },
}

/// Custom connection handler for Bitswap protocol
pub struct BitswapHandler {
	/// Inbound substream
	inbound: Option<Framed<libp2p::Stream, BitswapCodec>>,
	/// Outbound substream
	outbound: Option<Framed<libp2p::Stream, BitswapCodec>>,
	/// Pending outbound messages
	pending_messages: VecDeque<Vec<u8>>,
	/// Events to send to behaviour
	pending_events: VecDeque<InnerMessage>,
}

impl BitswapHandler {
	pub fn new() -> Self {
		Self {
			inbound: None,
			outbound: None,
			pending_messages: VecDeque::new(),
			pending_events: VecDeque::new(),
		}
	}
}

impl libp2p::swarm::ConnectionHandler for BitswapHandler {
	type FromBehaviour = InnerMessage;
	type ToBehaviour = InnerMessage;
	type InboundProtocol = BitswapProtocol;
	type OutboundProtocol = BitswapProtocol;
	type InboundOpenInfo = ();
	type OutboundOpenInfo = ();

	fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
		SubstreamProtocol::new(BitswapProtocol, ())
	}

	fn poll(
		&mut self,
		cx: &mut Context<'_>,
	) -> Poll<
		libp2p::swarm::ConnectionHandlerEvent<
			Self::OutboundProtocol,
			Self::OutboundOpenInfo,
			Self::ToBehaviour,
		>,
	> {
		// Check for pending events first
		if let Some(event) = self.pending_events.pop_front() {
			return Poll::Ready(libp2p::swarm::ConnectionHandlerEvent::NotifyBehaviour(event));
		}

		// Poll inbound substream for incoming messages
		if let Some(ref mut inbound) = self.inbound {
			match inbound.poll_next_unpin(cx) {
				Poll::Ready(Some(Ok(msg))) => {
					return Poll::Ready(libp2p::swarm::ConnectionHandlerEvent::NotifyBehaviour(
						InnerMessage::Rx { msg },
					));
				},
				Poll::Ready(Some(Err(e))) => {
					warn!(target: LOG_TARGET, "Inbound stream error: {:?}", e);
					self.inbound = None;
				},
				Poll::Ready(None) => {
					debug!(target: LOG_TARGET, "Inbound stream closed");
					self.inbound = None;
				},
				Poll::Pending => {},
			}
		}

		// Poll outbound substream to send pending messages
		if let Some(ref mut outbound) = self.outbound {
			// Try to send pending messages
			while let Some(msg) = self.pending_messages.front() {
				match outbound.poll_ready_unpin(cx) {
					Poll::Ready(Ok(())) => {
						let msg = self.pending_messages.pop_front().unwrap();
						if let Err(e) = outbound.start_send_unpin(msg) {
							error!(target: LOG_TARGET, "Failed to send message: {:?}", e);
							self.outbound = None;
							break;
						}
					},
					Poll::Ready(Err(e)) => {
						error!(target: LOG_TARGET, "Outbound stream error: {:?}", e);
						self.outbound = None;
						break;
					},
					Poll::Pending => break,
				}
			}

			// Flush the outbound stream
			match outbound.poll_flush_unpin(cx) {
				Poll::Ready(Err(e)) => {
					error!(target: LOG_TARGET, "Failed to flush outbound stream: {:?}", e);
					self.outbound = None;
				},
				_ => {},
			}
		}

		// Request a new outbound substream if we have pending messages and no outbound stream
		if !self.pending_messages.is_empty() && self.outbound.is_none() {
			return Poll::Ready(libp2p::swarm::ConnectionHandlerEvent::OutboundSubstreamRequest {
				protocol: SubstreamProtocol::new(BitswapProtocol, ()),
			});
		}

		Poll::Pending
	}

	fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
		match event {
			InnerMessage::Tx { msg } => {
				self.pending_messages.push_back(msg);
			},
			InnerMessage::Rx { .. } => {
				// This shouldn't happen - Rx is from handler to behaviour
			},
		}
	}

	fn on_connection_event(
		&mut self,
		event: libp2p::swarm::handler::ConnectionEvent<
			Self::InboundProtocol,
			Self::OutboundProtocol,
			Self::InboundOpenInfo,
			Self::OutboundOpenInfo,
		>,
	) {
		match event {
			libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedInbound(
				libp2p::swarm::handler::FullyNegotiatedInbound { protocol: stream, .. },
			) => {
				debug!(target: LOG_TARGET, "Inbound substream established");
				self.inbound = Some(Framed::new(stream, BitswapCodec));
			},
			libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(
				libp2p::swarm::handler::FullyNegotiatedOutbound { protocol: stream, .. },
			) => {
				debug!(target: LOG_TARGET, "Outbound substream established");
				self.outbound = Some(Framed::new(stream, BitswapCodec));
			},
			libp2p::swarm::handler::ConnectionEvent::DialUpgradeError(
				libp2p::swarm::handler::DialUpgradeError { error, .. },
			) => {
				warn!(target: LOG_TARGET, "Dial upgrade error: {:?}", error);
			},
			libp2p::swarm::handler::ConnectionEvent::ListenUpgradeError(
				libp2p::swarm::handler::ListenUpgradeError { error, .. },
			) => {
				warn!(target: LOG_TARGET, "Listen upgrade error: {:?}", error);
			},
			_ => {},
		}
	}
}

/// Bitswap protocol error
#[derive(Debug, thiserror::Error)]
pub enum BitswapError {
	#[error("Failed to decode request: {0}")]
	DecodeProto(#[from] prost::DecodeError),

	#[error("Failed to encode response: {0}")]
	EncodeProto(#[from] prost::EncodeError),

	#[error(transparent)]
	Client(#[from] sp_blockchain::Error),

	#[error(transparent)]
	BadCid(#[from] cid::Error),

	#[error("Invalid WANT list")]
	InvalidWantList,

	#[error("Too many block entries in the request")]
	TooManyEntries,
}

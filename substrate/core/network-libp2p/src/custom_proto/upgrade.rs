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

use crate::ProtocolId;
use bytes::Bytes;
use libp2p::core::{Negotiated, Endpoint, UpgradeInfo, InboundUpgrade, OutboundUpgrade, upgrade::ProtocolName};
use libp2p::tokio_codec::Framed;
use log::warn;
use std::{collections::VecDeque, io, iter, marker::PhantomData, vec::IntoIter as VecIntoIter};
use futures::{prelude::*, future, stream};
use tokio_io::{AsyncRead, AsyncWrite};
use unsigned_varint::codec::UviBytes;

/// Connection upgrade for a single protocol.
///
/// Note that "a single protocol" here refers to `par` for example. However
/// each protocol can have multiple different versions for networking purposes.
pub struct RegisteredProtocol<TMessage> {
	/// Id of the protocol for API purposes.
	id: ProtocolId,
	/// Base name of the protocol as advertised on the network.
	/// Ends with `/` so that we can append a version number behind.
	base_name: Bytes,
	/// List of protocol versions that we support.
	/// Ordered in descending order so that the best comes first.
	supported_versions: Vec<u8>,
	/// Marker to pin the generic.
	marker: PhantomData<TMessage>,
}

impl<TMessage> RegisteredProtocol<TMessage> {
	/// Creates a new `RegisteredProtocol`. The `custom_data` parameter will be
	/// passed inside the `RegisteredProtocolOutput`.
	pub fn new(protocol: ProtocolId, versions: &[u8])
		-> Self {
		let mut base_name = Bytes::from_static(b"/substrate/");
		base_name.extend_from_slice(&protocol);
		base_name.extend_from_slice(b"/");

		RegisteredProtocol {
			base_name,
			id: protocol,
			supported_versions: {
				let mut tmp = versions.to_vec();
				tmp.sort_unstable_by(|a, b| b.cmp(&a));
				tmp
			},
			marker: PhantomData,
		}
	}

	/// Returns the ID of the protocol.
	#[inline]
	pub fn id(&self) -> ProtocolId {
		self.id
	}
}

impl<TMessage> Clone for RegisteredProtocol<TMessage> {
	fn clone(&self) -> Self {
		RegisteredProtocol {
			id: self.id,
			base_name: self.base_name.clone(),
			supported_versions: self.supported_versions.clone(),
			marker: PhantomData,
		}
	}
}

/// Output of a `RegisteredProtocol` upgrade.
pub struct RegisteredProtocolSubstream<TMessage, TSubstream> {
	/// If true, we are in the process of closing the sink.
	is_closing: bool,
	/// Whether the local node opened this substream (dialer), or we received this substream from
	/// the remote (listener).
	endpoint: Endpoint,
	/// Buffer of packets to send.
	send_queue: VecDeque<Vec<u8>>,
	/// If true, we should call `poll_complete` on the inner sink.
	requires_poll_complete: bool,
	/// The underlying substream.
	inner: stream::Fuse<Framed<Negotiated<TSubstream>, UviBytes<Vec<u8>>>>,
	/// Id of the protocol.
	protocol_id: ProtocolId,
	/// Version of the protocol that was negotiated.
	protocol_version: u8,
	/// If true, we have sent a "remote is clogged" event recently and shouldn't send another one
	/// unless the buffer empties then fills itself again.
	clogged_fuse: bool,
	/// If true, then this substream uses the "/multi/" version of the protocol. This is a hint
	/// that the handler can behave differently.
	is_multiplex: bool,
	/// Marker to pin the generic.
	marker: PhantomData<TMessage>,
}

impl<TMessage, TSubstream> RegisteredProtocolSubstream<TMessage, TSubstream> {
	/// Returns the protocol id.
	#[inline]
	pub fn protocol_id(&self) -> ProtocolId {
		self.protocol_id
	}

	/// Returns the version of the protocol that was negotiated.
	#[inline]
	pub fn protocol_version(&self) -> u8 {
		self.protocol_version
	}

	/// Returns whether the local node opened this substream (dialer), or we received this
	/// substream from the remote (listener).
	pub fn endpoint(&self) -> Endpoint {
		self.endpoint
	}

	/// Returns true if we negotiated the "multiplexed" version. This means that the handler can
	/// open multiple substreams instead of just one.
	pub fn is_multiplex(&self) -> bool {
		self.is_multiplex
	}

	/// Starts a graceful shutdown process on this substream.
	///
	/// Note that "graceful" means that we sent a closing message. We don't wait for any
	/// confirmation from the remote.
	///
	/// After calling this, the stream is guaranteed to finish soon-ish.
	pub fn shutdown(&mut self) {
		self.is_closing = true;
		self.send_queue.clear();
	}

	/// Sends a message to the substream.
	pub fn send_message(&mut self, data: TMessage)
	where TMessage: CustomMessage {
		if self.is_closing {
			return
		}

		self.send_queue.push_back(data.into_bytes());
	}
}

/// Implemented on messages that can be sent or received on the network.
pub trait CustomMessage {
	/// Turns a message into the raw bytes to send over the network.
	fn into_bytes(self) -> Vec<u8>;

	/// Tries to parse `bytes` received from the network into a message.
	fn from_bytes(bytes: &[u8]) -> Result<Self, ()>
		where Self: Sized;

	/// Returns a unique ID that is used to match request and responses.
	///
	/// The networking layer employs multiplexing in order to have multiple parallel data streams.
	/// Transmitting messages over the network uses two kinds of substreams:
	///
	/// - Undirectional substreams, where we send a single message then close the substream.
	/// - Bidirectional substreams, where we send a message then wait for a response. Once the
	///   response has arrived, we close the substream.
	///
	/// If `request_id()` returns `OneWay`, then this message will be sent or received over a
	/// unidirectional substream. If instead it returns `Request` or `Response`, then we use the
	/// value to match a request with its response.
	fn request_id(&self) -> CustomMessageId;
}

/// See the documentation of `CustomMessage::request_id`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CustomMessageId {
	OneWay,
	Request(u64),
	Response(u64),
}

// These trait implementations exist mostly for testing convenience. This should eventually be
// removed.

impl CustomMessage for Vec<u8> {
	fn into_bytes(self) -> Vec<u8> {
		self
	}

	fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
		Ok(bytes.to_vec())
	}

	fn request_id(&self) -> CustomMessageId {
		CustomMessageId::OneWay
	}
}

impl CustomMessage for (Option<u64>, Vec<u8>) {
	fn into_bytes(self) -> Vec<u8> {
		use byteorder::WriteBytesExt;
		use std::io::Write;
		let mut out = Vec::new();
		out.write_u64::<byteorder::BigEndian>(self.0.unwrap_or(u64::max_value()))
			.expect("Writing to a Vec can never fail");
		out.write_all(&self.1).expect("Writing to a Vec can never fail");
		out
	}

	fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
		use byteorder::ReadBytesExt;
		use std::io::Read;
		let mut rdr = std::io::Cursor::new(bytes);
		let id = rdr.read_u64::<byteorder::BigEndian>().map_err(|_| ())?;
		let mut out = Vec::new();
		rdr.read_to_end(&mut out).map_err(|_| ())?;
		let id = if id == u64::max_value() {
			None
		} else {
			Some(id)
		};
		Ok((id, out))
	}

	fn request_id(&self) -> CustomMessageId {
		if let Some(id) = self.0 {
			CustomMessageId::Request(id)
		} else {
			CustomMessageId::OneWay
		}
	}
}

/// Event produced by the `RegisteredProtocolSubstream`.
#[derive(Debug, Clone)]
pub enum RegisteredProtocolEvent<TMessage> {
	/// Received a message from the remote.
	Message(TMessage),

	/// Diagnostic event indicating that the connection is clogged and we should avoid sending too
	/// many messages to it.
	Clogged {
		/// Copy of the messages that are within the buffer, for further diagnostic.
		messages: Vec<TMessage>,
	},
}

impl<TMessage, TSubstream> Stream for RegisteredProtocolSubstream<TMessage, TSubstream>
where TSubstream: AsyncRead + AsyncWrite, TMessage: CustomMessage {
	type Item = RegisteredProtocolEvent<TMessage>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		// Flushing the local queue.
		while let Some(packet) = self.send_queue.pop_front() {
			match self.inner.start_send(packet)? {
				AsyncSink::NotReady(packet) => {
					self.send_queue.push_front(packet);
					break
				},
				AsyncSink::Ready => self.requires_poll_complete = true,
			}
		}

		// If we are closing, close as soon as the Sink is closed.
		if self.is_closing {
			return Ok(self.inner.close()?.map(|()| None))
		}

		// Indicating that the remote is clogged if that's the case.
		if self.send_queue.len() >= 2048 {
			if !self.clogged_fuse {
				// Note: this fuse is important not just for preventing us from flooding the logs;
				// 	if you remove the fuse, then we will always return early from this function and
				//	thus never read any message from the network.
				self.clogged_fuse = true;
				return Ok(Async::Ready(Some(RegisteredProtocolEvent::Clogged {
					messages: self.send_queue.iter()
						.map(|m| CustomMessage::from_bytes(&m))
						.filter_map(Result::ok)
						.collect(),
				})))
			}
		} else {
			self.clogged_fuse = false;
		}

		// Flushing if necessary.
		if self.requires_poll_complete {
			if let Async::Ready(()) = self.inner.poll_complete()? {
				self.requires_poll_complete = false;
			}
		}

		// Receiving incoming packets.
		// Note that `inner` is wrapped in a `Fuse`, therefore we can poll it forever.
		match self.inner.poll()? {
			Async::Ready(Some(data)) => {
				let message = <TMessage as CustomMessage>::from_bytes(&data)
					.map_err(|()| {
						warn!(target: "sub-libp2p", "Couldn't decode packet sent by the remote: {:?}", data);
						io::ErrorKind::InvalidData
					})?;
				Ok(Async::Ready(Some(RegisteredProtocolEvent::Message(message))))
			}
			Async::Ready(None) =>
				if !self.requires_poll_complete && self.send_queue.is_empty() {
					Ok(Async::Ready(None))
				} else {
					Ok(Async::NotReady)
				}
			Async::NotReady => Ok(Async::NotReady),
		}
	}
}

impl<TMessage> UpgradeInfo for RegisteredProtocol<TMessage> {
	type Info = RegisteredProtocolName;
	type InfoIter = VecIntoIter<Self::Info>;

	#[inline]
	fn protocol_info(&self) -> Self::InfoIter {
		// Report each version as an individual protocol.
		self.supported_versions.iter().flat_map(|&version| {
			let num = version.to_string();

			// Note that `name1` is the multiplex version, as we priviledge it over the old one.
			let mut name1 = self.base_name.clone();
			name1.extend_from_slice(b"multi/");
			name1.extend_from_slice(num.as_bytes());
			let proto1 = RegisteredProtocolName {
				name: name1,
				version,
				is_multiplex: true,
			};

			let mut name2 = self.base_name.clone();
			name2.extend_from_slice(num.as_bytes());
			let proto2 = RegisteredProtocolName {
				name: name2,
				version,
				is_multiplex: false,
			};

			// Important note: we prioritize the backwards compatible mode for now.
			// After some intensive testing has been done, we should switch to the new mode by
			// default.
			// Then finally we can remove the old mode after everyone has switched.
			// See https://github.com/paritytech/substrate/issues/1692
			iter::once(proto2).chain(iter::once(proto1))
		}).collect::<Vec<_>>().into_iter()
	}
}

/// Implementation of `ProtocolName` for a custom protocol.
#[derive(Debug, Clone)]
pub struct RegisteredProtocolName {
	/// Protocol name, as advertised on the wire.
	name: Bytes,
	/// Version number. Stored in string form in `name`, but duplicated here for easier retrieval.
	version: u8,
	/// If true, then this version is the one with the multiplexing.
	is_multiplex: bool,
}

impl ProtocolName for RegisteredProtocolName {
	fn protocol_name(&self) -> &[u8] {
		&self.name
	}
}

impl<TMessage, TSubstream> InboundUpgrade<TSubstream> for RegisteredProtocol<TMessage>
where TSubstream: AsyncRead + AsyncWrite,
{
	type Output = RegisteredProtocolSubstream<TMessage, TSubstream>;
	type Future = future::FutureResult<Self::Output, io::Error>;
	type Error = io::Error;

	fn upgrade_inbound(
		self,
		socket: Negotiated<TSubstream>,
		info: Self::Info,
	) -> Self::Future {
		let framed = {
			let mut codec = UviBytes::default();
			codec.set_max_len(16 * 1024 * 1024);		// 16 MiB hard limit for packets.
			Framed::new(socket, codec)
		};

		future::ok(RegisteredProtocolSubstream {
			is_closing: false,
			endpoint: Endpoint::Listener,
			send_queue: VecDeque::new(),
			requires_poll_complete: false,
			inner: framed.fuse(),
			protocol_id: self.id,
			protocol_version: info.version,
			clogged_fuse: false,
			is_multiplex: info.is_multiplex,
			marker: PhantomData,
		})
	}
}

impl<TMessage, TSubstream> OutboundUpgrade<TSubstream> for RegisteredProtocol<TMessage>
where TSubstream: AsyncRead + AsyncWrite,
{
	type Output = <Self as InboundUpgrade<TSubstream>>::Output;
	type Future = <Self as InboundUpgrade<TSubstream>>::Future;
	type Error = <Self as InboundUpgrade<TSubstream>>::Error;

	fn upgrade_outbound(
		self,
		socket: Negotiated<TSubstream>,
		info: Self::Info,
	) -> Self::Future {
		let framed = Framed::new(socket, UviBytes::default());

		future::ok(RegisteredProtocolSubstream {
			is_closing: false,
			endpoint: Endpoint::Dialer,
			send_queue: VecDeque::new(),
			requires_poll_complete: false,
			inner: framed.fuse(),
			protocol_id: self.id,
			protocol_version: info.version,
			clogged_fuse: false,
			is_multiplex: info.is_multiplex,
			marker: PhantomData,
		})
	}
}

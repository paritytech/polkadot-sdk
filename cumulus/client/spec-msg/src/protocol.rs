// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The `/spec-msg/exchange` request-response protocol.
//!
//! A parachain-network protocol between the *sender's* and the *receiver's*
//! parachain peers: the receiver's collators dial the sender chain's nodes
//! (discovered via the relay DHT) and speak this protocol on their network.
//! The name is therefore deliberately NOT genesis-scoped — the two ends
//! belong to different chains and must agree on it a priori. Nothing is
//! trusted by connection anyway: every request names the `StreamsRoot` it
//! is willing to depend on, and the requester verifies node-side
//! ([`crate::verify`]) against exactly that root.
//!
//! Servers refuse unservable requests at the transport level (`Err` /
//! dropped response), carrying no detail: a request is a pure function of
//! `(stream, position, under)` — it either serves or fails.
//!
//! Registration: [`SpecMsgRequestHandler::new`] returns the protocol config
//! for `net_config.add_request_response_protocol` plus the handler to
//! spawn; requesters (the receiver-side fetch loop) send SCALE-encoded
//! [`ExchangeRequest`]s via `NetworkRequest::request` under
//! [`PROTOCOL_NAME`].

use std::{sync::Arc, time::Duration};

use codec::{DecodeAll, Encode};
use futures::StreamExt;
use parking_lot::RwLock;
use sc_client_api::AuxStore;
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	NetworkBackend, PeerId,
};
use sp_runtime::traits::Block as BlockT;

use cumulus_primitives_spec_messaging::{ExchangeRequest, ExchangeResponse};
use polkadot_core_primitives::Hash;

use crate::{archive::SpecMsgArchive, LOG_TARGET};

/// The exchange protocol's name. Chain-agnostic by design (see the module
/// docs); versioned for future wire evolution.
pub const PROTOCOL_NAME: &str = "/spec-msg/exchange/1";

/// Requests are two fixed-size structs plus the envelope byte; generous
/// slack for future request kinds.
const MAX_REQUEST_SIZE: u64 = 256;

/// Transport-level response cap: served payload bytes are bounded by the
/// server at 4 MiB, the rest is proof material (O(log) hashes) and
/// envelope.
pub const MAX_RESPONSE_SIZE: u64 = 8 * 1024 * 1024;

/// Requests may cross chain boundaries onto a fresh connection; allow for
/// dial + transfer of a full response.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Pending incoming requests; beyond it, peers get an immediate refusal
/// rather than a slow timeout.
const INBOUND_QUEUE_SIZE: usize = 64;

/// Creates the `/spec-msg/exchange` protocol config feeding `inbound_queue`
/// — pass to `net_config.add_request_response_protocol`. Use
/// [`SpecMsgRequestHandler::new`] unless the receiver is wired manually
/// (e.g. an outbound-only node would pass no queue via its own config).
pub fn spec_msg_protocol_config<Block: BlockT, Network: NetworkBackend<Block, Block::Hash>>(
	inbound_queue: async_channel::Sender<IncomingRequest>,
) -> Network::RequestResponseProtocolConfig {
	Network::request_response_config(
		PROTOCOL_NAME.into(),
		Vec::new(),
		MAX_REQUEST_SIZE,
		MAX_RESPONSE_SIZE,
		REQUEST_TIMEOUT,
		Some(inbound_queue),
	)
}

/// Answers incoming `/spec-msg/exchange` requests from the sender-side
/// archive. Spawn [`Self::run`] as a task.
pub struct SpecMsgRequestHandler<Block: BlockT<Hash = Hash>, AUX> {
	archive: Arc<RwLock<SpecMsgArchive<Block, AUX>>>,
	request_receiver: async_channel::Receiver<IncomingRequest>,
}

impl<Block: BlockT<Hash = Hash>, AUX: AuxStore> SpecMsgRequestHandler<Block, AUX> {
	/// New handler over `archive`, plus the protocol config to register.
	pub fn new<Network: NetworkBackend<Block, Block::Hash>>(
		archive: Arc<RwLock<SpecMsgArchive<Block, AUX>>>,
	) -> (Self, Network::RequestResponseProtocolConfig) {
		let (tx, request_receiver) = async_channel::bounded(INBOUND_QUEUE_SIZE);
		let config = spec_msg_protocol_config::<Block, Network>(tx);
		(Self { archive, request_receiver }, config)
	}

	/// Serves requests until the network side drops the queue.
	pub async fn run(mut self) {
		while let Some(request) = self.request_receiver.next().await {
			let IncomingRequest { peer, payload, pending_response } = request;
			let result = self.handle_request(&peer, &payload);
			if let Err(()) = &result {
				tracing::debug!(target: LOG_TARGET, ?peer, "Refused exchange request");
			}
			let _ = pending_response.send(OutgoingResponse {
				result,
				reputation_changes: Vec::new(),
				sent_feedback: None,
			});
		}
	}

	/// One request in, one encoded response (or a bare refusal) out — pure
	/// function of the request against the archive.
	fn handle_request(&self, peer: &PeerId, payload: &[u8]) -> Result<Vec<u8>, ()> {
		let request = ExchangeRequest::decode_all(&mut &payload[..]).map_err(|_| ())?;
		let archive = self.archive.read();
		match request {
			ExchangeRequest::Messages(request) => archive
				.serve_messages(&request)
				.map(|response| {
					let bytes: usize = response.payloads.iter().map(Vec::len).sum();
					tracing::debug!(
						target: LOG_TARGET,
						?peer,
						?request,
						payloads = response.payloads.len(),
						bytes,
						"Served messages request",
					);
					ExchangeResponse::Messages(response).encode()
				})
				.map_err(|error| {
					tracing::debug!(target: LOG_TARGET, %error, ?request, "Messages request");
				}),
			ExchangeRequest::Event(request) => archive
				.serve_event(&request)
				.map(|response| {
					tracing::debug!(
						target: LOG_TARGET,
						?peer,
						?request,
						bytes = response.payload.len(),
						"Served event request",
					);
					ExchangeResponse::Event(response).encode()
				})
				.map_err(|error| {
					tracing::debug!(target: LOG_TARGET, %error, ?request, "Event request");
				}),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::*;
	use cumulus_primitives_spec_messaging::{MessagePosition, MessagesRequest, StreamsRoot};

	fn handler(archive: TestArchive) -> SpecMsgRequestHandler<TestBlock, TestAux> {
		let (_tx, request_receiver) = async_channel::bounded(1);
		SpecMsgRequestHandler { archive: Arc::new(RwLock::new(archive)), request_receiver }
	}

	#[test]
	fn handler_round_trips_the_wire_encoding() {
		let (_aux, mut archive) = new_archive();
		let stream = channel(2001);
		import_blocks(&mut archive, &[stream], 3, 2, 1_000);
		let under = archive
			.import_block_at(block_hash(4), block_hash(3), 4, Vec::new(), 1_004)
			.expect("extends the tip")
			.expect("streams exist");

		let request =
			MessagesRequest { stream, start: MessagePosition(0), under, max_bytes: u32::MAX };
		let direct = archive.serve_messages(&request).expect("serves under the head root");

		let handler = handler(archive);
		let peer = PeerId::random();
		let encoded = handler
			.handle_request(&peer, &ExchangeRequest::Messages(request).encode())
			.expect("same request over the wire serves too");
		assert_eq!(
			ExchangeResponse::decode_all(&mut &encoded[..]).expect("valid encoding"),
			ExchangeResponse::Messages(direct)
		);

		// Garbage and unservable requests are bare refusals.
		assert_eq!(handler.handle_request(&peer, b"not a request"), Err(()));
		let unknown = MessagesRequest {
			stream,
			start: MessagePosition(0),
			under: StreamsRoot(Hash::repeat_byte(0xAA)),
			max_bytes: 0,
		};
		assert_eq!(
			handler.handle_request(&peer, &ExchangeRequest::Messages(unknown).encode()),
			Err(())
		);
	}
}

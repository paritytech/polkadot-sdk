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

//! The `/spec-msg/exchange/1` request-response protocol. Not genesis-scoped, since the two ends are
//! on different chains. Nothing is trusted by connection: the requester verifies each response
//! under the `StreamsRoot` it named. A server that cannot serve refuses without detail.

use crate::{archive::Archive, store::ArchiveStore, LOG_TARGET};
use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_spec_messaging::{ExchangeRequest, ExchangeResponse};
use futures::StreamExt;
use parking_lot::RwLock;
use sc_network::{
	config::OutgoingResponse, request_responses::IncomingRequest, service::traits::NetworkBackend,
	ProtocolName,
};
use sp_runtime::traits::Block as BlockT;
use std::{fmt::Debug, sync::Arc, time::Duration};

/// The protocol name.
pub const PROTOCOL_NAME: &str = "/spec-msg/exchange/1";

/// Maximum encoded request size. A request is a stream id, a position, a root and a budget.
pub const MAX_REQUEST_SIZE: u64 = 1024;

/// Maximum payload bytes served per response, whatever the request's budget.
pub const MAX_PAYLOAD_BYTES: u32 = 4 * 1024 * 1024;

/// Maximum encoded response size: the payload cap plus generous room for the proofs and the
/// per-payload length prefixes.
pub const MAX_RESPONSE_SIZE: u64 = MAX_PAYLOAD_BYTES as u64 + 1024 * 1024;

/// Pending inbound requests the handler queues before the network drops new ones.
const INBOUND_QUEUE_SIZE: usize = 1024;

/// Request timeout.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Decoding depth limit for requests. They are flat; this only blocks crafted nesting.
const MAX_DECODE_DEPTH: u32 = 16;

/// The protocol config to register with the network, and the receiver of inbound requests for
/// [`run_request_handler`].
pub fn exchange_protocol_config<B: BlockT, N: NetworkBackend<B, <B as BlockT>::Hash>>(
) -> (N::RequestResponseProtocolConfig, async_channel::Receiver<IncomingRequest>) {
	let (inbound_tx, inbound_rx) = async_channel::bounded(INBOUND_QUEUE_SIZE);
	let config = N::request_response_config(
		ProtocolName::from(PROTOCOL_NAME),
		Vec::new(),
		MAX_REQUEST_SIZE,
		MAX_RESPONSE_SIZE,
		TIMEOUT,
		Some(inbound_tx),
	);
	(config, inbound_rx)
}

/// Answer one encoded request from `archive`. `None` refuses.
pub fn answer<H, S>(archive: &Archive<H, S>, request: &[u8]) -> Option<Vec<u8>>
where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	let mut input = request;
	let request = ExchangeRequest::decode_with_depth_limit(MAX_DECODE_DEPTH, &mut input).ok()?;
	if !input.is_empty() {
		return None;
	}
	let response = match request {
		ExchangeRequest::Messages(req) => {
			ExchangeResponse::Messages(archive.serve_messages(&req, MAX_PAYLOAD_BYTES)?)
		},
		ExchangeRequest::Event(req) => ExchangeResponse::Event(archive.serve_event(&req)?),
	};
	Some(response.encode())
}

/// Serve inbound `/spec-msg/exchange/1` requests from `archive` until the network closes the
/// channel. Reads the archive store synchronously; spawn it as a blocking task.
pub async fn run_request_handler<H, S>(
	archive: Arc<RwLock<Archive<H, S>>>,
	mut requests: async_channel::Receiver<IncomingRequest>,
) where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	while let Some(IncomingRequest { peer, payload, pending_response }) = requests.next().await {
		let result = answer(&archive.read(), &payload).ok_or(());
		match &result {
			Ok(response) => log::debug!(
				target: LOG_TARGET,
				"Served a spec-msg exchange request from {peer}: {} bytes",
				response.len(),
			),
			Err(()) => {
				log::debug!(target: LOG_TARGET, "Refused a spec-msg exchange request from {peer}")
			},
		}
		let response =
			OutgoingResponse { result, reputation_changes: Vec::new(), sent_feedback: None };
		if pending_response.send(response).is_err() {
			log::debug!(target: LOG_TARGET, "Peer {peer} dropped its spec-msg exchange request");
		}
	}
}

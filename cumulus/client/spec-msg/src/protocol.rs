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

//! The `/spec-msg/fetch/1` request-response protocol. Not genesis-scoped, since the two ends are
//! on different chains. Nothing is trusted by connection: the requester verifies each response
//! under the `StreamsRoot` it named. A server that cannot serve answers with a typed `FetchError`;
//! a request that does not decode is refused without detail.

use crate::{archive::Archive, store::ArchiveStore, LOG_TARGET};
use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_spec_messaging::MessagesRequest;
use parking_lot::RwLock;
use sc_network::{
	config::OutgoingResponse, request_responses::IncomingRequest, service::traits::NetworkBackend,
	PeerId, ProtocolName,
};
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, VecDeque},
	fmt::Debug,
	sync::Arc,
	time::Duration,
};

/// The protocol name.
pub const PROTOCOL_NAME: &str = "/spec-msg/fetch/1";

/// Maximum encoded request size. A request is a stream id, a position, a root and a budget.
pub const MAX_REQUEST_SIZE: u64 = 1024;

/// Maximum payload bytes served per response, whatever the request's budget.
pub const MAX_PAYLOAD_BYTES: u32 = 4 * 1024 * 1024;

/// Maximum encoded response size, enforced by the network before decode: the payload cap plus
/// generous room for the proofs and the per-payload length prefixes.
pub const MAX_RESPONSE_SIZE: u64 = MAX_PAYLOAD_BYTES as u64 + 1024 * 1024;

/// Pending inbound requests the handler queues before the network drops new ones.
const INBOUND_QUEUE_SIZE: usize = 1024;

/// Requests queued per peer. Beyond it a peer's requests are refused, so one peer cannot fill the
/// queue and starve the others.
const MAX_QUEUED_PER_PEER: usize = 4;

/// Request timeout.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Decoding depth limit for requests. They are flat; this only blocks crafted nesting.
const MAX_DECODE_DEPTH: u32 = 16;

/// The protocol config to register with the network, and the receiver of inbound requests for
/// [`run_request_handler`].
pub fn fetch_protocol_config<B: BlockT, N: NetworkBackend<B, <B as BlockT>::Hash>>(
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

/// Answer one encoded request from `archive` with an encoded `FetchResponse`. `None` refuses a
/// request that does not decode.
pub fn answer<H, S>(archive: &Archive<H, S>, request: &[u8]) -> Option<Vec<u8>>
where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	let mut input = request;
	let request = MessagesRequest::decode_with_depth_limit(MAX_DECODE_DEPTH, &mut input).ok()?;
	if !input.is_empty() {
		return None;
	}
	Some(archive.serve_messages(&request, MAX_PAYLOAD_BYTES).encode())
}

/// Queued requests per peer, served round-robin across peers.
#[derive(Default)]
struct InboundQueue {
	per_peer: HashMap<PeerId, VecDeque<IncomingRequest>>,
	rotation: VecDeque<PeerId>,
}

impl InboundQueue {
	/// Queue `request`, or hand it back when its peer's queue is full.
	fn push(&mut self, request: IncomingRequest) -> Result<(), IncomingRequest> {
		let queue = self.per_peer.entry(request.peer).or_default();
		if queue.len() >= MAX_QUEUED_PER_PEER {
			return Err(request);
		}
		// A peer is in the rotation exactly while its queue is non-empty.
		if queue.is_empty() {
			self.rotation.push_back(request.peer);
		}
		queue.push_back(request);
		Ok(())
	}

	/// Take the oldest request of the next peer in the rotation.
	fn pop(&mut self) -> Option<IncomingRequest> {
		let peer = self.rotation.pop_front()?;
		let queue = self.per_peer.get_mut(&peer)?;
		let request = queue.pop_front();
		if queue.is_empty() {
			self.per_peer.remove(&peer);
		} else {
			self.rotation.push_back(peer);
		}
		request
	}
}

fn respond(
	peer: PeerId,
	pending_response: futures::channel::oneshot::Sender<OutgoingResponse>,
	result: Result<Vec<u8>, ()>,
) {
	let response = OutgoingResponse { result, reputation_changes: Vec::new(), sent_feedback: None };
	if pending_response.send(response).is_err() {
		log::debug!(target: LOG_TARGET, "Peer {peer} dropped its spec-msg fetch request");
	}
}

/// Serve inbound `/spec-msg/fetch/1` requests from `archive` until the network closes the
/// channel and the queued requests are answered. Reads the archive store synchronously; spawn it
/// as a blocking task.
pub async fn run_request_handler<H, S>(
	archive: Arc<RwLock<Archive<H, S>>>,
	requests: async_channel::Receiver<IncomingRequest>,
) where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	let mut queue = InboundQueue::default();
	let mut closed = false;
	loop {
		loop {
			match requests.try_recv() {
				Ok(request) => admit(&mut queue, request),
				Err(async_channel::TryRecvError::Empty) => break,
				Err(async_channel::TryRecvError::Closed) => {
					closed = true;
					break;
				},
			}
		}
		let Some(IncomingRequest { peer, payload, pending_response }) = queue.pop() else {
			if closed {
				return;
			}
			match requests.recv().await {
				Ok(request) => admit(&mut queue, request),
				Err(_) => closed = true,
			}
			continue;
		};
		let result = answer(&archive.read(), &payload).ok_or(());
		match &result {
			Ok(response) => log::debug!(
				target: LOG_TARGET,
				"Served a spec-msg fetch request from {peer}: {} bytes",
				response.len(),
			),
			Err(()) => {
				log::debug!(target: LOG_TARGET, "Refused a spec-msg fetch request from {peer}")
			},
		}
		respond(peer, pending_response, result);
	}
}

fn admit(queue: &mut InboundQueue, request: IncomingRequest) {
	if let Err(IncomingRequest { peer, pending_response, .. }) = queue.push(request) {
		log::debug!(target: LOG_TARGET, "Refused a spec-msg fetch request from {peer}: queue full");
		respond(peer, pending_response, Err(()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{channel, import, messages, payloads, Model, TestArchive},
		MemoryStore,
	};
	use futures::channel::oneshot;

	type Response = oneshot::Receiver<OutgoingResponse>;

	fn request(peer: PeerId, payload: Vec<u8>) -> (IncomingRequest, Response) {
		let (pending_response, response) = oneshot::channel();
		(IncomingRequest { peer, payload, pending_response }, response)
	}

	fn valid_payload() -> Vec<u8> {
		let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
		let mut model = Model::default();
		let stream = channel(2000);
		let root = import(&mut archive, &mut model, 1, vec![(stream, payloads(1, 1))]);
		messages(stream, 0, root, u32::MAX).encode()
	}

	fn archive() -> Arc<RwLock<TestArchive>> {
		Arc::new(RwLock::new(TestArchive::open(Arc::new(MemoryStore::default()))))
	}

	#[test]
	fn peers_are_served_in_turn() {
		let (a, b) = (PeerId::random(), PeerId::random());
		let mut queue = InboundQueue::default();
		for _ in 0..4 {
			assert!(queue.push(request(a, vec![]).0).is_ok());
		}
		assert!(queue.push(request(b, vec![]).0).is_ok());

		let order: Vec<_> = std::iter::from_fn(|| queue.pop()).map(|r| r.peer).collect();
		assert_eq!(order, vec![a, b, a, a, a]);
	}

	#[test]
	fn a_peer_beyond_its_cap_is_refused_at_once() {
		let peer = PeerId::random();
		let (tx, rx) = async_channel::bounded(16);
		let responses: Vec<_> = (0..=MAX_QUEUED_PER_PEER)
			.map(|_| {
				let (incoming, response) = request(peer, valid_payload());
				tx.try_send(incoming).unwrap();
				response
			})
			.collect();
		drop(tx);
		futures::executor::block_on(run_request_handler(archive(), rx));

		let results: Vec<_> = responses
			.into_iter()
			.map(|r| futures::executor::block_on(r).unwrap().result.is_ok())
			.collect();
		assert_eq!(results, vec![true, true, true, true, false]);
	}

	#[test]
	fn the_handler_returns_once_the_channel_is_closed_and_drained() {
		let (tx, rx) = async_channel::bounded(16);
		let (incoming, response) = request(PeerId::random(), valid_payload());
		tx.try_send(incoming).unwrap();
		drop(tx);
		futures::executor::block_on(run_request_handler(archive(), rx));
		assert!(futures::executor::block_on(response).unwrap().result.is_ok());
	}
}

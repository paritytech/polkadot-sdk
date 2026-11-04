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
use futures::{
	channel::oneshot,
	future::{self, Either, Fuse},
	select,
	stream::FuturesUnordered,
	FutureExt, StreamExt,
};
use futures_timer::Delay;
use parking_lot::RwLock;
use sc_network::{
	config::OutgoingResponse, request_responses::IncomingRequest, service::traits::NetworkBackend,
	PeerId, ProtocolName,
};
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	fmt::Debug,
	sync::Arc,
	time::Duration,
};

/// The protocol name.
pub const PROTOCOL_NAME: &str = "/spec-msg/fetch/1";

/// Maximum encoded request size. A request is a stream id, a position, a root and a budget.
pub const MAX_REQUEST_SIZE: u64 = 1024;

/// Maximum payload bytes served per response, whatever the request's budget. The receiver asks for
/// exactly this much ([`crate::receiver::FETCH_MAX_BYTES`]) and every other request for less, so a
/// larger cap would only let peers that ask for more pin more of [`MAX_IN_FLIGHT_BYTES`].
pub const MAX_PAYLOAD_BYTES: u32 = 1024 * 1024;

/// Room in a response for everything but the payload bytes: the result envelope, the frontier and
/// the extension (48 hashes each at most), the tree proof (64 steps) and up to
/// `MAX_MESSAGES_PER_RESPONSE` payload length prefixes. About 9.3 KiB at worst.
const MAX_FRAMING_BYTES: u64 = 16 * 1024;

/// Maximum encoded response size, enforced by the network before decode: the payload cap and the
/// worst-case framing around it.
pub const MAX_RESPONSE_SIZE: u64 = MAX_PAYLOAD_BYTES as u64 + MAX_FRAMING_BYTES;

/// Pending inbound requests the handler queues before the network drops new ones.
const INBOUND_QUEUE_SIZE: usize = 1024;

/// Requests queued per peer. Beyond it a peer's requests are refused, so one peer cannot fill the
/// queue and starve the others.
const MAX_QUEUED_PER_PEER: usize = 4;

/// Request timeout.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Response bytes handed to the network and not yet sent, across all peers. A response waits in
/// the network until the peer reads it or the request times out, so peers that do not read could
/// otherwise pin many full responses. Beyond it requests are refused.
const MAX_IN_FLIGHT_BYTES: usize = 64 * 1024 * 1024;

/// How long a response counts as in flight when the network never reports it sent or dropped.
/// The network gives up on it after [`TIMEOUT`].
const IN_FLIGHT_TIMEOUT: Duration = Duration::from_secs(2 * TIMEOUT.as_secs());

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

/// Queued requests per peer, served round-robin across peers. A peer whose request is being
/// served, or whose response is in flight, is out of the rotation until [`Self::release`].
#[derive(Default)]
struct InboundQueue {
	per_peer: HashMap<PeerId, VecDeque<IncomingRequest>>,
	rotation: VecDeque<PeerId>,
	held: HashSet<PeerId>,
}

impl InboundQueue {
	/// Queue `request`, or hand it back when its peer's queue is full.
	fn push(&mut self, request: IncomingRequest) -> Result<(), IncomingRequest> {
		let queue = self.per_peer.entry(request.peer).or_default();
		if queue.len() >= MAX_QUEUED_PER_PEER {
			return Err(request);
		}
		// A peer is in the rotation exactly while its queue is non-empty and it is not held.
		if queue.is_empty() && !self.held.contains(&request.peer) {
			self.rotation.push_back(request.peer);
		}
		queue.push_back(request);
		Ok(())
	}

	/// Take the oldest request of the next peer in the rotation, and hold that peer.
	fn pop(&mut self) -> Option<IncomingRequest> {
		let peer = self.rotation.pop_front()?;
		let queue = self.per_peer.get_mut(&peer)?;
		let request = queue.pop_front();
		if queue.is_empty() {
			self.per_peer.remove(&peer);
		}
		self.held.insert(peer);
		request
	}

	/// `peer` may be served again: back in the rotation if it has queued requests.
	fn release(&mut self, peer: PeerId) {
		if self.held.remove(&peer) && self.per_peer.contains_key(&peer) {
			self.rotation.push_back(peer);
		}
	}

	fn is_empty(&self) -> bool {
		self.per_peer.is_empty()
	}
}

/// Bounds on the responses handed to the network and not yet sent.
#[derive(Clone, Copy, Debug)]
struct InFlightLimits {
	/// Total response bytes, across peers.
	max_bytes: usize,
	/// How long a response counts as in flight at most.
	timeout: Duration,
}

impl Default for InFlightLimits {
	fn default() -> Self {
		Self { max_bytes: MAX_IN_FLIGHT_BYTES, timeout: IN_FLIGHT_TIMEOUT }
	}
}

/// Hand `result` to the network; `false` if the request is gone.
fn respond(
	peer: PeerId,
	pending_response: oneshot::Sender<OutgoingResponse>,
	result: Result<Vec<u8>, ()>,
	sent_feedback: Option<oneshot::Sender<()>>,
) -> bool {
	let response = OutgoingResponse { result, reputation_changes: Vec::new(), sent_feedback };
	let handed = pending_response.send(response).is_ok();
	if !handed {
		log::debug!(target: LOG_TARGET, "Peer {peer} dropped its spec-msg fetch request");
	}
	handed
}

/// Serve inbound `/spec-msg/fetch/1` requests from `archive` until the network closes the
/// channel and the queued requests are answered. Reads the archive store synchronously; spawn it
/// as a blocking task.
///
/// A peer has at most one response in flight (served, not yet sent); its other requests wait in
/// its queue. Requests that would take the responses in flight past [`MAX_IN_FLIGHT_BYTES`] are
/// refused.
pub async fn run_request_handler<H, S>(
	archive: Arc<RwLock<Archive<H, S>>>,
	requests: async_channel::Receiver<IncomingRequest>,
) where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	serve(archive, requests, InFlightLimits::default()).await
}

async fn serve<H, S>(
	archive: Arc<RwLock<Archive<H, S>>>,
	requests: async_channel::Receiver<IncomingRequest>,
	limits: InFlightLimits,
) where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	let mut queue = InboundQueue::default();
	// Each in-flight response: resolves with its peer and size once sent, dropped or timed out.
	let mut in_flight = FuturesUnordered::new();
	let mut in_flight_bytes = 0usize;
	let mut closed = false;
	loop {
		while let Some(Some((peer, bytes))) = in_flight.next().now_or_never() {
			in_flight_bytes -= bytes;
			queue.release(peer);
		}
		while !closed {
			match requests.try_recv() {
				Ok(request) => admit(&mut queue, request),
				Err(async_channel::TryRecvError::Empty) => break,
				Err(async_channel::TryRecvError::Closed) => closed = true,
			}
		}
		let Some(IncomingRequest { peer, payload, pending_response }) = queue.pop() else {
			if closed && queue.is_empty() {
				return;
			}
			let mut next = if closed { Fuse::terminated() } else { requests.recv().fuse() };
			select! {
				request = next => match request {
					Ok(request) => admit(&mut queue, request),
					Err(_) => closed = true,
				},
				(peer, bytes) = in_flight.select_next_some() => {
					in_flight_bytes -= bytes;
					queue.release(peer);
				},
				complete => return,
			}
			continue;
		};
		let response = match answer(&archive.read(), &payload) {
			Some(response) if in_flight_bytes + response.len() <= limits.max_bytes => response,
			Some(response) => {
				log::debug!(
					target: LOG_TARGET,
					"Refused a spec-msg fetch request from {peer}: {} response bytes in flight, \
					 {} more would pass the cap",
					in_flight_bytes,
					response.len(),
				);
				respond(peer, pending_response, Err(()), None);
				queue.release(peer);
				continue;
			},
			None => {
				log::debug!(target: LOG_TARGET, "Refused a spec-msg fetch request from {peer}");
				respond(peer, pending_response, Err(()), None);
				queue.release(peer);
				continue;
			},
		};
		let bytes = response.len();
		log::debug!(
			target: LOG_TARGET,
			"Served a spec-msg fetch request from {peer}: {bytes} bytes",
		);
		let (sent_tx, sent) = oneshot::channel();
		if !respond(peer, pending_response, Ok(response), Some(sent_tx)) {
			queue.release(peer);
			continue;
		}
		in_flight_bytes += bytes;
		let timeout = limits.timeout;
		in_flight.push(async move {
			// Sent, or dropped by the network: either way no longer held.
			if let Either::Right(_) = future::select(sent, Delay::new(timeout)).await {
				log::debug!(
					target: LOG_TARGET,
					"Response to {peer} not reported sent within {timeout:?}; no longer counted",
				);
			}
			(peer, bytes)
		});
	}
}

fn admit(queue: &mut InboundQueue, request: IncomingRequest) {
	if let Err(IncomingRequest { peer, pending_response, .. }) = queue.push(request) {
		log::debug!(target: LOG_TARGET, "Refused a spec-msg fetch request from {peer}: queue full");
		respond(peer, pending_response, Err(()), None);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{channel, import, messages, Model, TestArchive},
		MemoryStore,
	};
	use futures::{executor::LocalPool, task::LocalSpawnExt};

	type Response = oneshot::Receiver<OutgoingResponse>;

	fn request(peer: PeerId, payload: Vec<u8>) -> (IncomingRequest, Response) {
		let (pending_response, response) = oneshot::channel();
		(IncomingRequest { peer, payload, pending_response }, response)
	}

	/// An archive with eight 1 KiB sends, and a request for them.
	fn archive_and_request() -> (Arc<RwLock<TestArchive>>, Vec<u8>) {
		let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
		let stream = channel(2000);
		let sends = vec![(stream, vec![vec![7; 1024]; 8])];
		let root = import(&mut archive, &mut Model::default(), 1, sends);
		(Arc::new(RwLock::new(archive)), messages(stream, 0, root, u32::MAX).encode())
	}

	/// The handler's answer so far; `None` while the request waits.
	fn answered(response: &mut Response) -> Option<OutgoingResponse> {
		response.try_recv().expect("no request is dropped")
	}

	/// [`serve`] on a local pool.
	struct Handler {
		pool: LocalPool,
		requests: async_channel::Sender<IncomingRequest>,
		payload: Vec<u8>,
	}

	impl Handler {
		fn new(limits: InFlightLimits) -> Self {
			let (archive, payload) = archive_and_request();
			let (requests, inbound) = async_channel::bounded(16);
			let pool = LocalPool::new();
			pool.spawner().spawn_local(serve(archive, inbound, limits)).unwrap();
			Self { pool, requests, payload }
		}

		/// `peer` sends a request; run until nothing can progress.
		fn request(&mut self, peer: PeerId) -> Response {
			let (incoming, response) = request(peer, self.payload.clone());
			self.requests.try_send(incoming).unwrap();
			self.pool.run_until_stalled();
			response
		}
	}

	#[test]
	fn peers_are_served_in_turn() {
		let (a, b) = (PeerId::random(), PeerId::random());
		let mut queue = InboundQueue::default();
		for _ in 0..4 {
			assert!(queue.push(request(a, vec![]).0).is_ok());
		}
		assert!(queue.push(request(b, vec![]).0).is_ok());

		let order: Vec<_> = std::iter::from_fn(|| {
			let request = queue.pop()?;
			queue.release(request.peer);
			Some(request.peer)
		})
		.collect();
		assert_eq!(order, vec![a, b, a, a, a]);
	}

	#[test]
	fn a_held_peer_is_out_of_the_rotation_until_released() {
		let (a, b) = (PeerId::random(), PeerId::random());
		let mut queue = InboundQueue::default();
		queue.push(request(a, vec![]).0).unwrap();
		assert_eq!(queue.pop().map(|r| r.peer), Some(a));
		// Requests of the held peer queue up but are not taken.
		queue.push(request(a, vec![]).0).unwrap();
		queue.push(request(b, vec![]).0).unwrap();
		assert_eq!(queue.pop().map(|r| r.peer), Some(b));
		assert!(queue.pop().is_none());
		queue.release(a);
		assert_eq!(queue.pop().map(|r| r.peer), Some(a));
	}

	#[test]
	fn a_peer_beyond_its_cap_is_refused_at_once() {
		let mut handler = Handler::new(InFlightLimits::default());
		let peer = PeerId::random();
		// One response in flight, four requests waiting; the next is refused.
		let mut responses: Vec<_> =
			(0..MAX_QUEUED_PER_PEER + 2).map(|_| handler.request(peer)).collect();
		let mut first = answered(&mut responses[0]).expect("served");
		assert!(first.result.is_ok());
		for waiting in &mut responses[1..=MAX_QUEUED_PER_PEER] {
			assert!(answered(waiting).is_none());
		}
		let refused = answered(&mut responses[MAX_QUEUED_PER_PEER + 1]).expect("refused");
		assert_eq!(refused.result, Err(()));

		// Each sent response lets the next request through.
		for waiting in &mut responses[1..=MAX_QUEUED_PER_PEER] {
			first.sent_feedback.take().unwrap().send(()).unwrap();
			handler.pool.run_until_stalled();
			first = answered(waiting).expect("served once the previous response is sent");
			assert!(first.result.is_ok());
		}
	}

	#[test]
	fn a_peer_is_served_again_once_its_response_is_sent() {
		let mut handler = Handler::new(InFlightLimits::default());
		let (a, b) = (PeerId::random(), PeerId::random());
		let (mut first, mut second, mut other) =
			(handler.request(a), handler.request(a), handler.request(b));
		let first = answered(&mut first).expect("served");
		assert!(first.result.is_ok());
		assert!(answered(&mut second).is_none(), "waits while the first response is not sent");
		assert!(answered(&mut other).expect("another peer is served").result.is_ok());

		first.sent_feedback.unwrap().send(()).unwrap();
		handler.pool.run_until_stalled();
		assert!(answered(&mut second).expect("served").result.is_ok());
	}

	#[test]
	fn a_response_the_network_drops_frees_its_peer() {
		let mut handler = Handler::new(InFlightLimits::default());
		let peer = PeerId::random();
		let (mut first, mut second) = (handler.request(peer), handler.request(peer));
		// The network drops the response, and its feedback sender with it.
		drop(answered(&mut first).expect("served"));
		handler.pool.run_until_stalled();
		assert!(answered(&mut second).expect("served").result.is_ok());
	}

	#[test]
	fn a_response_never_reported_sent_frees_its_peer_after_the_timeout() {
		let timeout = Duration::from_millis(50);
		let mut handler = Handler::new(InFlightLimits { timeout, ..Default::default() });
		let peer = PeerId::random();
		let (mut first, mut second) = (handler.request(peer), handler.request(peer));
		// The network keeps the response and never resolves its feedback.
		let _unreported = answered(&mut first).expect("served");
		assert!(answered(&mut second).is_none());
		handler.pool.run_until(Delay::new(timeout * 4));
		assert!(answered(&mut second).expect("served after the timeout").result.is_ok());
	}

	#[test]
	fn response_bytes_in_flight_are_capped_across_peers() {
		let (archive, payload) = archive_and_request();
		let size = answer(&archive.read(), &payload).unwrap().len();
		assert!(size > 8 * 1024);
		let mut handler =
			Handler::new(InFlightLimits { max_bytes: 2 * size, ..Default::default() });
		let peers: Vec<_> = (0..3).map(|_| PeerId::random()).collect();
		let mut answers = peers.iter().map(|peer| {
			let mut response = handler.request(*peer);
			answered(&mut response).expect("answered at once")
		});
		// Two responses fill the cap and are never reported sent; the third peer is refused.
		let (first, second) = (answers.next().unwrap(), answers.next().unwrap());
		assert!(first.result.is_ok() && second.result.is_ok());
		assert_eq!(answers.next().unwrap().result, Err(()));
		drop(answers);

		// One is sent: room for one more.
		first.sent_feedback.unwrap().send(()).unwrap();
		handler.pool.run_until_stalled();
		let mut again = handler.request(peers[2]);
		assert!(answered(&mut again).expect("answered").result.is_ok());
	}

	#[test]
	fn a_full_response_fits_the_response_size() {
		use codec::{Compact, CompactLen, MaxEncodedLen};
		use cumulus_primitives_spec_messaging::{
			streams_root::TreeStep, FetchError, FetchResponse, MMRExtensionProof, MessagesResponse,
			MmrFrontier, StreamProof, MAX_MESSAGES_PER_RESPONSE, MAX_SPECULATIVE_MESSAGE_LEN,
		};
		use sp_core::H256;

		// Each part at its bound; a length prefix is at most that of the largest payload.
		let framing = 1 +
			MmrFrontier::max_encoded_len() +
			MMRExtensionProof::max_encoded_len() +
			StreamProof::max_encoded_len() +
			Compact::<u32>::compact_len(&MAX_MESSAGES_PER_RESPONSE) +
			MAX_MESSAGES_PER_RESPONSE as usize *
				Compact::<u32>::compact_len(&MAX_SPECULATIVE_MESSAGE_LEN);
		assert!(framing.max(FetchError::max_encoded_len() + 1) as u64 <= MAX_FRAMING_BYTES);

		// A response that reaches every bound at once encodes within the size.
		let peaks = vec![H256::repeat_byte(1); 48];
		let full = MmrFrontier::from_parts(peaks.clone(), (1 << 48) - 1).unwrap();
		let count = MAX_MESSAGES_PER_RESPONSE as usize;
		let payload = vec![7u8; MAX_PAYLOAD_BYTES as usize / count];
		let step = TreeStep { split_bit: 0, sibling: H256::repeat_byte(2) };
		let response: FetchResponse = Ok(MessagesResponse {
			frontier: full,
			payloads: vec![payload; count].try_into().unwrap(),
			extension: MMRExtensionProof {
				leaf_count: 1 << 48,
				connecting_nodes: peaks.try_into().unwrap(),
			},
			tree_proof: StreamProof { steps: vec![step; 64].try_into().unwrap() },
		});
		assert!(response.encode().len() as u64 <= MAX_RESPONSE_SIZE);
	}

	#[test]
	fn the_handler_returns_once_the_channel_is_closed_and_drained() {
		let (archive, payload) = archive_and_request();
		let (tx, rx) = async_channel::bounded(16);
		let (incoming, response) = request(PeerId::random(), payload);
		tx.try_send(incoming).unwrap();
		drop(tx);
		futures::executor::block_on(run_request_handler(archive, rx));
		assert!(futures::executor::block_on(response).unwrap().result.is_ok());
	}
}

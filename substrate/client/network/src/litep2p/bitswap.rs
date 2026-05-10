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

//! Bidirectional bitswap shim for litep2p.
//!
//! Wraps litep2p's native [`BitswapHandle`] to provide both server-side (inbound WANT handling)
//! and client-side (outbound WANT dispatch + response correlation) functionality.

use crate::{
	bitswap::{is_cid_supported, BitswapProtoMessage, Prefix, LOG_TARGET, PROTOCOL_NAME},
	request_responses::RequestFailure,
	OutboundFailure, ProtocolName, MAX_RESPONSE_SIZE,
};
use cid::Cid;
use futures::{channel::oneshot, StreamExt};
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle, BlockPresenceType, Config, ResponseType, WantType,
};
use prost::Message as ProstMessage;
use sc_client_api::BlockBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::HashMap,
	future::Future,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// Command channel capacity.
const CMD_CHANNEL_CAPACITY: usize = 256;
/// Timeout for pending bitswap requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Interval for reaping expired pending batches.
const EXPIRY_TICK_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) type ResponseSender = oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>;

/// Outbound bitswap command sent from [`super::service::Litep2pNetworkService`].
pub(crate) struct BitswapOutboundCmd {
	pub(crate) peer: litep2p::PeerId,
	pub(crate) wants: Vec<(Cid, WantType)>,
	pub(crate) response_tx: ResponseSender,
}

/// Pending outbound WANT batch.
struct PendingBatch {
	peer: litep2p::PeerId,
	cids: Vec<Cid>,
	responses: HashMap<Cid, ResponseType>,
	response_bytes: usize,
	response_tx: ResponseSender,
	inserted: Instant,
}

/// Litep2p-specific bitswap configuration returned by [`BitswapService::new`].
///
/// Carries the native litep2p [`Config`] and the sender half of the command
/// channel so that [`super::service::Litep2pNetworkService`] can forward
/// client-side bitswap requests.
pub struct BitswapConfig {
	pub(crate) litep2p_config: Config,
	pub(crate) cmd_tx: mpsc::Sender<BitswapOutboundCmd>,
}

/// Type alias for the pending-batches queue.
type PendingBatches = Vec<PendingBatch>;

/// Bidirectional bitswap service for litep2p.
pub(crate) struct BitswapService<Block: BlockT> {
	handle: BitswapHandle,
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	cmd_rx: mpsc::Receiver<BitswapOutboundCmd>,
	pending: PendingBatches,
}

impl<Block: BlockT> BitswapService<Block> {
	/// Create a new bidirectional bitswap service.
	///
	/// Returns the boxed task future (to be spawned on the executor) and the
	/// [`BitswapConfig`] to be passed into the litep2p config builder.
	pub(crate) fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, BitswapConfig) {
		let (litep2p_config, handle) = Config::new();
		let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
		let service = Self { handle, client, cmd_rx, pending: Vec::new() };
		let future = Box::pin(async move { service.run().await });
		let config = BitswapConfig { litep2p_config, cmd_tx };
		(future, config)
	}

	/// Run the bitswap event loop.
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting bidirectional bitswap service");
		let mut expiry_ticker = tokio::time::interval(EXPIRY_TICK_INTERVAL);
		expiry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

		loop {
			tokio::select! {
				event = self.handle.next() => match event {
					Some(BitswapEvent::Request { peer, cids }) =>
						self.handle_inbound_request(peer, cids).await,
					Some(BitswapEvent::Response { peer, responses }) =>
						handle_inbound_response(&mut self.pending, peer, responses),
					None => {
						log::debug!(target: LOG_TARGET, "bitswap handle stream ended");
						return;
					},
				},
				cmd = self.cmd_rx.recv() => match cmd {
					Some(BitswapOutboundCmd { peer, wants, response_tx }) =>
						self.handle_outbound_cmd(peer, wants, response_tx).await,
					None => {
						log::debug!(target: LOG_TARGET, "bitswap cmd channel closed");
						return;
					},
				},
				_ = expiry_ticker.tick() => {
					reap_expired_pending(&mut self.pending, REQUEST_TIMEOUT, Instant::now());
				},
			}
		}
	}

	/// Handle an inbound bitswap WANT request from `peer`.
	async fn handle_inbound_request(&mut self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		log::debug!(target: LOG_TARGET, "bitswap: handle inbound request from {peer:?} for {cids:?}");

		let response: Vec<ResponseType> = cids
			.into_iter()
			.filter(|(cid, _)| is_cid_supported(&cid))
			.map(|(cid, want_type)| {
				let mut hash = H256::default();
				hash.as_mut().copy_from_slice(&cid.hash().digest()[0..32]);
				let transaction = match self.client.indexed_transaction(hash) {
					Ok(ex) => ex,
					Err(error) => {
						log::error!(target: LOG_TARGET, "error retrieving transaction {hash}: {error}");
						None
					},
				};
				match transaction {
					Some(transaction) => match want_type {
						WantType::Block => ResponseType::Block { cid, block: transaction },
						_ => ResponseType::Presence { cid, presence: BlockPresenceType::Have },
					},
					None => ResponseType::Presence { cid, presence: BlockPresenceType::DontHave },
				}
			})
			.collect();

		self.handle.send_response(peer, response).await;
	}

	/// Handle an outbound bitswap command from the network service.
	async fn handle_outbound_cmd(
		&mut self,
		peer: litep2p::PeerId,
		wants: Vec<(Cid, WantType)>,
		response_tx: ResponseSender,
	) {
		log::debug!(
			target: LOG_TARGET,
			"bitswap: outbound WANT for {} CIDs to {peer:?}",
			wants.len(),
		);
		let cids: Vec<_> = wants.iter().map(|(cid, _)| *cid).collect();
		self.pending.push(PendingBatch {
			peer,
			cids,
			responses: HashMap::new(),
			response_bytes: 0,
			response_tx,
			inserted: Instant::now(),
		});
		self.handle.send_request(peer, wants).await;
	}
}

/// Collapse a response list into at most one entry per CID, preferring `Block`
/// over `Presence` when both arrive for the same CID.
fn select_best_response_per_cid(responses: Vec<ResponseType>) -> HashMap<Cid, ResponseType> {
	let mut best: HashMap<Cid, ResponseType> = HashMap::new();
	for resp in responses {
		let cid = match &resp {
			ResponseType::Block { cid, .. } => *cid,
			ResponseType::Presence { cid, .. } => *cid,
		};
		match best.entry(cid) {
			std::collections::hash_map::Entry::Vacant(e) => {
				e.insert(resp);
			},
			std::collections::hash_map::Entry::Occupied(mut e) => {
				if matches!(resp, ResponseType::Block { .. }) &&
					matches!(*e.get(), ResponseType::Presence { .. })
				{
					e.insert(resp);
				}
			},
		}
	}
	best
}

/// Route a [`BitswapEvent::Response`] to waiting oneshot senders.
///
/// Accumulates per-CID responses by peer, re-encodes complete batches as a
/// [`BitswapProtoMessage`], and resolves the corresponding waiter.
fn handle_inbound_response(
	pending: &mut PendingBatches,
	peer: litep2p::PeerId,
	responses: Vec<ResponseType>,
) {
	handle_inbound_response_with_limit(pending, peer, responses, MAX_RESPONSE_SIZE as usize)
}

/// Route a response to the matching pending batch, capped at `max_response_bytes`.
fn handle_inbound_response_with_limit(
	pending: &mut PendingBatches,
	peer: litep2p::PeerId,
	responses: Vec<ResponseType>,
	max_response_bytes: usize,
) {
	log::debug!(
		target: LOG_TARGET,
		"bitswap: received response from {peer:?} with {} entries",
		responses.len()
	);

	let best = select_best_response_per_cid(responses);
	let mut index = 0;
	while index < pending.len() {
		if pending[index].peer != peer {
			index += 1;
			continue;
		}

		for cid in pending[index].cids.clone() {
			if pending[index].responses.contains_key(&cid) {
				continue;
			}
			let Some(resp) = best.get(&cid) else { continue };
			pending[index].response_bytes =
				pending[index].response_bytes.saturating_add(response_retained_bytes(resp));
			pending[index].responses.insert(cid, resp.clone());
		}

		if pending[index].response_bytes > max_response_bytes {
			let batch = pending.remove(index);
			log::warn!(
				target: LOG_TARGET,
				"bitswap: response from {peer:?} exceeded pending batch byte limit: {} > {}",
				batch.response_bytes,
				max_response_bytes,
			);
			let _ = batch
				.response_tx
				.send(Err(RequestFailure::Network(OutboundFailure::ConnectionClosed)));
		} else if pending[index].cids.iter().all(|cid| pending[index].responses.contains_key(cid)) {
			let batch = pending.remove(index);
			let responses: Vec<ResponseType> =
				batch.cids.iter().filter_map(|cid| batch.responses.get(cid).cloned()).collect();
			let encoded = encode_responses_as_bitswap_message(&responses);
			let _ = batch.response_tx.send(Ok((encoded, ProtocolName::from(PROTOCOL_NAME))));
		} else {
			index += 1;
		}
	}
}

/// Return the byte size of a response that counts toward the pending batch cap.
fn response_retained_bytes(response: &ResponseType) -> usize {
	match response {
		ResponseType::Block { block, .. } => block.len(),
		ResponseType::Presence { .. } => 0,
	}
}

/// Remove pending entries older than `timeout`; send [`RequestFailure::Network`] timeout
/// failures to their waiters.
fn reap_expired_pending(pending: &mut PendingBatches, timeout: Duration, now: Instant) {
	let mut index = pending.len();
	while index > 0 {
		index -= 1;
		if now.duration_since(pending[index].inserted) >= timeout {
			let batch = pending.remove(index);
			let _ = batch.response_tx.send(Err(RequestFailure::Network(OutboundFailure::Timeout)));
			log::debug!(
				target: LOG_TARGET,
				"bitswap: expired pending batch for {} CIDs from {:?}",
				batch.cids.len(),
				batch.peer,
			);
		}
	}
}

/// Encode litep2p [`ResponseType`] values into a [`BitswapProtoMessage`] byte vector.
fn encode_responses_as_bitswap_message(responses: &[ResponseType]) -> Vec<u8> {
	use crate::bitswap::schema::bitswap::message::{
		Block as MessageBlock, BlockPresence, BlockPresenceType as ProtoPresenceType,
	};

	let mut msg = BitswapProtoMessage::default();

	for resp in responses {
		match resp {
			ResponseType::Block { cid, block } => {
				let prefix = Prefix {
					version: cid.version(),
					codec: cid.codec(),
					mh_type: cid.hash().code(),
					mh_len: cid.hash().size(),
				};
				msg.payload
					.push(MessageBlock { prefix: prefix.to_bytes(), data: block.clone() });
			},
			ResponseType::Presence { cid, presence } => {
				let r#type = match presence {
					BlockPresenceType::Have => ProtoPresenceType::Have as i32,
					BlockPresenceType::DontHave => ProtoPresenceType::DontHave as i32,
				};
				msg.block_presences.push(BlockPresence { cid: cid.to_bytes(), r#type });
			},
		}
	}

	msg.encode_to_vec()
}

#[cfg(test)]
mod tests {
	use super::*;
	use cid::multihash::Multihash as CidMultihash;

	fn make_peer() -> litep2p::PeerId {
		litep2p::PeerId::random()
	}

	fn make_cid(byte: u8) -> Cid {
		let digest = [byte; 32];
		let mh = CidMultihash::<64>::wrap(0xb220, &digest).unwrap();
		Cid::new_v1(0x55, mh)
	}

	fn pending_batch(
		peer: litep2p::PeerId,
		cids: Vec<Cid>,
		response_tx: ResponseSender,
		inserted: Instant,
	) -> PendingBatch {
		PendingBatch {
			peer,
			cids,
			responses: HashMap::new(),
			response_bytes: 0,
			response_tx,
			inserted,
		}
	}

	#[test]
	fn encode_responses_are_decodable() {
		use crate::bitswap::schema::bitswap::message::BlockPresenceType as ProtoPresenceType;
		let block_cid = make_cid(1);
		let presence_cid = make_cid(2);
		let data = b"block-data-payload".to_vec();
		let responses = vec![
			ResponseType::Block { cid: block_cid, block: data.clone() },
			ResponseType::Presence { cid: presence_cid, presence: BlockPresenceType::DontHave },
		];

		let bytes = encode_responses_as_bitswap_message(&responses);
		let msg = BitswapProtoMessage::decode(bytes.as_slice()).unwrap();

		assert_eq!(msg.payload.len(), 1);
		assert_eq!(msg.payload[0].data, data);
		assert_eq!(msg.block_presences.len(), 1);
		assert_eq!(msg.block_presences[0].r#type, ProtoPresenceType::DontHave as i32);
	}

	#[test]
	fn select_best_prefers_block_over_presence() {
		let cid = make_cid(3);
		let data = b"data".to_vec();
		let responses = vec![
			ResponseType::Presence { cid, presence: BlockPresenceType::Have },
			ResponseType::Block { cid, block: data.clone() },
		];
		let best = select_best_response_per_cid(responses);
		assert_eq!(best.len(), 1);
		match best.into_iter().next().unwrap().1 {
			ResponseType::Block { block, .. } => assert_eq!(block, data),
			_ => panic!("expected Block to win"),
		}
	}

	#[test]
	fn select_best_prefers_block_over_presence_regardless_of_order() {
		let cid = make_cid(4);
		let data = b"data-reversed".to_vec();
		let responses = vec![
			ResponseType::Block { cid, block: data.clone() },
			ResponseType::Presence { cid, presence: BlockPresenceType::Have },
		];
		let best = select_best_response_per_cid(responses);
		assert_eq!(best.len(), 1);
		match best.into_iter().next().unwrap().1 {
			ResponseType::Block { block, .. } => assert_eq!(block, data),
			_ => panic!("expected Block to win"),
		}
	}

	#[test]
	fn select_best_keeps_distinct_cids() {
		let cid_a = make_cid(5);
		let cid_b = make_cid(6);
		let responses = vec![
			ResponseType::Block { cid: cid_a, block: b"a".to_vec() },
			ResponseType::Presence { cid: cid_b, presence: BlockPresenceType::DontHave },
		];
		let best = select_best_response_per_cid(responses);
		assert_eq!(best.len(), 2);
		assert!(best.contains_key(&cid_a));
		assert!(best.contains_key(&cid_b));
	}

	#[tokio::test]
	async fn pending_batch_single_request_resolves() {
		let peer = make_peer();
		let cid = make_cid(7);
		let data = b"resolved-data".to_vec();

		let (tx, rx) = oneshot::channel();
		let mut pending = vec![pending_batch(peer, vec![cid], tx, Instant::now())];

		handle_inbound_response(
			&mut pending,
			peer,
			vec![ResponseType::Block { cid, block: data.clone() }],
		);

		let (payload, _) = rx.await.unwrap().unwrap();
		let msg = BitswapProtoMessage::decode(payload.as_slice()).unwrap();
		assert_eq!(msg.payload.len(), 1);
		assert_eq!(msg.payload[0].data, data);
		assert!(pending.is_empty());
	}

	#[tokio::test]
	async fn pending_batch_duplicate_requests_both_resolve() {
		let peer = make_peer();
		let cid = make_cid(8);
		let data = b"shared-blob".to_vec();

		let (tx_a, rx_a) = oneshot::channel();
		let (tx_b, rx_b) = oneshot::channel();
		let mut pending = vec![
			pending_batch(peer, vec![cid], tx_a, Instant::now()),
			pending_batch(peer, vec![cid], tx_b, Instant::now()),
		];

		handle_inbound_response(
			&mut pending,
			peer,
			vec![ResponseType::Block { cid, block: data.clone() }],
		);

		let a = rx_a.await.unwrap().unwrap();
		let b = rx_b.await.unwrap().unwrap();
		let msg_a = BitswapProtoMessage::decode(a.0.as_slice()).unwrap();
		let msg_b = BitswapProtoMessage::decode(b.0.as_slice()).unwrap();
		assert_eq!(msg_a.payload[0].data, data);
		assert_eq!(msg_b.payload[0].data, data);
		assert!(pending.is_empty());
	}

	#[tokio::test]
	async fn pending_batch_multi_want_waits_for_all_cids() {
		let peer = make_peer();
		let cid_a = make_cid(11);
		let cid_b = make_cid(12);
		let data_a = b"first".to_vec();
		let data_b = b"second".to_vec();

		let (tx, rx) = oneshot::channel();
		let mut pending = vec![pending_batch(peer, vec![cid_a, cid_b], tx, Instant::now())];

		handle_inbound_response(
			&mut pending,
			peer,
			vec![ResponseType::Block { cid: cid_a, block: data_a.clone() }],
		);
		assert_eq!(pending.len(), 1);

		handle_inbound_response(
			&mut pending,
			peer,
			vec![ResponseType::Block { cid: cid_b, block: data_b.clone() }],
		);

		let (payload, _) = rx.await.unwrap().unwrap();
		let msg = BitswapProtoMessage::decode(payload.as_slice()).unwrap();
		assert_eq!(msg.payload.len(), 2);
		assert_eq!(msg.payload[0].data, data_a);
		assert_eq!(msg.payload[1].data, data_b);
		assert!(pending.is_empty());
	}

	#[tokio::test]
	async fn pending_batch_fails_when_partial_responses_exceed_byte_limit() {
		let peer = make_peer();
		let cid_a = make_cid(13);
		let cid_b = make_cid(14);

		let (tx, rx) = oneshot::channel();
		let mut pending = vec![pending_batch(peer, vec![cid_a, cid_b], tx, Instant::now())];

		handle_inbound_response_with_limit(
			&mut pending,
			peer,
			vec![ResponseType::Block { cid: cid_a, block: vec![0u8; 8] }],
			4,
		);

		let result = rx.await.unwrap();
		assert!(matches!(result, Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))));
		assert!(pending.is_empty());
	}

	#[tokio::test]
	async fn pending_batch_expiry_sends_failure() {
		let peer = make_peer();
		let cid = make_cid(9);

		let (tx_stale, rx_stale) = oneshot::channel();
		let (tx_fresh, rx_fresh) = oneshot::channel();
		let past = Instant::now() - Duration::from_secs(60);
		let fresh_time = Instant::now();

		let mut pending = vec![
			pending_batch(peer, vec![cid], tx_stale, past),
			pending_batch(peer, vec![cid], tx_fresh, fresh_time),
		];

		reap_expired_pending(&mut pending, Duration::from_secs(30), Instant::now());

		let stale_result = rx_stale.await.unwrap();
		assert!(matches!(stale_result, Err(RequestFailure::Network(OutboundFailure::Timeout))));
		assert_eq!(pending.len(), 1);
		drop(rx_fresh);
	}

	#[tokio::test]
	async fn pending_batch_mismatched_peer_does_not_resolve() {
		let peer_a = make_peer();
		let peer_b = make_peer();
		let cid = make_cid(10);

		let (tx, mut rx) = oneshot::channel();
		let mut pending = vec![pending_batch(peer_a, vec![cid], tx, Instant::now())];

		handle_inbound_response(
			&mut pending,
			peer_b,
			vec![ResponseType::Block { cid, block: b"data".to_vec() }],
		);

		assert_eq!(pending.len(), 1);
		assert!(rx.try_recv().unwrap().is_none());
	}
}

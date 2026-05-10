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
//! Wraps litep2p's native [`BitswapHandle`] to provide both server-side
//! (inbound WANT handling) and client-side (outbound WANT dispatch + response
//! correlation) functionality.
//!
//! Outbound flow:
//! 1. [`Litep2pNetworkService::start_request`] decodes the WANT protobuf and forwards a
//!    [`BitswapOutboundCmd`] on the command channel.
//! 2. [`BitswapService::run`] consumes the command, records the pending `(peer, cid) ->
//!    response_tx` entry, and calls `handle.send_request`.
//! 3. When litep2p fires a [`BitswapEvent::Response`], the service correlates responses by CID,
//!    re-encodes each as a [`BitswapProtoMessage`], and resolves the oneshot senders.
//! 4. Stale entries are reaped by a periodic ticker (avoids leak since `send_request` has no
//!    delivery failure event).

use crate::{
	bitswap::{is_cid_supported, BitswapProtoMessage, Prefix, PROTOCOL_NAME},
	request_responses::RequestFailure,
	OutboundFailure, ProtocolName,
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

const LOG_TARGET: &str = "sub-libp2p::bitswap";

const CMD_CHANNEL_CAPACITY: usize = 256;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EXPIRY_TICK_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) type ResponseSender = oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>;

/// Outbound bitswap command sent from [`super::service::Litep2pNetworkService`].
pub(crate) struct BitswapOutboundCmd {
	pub(crate) peer: litep2p::PeerId,
	pub(crate) cid: Cid,
	pub(crate) response_tx: ResponseSender,
}

/// Litep2p-specific bitswap configuration returned by [`BitswapService::new`].
///
/// Carries the native litep2p [`Config`] and the sender half of the command
/// channel so that [`super::service::Litep2pNetworkService`] can forward
/// client-side bitswap requests.
pub struct LiteBitswapConfig {
	pub(crate) litep2p_config: Config,
	pub(crate) cmd_tx: mpsc::Sender<BitswapOutboundCmd>,
}

type PendingMap = HashMap<(litep2p::PeerId, Cid), Vec<(ResponseSender, Instant)>>;

pub struct BitswapService<Block: BlockT> {
	handle: BitswapHandle,
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	cmd_rx: mpsc::Receiver<BitswapOutboundCmd>,
	pending: PendingMap,
}

impl<Block: BlockT> BitswapService<Block> {
	/// Create a new bidirectional bitswap service.
	///
	/// Returns the boxed task future (to be spawned on the executor) and the
	/// [`LiteBitswapConfig`] to be passed into the litep2p config builder.
	pub fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, LiteBitswapConfig) {
		let (litep2p_config, handle) = Config::new();
		let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
		let service = Self { handle, client, cmd_rx, pending: HashMap::new() };
		let future = Box::pin(async move { service.run().await });
		let config = LiteBitswapConfig { litep2p_config, cmd_tx };
		(future, config)
	}

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
					Some(BitswapOutboundCmd { peer, cid, response_tx }) =>
						self.handle_outbound_cmd(peer, cid, response_tx).await,
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

	async fn handle_outbound_cmd(
		&mut self,
		peer: litep2p::PeerId,
		cid: Cid,
		response_tx: ResponseSender,
	) {
		log::debug!(target: LOG_TARGET, "bitswap: outbound WANT for {cid} to {peer:?}");
		self.pending.entry((peer, cid)).or_default().push((response_tx, Instant::now()));
		self.handle.send_request(peer, vec![(cid, WantType::Block)]).await;
	}
}

/// Collapse a response list into at most one entry per CID, preferring `Block`
/// over `Presence` when both arrive for the same CID.
pub(crate) fn select_best_response_per_cid(
	responses: Vec<ResponseType>,
) -> HashMap<Cid, ResponseType> {
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
/// For each (peer, cid) key in the response, re-encodes the best response as a
/// [`BitswapProtoMessage`] and delivers to all waiters. Expired / closed
/// waiters are dropped.
pub(crate) fn handle_inbound_response(
	pending: &mut PendingMap,
	peer: litep2p::PeerId,
	responses: Vec<ResponseType>,
) {
	log::debug!(
		target: LOG_TARGET,
		"bitswap: received response from {peer:?} with {} entries",
		responses.len()
	);

	let best = select_best_response_per_cid(responses);
	for (cid, resp) in best {
		let Some(waiters) = pending.remove(&(peer, cid)) else {
			log::trace!(target: LOG_TARGET, "bitswap: no waiters for {cid} from {peer:?}");
			continue;
		};

		let encoded = match encode_response_as_bitswap_message(&resp) {
			Ok(bytes) => bytes,
			Err(e) => {
				log::warn!(target: LOG_TARGET, "bitswap: failed to encode response for {cid}: {e:?}");
				for (tx, _) in waiters {
					let _ =
						tx.send(Err(RequestFailure::Network(OutboundFailure::ConnectionClosed)));
				}
				let _ = e;
				continue;
			},
		};

		for (tx, _inserted) in waiters {
			let _ = tx.send(Ok((encoded.clone(), ProtocolName::from(PROTOCOL_NAME))));
		}
	}
}

/// Remove pending entries older than `timeout`; send [`RequestFailure::Network`] timeout
/// failures to their waiters.
pub(crate) fn reap_expired_pending(pending: &mut PendingMap, timeout: Duration, now: Instant) {
	let mut drop_keys = Vec::new();
	for ((peer, cid), waiters) in pending.iter_mut() {
		let original_len = waiters.len();
		let mut i = waiters.len();
		while i > 0 {
			i -= 1;
			if now.duration_since(waiters[i].1) >= timeout {
				let (tx, _) = waiters.remove(i);
				let _ = tx.send(Err(RequestFailure::Network(OutboundFailure::Timeout)));
			}
		}
		if waiters.is_empty() {
			drop_keys.push((*peer, *cid));
		} else if waiters.len() != original_len {
			log::trace!(
				target: LOG_TARGET,
				"bitswap: reaped {} expired waiters for {cid} from {peer:?}",
				original_len - waiters.len(),
			);
		}
	}
	for key in drop_keys {
		pending.remove(&key);
		log::debug!(target: LOG_TARGET, "bitswap: expired pending entry for {key:?}");
	}
}

/// Encode a litep2p [`ResponseType`] into a [`BitswapProtoMessage`] byte vector
/// matching what [`crate::bitswap::BitswapClient::fetch`] expects to decode.
pub(crate) fn encode_response_as_bitswap_message(
	resp: &ResponseType,
) -> Result<Vec<u8>, RequestFailure> {
	use crate::bitswap::schema::bitswap::message::{
		Block as MessageBlock, BlockPresence, BlockPresenceType as ProtoPresenceType,
	};

	let mut msg = BitswapProtoMessage::default();

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

	Ok(msg.encode_to_vec())
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

	#[test]
	fn encode_block_response_is_decodable() {
		let cid = make_cid(1);
		let data = b"block-data-payload".to_vec();
		let resp = ResponseType::Block { cid, block: data.clone() };

		let bytes = encode_response_as_bitswap_message(&resp).unwrap();
		let msg = BitswapProtoMessage::decode(bytes.as_slice()).unwrap();

		assert_eq!(msg.payload.len(), 1);
		assert_eq!(msg.payload[0].data, data);
		assert!(msg.block_presences.is_empty());
	}

	#[test]
	fn encode_presence_dont_have_response() {
		use crate::bitswap::schema::bitswap::message::BlockPresenceType as ProtoPresenceType;
		let cid = make_cid(2);
		let resp = ResponseType::Presence { cid, presence: BlockPresenceType::DontHave };

		let bytes = encode_response_as_bitswap_message(&resp).unwrap();
		let msg = BitswapProtoMessage::decode(bytes.as_slice()).unwrap();

		assert_eq!(msg.block_presences.len(), 1);
		assert_eq!(msg.block_presences[0].r#type, ProtoPresenceType::DontHave as i32);
		assert!(msg.payload.is_empty());
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
	async fn pending_map_single_request_resolves() {
		let peer = make_peer();
		let cid = make_cid(7);
		let data = b"resolved-data".to_vec();

		let (tx, rx) = oneshot::channel();
		let mut pending: PendingMap = HashMap::new();
		pending.insert((peer, cid), vec![(tx, Instant::now())]);

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
	async fn pending_map_duplicate_requests_both_resolve() {
		let peer = make_peer();
		let cid = make_cid(8);
		let data = b"shared-blob".to_vec();

		let (tx_a, rx_a) = oneshot::channel();
		let (tx_b, rx_b) = oneshot::channel();
		let mut pending: PendingMap = HashMap::new();
		pending.insert((peer, cid), vec![(tx_a, Instant::now()), (tx_b, Instant::now())]);

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
	async fn pending_map_expiry_sends_failure() {
		let peer = make_peer();
		let cid = make_cid(9);

		let (tx_stale, rx_stale) = oneshot::channel();
		let (tx_fresh, mut rx_fresh) = oneshot::channel();
		let past = Instant::now() - Duration::from_secs(60);
		let fresh_time = Instant::now();

		let mut pending: PendingMap = HashMap::new();
		pending.insert((peer, cid), vec![(tx_stale, past), (tx_fresh, fresh_time)]);

		reap_expired_pending(&mut pending, Duration::from_secs(30), Instant::now());

		let stale_result = rx_stale.await.unwrap();
		assert!(matches!(stale_result, Err(RequestFailure::Network(OutboundFailure::Timeout))));
		assert_eq!(pending.get(&(peer, cid)).map(|v| v.len()), Some(1));
		assert!(rx_fresh.try_recv().is_err() || rx_fresh.try_recv().unwrap().is_none());
	}

	#[tokio::test]
	async fn pending_map_mismatched_peer_does_not_resolve() {
		let peer_a = make_peer();
		let peer_b = make_peer();
		let cid = make_cid(10);

		let (tx, mut rx) = oneshot::channel();
		let mut pending: PendingMap = HashMap::new();
		pending.insert((peer_a, cid), vec![(tx, Instant::now())]);

		handle_inbound_response(
			&mut pending,
			peer_b,
			vec![ResponseType::Block { cid, block: b"data".to_vec() }],
		);

		assert!(pending.contains_key(&(peer_a, cid)));
		assert!(rx.try_recv().unwrap().is_none());
	}
}

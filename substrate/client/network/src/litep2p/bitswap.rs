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
	bitswap::{
		is_cid_supported,
		schema::bitswap::message::{
			wantlist::WantType as ProtoWantType,
			Block as MessageBlock, BlockPresence, BlockPresenceType as ProtoPresenceType,
		},
		BitswapClient, BitswapError, BitswapProtoMessage, BitswapRequest, BitswapResponse, Cid,
		FetchOutcome, Prefix, VerificationMode, LOG_TARGET, MAX_WANTED_BLOCKS,
	},
	litep2p::bitswap_metrics::{errors, outcomes, BitswapMetrics},
	request_responses::RequestFailure,
	OutboundFailure, MAX_RESPONSE_SIZE,
};
use futures::{channel::oneshot, future::join_all, StreamExt};
use rand::seq::IteratorRandom;
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle, BlockPresenceType, Config, ResponseType, WantType,
};
use prometheus_endpoint::Registry;
use prost::Message as ProstMessage;
use sc_client_api::BlockBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	future::Future,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::mpsc;

type RequestId = u64;

/// Command channel capacity.
const CMD_CHANNEL_CAPACITY: usize = 256;
/// Timeout for pending bitswap requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Interval for reaping expired pending batches.
const EXPIRY_TICK_INTERVAL: Duration = Duration::from_secs(10);
/// Maximum number of peers to fan out an outbound request to.
const MAX_FANOUT_PEERS: usize = 3;

// pub(crate) type ResponseSender = oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>;
pub(crate) type ResponseSender = oneshot::Sender<BitswapResponse>;

/// Outbound bitswap command sent from [`super::service::Litep2pNetworkService`].
// pub(crate) struct BitswapOutboundCmd {
// 	pub(crate) peer: litep2p::PeerId,
// 	pub(crate) wants: Vec<(Cid, WantType)>,
// 	pub(crate) response_tx: ResponseSender,
// }

/// Pending outbound WANT batch.
struct PendingBatch {
	remaining: HashSet<Cid>,
	collected: HashMap<Cid, FetchOutcome>,
	response_tx: Option<ResponseSender>,
	cids: Vec<Cid>,
	response_bytes: usize,
	verification: VerificationMode,
	inserted: Instant,
}

impl PendingBatch {
	fn new(
		cids: Vec<Cid>,
		response_tx: ResponseSender,
		verification: VerificationMode,
		inserted: Instant,
	) -> Self {
		Self {
			remaining: cids.iter().copied().collect(),
			collected: HashMap::new(),
			response_tx: Some(response_tx),
			cids,
			response_bytes: 0,
			verification,
			inserted,
		}
	}

	// fn record_responses(&mut self, responses: &HashMap<Cid, ResponseType>) {
	// 	for cid in &self.cids {
	// 		if self.responses.contains_key(cid) {
	// 			continue;
	// 		}
	// 		let Some(resp) = responses.get(cid) else { continue };
	// 		self.response_bytes = self.response_bytes.saturating_add(response_retained_bytes(resp));
	// 		self.responses.insert(*cid, resp.clone());
	// 	}
	// }

	fn record_response(&mut self, cid: &Cid, resp: ResponseType) {
		self.response_bytes = self.response_bytes.saturating_add(response_retained_bytes(&resp));
		self.remaining.remove(cid);
		let outcome = match resp {
			ResponseType::Block { cid: block_cid, block } => match self.verification {
				VerificationMode::Verified =>
					if verify_block(&block_cid, &block) {
						FetchOutcome::Block(block)
					} else {
						log::warn!(
							target: LOG_TARGET,
							"bitswap: block for CID {block_cid} failed hash verification; treating as missing",
						);
						FetchOutcome::Missing
					},
				VerificationMode::Unverified => FetchOutcome::Block(block),
			},
			ResponseType::Presence { .. } => FetchOutcome::Missing,
		};
		self.collected.insert(*cid, outcome);
	}

	fn is_complete(&self) -> bool {
		self.remaining.is_empty()
	}

	fn is_over_limit(&self, max_response_bytes: usize) -> bool {
		self.response_bytes > max_response_bytes
	}

	fn is_expired(&self, timeout: Duration, now: Instant) -> bool {
		now.saturating_duration_since(self.inserted) >= timeout
	}

	// fn send_success(&mut self) {
	// 	let responses: Vec<ResponseType> =
	// 		self.cids.iter().filter_map(|cid| self.responses.get(cid).cloned()).collect();
	// 	let encoded = encode_responses_as_bitswap_message(&responses);
	// 	if let Some(response_tx) = self.response_tx.take() {
	// 		let _ = response_tx.send(Ok((encoded, ProtocolName::from(PROTOCOL_NAME))));
	// 	}
	// }

	fn send_success(&mut self) {
		if let Some(response_tx) = self.response_tx.take() {
			let _ = response_tx.send(Ok(std::mem::take(&mut self.collected)));
		}
	}

	// fn send_failure(&mut self, failure: RequestFailure) {
	// 	if let Some(response_tx) = self.response_tx.take() {
	// 		let _ = response_tx.send(Err(failure));
	// 	}
	// }

	fn send_failure(&mut self, failure: RequestFailure) {
		if let Some(response_tx) = self.response_tx.take() {
			let _ = response_tx.send(Err(BitswapError::RequestFailed(failure.to_string())));
		}
	}
}

/// Litep2p-specific bitswap configuration returned by [`BitswapService::new`].
///
/// Carries the native litep2p [`Config`] and the sender half of the command
/// channel so that [`super::service::Litep2pNetworkService`] can forward
/// client-side bitswap requests.
pub struct BitswapConfig {
	pub(crate) litep2p_config: Config,
	// pub(crate) cmd_tx: mpsc::Sender<BitswapOutboundCmd>,
	pub(crate) client: BitswapClient,
}

/// Pending outbound WANT batches, indexed by peer.
/// Use an inverted index to enable fast searching.
#[derive(Default)]
struct PendingBatches {
	// by_peer: HashMap<litep2p::PeerId, Vec<PendingBatch>>,
	by_request_id: HashMap<RequestId, PendingBatch>,
	by_cid: HashMap<Cid, Vec<RequestId>>,
}

impl PendingBatches {
	// fn insert(&mut self, peer: litep2p::PeerId, batch: PendingBatch) {
	// 	self.by_peer.entry(peer).or_default().push(batch);
	// }

	fn insert(&mut self, id: RequestId, batch: PendingBatch) {
		for cid in &batch.cids {
			self.by_cid.entry(*cid).or_default().push(id);
		}
		self.by_request_id.insert(id, batch);
	}

	fn handle_response(&mut self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
		self.handle_response_with_limit(peer, responses, MAX_RESPONSE_SIZE as usize);
	}

	// fn handle_response_with_limit(
	// 	&mut self,
	// 	peer: litep2p::PeerId,
	// 	responses: Vec<ResponseType>,
	// 	max_response_bytes: usize,
	// ) {
	// 	log::debug!(
	// 		target: LOG_TARGET,
	// 		"bitswap: received response from {peer:?} with {} entries",
	// 		responses.len()
	// 	);

	// 	let Some(peer_batches) = self.by_peer.get_mut(&peer) else { return };
	// 	let best = select_best_response_per_cid(responses);

	// 	peer_batches.retain_mut(|batch| {
	// 		batch.record_responses(&best);

	// 		if batch.is_over_limit(max_response_bytes) {
	// 			log::warn!(
	// 				target: LOG_TARGET,
	// 				"bitswap: response from {peer:?} exceeded pending batch byte limit: {} > {}",
	// 				batch.response_bytes,
	// 				max_response_bytes,
	// 			);
	// 			batch.send_failure(RequestFailure::Network(OutboundFailure::ConnectionClosed));
	// 			false
	// 		} else if batch.is_complete() {
	// 			batch.send_success();
	// 			false
	// 		} else {
	// 			true
	// 		}
	// 	});

	// 	if peer_batches.is_empty() {
	// 		self.by_peer.remove(&peer);
	// 	}
	// }

	fn handle_response_with_limit(
		&mut self,
		peer: litep2p::PeerId,
		responses: Vec<ResponseType>,
		max_response_bytes: usize,
	) {
		log::debug!(
			target: LOG_TARGET,
			"bitswap: received response from {peer:?} with {} entries",
			responses.len()
		);

		// let Some(peer_batches) = self.by_peer.get_mut(&peer) else { return };
		let best = select_best_response_per_cid(responses);

		for (_, response) in &best {
			let cid = match &response {
				ResponseType::Block { cid, .. } => *cid,
				ResponseType::Presence { cid, .. } => *cid,
			};

			let Some(request_ids) = self.by_cid.remove(&cid) else { continue };
			
			for id in request_ids {
				let Some(batch) = self.by_request_id.get_mut(&id) else { continue };
				batch.record_response(&cid, response.clone());

				let done = if batch.is_over_limit(max_response_bytes) {
					log::warn!(
						target: LOG_TARGET,
						"bitswap: response from {peer:?} exceeded pending batch byte limit: {} > {}",
						batch.response_bytes,
						max_response_bytes,
					);
					batch.send_failure(RequestFailure::Network(OutboundFailure::ConnectionClosed));
					true
				} else if batch.is_complete() {
					batch.send_success();
					true
				} else {
					false
				};

				if done {
					// Remove the batch and sweep its remaining CIDs out of the inverted index.
					// The current `cid` is already removed (line above the outer for loop);
					// only the unresolved ones need cleaning up.
					if let Some(removed) = self.by_request_id.remove(&id) {
						for remaining_cid in &removed.cids {
							if remaining_cid == &cid {
								continue;
							}
							if let Some(ids) = self.by_cid.get_mut(remaining_cid) {
								ids.retain(|rid| *rid != id);
								if ids.is_empty() {
									self.by_cid.remove(remaining_cid);
								}
							}
						}
					}
				}
			}
		}
	}

	// fn expire(&mut self, timeout: Duration, now: Instant) {
	// 	self.by_peer.retain(|peer, peer_batches| {
	// 		peer_batches.retain_mut(|batch| {
	// 			if batch.is_expired(timeout, now) {
	// 				log::debug!(
	// 					target: LOG_TARGET,
	// 					"bitswap: expired pending batch for {} CIDs from {:?}",
	// 					batch.cids.len(),
	// 					peer,
	// 				);
	// 				batch.send_failure(RequestFailure::Network(OutboundFailure::Timeout));
	// 				false
	// 			} else {
	// 				true
	// 			}
	// 		});

	// 		!peer_batches.is_empty()
	// 	});
	// }

	fn expire(&mut self, timeout: Duration, now: Instant) {
		let expired_ids: Vec<RequestId> = self
			.by_request_id
			.iter()
			.filter(|(_, batch)| batch.is_expired(timeout, now))
			.map(|(id, _)| *id)
			.collect();

		for id in expired_ids {
			let Some(mut batch) = self.by_request_id.remove(&id) else { continue };

			log::debug!(
				target: LOG_TARGET,
				"bitswap: expired pending batch for {} CIDs (request {id:?})",
				batch.cids.len(),
			);

			// Clean up inverted index for every CID in the expired batch
			for cid in &batch.cids {
				if let Some(ids) = self.by_cid.get_mut(cid) {
					ids.retain(|rid| *rid != id);
					if ids.is_empty() {
						self.by_cid.remove(cid);
					}
				}
			}

			batch.send_failure(RequestFailure::Network(OutboundFailure::Timeout));
		}
	}

	#[cfg(test)]
	fn is_empty(&self) -> bool {
		self.by_request_id.is_empty()
	}

	#[cfg(test)]
	fn len(&self) -> usize {
		self.by_request_id.len()
	}

	#[cfg(test)]
	fn contains_key(&self, id: &RequestId) -> bool {
		self.by_request_id.contains_key(id)
	}
}

/// Bidirectional bitswap service for litep2p.
pub(crate) struct BitswapService<Block: BlockT> {
	handle: BitswapHandle,
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	// cmd_rx: mpsc::Receiver<BitswapOutboundCmd>,
	request_rx: mpsc::Receiver<BitswapRequest>,
	next_request_id: RequestId,
	pending: PendingBatches,
	metrics: BitswapMetrics,
	known_peers: HashSet<litep2p::PeerId>,
}

impl<Block: BlockT> BitswapService<Block> {
	/// Create a new bidirectional bitswap service.
	///
	/// Returns the boxed task future (to be spawned on the executor) and the
	/// [`BitswapConfig`] to be passed into the litep2p config builder.
	///
	/// If `metrics_registry` is `Some`, Prometheus metrics are registered with
	/// it. A registration failure is logged and falls back to disabled metrics
	/// (the service still runs).
	pub(crate) fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
		metrics_registry: Option<&Registry>,
		known_peers: Vec<litep2p::PeerId>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, BitswapConfig) {
		let metrics = BitswapMetrics::new(metrics_registry).unwrap_or_else(|err| {
			log::debug!(target: LOG_TARGET, "failed to register bitswap metrics: {err}");
			BitswapMetrics::new(None).expect("registering with None registry never fails; qed")
		});
		let (litep2p_config, handle) = Config::new();
		let (request_tx, request_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
		let service = Self { 
			handle,
			client,
			request_rx,
			next_request_id: 0,
			pending: PendingBatches::default(),
			metrics,
			known_peers: known_peers.into_iter().collect(),
		};
		let future = Box::pin(async move { service.run().await });
		let client = BitswapClient { request_tx };
		let config = BitswapConfig { litep2p_config, client };
		(future, config)
	}

	fn next_id(&mut self) -> RequestId {
		let id = self.next_request_id;
		self.next_request_id += 1;
		id
	}

	/// Run the bitswap event loop.
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting bidirectional bitswap service");
		let mut expiry_ticker = tokio::time::interval(EXPIRY_TICK_INTERVAL);
		expiry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		expiry_ticker.tick().await;

		loop {
			tokio::select! {
				event = self.handle.next() => match event {
					Some(BitswapEvent::Request { peer, cids }) => {
						self.known_peers.insert(peer);
						self.handle_inbound_request(peer, cids).await
					}
					Some(BitswapEvent::Response { peer, responses }) => {
						self.known_peers.insert(peer);
						self.pending.handle_response(peer, responses)
					}
					None => {
						log::debug!(target: LOG_TARGET, "bitswap handle stream ended");
						return;
					},
				},
				cmd = self.request_rx.recv() => match cmd {
					Some(BitswapRequest { cids, response_tx, verification }) =>
						self.handle_outbound_cmd(cids, response_tx, verification).await,
					None => {
						log::debug!(target: LOG_TARGET, "bitswap cmd channel closed");
						return;
					},
				},
				_ = expiry_ticker.tick() => {
					self.pending.expire(REQUEST_TIMEOUT, Instant::now());
				},
			}
		}
	}

	/// Handle an inbound bitswap WANT request from `peer`.
	async fn handle_inbound_request(&mut self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		let started = Instant::now();
		let want_count = cids.len();
		if inbound_wantlist_exceeds_limit(want_count) {
			self.metrics.record_error(errors::TOO_MANY_ENTRIES);
			log::trace!(target: LOG_TARGET, "bitswap: ignored inbound request with {want_count} entries");
			return;
		}

		log::debug!(target: LOG_TARGET, "bitswap: handle inbound request from {peer:?} for {cids:?}");

		let metrics = &self.metrics;
		let response: Vec<ResponseType> = cids
			.into_iter()
			.filter(|(cid, _)| {
				let supported = is_cid_supported(cid);
				if !supported {
					metrics.record_entry(outcomes::UNSUPPORTED_CID);
				}
				supported
			})
			.map(|(cid, want_type)| {
				let hash = H256::from_slice(&cid.hash().digest()[0..32]);
				let transaction = match self.client.indexed_transaction(hash) {
					Ok(ex) => ex,
					Err(error) => {
						metrics.record_error(errors::CLIENT);
						log::error!(target: LOG_TARGET, "error retrieving transaction {hash}: {error}");
						None
					},
				};
				let response = match transaction {
					Some(transaction) => match want_type {
						WantType::Block => ResponseType::Block { cid, block: transaction },
						_ => ResponseType::Presence { cid, presence: BlockPresenceType::Have },
					},
					None => ResponseType::Presence { cid, presence: BlockPresenceType::DontHave },
				};
				metrics.record_response(&response);
				response
			})
			.collect();

		// note: we assume the duplicate encode (litep2p re-serialises internally inside
		// `send_response`) is cheap.
		let response_bytes = encode_responses_as_bitswap_message(&response).len();
		self.metrics.add_response_bytes(response_bytes as u64);

		self.handle.send_response(peer, response).await;
		self.metrics.record_duration(started.elapsed());
	}

	/// Handle an outbound bitswap command from the network service.
	async fn handle_outbound_cmd(
		&mut self,
		cids: Vec<(Cid, ProtoWantType)>,
		response_tx: ResponseSender,
		verification: VerificationMode,
	) {
		log::debug!(
			target: LOG_TARGET,
			"bitswap: outbound WANT for {} CIDs",
			cids.len(),
		);

		let cid_keys: Vec<Cid> = cids.iter().map(|(cid, _)| *cid).collect();

		// Convert Substrate proto WantType to litep2p WantType at the transport boundary.
		let wants: Vec<(Cid, WantType)> = cids
			.into_iter()
			.map(|(cid, wt)| (cid, match wt {
				ProtoWantType::Block => WantType::Block,
				ProtoWantType::Have  => WantType::Have,
			}))
			.collect();

		let id = self.next_id();
		self.pending.insert(id, PendingBatch::new(cid_keys, response_tx, verification, Instant::now()));


		// Pick three pairs in random and send concurrent requests to all of them
		let peers: Vec<litep2p::PeerId> = self.known_peers
			.iter()
			.copied()
			.choose_multiple(&mut rand::thread_rng(), MAX_FANOUT_PEERS);

		log::debug!(
			target: LOG_TARGET,
			"bitswap: fanning out WANT to {}/{} peers",
			peers.len(),
			self.known_peers.len(),
		);

		join_all(peers.iter().map(|peer| self.handle.send_request(*peer, wants.clone()))).await;
	}
}

fn inbound_wantlist_exceeds_limit(len: usize) -> bool {
	len > MAX_WANTED_BLOCKS
}

/// Re-hash `block` bytes and confirm the digest matches the multihash embedded in `cid`.
///
/// Uses `sp_crypto_hashing` for the three supported codes (BLAKE2b-256, SHA2-256, Keccak-256).
/// Returns `false` for any unsupported multihash code, treating the block as unverifiable.
fn verify_block(cid: &Cid, block: &[u8]) -> bool {
	use crate::bitswap::{
		BLAKE2B_256_MULTIHASH_CODE, KECCAK_256_MULTIHASH_CODE, SHA2_256_MULTIHASH_CODE,
	};

	let expected = cid.hash().digest();
	let computed: Option<[u8; 32]> = match cid.hash().code() {
		BLAKE2B_256_MULTIHASH_CODE => Some(sp_crypto_hashing::blake2_256(block)),
		SHA2_256_MULTIHASH_CODE => Some(sp_crypto_hashing::sha2_256(block)),
		KECCAK_256_MULTIHASH_CODE => Some(sp_crypto_hashing::keccak_256(block)),
		_ => None,
	};

	computed.map_or(false, |digest| digest.as_ref() == expected)
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

/// Return the byte size of a response that counts toward the pending batch cap.
fn response_retained_bytes(response: &ResponseType) -> usize {
	match response {
		ResponseType::Block { block, .. } => block.len(),
		ResponseType::Presence { .. } => 0,
	}
}

/// Encode litep2p [`ResponseType`] values into a [`BitswapProtoMessage`] byte vector.
fn encode_responses_as_bitswap_message(responses: &[ResponseType]) -> Vec<u8> {
	let mut msg = BitswapProtoMessage::default();

	for resp in responses {
		match resp {
			ResponseType::Block { cid, block } => {
				let prefix: Prefix = cid.into();
				msg.payload
					.push(MessageBlock { prefix: prefix.to_bytes(), data: block.clone() });
			},
			ResponseType::Presence { cid, presence } => {
				msg.block_presences.push(BlockPresence {
					cid: cid.to_bytes(),
					r#type: match presence {
						BlockPresenceType::Have => ProtoPresenceType::Have as i32,
						BlockPresenceType::DontHave => ProtoPresenceType::DontHave as i32,
					},
				});
			},
		}
	}

	msg.encode_to_vec()
}

#[cfg(test)]
mod tests {
	use super::*;
	use cid::multihash::Multihash as CidMultihash;
	use prometheus_endpoint::Registry;
	use substrate_test_runtime_client;

	fn make_peer() -> litep2p::PeerId {
		litep2p::PeerId::random()
	}

	fn make_cid(byte: u8) -> Cid {
		let digest = [byte; 32];
		let mh = CidMultihash::<64>::wrap(0xb220, &digest).unwrap();
		Cid::new_v1(0x55, mh)
	}

	#[test]
	fn inbound_wantlist_limit_rejects_only_over_cap_requests() {
		assert!(!inbound_wantlist_exceeds_limit(MAX_WANTED_BLOCKS));
		assert!(inbound_wantlist_exceeds_limit(MAX_WANTED_BLOCKS + 1));
	}

	#[test]
	fn bitswap_service_constructs_without_registry() {
		let client = Arc::new(substrate_test_runtime_client::new());
		let (_future, _config) = BitswapService::new(client, None);
	}

	#[test]
	fn bitswap_service_constructs_with_registry() {
		let registry = Registry::new();
		let client = Arc::new(substrate_test_runtime_client::new());
		let (_future, _config) = BitswapService::new(client, Some(&registry));

		// Sanity check: registering the same metric names a second time on the same
		// registry must fail — proves the first registration actually went through.
		let second = crate::litep2p::bitswap_metrics::BitswapMetrics::new(Some(&registry));
		assert!(second.is_err(), "double registration should fail");
	}

	fn pending_batch(
		cids: Vec<Cid>,
		response_tx: ResponseSender,
		inserted: Instant,
	) -> PendingBatch {
		PendingBatch::new(cids, response_tx, inserted)
	}

	#[test]
	fn encode_responses_are_decodable() {
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
		let mut pending = PendingBatches::default();
		pending.insert(peer, pending_batch(vec![cid], tx, Instant::now()));

		pending.handle_response(peer, vec![ResponseType::Block { cid, block: data.clone() }]);

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
		let mut pending = PendingBatches::default();
		pending.insert(peer, pending_batch(vec![cid], tx_a, Instant::now()));
		pending.insert(peer, pending_batch(vec![cid], tx_b, Instant::now()));

		pending.handle_response(peer, vec![ResponseType::Block { cid, block: data.clone() }]);

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
		let mut pending = PendingBatches::default();
		pending.insert(peer, pending_batch(vec![cid_a, cid_b], tx, Instant::now()));

		pending
			.handle_response(peer, vec![ResponseType::Block { cid: cid_a, block: data_a.clone() }]);
		assert_eq!(pending.len(), 1);

		pending
			.handle_response(peer, vec![ResponseType::Block { cid: cid_b, block: data_b.clone() }]);

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
		let mut pending = PendingBatches::default();
		pending.insert(peer, pending_batch(vec![cid_a, cid_b], tx, Instant::now()));

		pending.handle_response_with_limit(
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

		let mut pending = PendingBatches::default();
		pending.insert(peer, pending_batch(vec![cid], tx_stale, past));
		pending.insert(peer, pending_batch(vec![cid], tx_fresh, fresh_time));

		pending.expire(Duration::from_secs(30), Instant::now());

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
		let mut pending = PendingBatches::default();
		pending.insert(peer_a, pending_batch(vec![cid], tx, Instant::now()));

		pending.handle_response(peer_b, vec![ResponseType::Block { cid, block: b"data".to_vec() }]);

		assert_eq!(pending.len(), 1);
		assert!(rx.try_recv().unwrap().is_none());
	}

	#[tokio::test]
	async fn pending_batch_response_from_one_peer_does_not_affect_other_peer() {
		let peer_a = make_peer();
		let peer_b = make_peer();
		let cid_a = make_cid(20);
		let cid_b = make_cid(21);
		let data_b = b"peer-b-data".to_vec();

		let (tx_a, mut rx_a) = oneshot::channel();
		let (tx_b, rx_b) = oneshot::channel();
		let mut pending = PendingBatches::default();
		pending.insert(peer_a, pending_batch(vec![cid_a], tx_a, Instant::now()));
		pending.insert(peer_b, pending_batch(vec![cid_b], tx_b, Instant::now()));

		pending.handle_response(
			peer_b,
			vec![ResponseType::Block { cid: cid_b, block: data_b.clone() }],
		);

		let (payload, _) = rx_b.await.unwrap().unwrap();
		let msg = BitswapProtoMessage::decode(payload.as_slice()).unwrap();
		assert_eq!(msg.payload.len(), 1);
		assert_eq!(msg.payload[0].data, data_b);

		assert!(rx_a.try_recv().unwrap().is_none());
		assert_eq!(pending.len(), 1);
		assert!(pending.contains_key(&peer_a));
	}
}

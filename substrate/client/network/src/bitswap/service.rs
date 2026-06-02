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

//! Bitswap service actor.
//!
//! Owns the litep2p [`litep2p::protocol::libp2p::bitswap::BitswapHandle`] and drives both
//! the inbound (serve indexed-transaction blocks to peers) and outbound (fetch CIDs from
//! peers on behalf of [`BitswapHandle`] callers) Bitswap flows.
//!
//! The actor runs a single [`tokio::select!`] loop with six arms; see [`BitswapService::run`].

use super::{
	is_cid_supported, BitswapCommand, BitswapHandle, BitswapServiceConfig, BitswapWiring, Cid,
	FetchItem, FetchOutcome, PeerEvent, Prefix, BLAKE2B_256_MULTIHASH_CODE,
	KECCAK_256_MULTIHASH_CODE, LOG_TARGET, MAX_WANTED_BLOCKS, SHA2_256_MULTIHASH_CODE,
};
use crate::bitswap::handle::BitswapError;

use async_trait::async_trait;
use cid::multihash::Multihash as CidMultihash;
use futures::StreamExt;
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle as LitepBitswapHandle, BlockPresenceType, Config as LitepConfig,
	ResponseType, WantType,
};
use sc_client_api::BlockBackend;
use slotmap::{new_key_type, SlotMap};
use smallvec::SmallVec;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	future::Future,
	pin::Pin,
	sync::Arc,
	time::Duration,
};
use tokio::{
	sync::{mpsc, Semaphore},
	time::Instant,
};
use tokio_util::time::{delay_queue, DelayQueue};

#[async_trait]
pub(crate) trait BitswapTransport: Send {
	async fn next_event(&mut self) -> Option<BitswapEvent>;
	async fn send_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>);
	async fn send_response(&self, peer: litep2p::PeerId, responses: Vec<ResponseType>);
}

#[async_trait]
impl BitswapTransport for LitepBitswapHandle {
	async fn next_event(&mut self) -> Option<BitswapEvent> {
		StreamExt::next(self).await
	}

	async fn send_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		LitepBitswapHandle::send_request(self, peer, cids).await
	}

	async fn send_response(&self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
		LitepBitswapHandle::send_response(self, peer, responses).await
	}
}

const MAX_OUTSTANDING_CIDS: usize = 1024;
const MAX_WAITERS_PER_CID: usize = 64;
const MAX_CONCURRENT_INBOUND_LOOKUPS: usize = 8;
const CMD_CHANNEL_CAPACITY: usize = 256;
const PEER_EVENT_CHANNEL_CAPACITY: usize = 64;
const LOOKUP_CHANNEL_CAPACITY: usize = 64;
const PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
const PEER_FANOUT_CAP: usize = 1;
const PEER_TIMEOUT_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

new_key_type! { struct WaiterId; }

// Per-CID network scheduling state, deduplicated across overlapping waiters.
struct CidState {
	tried_peers: HashSet<litep2p::PeerId>,
	in_flight_peers: HashMap<litep2p::PeerId, Instant>,
	waiters: SmallVec<[WaiterId; 2]>,
}

impl CidState {
	fn new() -> Self {
		Self {
			tried_peers: HashSet::new(),
			in_flight_peers: HashMap::new(),
			waiters: SmallVec::new(),
		}
	}

	fn has_waiters(&self) -> bool {
		!self.waiters.is_empty()
	}

	fn is_idle(&self) -> bool {
		self.waiters.is_empty() && self.in_flight_peers.is_empty()
	}
}

struct Waiter {
	cids_remaining: HashSet<Cid>,
	sink: mpsc::Sender<FetchItem>,
	delay_key: delay_queue::Key,
}

pub(crate) struct BitswapService<B: BlockT> {
	handle: Box<dyn BitswapTransport>,
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	config: BitswapServiceConfig,

	cmd_rx: mpsc::Receiver<BitswapCommand>,
	peer_event_rx: mpsc::Receiver<PeerEvent>,
	lookup_tx: mpsc::Sender<(litep2p::PeerId, Vec<ResponseType>)>,
	lookup_rx: mpsc::Receiver<(litep2p::PeerId, Vec<ResponseType>)>,
	lookup_semaphore: Arc<Semaphore>,

	waiter_deadlines: DelayQueue<WaiterId>,
	connected_peers: HashSet<litep2p::PeerId>,
	wants: HashMap<Cid, CidState>,
	waiters: SlotMap<WaiterId, Waiter>,
}

/// Build, wire and return the Bitswap service.
///
/// The returned future MUST be spawned on the runtime; the [`BitswapWiring`] MUST be passed
/// to the litep2p network backend at construction (it carries the litep2p protocol config
/// and the user-facing handle).
pub fn start<B: BlockT>(
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	config: BitswapServiceConfig,
) -> (Pin<Box<dyn Future<Output = ()> + Send>>, BitswapWiring) {
	let (litep2p_config, litep2p_handle) = LitepConfig::new();
	let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
	let (peer_event_tx, peer_event_rx) = mpsc::channel(PEER_EVENT_CHANNEL_CAPACITY);
	let (lookup_tx, lookup_rx) = mpsc::channel(LOOKUP_CHANNEL_CAPACITY);

	let user_handle = BitswapHandle::new(cmd_tx);

	let service = BitswapService {
		handle: Box::new(litep2p_handle),
		client,
		config,
		cmd_rx,
		peer_event_rx,
		lookup_tx,
		lookup_rx,
		lookup_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INBOUND_LOOKUPS)),
		waiter_deadlines: DelayQueue::new(),
		connected_peers: HashSet::new(),
		wants: HashMap::new(),
		waiters: SlotMap::with_key(),
	};

	let future = Box::pin(async move { service.run().await });
	let wiring = BitswapWiring { litep2p_config, user_handle: user_handle.clone(), peer_event_tx };

	(future, wiring)
}

impl<B: BlockT> BitswapService<B> {
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "BitswapService starting");
		let mut peer_timeout_ticker = tokio::time::interval(PEER_TIMEOUT_SWEEP_INTERVAL);
		peer_timeout_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		peer_timeout_ticker.tick().await;

		loop {
			tokio::select! {
				event = self.handle.next_event() => match event {
					Some(BitswapEvent::Request { peer, cids }) =>
						self.on_inbound_request(peer, cids),
					Some(BitswapEvent::Response { peer, responses }) =>
						self.on_inbound_response(peer, responses).await,
					None => {
						log::debug!(target: LOG_TARGET, "litep2p bitswap stream ended; shutting down");
						self.shutdown_waiters();
						return;
					},
				},

				cmd = self.cmd_rx.recv() => match cmd {
					Some(BitswapCommand::RequestStream { cids, sink }) =>
						self.on_request_stream(cids, sink).await,
					None => {
						log::debug!(target: LOG_TARGET, "command channel closed; shutting down");
						self.shutdown_waiters();
						return;
					},
				},

				peer_ev = self.peer_event_rx.recv() => match peer_ev {
					Some(PeerEvent::Snapshot { peers }) => self.on_peer_snapshot(peers).await,
					Some(PeerEvent::Connected { peer }) => self.on_peer_connected(peer).await,
					Some(PeerEvent::Disconnected { peer }) => self.on_peer_disconnected(peer).await,
					None => {
						log::debug!(target: LOG_TARGET, "peer event channel closed; shutting down");
						self.shutdown_waiters();
						return;
					},
				},

				maybe_expired = self.waiter_deadlines.next(), if !self.waiter_deadlines.is_empty() => {
					if let Some(expired) = maybe_expired {
						self.on_waiter_expired(expired.into_inner());
					}
				},

				Some((peer, responses)) = self.lookup_rx.recv() => {
					self.handle.send_response(peer, responses).await;
				},

				_ = peer_timeout_ticker.tick() => {
					self.sweep_per_peer_timeouts().await;
				},
			}
		}
	}

	async fn on_request_stream(&mut self, cids: Vec<Cid>, sink: mpsc::Sender<FetchItem>) {
		// New CIDs are CIDs not already in `wants`. We re-check the overall outstanding-CIDs
		// budget against the *new* additions, otherwise overlapping waiters would be charged
		// twice for the same CID.
		let new_cid_count = cids.iter().filter(|cid| !self.wants.contains_key(cid)).count();
		if self.wants.len() + new_cid_count > MAX_OUTSTANDING_CIDS {
			let _ = sink.try_send(Err(BitswapError::Overloaded));
			return;
		}

		for cid in &cids {
			let waiters_for_cid = self.wants.get(cid).map_or(0, |cs| cs.waiters.len());
			if waiters_for_cid >= MAX_WAITERS_PER_CID {
				let _ = sink.try_send(Err(BitswapError::Overloaded));
				return;
			}
		}

		let deadline = Instant::now() + self.config.request_timeout;
		let cids_remaining: HashSet<Cid> = cids.iter().copied().collect();

		let waiter_id = self.waiters.insert_with_key(|id| Waiter {
			cids_remaining,
			sink,
			delay_key: self.waiter_deadlines.insert_at(id, deadline),
		});

		for cid in &cids {
			let cid_state = self.wants.entry(*cid).or_insert_with(CidState::new);
			cid_state.waiters.push(waiter_id);
		}

		for cid in cids {
			self.top_up_in_flight(cid).await;
		}
	}

	async fn top_up_in_flight(&mut self, cid: Cid) {
		let Some(cid_state) = self.wants.get_mut(&cid) else { return };
		if !cid_state.has_waiters() {
			return;
		}
		if cid_state.in_flight_peers.len() >= PEER_FANOUT_CAP {
			return;
		}

		let Some(peer) = self
			.connected_peers
			.iter()
			.find(|p| {
				!cid_state.tried_peers.contains(p) && !cid_state.in_flight_peers.contains_key(p)
			})
			.copied()
		else {
			return;
		};

		cid_state.in_flight_peers.insert(peer, Instant::now() + PER_PEER_TIMEOUT);

		log::trace!(target: LOG_TARGET, "WANT-BLOCK {cid} -> {peer:?}");
		self.handle.send_request(peer, vec![(cid, WantType::Block)]).await;
	}

	async fn on_inbound_response(&mut self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
		let mut cids_to_top_up: HashSet<Cid> = HashSet::new();

		for response in responses {
			match response {
				ResponseType::Block { cid: claimed_cid, block } => {
					self.mark_peer_done_for_cid(peer, claimed_cid);

					let recomputed = match recompute_cid(&claimed_cid, &block) {
						Ok(c) => c,
						Err(e) => {
							log::debug!(
								target: LOG_TARGET,
								"{peer:?} sent block for {claimed_cid} that failed prefix decode: {e}",
							);
							cids_to_top_up.insert(claimed_cid);
							continue;
						},
					};

					if !self.wants.contains_key(&recomputed) {
						log::debug!(
							target: LOG_TARGET,
							"{peer:?} sent unsolicited or unwanted block for {recomputed}",
						);
						continue;
					}

					self.deliver_block(recomputed, block);
				},
				ResponseType::Presence { cid, presence } => {
					self.mark_peer_done_for_cid(peer, cid);
					match presence {
						BlockPresenceType::DontHave => {
							log::trace!(target: LOG_TARGET, "{peer:?} DONT_HAVE {cid}");
							cids_to_top_up.insert(cid);
						},
						BlockPresenceType::Have => {
							log::trace!(target: LOG_TARGET, "{peer:?} HAVE {cid}");
							cids_to_top_up.insert(cid);
						},
					}
				},
			}
		}

		for cid in cids_to_top_up {
			if self.wants.contains_key(&cid) {
				self.top_up_in_flight(cid).await;
			}
		}
	}

	fn mark_peer_done_for_cid(&mut self, peer: litep2p::PeerId, cid: Cid) {
		if let Some(cid_state) = self.wants.get_mut(&cid) {
			cid_state.in_flight_peers.remove(&peer);
			cid_state.tried_peers.insert(peer);
		}
	}

	fn deliver_block(&mut self, cid: Cid, bytes: Vec<u8>) {
		let Some(cid_state) = self.wants.remove(&cid) else { return };
		let bytes = Arc::new(bytes);

		for waiter_id in cid_state.waiters {
			let Some(waiter) = self.waiters.get_mut(waiter_id) else { continue };
			if !waiter.cids_remaining.remove(&cid) {
				continue;
			}
			let payload = Vec::clone(&bytes);
			if waiter.sink.try_send(Ok((cid, FetchOutcome::Block(payload)))).is_err() {
				log::trace!(target: LOG_TARGET, "waiter sink full/closed for {cid}");
				self.drop_waiter(waiter_id);
				continue;
			}
			if waiter.cids_remaining.is_empty() {
				self.drop_waiter(waiter_id);
			}
		}
	}

	fn drop_waiter(&mut self, id: WaiterId) {
		let Some(waiter) = self.waiters.remove(id) else { return };
		self.waiter_deadlines.remove(&waiter.delay_key);

		for cid in &waiter.cids_remaining {
			if let Some(cid_state) = self.wants.get_mut(cid) {
				cid_state.waiters.retain(|w| *w != id);
				if cid_state.is_idle() {
					self.wants.remove(cid);
				}
			}
		}
	}

	fn on_waiter_expired(&mut self, id: WaiterId) {
		let Some(mut waiter) = self.waiters.remove(id) else { return };
		let remaining: Vec<Cid> = waiter.cids_remaining.drain().collect();
		for cid in &remaining {
			let _ = waiter.sink.try_send(Ok((*cid, FetchOutcome::Missing)));
			if let Some(cid_state) = self.wants.get_mut(cid) {
				cid_state.waiters.retain(|w| *w != id);
				if cid_state.is_idle() {
					self.wants.remove(cid);
				}
			}
		}
		log::trace!(
			target: LOG_TARGET,
			"waiter {id:?} expired; emitted Missing for {} CIDs",
			remaining.len(),
		);
	}

	async fn on_peer_snapshot(&mut self, peers: Vec<litep2p::PeerId>) {
		self.connected_peers = peers.into_iter().collect();
		log::debug!(
			target: LOG_TARGET,
			"snapshot: {} connected peers at startup",
			self.connected_peers.len(),
		);
		let cids: Vec<Cid> = self.wants.keys().copied().collect();
		for cid in cids {
			self.top_up_in_flight(cid).await;
		}
	}

	async fn on_peer_connected(&mut self, peer: litep2p::PeerId) {
		self.connected_peers.insert(peer);
		let cids: Vec<Cid> = self.wants.keys().copied().collect();
		for cid in cids {
			self.top_up_in_flight(cid).await;
		}
	}

	async fn on_peer_disconnected(&mut self, peer: litep2p::PeerId) {
		self.connected_peers.remove(&peer);
		let cids_to_top_up: Vec<Cid> = self
			.wants
			.iter_mut()
			.filter_map(|(cid, cs)| cs.in_flight_peers.remove(&peer).map(|_| *cid))
			.collect();
		for cid in cids_to_top_up {
			self.top_up_in_flight(cid).await;
		}
	}

	async fn sweep_per_peer_timeouts(&mut self) {
		let now = Instant::now();
		let mut timed_out: Vec<(Cid, litep2p::PeerId)> = Vec::new();
		for (cid, cid_state) in self.wants.iter_mut() {
			cid_state.in_flight_peers.retain(|peer, deadline| {
				if *deadline <= now {
					timed_out.push((*cid, *peer));
					false
				} else {
					true
				}
			});
		}
		for (cid, peer) in &timed_out {
			if let Some(cid_state) = self.wants.get_mut(cid) {
				cid_state.tried_peers.insert(*peer);
			}
		}
		for (cid, _) in timed_out {
			if self.wants.contains_key(&cid) {
				self.top_up_in_flight(cid).await;
			}
		}
	}

	fn on_inbound_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		if cids.len() > MAX_WANTED_BLOCKS {
			log::trace!(
				target: LOG_TARGET,
				"ignored inbound wantlist from {peer:?} with {} entries (cap {MAX_WANTED_BLOCKS})",
				cids.len(),
			);
			return;
		}
		let Ok(permit) = self.lookup_semaphore.clone().try_acquire_owned() else {
			log::trace!(
				target: LOG_TARGET,
				"inbound serving pool saturated; dropping wantlist from {peer:?}",
			);
			return;
		};
		let client = self.client.clone();
		let lookup_tx = self.lookup_tx.clone();
		tokio::task::spawn_blocking(move || {
			let _permit = permit;
			let responses = serve_inbound(&*client, cids);
			let _ = lookup_tx.try_send((peer, responses));
		});
	}

	fn shutdown_waiters(&mut self) {
		let ids: Vec<WaiterId> = self.waiters.keys().collect();
		for id in ids {
			if let Some(waiter) = self.waiters.remove(id) {
				let _ = waiter.sink.try_send(Err(BitswapError::ServiceClosed));
				self.waiter_deadlines.remove(&waiter.delay_key);
			}
		}
		self.wants.clear();
	}
}

fn recompute_cid(reference_cid: &Cid, data: &[u8]) -> Result<Cid, String> {
	let prefix: Prefix = reference_cid.into();
	let digest = hash_for_multihash_code(prefix.mh_type, data)
		.ok_or_else(|| format!("unsupported multihash code {}", prefix.mh_type))?;
	let mh = CidMultihash::<64>::wrap(prefix.mh_type, &digest)
		.map_err(|e| format!("multihash wrap failed: {e}"))?;
	Ok(Cid::new_v1(prefix.codec, mh))
}

fn hash_for_multihash_code(multihash_code: u64, data: &[u8]) -> Option<[u8; 32]> {
	match multihash_code {
		BLAKE2B_256_MULTIHASH_CODE => Some(sp_crypto_hashing::blake2_256(data)),
		SHA2_256_MULTIHASH_CODE => Some(sp_crypto_hashing::sha2_256(data)),
		KECCAK_256_MULTIHASH_CODE => Some(sp_crypto_hashing::keccak_256(data)),
		_ => None,
	}
}

fn serve_inbound<B: BlockT>(
	client: &(dyn BlockBackend<B> + Send + Sync),
	cids: Vec<(Cid, WantType)>,
) -> Vec<ResponseType> {
	cids.into_iter()
		.filter(|(cid, _)| is_cid_supported(cid))
		.map(|(cid, want_type)| {
			let hash = H256::from_slice(&cid.hash().digest()[0..32]);
			let transaction = match client.indexed_transaction(hash) {
				Ok(t) => t,
				Err(e) => {
					log::error!(target: LOG_TARGET, "indexed_transaction({hash}) failed: {e}");
					None
				},
			};
			match (transaction, want_type) {
				(Some(transaction), WantType::Block) => {
					ResponseType::Block { cid, block: transaction }
				},
				(Some(_), _) => ResponseType::Presence { cid, presence: BlockPresenceType::Have },
				(None, _) => ResponseType::Presence { cid, presence: BlockPresenceType::DontHave },
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bitswap::RAW_CODEC;
	use sc_block_builder::BlockBuilderBuilder;
	use sp_consensus::BlockOrigin;
	use sp_runtime::codec::Encode;
	use substrate_test_runtime::ExtrinsicBuilder;
	use substrate_test_runtime_client::{prelude::*, TestClientBuilder};
	use tokio::{
		sync::Mutex as AsyncMutex,
		time::{sleep, timeout},
	};

	struct MockTransport {
		inbound: AsyncMutex<mpsc::Receiver<BitswapEvent>>,
		outbound_req_tx: mpsc::Sender<(litep2p::PeerId, Vec<(Cid, WantType)>)>,
		outbound_resp_tx: mpsc::Sender<(litep2p::PeerId, Vec<ResponseType>)>,
	}

	#[async_trait]
	impl BitswapTransport for MockTransport {
		async fn next_event(&mut self) -> Option<BitswapEvent> {
			self.inbound.get_mut().recv().await
		}

		async fn send_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
			let _ = self.outbound_req_tx.send((peer, cids)).await;
		}

		async fn send_response(&self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
			let _ = self.outbound_resp_tx.send((peer, responses)).await;
		}
	}

	struct TestRig {
		user_handle: BitswapHandle,
		peer_event_tx: mpsc::Sender<PeerEvent>,
		inbound_tx: mpsc::Sender<BitswapEvent>,
		outbound_req_rx: mpsc::Receiver<(litep2p::PeerId, Vec<(Cid, WantType)>)>,
		outbound_resp_rx: mpsc::Receiver<(litep2p::PeerId, Vec<ResponseType>)>,
		_handle: tokio::task::JoinHandle<()>,
	}

	fn build_rig_with(
		client: Arc<dyn BlockBackend<substrate_test_runtime::Block> + Send + Sync>,
		config: BitswapServiceConfig,
	) -> TestRig {
		let (inbound_tx, inbound_rx) = mpsc::channel(64);
		let (outbound_req_tx, outbound_req_rx) = mpsc::channel(64);
		let (outbound_resp_tx, outbound_resp_rx) = mpsc::channel(64);
		let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
		let (peer_event_tx, peer_event_rx) = mpsc::channel(PEER_EVENT_CHANNEL_CAPACITY);
		let (lookup_tx, lookup_rx) = mpsc::channel(LOOKUP_CHANNEL_CAPACITY);

		let transport = MockTransport {
			inbound: AsyncMutex::new(inbound_rx),
			outbound_req_tx,
			outbound_resp_tx,
		};

		let service: BitswapService<substrate_test_runtime::Block> = BitswapService {
			handle: Box::new(transport),
			client,
			config,
			cmd_rx,
			peer_event_rx,
			lookup_tx,
			lookup_rx,
			lookup_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INBOUND_LOOKUPS)),
			waiter_deadlines: DelayQueue::new(),
			connected_peers: HashSet::new(),
			wants: HashMap::new(),
			waiters: SlotMap::with_key(),
		};

		let user_handle = BitswapHandle::new(cmd_tx);
		let _handle = tokio::spawn(async move { service.run().await });

		TestRig {
			user_handle,
			peer_event_tx,
			inbound_tx,
			outbound_req_rx,
			outbound_resp_rx,
			_handle,
		}
	}

	fn empty_rig() -> TestRig {
		let client = Arc::new(substrate_test_runtime_client::new());
		build_rig_with(client, BitswapServiceConfig { request_timeout: Duration::from_secs(30) })
	}

	fn short_deadline_rig(deadline: Duration) -> TestRig {
		let client = Arc::new(substrate_test_runtime_client::new());
		build_rig_with(client, BitswapServiceConfig { request_timeout: deadline })
	}

	fn cid_for_data(mh_code: u64, data: &[u8]) -> Cid {
		let digest = hash_for_multihash_code(mh_code, data).expect("supported");
		let mh = CidMultihash::<64>::wrap(mh_code, &digest).unwrap();
		Cid::new_v1(RAW_CODEC, mh)
	}

	fn cid_for_digest(mh_code: u64, digest: [u8; 32]) -> Cid {
		let mh = CidMultihash::<64>::wrap(mh_code, &digest).unwrap();
		Cid::new_v1(RAW_CODEC, mh)
	}

	async fn drain_next<T>(rx: &mut mpsc::Receiver<T>) -> Option<T> {
		timeout(Duration::from_secs(2), rx.recv()).await.ok().flatten()
	}

	#[tokio::test(start_paused = true)]
	async fn single_cid_single_peer_block_response() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"payload-a".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let (out_peer, out_cids) =
			drain_next(&mut rig.outbound_req_rx).await.expect("outbound WANT");
		assert_eq!(out_peer, peer);
		assert_eq!(out_cids, vec![(cid, WantType::Block)]);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		match item {
			Ok((got_cid, FetchOutcome::Block(bytes))) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn dont_have_then_missing_at_deadline() {
		let mut rig = short_deadline_rig(Duration::from_millis(50));
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [7u8; 32]);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound WANT");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Presence {
					cid,
					presence: BlockPresenceType::DontHave,
				}],
			})
			.await
			.unwrap();

		tokio::time::advance(Duration::from_millis(60)).await;

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((_, FetchOutcome::Missing))));
	}

	#[tokio::test(start_paused = true)]
	async fn per_peer_timeout_followed_by_deadline_missing() {
		let mut rig = short_deadline_rig(Duration::from_millis(100));
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [42u8; 32]);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		tokio::time::advance(Duration::from_millis(120)).await;

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((_, FetchOutcome::Missing))));
	}

	#[tokio::test(start_paused = true)]
	async fn two_peers_first_dont_have_second_block() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"after-failover".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer: peer_a }).await.unwrap();
		rig.peer_event_tx.send(PeerEvent::Connected { peer: peer_b }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let (first_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: first_peer,
				responses: vec![ResponseType::Presence {
					cid,
					presence: BlockPresenceType::DontHave,
				}],
			})
			.await
			.unwrap();

		let (second_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("second WANT");
		assert_ne!(first_peer, second_peer);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: second_peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		match item {
			Ok((got, FetchOutcome::Block(bytes))) => {
				assert_eq!(got, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn zero_peers_at_admission_missing_at_deadline() {
		let mut rig = short_deadline_rig(Duration::from_millis(50));
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);

		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		assert!(drain_next(&mut rig.outbound_req_rx).await.is_none());

		tokio::time::advance(Duration::from_millis(60)).await;

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((_, FetchOutcome::Missing))));
	}

	#[tokio::test(start_paused = true)]
	async fn two_waiters_overlapping_cid_both_get_block() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"shared".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();

		let mut rx_a = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let mut rx_b = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item_a = drain_next(&mut rx_a).await.expect("a");
		let item_b = drain_next(&mut rx_b).await.expect("b");
		assert!(matches!(&item_a, Ok((c, FetchOutcome::Block(b))) if *c == cid && *b == data));
		assert!(matches!(&item_b, Ok((c, FetchOutcome::Block(b))) if *c == cid && *b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn waiter_drop_does_not_break_other_waiter() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"survivor".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();

		let rx_a = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let mut rx_b = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		drop(rx_a);
		sleep(Duration::from_millis(1)).await;

		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item_b = drain_next(&mut rx_b).await.expect("b");
		assert!(matches!(item_b, Ok((c, FetchOutcome::Block(b))) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn waiter_deadline_emits_missing_for_unresolved() {
		let mut rig = short_deadline_rig(Duration::from_millis(40));
		let peer = litep2p::PeerId::random();
		let cid_a = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let cid_b = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [2u8; 32]);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid_a, cid_b]).await.unwrap();

		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound a or b");
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound the other");

		tokio::time::advance(Duration::from_millis(60)).await;

		let mut seen = HashSet::new();
		for _ in 0..2 {
			let item = drain_next(&mut rx).await.expect("missing item");
			match item {
				Ok((c, FetchOutcome::Missing)) => {
					seen.insert(c);
				},
				other => panic!("expected Missing, got {other:?}"),
			}
		}
		assert!(seen.contains(&cid_a));
		assert!(seen.contains(&cid_b));
	}

	#[tokio::test(start_paused = true)]
	async fn inbound_request_with_known_block_serves_it() {
		let client = TestClientBuilder::with_tx_storage(u32::MAX).build();
		let mut block_builder = BlockBuilderBuilder::new(&client)
			.on_parent_block(client.chain_info().genesis_hash)
			.with_parent_block_number(0)
			.build()
			.unwrap();

		let ext = ExtrinsicBuilder::new_indexed_call(vec![0x42, 0x42, 0x42, 0x42]).build();
		let pattern_index = ext.encoded_size() - 4;
		let data_hash = sp_crypto_hashing::blake2_256(&ext.encode()[pattern_index..]);
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, data_hash);

		block_builder.push(ext.clone()).unwrap();
		let block = block_builder.build().unwrap().block;
		client.import(BlockOrigin::File, block).await.unwrap();

		let mut rig = build_rig_with(
			Arc::new(client),
			BitswapServiceConfig { request_timeout: Duration::from_secs(30) },
		);

		let peer = litep2p::PeerId::random();
		rig.inbound_tx
			.send(BitswapEvent::Request { peer, cids: vec![(cid, WantType::Block)] })
			.await
			.unwrap();

		let (resp_peer, responses) = drain_next(&mut rig.outbound_resp_rx).await.expect("response");
		assert_eq!(resp_peer, peer);
		assert_eq!(responses.len(), 1);
		match &responses[0] {
			ResponseType::Block { cid: got_cid, block } => {
				assert_eq!(*got_cid, cid);
				assert_eq!(*block, vec![0x42, 0x42, 0x42, 0x42]);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn corrupted_block_rejected_then_missing_at_deadline() {
		let mut rig = short_deadline_rig(Duration::from_millis(50));
		let peer = litep2p::PeerId::random();
		let real = b"the-real-payload".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &real);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block {
					cid,
					block: b"NOT-the-real-payload".to_vec(),
				}],
			})
			.await
			.unwrap();

		tokio::time::advance(Duration::from_millis(60)).await;

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((_, FetchOutcome::Missing))));
	}

	#[tokio::test]
	async fn admission_too_many_cids_rejected() {
		let rig = empty_rig();
		let cids: Vec<Cid> = (0..=MAX_WANTED_BLOCKS as u8)
			.map(|i| {
				let mut d = [0u8; 32];
				d[0] = i;
				cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, d)
			})
			.collect();

		let err = rig.user_handle.request_stream(cids).await.err().expect("err");
		match err {
			BitswapError::TooManyCids { requested, max } => {
				assert_eq!(requested, MAX_WANTED_BLOCKS + 1);
				assert_eq!(max, MAX_WANTED_BLOCKS);
			},
			other => panic!("expected TooManyCids, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn admission_invalid_cid_rejected() {
		let rig = empty_rig();
		let bad = Cid::new_v1(
			RAW_CODEC,
			CidMultihash::<64>::wrap(0x99 /* unsupported */, &[0u8; 32]).unwrap(),
		);

		let err = rig.user_handle.request_stream(vec![bad]).await.err().expect("err");
		assert!(matches!(err, BitswapError::InvalidCid { .. }));
	}

	#[tokio::test]
	async fn admission_empty_returns_closed_receiver() {
		let rig = empty_rig();
		let mut rx = rig.user_handle.request_stream(vec![]).await.unwrap();
		assert!(rx.recv().await.is_none());
	}

	#[tokio::test(start_paused = true)]
	async fn service_shutdown_emits_service_closed() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [0xee; 32]);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await;

		drop(rig.inbound_tx);

		let item = drain_next(&mut rx).await.expect("expect Err");
		assert!(matches!(item, Err(BitswapError::ServiceClosed)));
	}

	#[tokio::test(start_paused = true)]
	async fn peer_disconnect_triggers_failover() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"after-disconnect".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer: peer_a }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let (first_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");
		assert_eq!(first_peer, peer_a);

		rig.peer_event_tx.send(PeerEvent::Connected { peer: peer_b }).await.unwrap();
		rig.peer_event_tx.send(PeerEvent::Disconnected { peer: peer_a }).await.unwrap();

		let (second_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("failover WANT");
		assert_eq!(second_peer, peer_b);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: peer_b,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((c, FetchOutcome::Block(b))) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn late_response_after_waiter_gone_is_dropped() {
		let mut rig = short_deadline_rig(Duration::from_millis(20));
		let peer = litep2p::PeerId::random();
		let data = b"too-late".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.peer_event_tx.send(PeerEvent::Connected { peer }).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		tokio::time::advance(Duration::from_millis(40)).await;
		let _missing = drain_next(&mut rx).await.expect("missing");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		sleep(Duration::from_millis(1)).await;
		assert!(rx.recv().await.is_none());
	}
}

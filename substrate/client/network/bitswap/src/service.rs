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
//! Outbound requests of any size are accepted; at most [`MAX_LIVE_CIDS`] CIDs have
//! in-flight peer requests at a time, the rest queue and are dispatched as window slots
//! free up. Requests carry no service-side deadline: the caller bounds the wait by
//! dropping the receiver, which a periodic sweep detects to release the wants.
//! Unresolved CIDs are retried in rounds: once every connected peer has been tried,
//! all peers become eligible again after [`ROUND_RETRY_DELAY`].
//!
//! Peer connect/disconnect tracking comes from `sc-network-sync`'s [`SyncEventStream`]. The
//! sync engine replays `PeerConnected` for every currently-connected peer when a new
//! subscriber registers, so the actor sees the full peer set on startup. Light-client
//! peers are not tracked: they do not hold the indexed-transaction data served over
//! bitswap.

use super::{
	is_cid_supported, BitswapCommand, BitswapHandle, Cid, FetchItem, BLAKE2B_256_MULTIHASH_CODE,
	KECCAK_256_MULTIHASH_CODE, LOG_TARGET, MAX_WANTED_BLOCKS, SHA2_256_MULTIHASH_CODE,
};
use crate::handle::BitswapError;

use async_trait::async_trait;
use cid::multihash::Multihash as CidMultihash;
use futures::{Stream, StreamExt};
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle as LitepBitswapHandle, BlockPresenceType, ResponseType, WantType,
};
use sc_client_api::BlockBackend;
use sc_network_common::role::Roles;
use sc_network_sync::{SyncEvent, SyncEventStream};
use slotmap::{new_key_type, SlotMap};
use smallvec::SmallVec;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	future::Future,
	pin::Pin,
	sync::Arc,
	time::Duration,
};
use tokio::{
	sync::{mpsc, Semaphore},
	time::Instant,
};

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

/// Dispatch-window size: max number of CIDs with in-flight peer requests at once. CIDs
/// beyond the window are queued and dispatched as slots free up, so requests of any size
/// are accepted.
const MAX_LIVE_CIDS: usize = 1024;
const MAX_WAITERS_PER_CID: usize = 64;
const MAX_CONCURRENT_INBOUND_LOOKUPS: usize = 8;
const CMD_CHANNEL_CAPACITY: usize = 256;
const LOOKUP_CHANNEL_CAPACITY: usize = 64;
const PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay before an exhausted CID (every connected peer tried) starts a new round.
const ROUND_RETRY_DELAY: Duration = Duration::from_secs(5);
const PEER_FANOUT_CAP: usize = 1;
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

new_key_type! { struct WaiterId; }

// Per-CID network scheduling state, deduplicated across overlapping waiters.
struct CidState {
	tried_peers: HashSet<litep2p::PeerId>,
	in_flight_peers: HashMap<litep2p::PeerId, Instant>,
	/// Peers that answered HAVE for this CID. Preferred on dispatch; a second HAVE from
	/// the same peer marks it tried.
	have_peers: SmallVec<[litep2p::PeerId; 1]>,
	waiters: SmallVec<[WaiterId; 2]>,
	/// Whether this CID currently has a valid entry in [`WantSet::pending`].
	pending: bool,
	/// When set, every connected peer has been tried; a new round (with cleared per-round
	/// peer state) starts once this instant passes. Cleared on dispatch.
	next_round_at: Option<Instant>,
}

impl CidState {
	fn new() -> Self {
		Self {
			tried_peers: HashSet::new(),
			in_flight_peers: HashMap::new(),
			have_peers: SmallVec::new(),
			waiters: SmallVec::new(),
			pending: false,
			next_round_at: None,
		}
	}

	fn has_waiters(&self) -> bool {
		!self.waiters.is_empty()
	}

	fn is_idle(&self) -> bool {
		self.waiters.is_empty() && self.in_flight_peers.is_empty()
	}
}

struct WantSet {
	inner: HashMap<Cid, CidState>,
	/// FIFO of CIDs waiting for a free dispatch-window slot. May contain stale entries
	/// (resolved, abandoned or already-dispatched CIDs); those are skipped on pop, guarded
	/// by [`CidState::pending`].
	pending: VecDeque<Cid>,
	/// Number of CIDs with at least one in-flight peer request.
	live: usize,
	/// Dispatch-window size.
	max_live: usize,
}

impl WantSet {
	fn new(max_live: usize) -> Self {
		Self { inner: HashMap::new(), pending: VecDeque::new(), live: 0, max_live }
	}

	fn contains(&self, cid: &Cid) -> bool {
		self.inner.contains_key(cid)
	}

	fn waiter_count(&self, cid: &Cid) -> usize {
		self.inner.get(cid).map_or(0, |state| state.waiters.len())
	}

	fn add_waiter(&mut self, cid: Cid, waiter: WaiterId) {
		self.inner.entry(cid).or_insert_with(CidState::new).waiters.push(waiter);
	}

	fn remove_waiter(&mut self, cid: Cid, waiter: WaiterId) {
		if let Some(state) = self.inner.get_mut(&cid) {
			state.waiters.retain(|w| *w != waiter);
		}
		self.remove_if_idle(cid);
	}

	fn all_cids(&self) -> Vec<Cid> {
		self.inner.keys().copied().collect()
	}

	fn take_waiters_for_delivered_cid(&mut self, cid: Cid) -> Option<SmallVec<[WaiterId; 2]>> {
		self.inner.remove(&cid).map(|state| {
			if !state.in_flight_peers.is_empty() {
				self.live -= 1;
			}
			state.waiters
		})
	}

	fn has_window_capacity(&self) -> bool {
		self.live < self.max_live
	}

	/// Pop the next CID waiting for a window slot, skipping stale queue entries.
	fn pop_pending(&mut self) -> Option<Cid> {
		while let Some(cid) = self.pending.pop_front() {
			if let Some(state) = self.inner.get_mut(&cid) {
				if state.pending {
					state.pending = false;
					return Some(cid);
				}
			}
		}
		None
	}

	fn next_peer_to_request(
		&mut self,
		cid: Cid,
		connected_peers: &HashSet<litep2p::PeerId>,
		now: Instant,
	) -> Option<litep2p::PeerId> {
		let state = self.inner.get(&cid)?;
		if !state.has_waiters() || state.in_flight_peers.len() >= PEER_FANOUT_CAP {
			return None;
		}

		// Dispatch window full: queue the CID for promotion when a slot frees up.
		if state.in_flight_peers.is_empty() && !self.has_window_capacity() {
			let state = self.inner.get_mut(&cid).expect("checked above; qed");
			if !state.pending {
				state.pending = true;
				self.pending.push_back(cid);
			}
			return None;
		}

		let eligible = |peer: &litep2p::PeerId| {
			!state.tried_peers.contains(peer) && !state.in_flight_peers.contains_key(peer)
		};
		// Peers that answered HAVE for this CID are asked first.
		let Some(peer) = state
			.have_peers
			.iter()
			.find(|peer| connected_peers.contains(*peer) && eligible(peer))
			.or_else(|| connected_peers.iter().find(|peer| eligible(peer)))
			.copied()
		else {
			// Round exhausted: every connected peer has been tried. Schedule a new round,
			// started by the sweep once the delay passes.
			if !connected_peers.is_empty() && state.in_flight_peers.is_empty() {
				let state = self.inner.get_mut(&cid).expect("checked above; qed");
				if state.next_round_at.is_none() {
					state.next_round_at = Some(now + ROUND_RETRY_DELAY);
					log::trace!(
						target: LOG_TARGET,
						"all peers tried for {cid}, scheduling new round",
					);
				}
			}
			return None;
		};

		let state = self.inner.get_mut(&cid).expect("checked above; qed");
		if state.in_flight_peers.is_empty() {
			self.live += 1;
		}
		state.in_flight_peers.insert(peer, now + PER_PEER_TIMEOUT);
		state.pending = false;
		state.next_round_at = None;

		Some(peer)
	}

	fn mark_peer_done_for_cid(&mut self, peer: litep2p::PeerId, cid: Cid) {
		if let Some(state) = self.inner.get_mut(&cid) {
			if state.in_flight_peers.remove(&peer).is_some() && state.in_flight_peers.is_empty() {
				self.live -= 1;
			}
			state.tried_peers.insert(peer);
		}
		self.remove_if_idle(cid);
	}

	/// Record a HAVE from `peer` for `cid` and release its in-flight slot.
	///
	/// A first HAVE keeps the peer eligible (and preferred) for a re-ask; a repeated HAVE
	/// without the block marks the peer tried.
	fn note_peer_have_for_cid(&mut self, peer: litep2p::PeerId, cid: Cid) {
		if let Some(state) = self.inner.get_mut(&cid) {
			if state.have_peers.contains(&peer) {
				state.tried_peers.insert(peer);
			} else {
				state.have_peers.push(peer);
			}
			if state.in_flight_peers.remove(&peer).is_some() && state.in_flight_peers.is_empty() {
				self.live -= 1;
			}
		}
		self.remove_if_idle(cid);
	}

	fn remove_in_flight_peer(&mut self, peer: litep2p::PeerId) -> Vec<Cid> {
		let mut affected: Vec<Cid> = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			if state.in_flight_peers.remove(&peer).is_some() {
				if state.in_flight_peers.is_empty() {
					self.live -= 1;
				}
				affected.push(*cid);
			}
		}

		self.remove_idle_and_filter_existing(affected)
	}

	fn expire_peer_timeouts(&mut self, now: Instant) -> Vec<Cid> {
		let mut timed_out: Vec<(Cid, litep2p::PeerId)> = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			let had_in_flight = !state.in_flight_peers.is_empty();
			state.in_flight_peers.retain(|peer, deadline| {
				if *deadline <= now {
					timed_out.push((*cid, *peer));
					false
				} else {
					true
				}
			});
			if had_in_flight && state.in_flight_peers.is_empty() {
				self.live -= 1;
			}
		}

		let mut cids = Vec::with_capacity(timed_out.len());
		for (cid, peer) in timed_out {
			if let Some(state) = self.inner.get_mut(&cid) {
				state.tried_peers.insert(peer);
				cids.push(cid);
			}
		}

		self.remove_idle_and_filter_existing(cids)
	}

	/// Start a new round for CIDs whose retry delay has passed: clear per-round peer state
	/// so every connected peer is eligible again.
	fn restart_exhausted_rounds(&mut self, now: Instant) -> Vec<Cid> {
		let mut cids = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			if state.has_waiters() &&
				state.in_flight_peers.is_empty() &&
				state.next_round_at.is_some_and(|at| at <= now)
			{
				state.tried_peers.clear();
				state.have_peers.clear();
				state.next_round_at = None;
				cids.push(*cid);
			}
		}
		cids
	}

	fn clear(&mut self) {
		self.inner.clear();
		self.pending.clear();
		self.live = 0;
	}

	fn remove_idle_and_filter_existing(&mut self, cids: Vec<Cid>) -> Vec<Cid> {
		for cid in &cids {
			self.remove_if_idle(*cid);
		}
		cids.into_iter().filter(|cid| self.inner.contains_key(cid)).collect()
	}

	fn remove_if_idle(&mut self, cid: Cid) {
		if self.inner.get(&cid).is_some_and(CidState::is_idle) {
			self.inner.remove(&cid);
		}
	}
}

struct Waiter {
	cids_remaining: HashSet<Cid>,
	sink: mpsc::Sender<FetchItem>,
}

type InboundLookupResult = (litep2p::PeerId, Vec<ResponseType>);

struct InboundLookupPool<B: BlockT> {
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	result_tx: mpsc::Sender<InboundLookupResult>,
	semaphore: Arc<Semaphore>,
}

impl<B: BlockT> InboundLookupPool<B> {
	fn new(
		client: Arc<dyn BlockBackend<B> + Send + Sync>,
	) -> (Self, mpsc::Receiver<InboundLookupResult>) {
		let (result_tx, result_rx) = mpsc::channel(LOOKUP_CHANNEL_CAPACITY);
		(
			Self {
				client,
				result_tx,
				semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INBOUND_LOOKUPS)),
			},
			result_rx,
		)
	}

	/// Serve the wantlist on a blocking worker. Returns `false` if the pool is saturated.
	fn try_submit(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) -> bool {
		let Ok(permit) = self.semaphore.clone().try_acquire_owned() else {
			return false;
		};

		let client = self.client.clone();
		let result_tx = self.result_tx.clone();
		tokio::task::spawn_blocking(move || {
			let _permit = permit;
			let responses = serve_inbound(&*client, cids);
			let _ = result_tx.try_send((peer, responses));
		});

		true
	}
}

pub(crate) struct BitswapService<B: BlockT> {
	handle: Box<dyn BitswapTransport>,

	cmd_rx: mpsc::Receiver<BitswapCommand>,
	/// Set once every [`BitswapHandle`] has been dropped; the actor then stops polling
	/// `cmd_rx` but keeps serving inbound wantlists.
	cmd_channel_closed: bool,
	sync_event_stream: Pin<Box<dyn Stream<Item = SyncEvent> + Send>>,
	inbound_lookup_pool: InboundLookupPool<B>,
	inbound_lookup_rx: mpsc::Receiver<InboundLookupResult>,

	connected_peers: HashSet<litep2p::PeerId>,
	wants: WantSet,
	waiters: SlotMap<WaiterId, Waiter>,
}

/// Build the Bitswap service, returning the service future and the user-facing handle.
///
/// The future must be spawned by the caller. `litep2p_handle` is the transport-side handle
/// created by `litep2p::protocol::libp2p::bitswap::Config::new`; the corresponding `Config`
/// must be installed into the litep2p backend by the caller.
pub fn start<B: BlockT, S>(
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	sync: &S,
	litep2p_handle: LitepBitswapHandle,
) -> (Pin<Box<dyn Future<Output = ()> + Send>>, BitswapHandle)
where
	S: SyncEventStream + ?Sized,
{
	let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
	let (inbound_lookup_pool, inbound_lookup_rx) = InboundLookupPool::new(client);

	let user_handle = BitswapHandle::new(cmd_tx);
	let sync_event_stream = sync.event_stream("bitswap");

	let service = BitswapService {
		handle: Box::new(litep2p_handle),
		cmd_rx,
		cmd_channel_closed: false,
		sync_event_stream,
		inbound_lookup_pool,
		inbound_lookup_rx,
		connected_peers: HashSet::new(),
		wants: WantSet::new(MAX_LIVE_CIDS),
		waiters: SlotMap::with_key(),
	};

	let future = Box::pin(async move { service.run().await });

	(future, user_handle)
}

impl<B: BlockT> BitswapService<B> {
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "BitswapService starting");
		let mut sweep_ticker = tokio::time::interval(SWEEP_INTERVAL);
		sweep_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		sweep_ticker.tick().await;

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

				cmd = self.cmd_rx.recv(), if !self.cmd_channel_closed => match cmd {
					Some(BitswapCommand::RequestStream { cids, sink }) =>
						self.on_request_stream(cids, sink).await,
					None => {
						// All user handles were dropped. Keep running: the node may still
						// serve inbound wantlists, and already-admitted waiters still
						// resolve normally.
						log::debug!(
							target: LOG_TARGET,
							"all bitswap handles dropped; serving inbound requests only",
						);
						self.cmd_channel_closed = true;
					},
				},

				sync_ev = self.sync_event_stream.next() => match sync_ev {
					Some(SyncEvent::PeerConnected { peer_id, roles }) =>
						self.on_peer_connected(peer_id.into(), roles).await,
					Some(SyncEvent::PeerDisconnected(peer)) => self.on_peer_disconnected(peer.into()).await,
					None => {
						log::debug!(target: LOG_TARGET, "sync event stream ended; shutting down");
						self.shutdown_waiters();
						return;
					},
				},

				Some((peer, responses)) = self.inbound_lookup_rx.recv() => {
					self.handle.send_response(peer, responses).await;
				},

				_ = sweep_ticker.tick() => {
					self.on_sweep().await;
				},
			}
		}
	}

	async fn on_request_stream(&mut self, cids: Vec<Cid>, sink: mpsc::Sender<FetchItem>) {
		for cid in &cids {
			if self.wants.waiter_count(cid) >= MAX_WAITERS_PER_CID {
				let _ = sink.try_send(Err(BitswapError::Overloaded));
				return;
			}
		}

		let cids_remaining: HashSet<Cid> = cids.iter().copied().collect();
		let waiter_id = self.waiters.insert(Waiter { cids_remaining, sink });

		for cid in &cids {
			self.wants.add_waiter(*cid, waiter_id);
		}

		self.top_up_in_flight(cids).await;
	}

	/// Dispatch WANT-BLOCK requests for the given CIDs, then promote queued CIDs into any
	/// remaining dispatch-window capacity. CIDs assigned to the same peer are bundled into
	/// wantlist messages of up to [`MAX_WANTED_BLOCKS`] entries.
	async fn top_up_in_flight(&mut self, cids: impl IntoIterator<Item = Cid>) {
		let now = Instant::now();
		let mut by_peer: HashMap<litep2p::PeerId, Vec<(Cid, WantType)>> = HashMap::new();
		for cid in cids {
			if let Some(peer) = self.wants.next_peer_to_request(cid, &self.connected_peers, now) {
				log::trace!(target: LOG_TARGET, "WANT-BLOCK {cid} -> {peer:?}");
				by_peer.entry(peer).or_default().push((cid, WantType::Block));
			}
		}

		while self.wants.has_window_capacity() {
			let Some(cid) = self.wants.pop_pending() else { break };
			if let Some(peer) = self.wants.next_peer_to_request(cid, &self.connected_peers, now) {
				log::trace!(target: LOG_TARGET, "WANT-BLOCK {cid} -> {peer:?} (promoted)");
				by_peer.entry(peer).or_default().push((cid, WantType::Block));
			}
		}

		for (peer, wants) in by_peer {
			for chunk in wants.chunks(MAX_WANTED_BLOCKS) {
				self.handle.send_request(peer, chunk.to_vec()).await;
			}
		}
	}

	async fn on_inbound_response(&mut self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
		let mut cids_to_top_up: HashSet<Cid> = HashSet::new();

		for response in responses {
			match response {
				ResponseType::Block { cid: claimed_cid, block } => {
					self.wants.mark_peer_done_for_cid(peer, claimed_cid);

					if recompute_cid(&claimed_cid, &block) != Some(claimed_cid) {
						log::debug!(
							target: LOG_TARGET,
							"{peer:?} sent block for {claimed_cid} that failed CID verification",
						);
						cids_to_top_up.insert(claimed_cid);
					} else if self.wants.contains(&claimed_cid) {
						self.deliver_block(claimed_cid, block);
					} else {
						log::debug!(
							target: LOG_TARGET,
							"{peer:?} sent unsolicited or unwanted block for {claimed_cid}",
						);
					}
				},
				ResponseType::Presence { cid, presence } => {
					match presence {
						BlockPresenceType::DontHave => {
							log::trace!(target: LOG_TARGET, "{peer:?} DONT_HAVE {cid}");
							self.wants.mark_peer_done_for_cid(peer, cid);
						},
						BlockPresenceType::Have => {
							log::trace!(target: LOG_TARGET, "{peer:?} HAVE {cid}");
							self.wants.note_peer_have_for_cid(peer, cid);
						},
					}
					cids_to_top_up.insert(cid);
				},
			}
		}

		self.top_up_in_flight(cids_to_top_up).await;
	}

	fn deliver_block(&mut self, cid: Cid, bytes: Vec<u8>) {
		let Some(waiter_ids) = self.wants.take_waiters_for_delivered_cid(cid) else { return };

		for waiter_id in waiter_ids {
			let Some(waiter) = self.waiters.get_mut(waiter_id) else { continue };
			if !waiter.cids_remaining.remove(&cid) {
				continue;
			}
			if waiter.sink.try_send(Ok((cid, bytes.clone()))).is_err() {
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

		for cid in &waiter.cids_remaining {
			self.wants.remove_waiter(*cid, id);
		}
	}

	async fn on_peer_connected(&mut self, peer: litep2p::PeerId, roles: Roles) {
		// Light clients do not hold the indexed-transaction data served over bitswap.
		if roles.is_light() {
			return;
		}
		self.connected_peers.insert(peer);
		let cids = self.wants.all_cids();
		self.top_up_in_flight(cids).await;
	}

	async fn on_peer_disconnected(&mut self, peer: litep2p::PeerId) {
		self.connected_peers.remove(&peer);
		let cids_to_top_up = self.wants.remove_in_flight_peer(peer);
		self.top_up_in_flight(cids_to_top_up).await;
	}

	/// Periodic housekeeping: drop waiters whose receiver was dropped (the caller gave up
	/// or applied its own timeout), expire per-peer request timeouts, and start new rounds
	/// for CIDs whose retry delay has passed.
	async fn on_sweep(&mut self) {
		let abandoned: Vec<WaiterId> = self
			.waiters
			.iter()
			.filter_map(|(id, waiter)| waiter.sink.is_closed().then_some(id))
			.collect();
		for id in abandoned {
			log::trace!(target: LOG_TARGET, "dropping abandoned waiter {id:?}");
			self.drop_waiter(id);
		}

		let now = Instant::now();
		let mut cids = self.wants.expire_peer_timeouts(now);
		cids.extend(self.wants.restart_exhausted_rounds(now));
		self.top_up_in_flight(cids).await;
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
		if !self.inbound_lookup_pool.try_submit(peer, cids) {
			log::trace!(
				target: LOG_TARGET,
				"inbound serving pool saturated; dropping wantlist from {peer:?}",
			);
		}
	}

	fn shutdown_waiters(&mut self) {
		for (_, waiter) in self.waiters.drain() {
			let _ = waiter.sink.try_send(Err(BitswapError::ServiceClosed));
		}
		self.wants.clear();
	}
}

/// Rebuild the CID for `data` using the hashing and codec of `reference_cid`. Returns `None`
/// for unsupported multihash codes.
fn recompute_cid(reference_cid: &Cid, data: &[u8]) -> Option<Cid> {
	let code = reference_cid.hash().code();
	let digest = hash_for_multihash_code(code, data)?;
	let mh = CidMultihash::<64>::wrap(code, &digest).ok()?;
	Some(Cid::new_v1(reference_cid.codec(), mh))
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
	use crate::RAW_CODEC;
	use sc_block_builder::BlockBuilderBuilder;
	use sc_network_sync::SyncEvent;
	use sc_network_types::PeerId as TypesPeerId;
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
		sync_event_tx: mpsc::Sender<SyncEvent>,
		inbound_tx: mpsc::Sender<BitswapEvent>,
		outbound_req_rx: mpsc::Receiver<(litep2p::PeerId, Vec<(Cid, WantType)>)>,
		outbound_resp_rx: mpsc::Receiver<(litep2p::PeerId, Vec<ResponseType>)>,
		_handle: tokio::task::JoinHandle<()>,
	}

	fn build_rig_with(
		client: Arc<dyn BlockBackend<substrate_test_runtime::Block> + Send + Sync>,
		max_live_cids: usize,
	) -> TestRig {
		let (inbound_tx, inbound_rx) = mpsc::channel(64);
		let (outbound_req_tx, outbound_req_rx) = mpsc::channel(64);
		let (outbound_resp_tx, outbound_resp_rx) = mpsc::channel(64);
		let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
		let (sync_event_tx, sync_event_rx) = mpsc::channel::<SyncEvent>(64);
		let (inbound_lookup_pool, inbound_lookup_rx) = InboundLookupPool::new(client);

		let transport = MockTransport {
			inbound: AsyncMutex::new(inbound_rx),
			outbound_req_tx,
			outbound_resp_tx,
		};

		let sync_event_stream: Pin<Box<dyn Stream<Item = SyncEvent> + Send>> =
			Box::pin(tokio_stream::wrappers::ReceiverStream::new(sync_event_rx));

		let service: BitswapService<substrate_test_runtime::Block> = BitswapService {
			handle: Box::new(transport),
			cmd_rx,
			cmd_channel_closed: false,
			sync_event_stream,
			inbound_lookup_pool,
			inbound_lookup_rx,
			connected_peers: HashSet::new(),
			wants: WantSet::new(max_live_cids),
			waiters: SlotMap::with_key(),
		};

		let user_handle = BitswapHandle::new(cmd_tx);
		let _handle = tokio::spawn(async move { service.run().await });

		TestRig {
			user_handle,
			sync_event_tx,
			inbound_tx,
			outbound_req_rx,
			outbound_resp_rx,
			_handle,
		}
	}

	fn empty_rig() -> TestRig {
		small_window_rig(MAX_LIVE_CIDS)
	}

	fn small_window_rig(max_live_cids: usize) -> TestRig {
		let client = Arc::new(substrate_test_runtime_client::new());
		build_rig_with(client, max_live_cids)
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

	/// [`SyncEvent`] carries [`sc_network_types::PeerId`]; convert from the byte-compatible
	/// [`litep2p::PeerId`] used on the transport side.
	fn to_types_peer(peer: litep2p::PeerId) -> TypesPeerId {
		TypesPeerId::from_bytes(&peer.to_bytes()).expect("valid peer-id bytes; qed")
	}

	fn sync_connected(peer: litep2p::PeerId) -> SyncEvent {
		SyncEvent::PeerConnected { peer_id: to_types_peer(peer), roles: Roles::FULL }
	}

	fn sync_connected_light(peer: litep2p::PeerId) -> SyncEvent {
		SyncEvent::PeerConnected { peer_id: to_types_peer(peer), roles: Roles::LIGHT }
	}

	fn sync_disconnected(peer: litep2p::PeerId) -> SyncEvent {
		SyncEvent::PeerDisconnected(to_types_peer(peer))
	}

	#[test]
	fn want_set_removes_cid_after_last_waiter_and_peer_complete() {
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [0xaa; 32]);
		let peer = litep2p::PeerId::random();
		let mut waiter_ids = SlotMap::with_key();
		let waiter_id = waiter_ids.insert(());
		let mut wants = WantSet::new(MAX_LIVE_CIDS);

		wants.add_waiter(cid, waiter_id);
		let selected =
			wants.next_peer_to_request(cid, &HashSet::from([peer]), Instant::now()).unwrap();
		assert_eq!(selected, peer);

		wants.remove_waiter(cid, waiter_id);
		assert!(wants.contains(&cid));

		wants.mark_peer_done_for_cid(peer, cid);
		assert!(!wants.contains(&cid));
	}

	#[test]
	fn want_set_window_queues_and_promotes() {
		let cid_a = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [0x01; 32]);
		let cid_b = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [0x02; 32]);
		let peer = litep2p::PeerId::random();
		let peers = HashSet::from([peer]);
		let mut waiter_ids = SlotMap::with_key();
		let waiter_id = waiter_ids.insert(());
		let mut wants = WantSet::new(1);

		wants.add_waiter(cid_a, waiter_id);
		wants.add_waiter(cid_b, waiter_id);

		assert_eq!(wants.next_peer_to_request(cid_a, &peers, Instant::now()), Some(peer));
		// Window (size 1) is full: `cid_b` must queue instead of dispatching.
		assert_eq!(wants.next_peer_to_request(cid_b, &peers, Instant::now()), None);
		assert!(!wants.has_window_capacity());

		// Delivering `cid_a` frees the slot; `cid_b` is promoted.
		wants.take_waiters_for_delivered_cid(cid_a);
		assert!(wants.has_window_capacity());
		assert_eq!(wants.pop_pending(), Some(cid_b));
		assert_eq!(wants.next_peer_to_request(cid_b, &peers, Instant::now()), Some(peer));
		assert_eq!(wants.pop_pending(), None);
	}

	#[tokio::test(start_paused = true)]
	async fn single_cid_single_peer_block_response() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"payload-a".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
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
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn have_response_reasks_same_peer_then_delivers() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"payload-have".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let (out_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");
		assert_eq!(out_peer, peer);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Presence { cid, presence: BlockPresenceType::Have }],
			})
			.await
			.unwrap();

		// A HAVE keeps the peer eligible: it is re-asked for the block.
		let (out_peer, out_cids) = drain_next(&mut rig.outbound_req_rx).await.expect("re-ask");
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
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn second_have_without_block_moves_to_other_peer() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"payload-have-2".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let (first, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");
		let other = if first == peer_a { peer_b } else { peer_a };

		let have = |peer| BitswapEvent::Response {
			peer,
			responses: vec![ResponseType::Presence { cid, presence: BlockPresenceType::Have }],
		};

		// First HAVE earns the peer a re-ask.
		rig.inbound_tx.send(have(first)).await.unwrap();
		let (out_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("re-ask");
		assert_eq!(out_peer, first);

		// Second HAVE without the block: the peer counts as tried, the want moves on.
		rig.inbound_tx.send(have(first)).await.unwrap();
		let (out_peer, out_cids) =
			drain_next(&mut rig.outbound_req_rx).await.expect("failover WANT");
		assert_eq!(out_peer, other);
		assert_eq!(out_cids, vec![(cid, WantType::Block)]);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: other,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		match item {
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn exhausted_round_reasks_peer_after_delay() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"payload-round".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

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

		// The only peer is tried: nothing is dispatched before the round delay passes.
		let no_req =
			timeout(ROUND_RETRY_DELAY - Duration::from_secs(1), rig.outbound_req_rx.recv()).await;
		assert!(no_req.is_err());

		// After the delay the round restarts and the same peer is asked again.
		let (out_peer, out_cids) =
			drain_next(&mut rig.outbound_req_rx).await.expect("new-round WANT");
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
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn timed_out_peer_is_reasked_next_round() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"payload-round-2".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

		// No response: the per-peer timeout marks the peer tried, then the round restarts
		// and the same peer is asked again.
		let (out_peer, out_cids) =
			timeout(PER_PEER_TIMEOUT + ROUND_RETRY_DELAY * 2, rig.outbound_req_rx.recv())
				.await
				.expect("re-ask after timeout and round delay")
				.expect("transport channel open");
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
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn new_peer_is_asked_immediately_while_round_is_parked() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"payload-round-3".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: peer_a,
				responses: vec![ResponseType::Presence {
					cid,
					presence: BlockPresenceType::DontHave,
				}],
			})
			.await
			.unwrap();

		// The CID is parked awaiting a new round; a fresh peer is dispatched immediately,
		// without waiting for the round delay.
		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
		let (out_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("WANT to new peer");
		assert_eq!(out_peer, peer_b);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: peer_b,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		match item {
			Ok((got_cid, bytes)) => {
				assert_eq!(got_cid, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}

		// The dispatch cancelled the scheduled round: no stray re-ask later.
		tokio::time::advance(ROUND_RETRY_DELAY * 2).await;
		assert!(rig.outbound_req_rx.try_recv().is_err());
	}

	#[tokio::test(start_paused = true)]
	async fn light_client_peers_are_not_tracked() {
		let mut rig = empty_rig();
		let light_peer = litep2p::PeerId::random();
		let full_peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [9u8; 32]);

		rig.sync_event_tx.send(sync_connected_light(light_peer)).await.unwrap();
		let _rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		// The only connected peer is a light client: no WANT must be dispatched.
		assert!(drain_next(&mut rig.outbound_req_rx).await.is_none());

		// Once a full peer connects, the pending want goes out to it.
		rig.sync_event_tx.send(sync_connected(full_peer)).await.unwrap();
		let (out_peer, out_cids) =
			drain_next(&mut rig.outbound_req_rx).await.expect("outbound WANT");
		assert_eq!(out_peer, full_peer);
		assert_eq!(out_cids, vec![(cid, WantType::Block)]);
	}

	#[tokio::test(start_paused = true)]
	async fn dont_have_from_only_peer_leaves_stream_open() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [7u8; 32]);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
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

		// No other peers to try: the want stays unresolved, the stream stays open until
		// the caller gives up.
		tokio::time::advance(Duration::from_secs(60)).await;
		assert!(matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
	}

	#[tokio::test(start_paused = true)]
	async fn per_peer_timeout_triggers_failover() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"after-timeout".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let (first_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

		// The unanswered request times out after `PER_PEER_TIMEOUT`; the sweep retries on
		// the other peer.
		tokio::time::advance(PER_PEER_TIMEOUT + Duration::from_secs(2)).await;

		let (second_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("failover WANT");
		assert_ne!(first_peer, second_peer);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: second_peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((c, b)) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn two_peers_first_dont_have_second_block() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"after-failover".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
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
			Ok((got, bytes)) => {
				assert_eq!(got, cid);
				assert_eq!(bytes, data);
			},
			other => panic!("expected Block, got {other:?}"),
		}
	}

	#[tokio::test(start_paused = true)]
	async fn receiver_drop_cancels_wants() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);

		let rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		// No peers connected: nothing is dispatched, the want just sits there.
		assert!(drain_next(&mut rig.outbound_req_rx).await.is_none());

		// The caller gives up; the sweep drops the abandoned waiter.
		drop(rx);
		tokio::time::advance(Duration::from_secs(2)).await;

		// A peer connecting afterwards must not trigger a WANT for the cancelled request.
		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
		assert!(drain_next(&mut rig.outbound_req_rx).await.is_none());
	}

	#[tokio::test(start_paused = true)]
	async fn two_waiters_overlapping_cid_both_get_block() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"shared".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();

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
		assert!(matches!(&item_a, Ok((c, b)) if *c == cid && *b == data));
		assert!(matches!(&item_b, Ok((c, b)) if *c == cid && *b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn waiter_drop_does_not_break_other_waiter() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"survivor".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();

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
		assert!(matches!(item_b, Ok((c, b)) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn dispatch_window_queues_excess_cids_and_promotes_on_delivery() {
		let mut rig = small_window_rig(4);
		let peer = litep2p::PeerId::random();
		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();

		let payloads: Vec<Vec<u8>> = (0..5u8).map(|i| vec![i; 8]).collect();
		let cids: Vec<Cid> =
			payloads.iter().map(|p| cid_for_data(BLAKE2B_256_MULTIHASH_CODE, p)).collect();

		let mut rx = rig.user_handle.request_stream(cids.clone()).await.unwrap();

		// Only the window (4 CIDs) is dispatched; the fifth is queued.
		let (_, entries) = drain_next(&mut rig.outbound_req_rx).await.expect("first bundle");
		assert_eq!(entries.len(), 4);
		let dispatched: HashSet<Cid> = entries.iter().map(|(cid, _)| *cid).collect();
		let queued_idx =
			cids.iter().position(|cid| !dispatched.contains(cid)).expect("one CID queued");

		// Answering one dispatched CID frees a slot and promotes the queued CID.
		let answered_idx = cids.iter().position(|cid| dispatched.contains(cid)).unwrap();
		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block {
					cid: cids[answered_idx],
					block: payloads[answered_idx].clone(),
				}],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("delivered block");
		assert!(matches!(item, Ok((c, _)) if c == cids[answered_idx]));

		let (_, promoted) = drain_next(&mut rig.outbound_req_rx).await.expect("promoted WANT");
		assert_eq!(promoted, vec![(cids[queued_idx], WantType::Block)]);
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

		let mut rig = build_rig_with(Arc::new(client), MAX_LIVE_CIDS);

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
	async fn corrupted_block_from_only_peer_is_not_delivered() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let real = b"the-real-payload".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &real);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
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

		// The corrupted block is rejected; no other peer can serve the CID, so the
		// stream stays open without delivering anything.
		tokio::time::advance(Duration::from_secs(60)).await;
		assert!(matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
	}

	#[tokio::test(start_paused = true)]
	async fn corrupted_block_triggers_failover_to_next_peer() {
		let mut rig = empty_rig();
		let peer_a = litep2p::PeerId::random();
		let peer_b = litep2p::PeerId::random();
		let data = b"genuine-payload".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();

		let (first_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: first_peer,
				responses: vec![ResponseType::Block { cid, block: b"corrupted".to_vec() }],
			})
			.await
			.unwrap();

		let (second_peer, _) = drain_next(&mut rig.outbound_req_rx)
			.await
			.expect("failover WANT after corrupted block");
		assert_ne!(first_peer, second_peer);

		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer: second_peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((c, b)) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn inbound_serving_continues_after_all_handles_dropped() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [3u8; 32]);

		drop(rig.user_handle);
		sleep(Duration::from_millis(1)).await;

		rig.inbound_tx
			.send(BitswapEvent::Request { peer, cids: vec![(cid, WantType::Block)] })
			.await
			.unwrap();

		let (resp_peer, responses) = drain_next(&mut rig.outbound_resp_rx)
			.await
			.expect("service must keep serving inbound after all handles are dropped");
		assert_eq!(resp_peer, peer);
		assert!(matches!(
			responses[0],
			ResponseType::Presence { presence: BlockPresenceType::DontHave, .. }
		));
	}

	#[tokio::test(start_paused = true)]
	async fn outbound_wants_bundled_and_split_at_message_cap() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();

		let cids: Vec<Cid> = (0..=MAX_WANTED_BLOCKS as u8)
			.map(|i| {
				let mut d = [0u8; 32];
				d[0] = i;
				cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, d)
			})
			.collect();

		let _rx = rig.user_handle.request_stream(cids.clone()).await.unwrap();

		let (peer_a, first) = drain_next(&mut rig.outbound_req_rx).await.expect("first bundle");
		let (peer_b, second) = drain_next(&mut rig.outbound_req_rx).await.expect("second bundle");
		assert_eq!(peer_a, peer);
		assert_eq!(peer_b, peer);

		let mut sizes = [first.len(), second.len()];
		sizes.sort();
		assert_eq!(sizes, [1, MAX_WANTED_BLOCKS]);

		let sent: HashSet<Cid> = first.into_iter().chain(second).map(|(cid, _)| cid).collect();
		assert_eq!(sent, cids.into_iter().collect::<HashSet<_>>());
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

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
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

		rig.sync_event_tx.send(sync_connected(peer_a)).await.unwrap();
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let (first_peer, _) = drain_next(&mut rig.outbound_req_rx).await.expect("first WANT");
		assert_eq!(first_peer, peer_a);

		rig.sync_event_tx.send(sync_connected(peer_b)).await.unwrap();
		rig.sync_event_tx.send(sync_disconnected(peer_a)).await.unwrap();

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
		assert!(matches!(item, Ok((c, b)) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn late_response_after_receiver_drop_is_ignored_and_cid_refetchable() {
		let mut rig = empty_rig();
		let peer = litep2p::PeerId::random();
		let data = b"too-late".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		rig.sync_event_tx.send(sync_connected(peer)).await.unwrap();
		let rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("outbound");

		// Caller gives up; the sweep drops the waiter while the peer request is still
		// in flight.
		drop(rx);
		tokio::time::advance(Duration::from_secs(2)).await;

		// The late response hits no waiter and is dropped.
		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();
		// Let the actor process the late response before the fresh request goes in.
		sleep(Duration::from_millis(1)).await;

		// A fresh request for the same CID starts from a clean slate: the peer is asked
		// again and the block is delivered.
		let mut rx = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let _ = drain_next(&mut rig.outbound_req_rx).await.expect("fresh WANT");
		rig.inbound_tx
			.send(BitswapEvent::Response {
				peer,
				responses: vec![ResponseType::Block { cid, block: data.clone() }],
			})
			.await
			.unwrap();

		let item = drain_next(&mut rx).await.expect("item");
		assert!(matches!(item, Ok((c, b)) if c == cid && b == data));
	}

	#[tokio::test(start_paused = true)]
	async fn too_many_waiters_per_cid_yields_overloaded() {
		let rig = empty_rig();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [0x77; 32]);

		let mut receivers = Vec::new();
		for _ in 0..MAX_WAITERS_PER_CID {
			receivers.push(rig.user_handle.request_stream(vec![cid]).await.unwrap());
		}
		// Give the actor a chance to admit all waiters before the one-too-many request.
		sleep(Duration::from_millis(10)).await;

		let mut rejected = rig.user_handle.request_stream(vec![cid]).await.unwrap();
		let item = drain_next(&mut rejected).await.expect("item");
		assert!(matches!(item, Err(BitswapError::Overloaded)));
	}
}

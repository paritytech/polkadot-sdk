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

//! Bitswap service.

use super::{
	is_cid_supported, BitswapCommand, BitswapHandle, Cid, FetchItem, LOG_TARGET, MAX_WANTED_BLOCKS,
};
use crate::{
	handle::BitswapError,
	metrics::{
		errors as metric_errors, outbound_events, outcomes as metric_outcomes, BitswapMetrics,
	},
};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle as LitepBitswapHandle, BlockPresenceType, ResponseType, WantType,
};
use prometheus_endpoint::Registry;
use rand::{seq::IteratorRandom, Rng};
use sc_client_api::BlockBackend;
use sc_network_common::role::Roles;
use sc_network_sync::{SyncEvent, SyncEventStream};
use slotmap::{new_key_type, SlotMap};
use smallvec::SmallVec;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
	future::Future,
	pin::Pin,
	sync::Arc,
	time::Duration,
};
use tokio::{
	sync::{mpsc, OwnedSemaphorePermit, Semaphore},
	time::Instant,
};

/// Transport boundary used by the Bitswap actor.
/// Implementations must verify block bytes before emitting `VerifiedBlock`.
#[async_trait]
trait BitswapTransport: Send {
	async fn next_event(&mut self) -> Option<TransportEvent>;
	async fn send_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>);
	async fn send_response(&self, peer: litep2p::PeerId, responses: Vec<ResponseType>);
}

enum TransportEvent {
	Request { peer: litep2p::PeerId, cids: Vec<(Cid, WantType)> },
	Response { peer: litep2p::PeerId, responses: Vec<TransportResponse> },
}

enum TransportResponse {
	/// A block whose CID is guaranteed by the transport to match `bytes`. litep2p derives
	/// the CID from the received bytes (the wire format only carries a CID prefix), so
	/// the pairing needs no further verification; any other transport must uphold this.
	VerifiedBlock {
		cid: Cid,
		bytes: Vec<u8>,
	},
	Presence {
		cid: Cid,
		presence: BlockPresenceType,
	},
}

#[async_trait]
impl BitswapTransport for LitepBitswapHandle {
	async fn next_event(&mut self) -> Option<TransportEvent> {
		StreamExt::next(self).await.map(|event| match event {
			BitswapEvent::Request { peer, cids } => TransportEvent::Request { peer, cids },
			BitswapEvent::Response { peer, responses } => TransportEvent::Response {
				peer,
				responses: responses
					.into_iter()
					.map(|response| match response {
						ResponseType::Block { cid, block } => {
							TransportResponse::VerifiedBlock { cid, bytes: block }
						},
						ResponseType::Presence { cid, presence } => {
							TransportResponse::Presence { cid, presence }
						},
					})
					.collect(),
			},
		})
	}

	async fn send_request(&self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		LitepBitswapHandle::send_request(self, peer, cids).await
	}

	async fn send_response(&self, peer: litep2p::PeerId, responses: Vec<ResponseType>) {
		LitepBitswapHandle::send_response(self, peer, responses).await
	}
}

const MAX_LIVE_CIDS: usize = 1024;
const MAX_WAITERS_PER_CID: usize = 64;
const MAX_CONCURRENT_INBOUND_LOOKUPS: usize = 8;
const MAX_QUEUED_INBOUND_ENTRIES_PER_PEER: usize = MAX_LIVE_CIDS;
const CMD_CHANNEL_CAPACITY: usize = 256;
const LOOKUP_CHANNEL_CAPACITY: usize = MAX_CONCURRENT_INBOUND_LOOKUPS;
const PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
const ROUND_RETRY_DELAY: Duration = Duration::from_secs(5);
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

new_key_type! { struct WaiterId; }

#[derive(Clone, Copy, Default)]
enum CidPhase {
	#[default]
	Ready,
	Queued {
		retry_at: Option<Instant>,
	},
	InFlight {
		peer: litep2p::PeerId,
		deadline: Instant,
	},
	RetryAt(Instant),
}

#[derive(Default)]
struct CidState {
	tried_peers: HashSet<litep2p::PeerId>,
	have_peers: SmallVec<[litep2p::PeerId; 1]>,
	waiters: SmallVec<[WaiterId; 2]>,
	phase: CidPhase,
}

impl CidState {
	fn has_waiters(&self) -> bool {
		!self.waiters.is_empty()
	}

	fn is_idle(&self) -> bool {
		self.waiters.is_empty() && !matches!(self.phase, CidPhase::InFlight { .. })
	}

	fn is_queued(&self) -> bool {
		matches!(self.phase, CidPhase::Queued { .. })
	}

	/// Moves a ready CID into the pending queue while preserving its retry deadline.
	/// Returns `false` when the CID is already queued or in flight.
	fn queue(&mut self) -> bool {
		let retry_at = match self.phase {
			CidPhase::Ready => None,
			CidPhase::RetryAt(at) => Some(at),
			CidPhase::Queued { .. } | CidPhase::InFlight { .. } => return false,
		};
		self.phase = CidPhase::Queued { retry_at };
		true
	}

	/// Removes a CID from the pending queue and restores its prior scheduling phase.
	/// Returns `false` when the state was not queued.
	fn dequeue(&mut self) -> bool {
		let CidPhase::Queued { retry_at } = self.phase else { return false };
		self.phase = retry_at.map_or(CidPhase::Ready, CidPhase::RetryAt);
		true
	}

	/// Finishes the active request only when `peer` currently owns it.
	/// The return value indicates whether an in-flight slot was released.
	fn finish_peer(&mut self, peer: litep2p::PeerId) -> bool {
		let CidPhase::InFlight { peer: active, .. } = self.phase else { return false };
		if active != peer {
			return false;
		}
		self.phase = CidPhase::Ready;
		true
	}

	/// Parks an exhausted CID until `at`, including while it waits in the queue.
	/// Returns `false` when a retry is already scheduled or work remains in flight.
	fn schedule_retry(&mut self, at: Instant) -> bool {
		match &mut self.phase {
			CidPhase::Ready => self.phase = CidPhase::RetryAt(at),
			CidPhase::Queued { retry_at: slot @ None } => *slot = Some(at),
			CidPhase::Queued { retry_at: Some(_) } |
			CidPhase::InFlight { .. } |
			CidPhase::RetryAt(_) => return false,
		}
		true
	}

	/// Starts a new peer-selection round once the retry deadline has elapsed.
	/// Peer history is cleared so every connected peer becomes eligible again.
	fn restart_round(&mut self, now: Instant) -> bool {
		self.phase = match self.phase {
			CidPhase::RetryAt(at) if at <= now => CidPhase::Ready,
			CidPhase::Queued { retry_at: Some(at) } if at <= now => {
				CidPhase::Queued { retry_at: None }
			},
			_ => return false,
		};
		self.tried_peers.clear();
		self.have_peers.clear();
		true
	}
}

struct WantSet {
	inner: HashMap<Cid, CidState>,
	/// FIFO of CIDs waiting for a free dispatch-window slot. May contain stale entries
	/// (resolved, abandoned or already-dispatched CIDs); those are skipped on pop, guarded
	/// by [`CidState::is_queued`].
	pending: VecDeque<Cid>,
	/// Number of CIDs with at least one in-flight peer request.
	live: usize,
	/// Number of queued [`CidState`] entries, maintained incrementally: the
	/// metrics path reads it after every actor event, so recomputing it by scanning
	/// `inner` would make large requests quadratic.
	queued: usize,
	/// Dispatch-window size.
	max_live: usize,
}

impl WantSet {
	fn new(max_live: usize) -> Self {
		Self { inner: HashMap::new(), pending: VecDeque::new(), live: 0, queued: 0, max_live }
	}

	fn contains(&self, cid: &Cid) -> bool {
		self.inner.contains_key(cid)
	}

	fn waiter_count(&self, cid: &Cid) -> usize {
		self.inner.get(cid).map_or(0, |state| state.waiters.len())
	}

	fn add_waiter(&mut self, cid: Cid, waiter: WaiterId) {
		self.inner.entry(cid).or_default().waiters.push(waiter);
	}

	/// Detaches a user request from `cid` and removes newly idle state.
	/// In-flight state remains until its response, timeout, or disconnect arrives.
	fn remove_waiter(&mut self, cid: Cid, waiter: WaiterId) {
		if let Some(state) = self.inner.get_mut(&cid) {
			state.waiters.retain(|w| *w != waiter);
		}
		self.remove_if_idle(cid);
	}

	fn all_cids(&self) -> Vec<Cid> {
		self.inner.keys().copied().collect()
	}

	/// Removes a delivered CID and returns every waiter that should receive it.
	/// Scheduler counters are released according to the CID's previous phase.
	fn take_waiters_for_delivered_cid(&mut self, cid: Cid) -> Option<SmallVec<[WaiterId; 2]>> {
		self.inner.remove(&cid).map(|state| {
			if matches!(state.phase, CidPhase::InFlight { .. }) {
				self.live -= 1;
			}
			if state.is_queued() {
				self.queued -= 1;
			}
			state.waiters
		})
	}

	fn has_window_capacity(&self) -> bool {
		self.live < self.max_live
	}

	/// Pops the next still-valid queued CID and updates its queue accounting.
	/// Stale FIFO entries are skipped until a queued state is found.
	fn pop_pending(&mut self) -> Option<Cid> {
		while let Some(cid) = self.pending.pop_front() {
			if let Some(state) = self.inner.get_mut(&cid) {
				if state.dequeue() {
					self.queued -= 1;
					return Some(cid);
				}
			}
		}
		None
	}

	/// Selects an eligible peer for `cid`, queueing it when the dispatch window is full.
	/// A selected peer owns one live slot until completion, timeout, or disconnect.
	fn next_peer_to_request<R: Rng + ?Sized>(
		&mut self,
		cid: Cid,
		connected_peers: &HashSet<litep2p::PeerId>,
		now: Instant,
		rng: &mut R,
	) -> Option<litep2p::PeerId> {
		let has_window_capacity = self.has_window_capacity();
		let state = self.inner.get_mut(&cid)?;
		if !state.has_waiters() || matches!(state.phase, CidPhase::InFlight { .. }) {
			return None;
		}

		if !has_window_capacity {
			// Preserve the CID for promotion when a dispatch slot opens.
			if state.queue() {
				self.pending.push_back(cid);
				self.queued += 1;
			}
			return None;
		}

		// Prefer peers that advertised HAVE before falling back to any untried peer.
		let eligible = |peer: &litep2p::PeerId| !state.tried_peers.contains(peer);
		let Some(peer) = state
			.have_peers
			.iter()
			.filter(|peer| connected_peers.contains(*peer) && eligible(peer))
			.choose(&mut *rng)
			.or_else(|| connected_peers.iter().filter(|peer| eligible(peer)).choose(&mut *rng))
			.copied()
		else {
			if !connected_peers.is_empty() {
				// Retry later once every connected peer has been tried.
				if state.schedule_retry(now + ROUND_RETRY_DELAY) {
					log::trace!(
						target: LOG_TARGET,
						"all peers tried for {cid}, scheduling new round",
					);
				}
			}
			return None;
		};

		if state.is_queued() {
			self.queued -= 1;
		}
		self.live += 1;
		state.phase = CidPhase::InFlight { peer, deadline: now + PER_PEER_TIMEOUT };

		Some(peer)
	}

	/// Records a terminal response from `peer` and releases its active slot.
	/// Late responses cannot finish a newer request owned by a different peer.
	fn mark_peer_done_for_cid(&mut self, peer: litep2p::PeerId, cid: Cid) {
		if let Some(state) = self.inner.get_mut(&cid) {
			if state.finish_peer(peer) {
				self.live -= 1;
			}
			state.tried_peers.insert(peer);
		}
		self.remove_if_idle(cid);
	}

	/// Records that `peer` has a CID and releases its active request.
	/// A repeated `HAVE` marks that peer tried so another peer is selected next.
	fn note_peer_have_for_cid(&mut self, peer: litep2p::PeerId, cid: Cid) {
		if let Some(state) = self.inner.get_mut(&cid) {
			if state.have_peers.contains(&peer) {
				state.tried_peers.insert(peer);
			} else {
				state.have_peers.push(peer);
			}
			if state.finish_peer(peer) {
				self.live -= 1;
			}
		}
		self.remove_if_idle(cid);
	}

	/// Releases every active request owned by a disconnected peer.
	/// Returned CIDs are still wanted and ready for immediate failover.
	fn remove_in_flight_peer(&mut self, peer: litep2p::PeerId) -> Vec<Cid> {
		let mut affected = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			if state.finish_peer(peer) {
				self.live -= 1;
				affected.push(*cid);
			}
		}

		self.remove_idle_and_filter_existing(affected)
	}

	/// Expire in-flight requests whose per-peer deadline passed. Returns the affected CIDs
	/// still wanted (for re-dispatch) and the total number of timed-out peer requests.
	fn expire_peer_timeouts(&mut self, now: Instant) -> (Vec<Cid>, usize) {
		let mut timed_out = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			if let CidPhase::InFlight { peer, deadline } = state.phase {
				if deadline <= now {
					state.phase = CidPhase::Ready;
					state.tried_peers.insert(peer);
					timed_out.push(*cid);
				}
			}
		}
		let timed_out_count = timed_out.len();
		self.live -= timed_out_count;

		(self.remove_idle_and_filter_existing(timed_out), timed_out_count)
	}

	/// Restarts every exhausted CID whose retry delay has elapsed.
	/// Returned CIDs should be reconsidered for immediate dispatch.
	fn restart_exhausted_rounds(&mut self, now: Instant) -> Vec<Cid> {
		let mut cids = Vec::new();
		for (cid, state) in self.inner.iter_mut() {
			if state.has_waiters() && state.restart_round(now) {
				cids.push(*cid);
			}
		}
		cids
	}

	fn clear(&mut self) {
		self.inner.clear();
		self.pending.clear();
		self.live = 0;
		self.queued = 0;
	}

	/// Removes idle states from `cids` and returns those still tracked.
	/// The filtered result is safe to feed back into peer selection.
	fn remove_idle_and_filter_existing(&mut self, cids: Vec<Cid>) -> Vec<Cid> {
		for cid in &cids {
			self.remove_if_idle(*cid);
		}
		cids.into_iter().filter(|cid| self.inner.contains_key(cid)).collect()
	}

	fn remove_if_idle(&mut self, cid: Cid) {
		let Entry::Occupied(entry) = self.inner.entry(cid) else { return };
		if !entry.get().is_idle() {
			return;
		}
		let state = entry.remove();
		if state.is_queued() {
			self.queued -= 1;
		}
	}
}

struct Waiter {
	cids_remaining: HashSet<Cid>,
	sink: mpsc::Sender<FetchItem>,
}

/// A served batch, carrying the worker permit: the slot is released only once the actor
/// has forwarded the responses, so downstream backpressure reaches the dispatch path and
/// at most [`MAX_CONCURRENT_INBOUND_LOOKUPS`] responses are retained at any time.
type InboundLookupResult = (litep2p::PeerId, Vec<ResponseType>, OwnedSemaphorePermit);

struct InboundLookupPool<B: BlockT> {
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	result_tx: mpsc::Sender<InboundLookupResult>,
	semaphore: Arc<Semaphore>,
	metrics: BitswapMetrics,
}

impl<B: BlockT> InboundLookupPool<B> {
	fn new(
		client: Arc<dyn BlockBackend<B> + Send + Sync>,
		max_lookups: usize,
		metrics: BitswapMetrics,
	) -> (Self, mpsc::Receiver<InboundLookupResult>) {
		let (result_tx, result_rx) = mpsc::channel(LOOKUP_CHANNEL_CAPACITY);
		(
			Self { client, result_tx, semaphore: Arc::new(Semaphore::new(max_lookups)), metrics },
			result_rx,
		)
	}

	/// Reserves a worker slot, or returns `None` if all workers are busy.
	/// Dropping an unused permit immediately returns the slot.
	fn try_acquire_worker(&self) -> Option<OwnedSemaphorePermit> {
		self.semaphore.clone().try_acquire_owned().ok()
	}

	/// Serves a wantlist on a blocking worker occupying `permit`'s slot.
	/// The permit travels with the result so backpressure retains the slot.
	fn submit(
		&self,
		permit: OwnedSemaphorePermit,
		peer: litep2p::PeerId,
		cids: Vec<(Cid, WantType)>,
	) {
		let client = self.client.clone();
		let result_tx = self.result_tx.clone();
		let metrics = self.metrics.clone();
		tokio::task::spawn_blocking(move || {
			let responses = serve_inbound(&*client, cids, &metrics);
			let _ = result_tx.blocking_send((peer, responses, permit));
		});
	}
}

/// Fair per-peer queues. Overflow is dropped rather than reported as `DONT_HAVE`.
struct InboundQueue {
	per_peer: HashMap<litep2p::PeerId, VecDeque<(Cid, WantType)>>,
	rotation: VecDeque<litep2p::PeerId>,
	max_entries_per_peer: usize,
}

impl InboundQueue {
	fn new(max_entries_per_peer: usize) -> Self {
		Self { per_peer: HashMap::new(), rotation: VecDeque::new(), max_entries_per_peer }
	}

	/// Queue `entries` for `peer`. Returns the number of entries dropped because the
	/// peer's queue is full.
	fn enqueue(&mut self, peer: litep2p::PeerId, mut entries: Vec<(Cid, WantType)>) -> usize {
		if entries.is_empty() {
			return 0;
		}
		let queue = self.per_peer.entry(peer).or_default();
		let free = self.max_entries_per_peer.saturating_sub(queue.len());
		let dropped = entries.len().saturating_sub(free);
		entries.truncate(free);
		// Enter the rotation only on the empty-to-nonempty transition.
		if queue.is_empty() && !entries.is_empty() {
			self.rotation.push_back(peer);
		}
		queue.extend(entries);
		if queue.is_empty() {
			self.per_peer.remove(&peer);
		}
		dropped
	}

	/// Take the next batch of up to [`MAX_WANTED_BLOCKS`] entries, rotating the serviced
	/// peer to the back of the queue.
	fn next_batch(&mut self) -> Option<(litep2p::PeerId, Vec<(Cid, WantType)>)> {
		while let Some(peer) = self.rotation.pop_front() {
			let Some(queue) = self.per_peer.get_mut(&peer) else {
				log::error!(target: LOG_TARGET, "stale peer in inbound queue rotation: {peer}");
				continue;
			};
			if queue.is_empty() {
				log::error!(target: LOG_TARGET, "empty peer queue in inbound rotation: {peer}");
				self.per_peer.remove(&peer);
				continue;
			}

			let batch: Vec<_> = queue.drain(..queue.len().min(MAX_WANTED_BLOCKS)).collect();
			if queue.is_empty() {
				self.per_peer.remove(&peer);
			} else {
				self.rotation.push_back(peer);
			}
			return Some((peer, batch));
		}
		None
	}
}

pub(crate) struct BitswapService<B: BlockT> {
	handle: Box<dyn BitswapTransport>,

	cmd_rx: mpsc::Receiver<BitswapCommand>,
	cmd_channel_closed: bool,
	sync_event_stream: Pin<Box<dyn Stream<Item = SyncEvent> + Send>>,
	inbound_lookup_pool: InboundLookupPool<B>,
	inbound_lookup_rx: mpsc::Receiver<InboundLookupResult>,
	inbound_queue: InboundQueue,

	connected_peers: HashSet<litep2p::PeerId>,
	wants: WantSet,
	waiters: SlotMap<WaiterId, Waiter>,
	metrics: BitswapMetrics,
}

/// Build the Bitswap service, returning the service future and the user-facing handle.
pub fn start<B: BlockT, S>(
	client: Arc<dyn BlockBackend<B> + Send + Sync>,
	sync: &S,
	litep2p_handle: LitepBitswapHandle,
	metrics_registry: Option<&Registry>,
) -> (Pin<Box<dyn Future<Output = ()> + Send>>, BitswapHandle)
where
	S: SyncEventStream + ?Sized,
{
	let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);
	let metrics = BitswapMetrics::new(metrics_registry).unwrap_or_else(|err| {
		log::debug!(target: LOG_TARGET, "failed to register bitswap metrics: {err}");
		BitswapMetrics::default()
	});
	let (inbound_lookup_pool, inbound_lookup_rx) =
		InboundLookupPool::new(client, MAX_CONCURRENT_INBOUND_LOOKUPS, metrics.clone());

	let user_handle = BitswapHandle::new(cmd_tx);
	let sync_event_stream = sync.event_stream("bitswap");

	let service = BitswapService {
		handle: Box::new(litep2p_handle),
		cmd_rx,
		cmd_channel_closed: false,
		sync_event_stream,
		inbound_lookup_pool,
		inbound_lookup_rx,
		inbound_queue: InboundQueue::new(MAX_QUEUED_INBOUND_ENTRIES_PER_PEER),
		connected_peers: HashSet::new(),
		wants: WantSet::new(MAX_LIVE_CIDS),
		waiters: SlotMap::with_key(),
		metrics,
	};

	let future = Box::pin(async move { service.run().await });

	(future, user_handle)
}

impl<B: BlockT> BitswapService<B> {
	/// Runs the service actor until a required input stream closes.
	/// Each event is handled serially before metrics are refreshed.
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "BitswapService starting");
		let mut sweep_ticker = tokio::time::interval(SWEEP_INTERVAL);
		sweep_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		sweep_ticker.tick().await;
		self.update_metrics();

		loop {
			tokio::select! {
				event = self.handle.next_event() => match event {
					Some(TransportEvent::Request { peer, cids }) =>
						self.on_inbound_request(peer, cids),
					Some(TransportEvent::Response { peer, responses }) =>
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

				Some((peer, responses, permit)) = self.inbound_lookup_rx.recv() => {
					self.handle.send_response(peer, responses).await;
					drop(permit);
					self.dispatch_inbound_lookups();
				},

				_ = sweep_ticker.tick() => {
					self.on_sweep().await;
				},
			}
			self.update_metrics();
		}
	}

	/// Admits a user wantlist and attaches one waiter to all requested CIDs.
	/// Requests exceeding the per-CID waiter limit are rejected as overloaded.
	async fn on_request_stream(&mut self, cids: Vec<Cid>, sink: mpsc::Sender<FetchItem>) {
		for cid in &cids {
			if self.wants.waiter_count(cid) >= MAX_WAITERS_PER_CID {
				self.metrics.record_outbound(outbound_events::OVERLOADED, 1);
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

	/// Schedules the supplied CIDs, then promotes queued CIDs until the dispatch window is full.
	/// Eligible wants are grouped by peer and sent in protocol-sized batches.
	async fn top_up_in_flight(&mut self, cids: impl IntoIterator<Item = Cid>) {
		let now = Instant::now();
		let by_peer = {
			let mut rng = rand::thread_rng();
			let mut by_peer: HashMap<litep2p::PeerId, Vec<(Cid, WantType)>> = HashMap::new();
			for cid in cids {
				if let Some(peer) =
					self.wants.next_peer_to_request(cid, &self.connected_peers, now, &mut rng)
				{
					log::trace!(target: LOG_TARGET, "WANT-BLOCK {cid} -> {peer:?}");
					by_peer.entry(peer).or_default().push((cid, WantType::Block));
				}
			}

			// Fill remaining dispatch slots from the pending FIFO.
			while self.wants.has_window_capacity() {
				let Some(cid) = self.wants.pop_pending() else { break };
				if let Some(peer) =
					self.wants.next_peer_to_request(cid, &self.connected_peers, now, &mut rng)
				{
					log::trace!(target: LOG_TARGET, "WANT-BLOCK {cid} -> {peer:?} (promoted)");
					by_peer.entry(peer).or_default().push((cid, WantType::Block));
				}
			}
			by_peer
		};

		for (peer, wants) in by_peer {
			self.metrics.record_outbound(outbound_events::REQUESTED, wants.len());
			for chunk in wants.chunks(MAX_WANTED_BLOCKS) {
				self.handle.send_request(peer, chunk.to_vec()).await;
			}
		}
	}

	/// Applies verified blocks and presence updates received from a peer.
	/// Unresolved presence responses are immediately reconsidered for dispatch.
	async fn on_inbound_response(
		&mut self,
		peer: litep2p::PeerId,
		responses: Vec<TransportResponse>,
	) {
		let mut cids_to_top_up: HashSet<Cid> = HashSet::new();

		for response in responses {
			match response {
				TransportResponse::VerifiedBlock { cid, bytes } => {
					// A late verified block may satisfy a want reassigned to another peer.
					self.wants.mark_peer_done_for_cid(peer, cid);

					if self.wants.contains(&cid) {
						self.deliver_block(cid, bytes);
					} else {
						log::debug!(
							target: LOG_TARGET,
							"{peer:?} sent unsolicited or unwanted block for {cid}",
						);
					}
				},
				TransportResponse::Presence { cid, presence } => {
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

	/// Delivers a resolved block to every waiter sharing its CID.
	/// The CID is removed once, releasing any replacement in-flight request.
	fn deliver_block(&mut self, cid: Cid, bytes: Vec<u8>) {
		let Some(waiter_ids) = self.wants.take_waiters_for_delivered_cid(cid) else { return };
		self.metrics.record_outbound(outbound_events::DELIVERED, 1);

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

	/// Removes a waiter and detaches it from every unresolved CID.
	/// CID state is retained only while another waiter or request needs it.
	fn drop_waiter(&mut self, id: WaiterId) {
		let Some(waiter) = self.waiters.remove(id) else { return };

		for cid in &waiter.cids_remaining {
			self.wants.remove_waiter(*cid, id);
		}
	}

	/// Adds a connected non-light peer and reconsiders every outstanding CID.
	/// Light peers are ignored because they do not store indexed transactions.
	async fn on_peer_connected(&mut self, peer: litep2p::PeerId, roles: Roles) {
		if roles.is_light() {
			return;
		}
		self.connected_peers.insert(peer);
		let cids = self.wants.all_cids();
		self.top_up_in_flight(cids).await;
	}

	/// Removes a peer and immediately fails over requests it owned.
	/// Late responses remain safe because peer ownership is checked on completion.
	async fn on_peer_disconnected(&mut self, peer: litep2p::PeerId) {
		self.connected_peers.remove(&peer);
		let cids_to_top_up = self.wants.remove_in_flight_peer(peer);
		self.top_up_in_flight(cids_to_top_up).await;
	}

	/// Periodic housekeeping: drop waiters whose receiver was dropped, expire per-peer request
	/// timeouts, and start new rounds for CIDs whose retry delay has passed.
	async fn on_sweep(&mut self) {
		let abandoned: Vec<WaiterId> = self
			.waiters
			.iter()
			.filter_map(|(id, waiter)| waiter.sink.is_closed().then_some(id))
			.collect();
		self.metrics.record_outbound(outbound_events::ABANDONED, abandoned.len());
		for id in abandoned {
			log::trace!(target: LOG_TARGET, "dropping abandoned waiter {id:?}");
			self.drop_waiter(id);
		}

		let now = Instant::now();
		let (mut cids, timed_out) = self.wants.expire_peer_timeouts(now);
		self.metrics.record_outbound(outbound_events::TIMED_OUT, timed_out);
		let restarted = self.wants.restart_exhausted_rounds(now);
		self.metrics.record_outbound(outbound_events::ROUND_RESTARTED, restarted.len());
		cids.extend(restarted);
		self.top_up_in_flight(cids).await;

		// Recover capacity after a lookup task exits without a result.
		self.dispatch_inbound_lookups();
	}

	/// Queues an inbound peer wantlist and records entries dropped by backpressure.
	/// Available lookup workers are dispatched immediately after admission.
	fn on_inbound_request(&mut self, peer: litep2p::PeerId, cids: Vec<(Cid, WantType)>) {
		let dropped = self.inbound_queue.enqueue(peer, cids);
		if dropped > 0 {
			self.metrics.record_entries(metric_outcomes::DROPPED_OVERFLOW, dropped);
			log::debug!(
				target: LOG_TARGET,
				"inbound queue for {peer:?} full; dropped {dropped} wantlist entries",
			);
		}
		self.dispatch_inbound_lookups();
	}

	/// Starts queued inbound lookups while worker permits remain available.
	/// Work stays queued when either the pool is full or no batch is ready.
	fn dispatch_inbound_lookups(&mut self) {
		while let Some(permit) = self.inbound_lookup_pool.try_acquire_worker() {
			let Some((peer, batch)) = self.inbound_queue.next_batch() else { break };
			self.inbound_lookup_pool.submit(permit, peer, batch);
		}
	}

	fn update_metrics(&self) {
		self.metrics.set_state(self.wants.live, self.wants.queued, self.waiters.len());
	}

	/// Notifies every waiter that the service closed and clears scheduler state.
	/// Send failures are ignored because the corresponding receiver already vanished.
	fn shutdown_waiters(&mut self) {
		for (_, waiter) in self.waiters.drain() {
			let _ = waiter.sink.try_send(Err(BitswapError::ServiceClosed));
		}
		self.wants.clear();
		self.update_metrics();
	}
}

/// Resolves an inbound wantlist against locally indexed transactions.
/// Unsupported CIDs and missing transactions become `DONT_HAVE` responses.
fn serve_inbound<B: BlockT>(
	client: &(dyn BlockBackend<B> + Send + Sync),
	cids: Vec<(Cid, WantType)>,
	metrics: &BitswapMetrics,
) -> Vec<ResponseType> {
	let started = std::time::Instant::now();
	let responses = cids
		.into_iter()
		.map(|(cid, want_type)| {
			if !is_cid_supported(&cid) {
				metrics.record_entry(metric_outcomes::UNSUPPORTED_CID);
				return ResponseType::Presence { cid, presence: BlockPresenceType::DontHave };
			}
			// Supported CIDs always carry a 32-byte digest.
			let hash = H256::from_slice(&cid.hash().digest()[0..32]);
			let transaction = match client.indexed_transaction(hash) {
				Ok(t) => t,
				Err(e) => {
					metrics.record_error(metric_errors::CLIENT);
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
		.collect::<Vec<_>>();
	metrics.record_responses(&responses);
	metrics.record_duration(started.elapsed());
	responses
}

#[cfg(test)]
mod tests;

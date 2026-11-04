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

//! The receiver, inclusion tier only:
//!
//! - [`Receiver::on_root`]: under a source's newest included root, fetch channel data and `Ack`
//!   heads, and rebind every unfinalized endpoint to it.
//! - [`Receiver::inherent_data`]: consume within a [`Budget`], only items already liftable.
//! - [`Receiver::on_block`]: note a block's record and fetch the leaves for a read-context gap.
//! - [`Receiver::assemble`]: one lift per recorded stream, checked with `build_requires_entry`.

use crate::{
	fetch::{fetch, RequestError, Transport},
	pool::{Binding, HeadRead, SourcePool},
	rebase::rebase,
	LOG_TARGET,
};
use codec::{DecodeAll, MaxEncodedLen};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	build_requires_entry, leaf_hash, mmr::SpecMerge, BroadcastId, ConsumedStreams,
	ConsumptionRecord, FetchError, InChannelId, Interval, LiftsBySource, LossyRead,
	MMRExtensionProof, MessagePosition, MessagesRequest, MessagesResponse, MessagingInherentData,
	MmrFrontier, MmrRoot, OutChannelId, Register, RequiresLift, SourceStreams, StreamId,
	StreamProof, StreamsRoot, MAX_SPECULATIVE_MESSAGE_LEN,
};
use futures::FutureExt;
use mmr_lib::{
	helper::{get_peaks, leaf_index_to_mmr_size},
	MMRStoreReadOps, MMRStoreWriteOps, MMR,
};
use parking_lot::Mutex;
use polkadot_core_primitives::Hash;
use sc_network::PeerId;
use std::{
	cell::RefCell,
	collections::{BTreeMap, HashMap},
	fmt::Debug,
	hash::Hash as StdHash,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};

/// Payload bytes asked for per channel fetch: what a server serves at most.
pub const FETCH_MAX_BYTES: u32 = crate::protocol::MAX_PAYLOAD_BYTES;

/// Payloads buffered per channel beyond its cursor before fetching pauses.
pub const MAX_BUFFERED_PAYLOADS: usize = 4096;

/// Payload bytes buffered per channel beyond its cursor before fetching pauses. A sender that
/// ignores our grant could otherwise send faster than the budget consumes and fill memory.
pub const MAX_BUFFERED_BYTES: u64 = 8 * 1024 * 1024;

/// How long building the inherent waits for a missing binding before it drops the item.
pub const INHERENT_FETCH_TIMEOUT: Duration = Duration::from_millis(500);

/// Per-block consumption limits. `enact_messages` is a mandatory inherent, so its cost must fit the
/// block whatever arrived. `max_streams` is clamped to the runtime's `max_touched_streams()`
/// ([`ChainView::max_touched_streams`]): an inherent over it fails the block.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
	/// Streams touched per block: channel items and register reads.
	pub max_streams: u32,
	/// Register reads per block.
	pub max_reads: u32,
	/// Payloads per block.
	pub max_payloads: u32,
	/// Payload bytes per block.
	pub max_bytes: u64,
}

impl Default for Budget {
	/// Sized against the benchmarked per-payload and enqueue weights.
	fn default() -> Self {
		Self { max_streams: 16, max_reads: 8, max_payloads: 256, max_bytes: 256 * 1024 }
	}
}

/// What the chain consumes from one source at some block.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct SourceWants {
	/// The source's channels to us, with our stored frontier of each.
	pub channels: BTreeMap<InChannelId, MmrFrontier>,
	/// Our channels to the source, with the cursor of the register to read from its `Ack` stream.
	pub registers: BTreeMap<OutChannelId, MessagePosition>,
	/// The source's broadcasts we follow, with their cursors. Not fetched yet.
	pub events: BTreeMap<BroadcastId, MessagePosition>,
}

/// The chain's consumption view at a block, per source.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ChainView {
	/// What is consumed from each source.
	pub sources: BTreeMap<ParaId, SourceWants>,
	/// The runtime's cap on streams touched per block (`max_touched_streams()`).
	pub max_touched_streams: u32,
}

impl ChainView {
	/// The view from `consumed_streams()` and `max_touched_streams()`: channels and broadcasts
	/// are grouped by their sender, registers by their recipient.
	pub fn new(consumed: ConsumedStreams, max_touched_streams: u32) -> Self {
		let mut sources: BTreeMap<ParaId, SourceWants> = BTreeMap::new();
		for (id, frontier) in consumed.channels {
			sources.entry(id.sender).or_default().channels.insert(id, frontier);
		}
		for (id, cursor) in consumed.registers {
			sources.entry(id.recipient).or_default().registers.insert(id, cursor);
		}
		for (id, cursor) in consumed.events {
			sources.entry(id.sender).or_default().events.insert(id, cursor);
		}
		Self { sources, max_touched_streams }
	}
}

/// Wraps a transport so the peer asked first changes every round: one unresponsive peer then
/// delays only the rounds it leads.
struct Rotating<T: ?Sized> {
	inner: Arc<T>,
	rounds: Mutex<BTreeMap<ParaId, usize>>,
}

impl<T: ?Sized> Rotating<T> {
	fn next_round(&self, source: ParaId) {
		*self.rounds.lock().entry(source).or_default() += 1;
	}
}

#[async_trait::async_trait]
impl<T: Transport + ?Sized> Transport for Rotating<T> {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		let mut peers = self.inner.peers(source);
		if !peers.is_empty() {
			let round = self.rounds.lock().get(&source).copied().unwrap_or_default();
			let by = round % peers.len();
			peers.rotate_left(by);
		}
		peers
	}

	async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
		self.inner.request(peer, request).await
	}
}

/// An unfinalized block's consumption.
struct LiveBlock<H> {
	number: u64,
	parent: H,
	record: ConsumptionRecord,
}

struct State<H> {
	sources: BTreeMap<ParaId, SourcePool>,
	live: HashMap<H, LiveBlock<H>>,
}

impl<H: Copy + Eq + StdHash> State<H> {
	/// The interval of the nearest live block at or above `from` that touched `stream`.
	fn nearest(&self, mut from: H, source: &ParaId, stream: &StreamId) -> Option<&Interval> {
		for _ in 0..self.live.len() {
			let block = self.live.get(&from)?;
			let touched = block.record.entries.get(source).and_then(|streams| {
				streams
					.iter()
					.find(|(touched, _)| touched == stream)
					.map(|(_, interval)| interval)
			});
			if touched.is_some() {
				return touched;
			}
			from = block.parent;
		}
		None
	}

	/// The `(stream, leaf count)` endpoints of `source` in live blocks, with their frontiers.
	fn live_endpoints(&self, source: &ParaId) -> BTreeMap<(StreamId, u64), MmrFrontier> {
		self.live
			.values()
			.filter_map(|block| block.record.entries.get(source))
			.flatten()
			.map(|(stream, interval)| ((*stream, interval.end.leaf_count()), interval.end.clone()))
			.collect()
	}
}

/// What a planned item puts in the inherent.
enum Item {
	Channel(InChannelId, Vec<Vec<u8>>),
	Register(OutChannelId, LossyRead),
}

/// One planned inherent item and the endpoint it creates.
struct Planned {
	source: ParaId,
	stream: StreamId,
	item: Item,
	endpoint: MmrFrontier,
	root: StreamsRoot,
}

/// A gap between two reads of a stream, and where its leaves come from.
pub(crate) struct Gap {
	source: ParaId,
	stream: StreamId,
	from: MmrFrontier,
	to_start: MmrRoot,
	to_count: u64,
	under: StreamsRoot,
}

/// The receiver side of chain `us`, fetching over `T`. `H` is the chain's block hash.
pub struct Receiver<H, T: ?Sized> {
	us: ParaId,
	budget: Budget,
	transport: Rotating<T>,
	state: Mutex<State<H>>,
	/// Where the next block starts handing out budget.
	rotation: AtomicUsize,
}

impl<H, T> Receiver<H, T>
where
	H: Copy + Eq + StdHash + Debug,
	T: Transport + ?Sized,
{
	/// A receiver with an empty pool.
	pub fn new(us: ParaId, budget: Budget, transport: Arc<T>) -> Self {
		Self {
			us,
			budget,
			transport: Rotating { inner: transport, rounds: Mutex::new(BTreeMap::new()) },
			state: Mutex::new(State { sources: BTreeMap::new(), live: HashMap::new() }),
			rotation: AtomicUsize::new(0),
		}
	}

	/// The chain this receiver consumes for.
	pub fn para_id(&self) -> ParaId {
		self.us
	}

	/// The newest root seen for `source`.
	pub fn newest_root(&self, source: &ParaId) -> Option<StreamsRoot> {
		self.state.lock().sources.get(source).and_then(SourcePool::newest_root)
	}

	/// Whether the record of block `hash` is noted and not final yet.
	#[cfg(test)]
	pub(crate) fn is_live(&self, hash: &H) -> bool {
		self.state.lock().live.contains_key(hash)
	}

	/// Whether the pool holds the advance of `source`'s `stream` from `from` to `to` leaves.
	#[cfg(test)]
	pub(crate) fn has_advance(
		&self,
		source: &ParaId,
		stream: StreamId,
		from: u64,
		to: u64,
	) -> bool {
		let state = self.state.lock();
		state.sources.get(source).is_some_and(|pool| {
			pool.advances.keys().any(|key| (key.0, key.1, key.2) == (stream, from, to))
		})
	}

	/// Whether everything `wants` names has been fetched under `root`, `source`'s newest root: the
	/// last round had no failed fetch, and the pool holds every channel's payloads from its cursor.
	pub fn is_settled(&self, source: &ParaId, root: &StreamsRoot, wants: &SourceWants) -> bool {
		let state = self.state.lock();
		let Some(pool) = state.sources.get(source) else { return false };
		pool.newest_root().as_ref() == Some(root) &&
			pool.settled &&
			wants.channels.iter().all(|(id, stored)| {
				covers(pool, &id.sender_data_stream(self.us).1, stored.leaf_count())
			})
	}

	/// `root` is `source`'s newest included root: fetch under it what `wants` names, and bind the
	/// live endpoints to it. Under a root already settled this does nothing; under one whose last
	/// round had a failed fetch, it fetches again.
	pub async fn on_root(&self, source: ParaId, root: StreamsRoot, wants: &SourceWants) {
		if self.is_settled(&source, &root, wants) {
			return;
		}
		self.transport.next_round(source);
		self.state.lock().sources.entry(source).or_default().push_root(root);
		log::debug!(target: LOG_TARGET, "New root {root:?} of {source:?}: fetching");
		let started = std::time::Instant::now();
		let mut settled = true;

		for (id, stored) in &wants.channels {
			let stream = id.sender_data_stream(self.us).1;
			let cursor = stored.leaf_count();
			let Some((frontier, max_bytes)) = self.channel_start(source, &stream, stored) else {
				continue;
			};
			let mut start = frontier.leaf_count();
			let mut result = self.fetch_channel(source, stream, root, &frontier, max_bytes).await;
			if start > cursor {
				if let Err(error) = &result {
					if self.held_is_stale(source, stream, stored, root, error).await {
						log::debug!(
							target: LOG_TARGET,
							"{stream:?} of {source:?} under {root:?} is another fork's from \
							 {cursor}: fetching it again",
						);
						// One statement: a guard `drop`ped by hand still makes the future `!Send`.
						self.state
							.lock()
							.sources
							.entry(source)
							.or_default()
							.drop_payloads_from(&stream, cursor);
						start = cursor;
						result =
							self.fetch_channel(source, stream, root, stored, FETCH_MAX_BYTES).await;
					}
				}
			}
			match result {
				Ok(response) => {
					let end = start + response.payloads.len() as u64;
					let reach = match response.extension.leaf_count {
						0 => end,
						count => count,
					};
					let mut state = self.state.lock();
					let pool = state.sources.entry(source).or_default();
					pool.insert_payloads(stream, start, response.payloads.into_inner());
					pool.reach.insert(stream, reach);
					let binding =
						Binding { extension: response.extension, tree_proof: response.tree_proof };
					pool.insert_binding(stream, end, root, binding);
				},
				Err(e) if start == 0 && unknown_stream(&e) => {
					log::trace!(target: LOG_TARGET, "{stream:?} of {source:?} does not exist yet");
					self.state.lock().sources.entry(source).or_default().reach.insert(stream, 0);
				},
				Err(_) => settled = false,
			}
		}

		for (id, cursor) in &wants.registers {
			let stream = id.peer_ack_stream(self.us).1;
			match self.fetch_head(source, stream, root).await {
				Ok((head, tree_proof)) => {
					let mut state = self.state.lock();
					state.sources.entry(source).or_default().insert_head(stream, head, tree_proof);
				},
				Err(e) if cursor.0 == 0 && unknown_stream(&e) => {
					log::trace!(target: LOG_TARGET, "{stream:?} of {source:?} does not exist yet");
				},
				Err(_) => settled = false,
			}
		}

		let unbound: Vec<_> = {
			let state = self.state.lock();
			let pool = state.sources.get(&source);
			state
				.live_endpoints(&source)
				.into_iter()
				.filter(|((stream, count), _)| {
					pool.and_then(|pool| pool.binding(stream, *count, &root)).is_none()
				})
				.collect()
		};
		for ((stream, _), endpoint) in unbound {
			settled &= self.fetch_binding(source, stream, &endpoint, root).await;
		}

		let mut state = self.state.lock();
		if let Some(pool) = state.sources.get_mut(&source) {
			// A newer root may have arrived meanwhile; it settles on its own.
			if pool.newest_root() == Some(root) {
				pool.settled = settled;
			}
		}
		log::debug!(
			target: LOG_TARGET,
			"Root {root:?} of {source:?}: fetch {} in {:?}",
			if settled { "settled" } else { "incomplete" },
			started.elapsed(),
		);
	}

	/// Where the next fetch of the channel `stream` starts: the frontier after the payloads the
	/// pool holds from the cursor, with the payload budget. While the buffer is full the budget is
	/// zero: the fetch only checks that the root's stream still runs through what the pool holds.
	fn channel_start(
		&self,
		source: ParaId,
		stream: &StreamId,
		stored: &MmrFrontier,
	) -> Option<(MmrFrontier, u32)> {
		let state = self.state.lock();
		let cursor = stored.leaf_count();
		let Some(pool) = state.sources.get(&source) else {
			return Some((stored.clone(), FETCH_MAX_BYTES));
		};
		let (count, bytes) = pool.buffered(stream, cursor);
		let max_bytes = if buffer_full(count, bytes) { 0 } else { FETCH_MAX_BYTES };
		Some((extended(stored, pool.contiguous_from(stream, cursor))?, max_bytes))
	}

	/// Fetch the channel `stream` under `root` from `frontier`, with up to `max_bytes` of payloads.
	async fn fetch_channel(
		&self,
		source: ParaId,
		stream: StreamId,
		root: StreamsRoot,
		frontier: &MmrFrontier,
		max_bytes: u32,
	) -> Result<MessagesResponse, RequestError> {
		let request = MessagesRequest {
			stream,
			under: root,
			start: Some(MessagePosition(frontier.leaf_count())),
			max_bytes,
		};
		fetch(&self.transport, source, &request, Some(frontier)).await
	}

	/// Whether the payloads the pool holds of the channel `stream` from `stored` are another
	/// fork's than the stream's under `root`, once the fetch from their end failed with `error`.
	/// Only a response that verifies under `root` tells: one with another frontier where they end,
	/// or a head that ends within them where they do not. A peer that refuses or lies proves
	/// nothing, so it cannot make the pool drop payloads that are the stream's.
	async fn held_is_stale(
		&self,
		source: ParaId,
		stream: StreamId,
		stored: &MmrFrontier,
		root: StreamsRoot,
		error: &RequestError,
	) -> bool {
		let beyond_head = match error {
			RequestError::Diverged => return true,
			RequestError::Unserved { refusals } => {
				refusals.iter().any(|(_, refusal)| *refusal == FetchError::BeyondHead)
			},
			RequestError::NoPeers => false,
		};
		if !beyond_head {
			return false;
		}
		// The stream may end within the payloads held: compare where its head ends.
		let request = MessagesRequest {
			stream,
			under: root,
			start: None,
			max_bytes: MAX_SPECULATIVE_MESSAGE_LEN,
		};
		let Ok(head) = fetch(&self.transport, source, &request, None).await else { return false };
		let Some(known) = extended(&head.frontier, head.payloads.iter()) else { return false };
		let Some(within) = known.leaf_count().checked_sub(stored.leaf_count()) else {
			return false;
		};
		let Ok(within) = usize::try_from(within) else { return false };
		let state = self.state.lock();
		let Some(pool) = state.sources.get(&source) else { return false };
		let held: Vec<_> =
			pool.contiguous_from(&stream, stored.leaf_count()).take(within).collect();
		held.len() == within && extended(stored, held).is_some_and(|expected| expected != known)
	}

	/// The messaging inherent for a block whose parent has `view`. `bundle` holds the records of
	/// the blocks before it in its bundle, oldest first.
	///
	/// The bundle's candidate needs, per source, one root that every stream's last endpoint in
	/// the bundle lifts to. A source's items are planned under its newest root, which may be newer
	/// than the one an earlier block of the bundle used, and that block may have been imported
	/// after the newer root's round bound the live endpoints. So the earlier endpoints are bound
	/// under the planned root as well, and a source whose earlier endpoints stay unbound gets no
	/// item in this block.
	pub async fn inherent_data(
		&self,
		view: &ChainView,
		bundle: &[ConsumptionRecord],
	) -> MessagingInherentData {
		let planned = self.plan(view);
		let roots: BTreeMap<ParaId, StreamsRoot> =
			planned.iter().map(|p| (p.source, p.root)).collect();
		let earlier: Vec<_> = last_endpoints(bundle)
			.into_iter()
			.filter_map(|((source, stream), end)| Some((source, stream, end, *roots.get(&source)?)))
			.collect();
		let bound = |p: &Planned| {
			self.state.lock().sources.get(&p.source).is_some_and(|pool| {
				pool.binding(&p.stream, p.endpoint.leaf_count(), &p.root).is_some()
			})
		};
		let liftable =
			|(source, stream, end, root): &(ParaId, StreamId, MmrFrontier, StreamsRoot)| {
				let state = self.state.lock();
				state.sources.get(source).is_some_and(|pool| lifts_to(pool, stream, end, root))
			};
		// Fetch every missing binding at once, under one deadline: authoring waits at most
		// `INHERENT_FETCH_TIMEOUT`, however many items there are and whatever their peers do.
		{
			let missing: Vec<_> = planned
				.iter()
				.filter(|p| !bound(p))
				.map(|p| (p.source, p.stream, p.endpoint.clone(), p.root))
				.chain(earlier.iter().filter(|e| !liftable(e)).cloned())
				.collect();
			if !missing.is_empty() {
				let fetches = missing.iter().map(|(source, stream, endpoint, root)| {
					self.fetch_binding(*source, *stream, endpoint, *root)
				});
				futures::select! {
					_ = futures::future::join_all(fetches).fuse() => {},
					_ = futures_timer::Delay::new(INHERENT_FETCH_TIMEOUT).fuse() => {},
				}
			}
		}
		let stuck: std::collections::BTreeSet<ParaId> = earlier
			.iter()
			.filter(|e| !liftable(e))
			.map(|(source, stream, end, _)| {
				log::debug!(
					target: LOG_TARGET,
					"Not consuming from {source:?}: the bundle's {stream:?} at {} does not lift to \
					 its newest root",
					end.leaf_count(),
				);
				*source
			})
			.collect();
		let planned = planned.into_iter().filter(|p| !stuck.contains(&p.source));
		let mut data = MessagingInherentData::default();
		for p in planned {
			if !bound(&p) {
				log::debug!(
					target: LOG_TARGET,
					"Not consuming {:?} of {:?}: no lift material at {}",
					p.stream,
					p.source,
					p.endpoint.leaf_count(),
				);
				continue;
			}
			match p.item {
				Item::Channel(id, payloads) => {
					data.channels.insert(id, payloads);
				},
				Item::Register(id, read) => {
					data.registers.insert(id, read);
				},
			}
		}
		if !data.is_empty() {
			log::debug!(target: LOG_TARGET, "Messaging inherent: {} streams", data.len());
		}
		data
	}

	/// Choose the items within the budget, from what the pool holds under each source's newest
	/// root. Register reads go first: one leaf each, so channel backlogs cannot take every stream
	/// slot and stall our own sends. Both lists start from a position that rotates each block, so
	/// no source's backlog takes every block.
	fn plan(&self, view: &ChainView) -> Vec<Planned> {
		let Budget { max_streams, max_reads, max_payloads, max_bytes } = self.budget;
		let max_streams = max_streams.min(view.max_touched_streams);
		let (mut streams, mut reads, mut payloads, mut bytes) = (0u32, 0u32, 0u32, 0u64);
		let mut planned = Vec::new();
		let rotation = self.rotation.fetch_add(1, Ordering::Relaxed);
		let state = self.state.lock();
		let newest = |source: &ParaId| {
			let pool = state.sources.get(source)?;
			Some((pool, pool.newest_root()?))
		};

		let registers: Vec<_> = view
			.sources
			.iter()
			.flat_map(|(source, wants)| {
				wants.registers.iter().map(move |(id, cursor)| (*source, *id, *cursor))
			})
			.collect();
		let (first, rest) = registers.split_at(rotation % registers.len().max(1));
		for (source, id, cursor) in rest.iter().chain(first) {
			if streams >= max_streams || reads >= max_reads {
				break;
			}
			let Some((pool, root)) = newest(source) else { continue };
			let stream = id.peer_ack_stream(self.us).1;
			let Some(head) = pool.heads.get(&stream) else { continue };
			// The pallet rejects the block on a head at or below the applied one, and on an
			// undecodable register.
			if head.frontier.leaf_count() < cursor.0 ||
				Register::decode_all(&mut &head.payload[..]).is_err()
			{
				continue;
			}
			let Some(endpoint) = extended(&head.frontier, std::iter::once(&head.payload)) else {
				continue;
			};
			streams += 1;
			reads += 1;
			planned.push(Planned {
				source: *source,
				stream,
				item: Item::Register(
					*id,
					LossyRead {
						frontier: head.frontier.clone(),
						payloads: vec![head.payload.clone()],
					},
				),
				endpoint,
				root,
			});
		}

		let channels: Vec<_> = view
			.sources
			.iter()
			.flat_map(|(source, wants)| {
				wants.channels.iter().map(move |(id, stored)| (*source, *id, stored))
			})
			.collect();
		let (first, rest) = channels.split_at(rotation % channels.len().max(1));
		for (source, id, stored) in rest.iter().chain(first) {
			if streams >= max_streams {
				break;
			}
			let Some((pool, root)) = newest(source) else { continue };
			let stream = id.sender_data_stream(self.us).1;
			let mut taken = Vec::new();
			for payload in pool.contiguous_from(&stream, stored.leaf_count()) {
				let len = payload.len() as u64;
				if payloads >= max_payloads || bytes + len > max_bytes {
					break;
				}
				payloads += 1;
				bytes += len;
				taken.push(payload.clone());
			}
			if taken.is_empty() {
				continue;
			}
			let Some(endpoint) = extended(stored, taken.iter()) else { continue };
			streams += 1;
			planned.push(Planned {
				source: *source,
				stream,
				item: Item::Channel(*id, taken),
				endpoint,
				root,
			});
		}
		planned
	}

	/// Note an imported block's consumption record, and fetch the leaves its reads skipped.
	pub async fn on_block(&self, hash: H, parent: H, number: u64, record: ConsumptionRecord) {
		let gaps = self.note_block(hash, parent, number, record);
		self.fetch_gaps(gaps).await;
	}

	/// Note an imported block's consumption record; return the gaps its reads skipped, for
	/// [`Self::fetch_gaps`].
	pub(crate) fn note_block(
		&self,
		hash: H,
		parent: H,
		number: u64,
		record: ConsumptionRecord,
	) -> Vec<Gap> {
		let mut state = self.state.lock();
		let mut gaps = Vec::new();
		for (source, streams) in &record.entries {
			for (stream, interval) in streams {
				let Some(previous) = state.nearest(parent, source, stream) else { continue };
				if interval.start == previous.end.root() {
					continue;
				}
				let from = previous.end.clone();
				let to_count = interval.end.leaf_count();
				let Some(pool) = state.sources.get(source) else { continue };
				if pool.advances.contains_key(&(
					*stream,
					from.leaf_count(),
					to_count,
					interval.start.0,
				)) {
					continue;
				}
				// Only a read context the pool fetched has a root to fetch the leaves under.
				let Some(under) = pool.read_under.get(&(*stream, to_count)).copied() else {
					continue;
				};
				gaps.push(Gap {
					source: *source,
					stream: *stream,
					from,
					to_start: interval.start,
					to_count,
					under,
				});
			}
		}
		state.live.insert(hash, LiveBlock { number, parent, record });
		gaps
	}

	/// Fetch the leaves of `gaps` and keep the advances over them.
	pub(crate) async fn fetch_gaps(&self, gaps: Vec<Gap>) {
		for gap in gaps {
			let key = (gap.stream, gap.from.leaf_count(), gap.to_count, gap.to_start.0);
			let source = gap.source;
			if let Some(extension) = self.fetch_skip_proof(gap).await {
				let mut state = self.state.lock();
				let pool = state.sources.entry(source).or_default();
				pool.advances.insert(key, extension);
			}
		}
	}

	/// The extension from where the previous read ended to the start of the next read, built from
	/// the leaves between them.
	async fn fetch_skip_proof(&self, gap: Gap) -> Option<MMRExtensionProof> {
		let Gap { source, stream, from, to_start, to_count, under } = gap;
		// The next read's own leaf is not part of the gap.
		let read_position = to_count.checked_sub(1)?;
		let mut frontier = from.clone();
		let mut leaves = Vec::new();
		while frontier.leaf_count() < read_position {
			let request = MessagesRequest {
				stream,
				under,
				start: Some(MessagePosition(frontier.leaf_count())),
				max_bytes: FETCH_MAX_BYTES,
			};
			let response = fetch(&self.transport, source, &request, Some(&frontier)).await.ok()?;
			let wanted = (read_position - frontier.leaf_count()) as usize;
			let payloads = &response.payloads[..response.payloads.len().min(wanted)];
			if payloads.is_empty() {
				return None;
			}
			for payload in payloads {
				let leaf = leaf_hash(payload);
				frontier.try_append(leaf).ok()?;
				leaves.push(leaf);
			}
		}
		if frontier.root() != to_start {
			return None;
		}
		skip_proof(&from, &leaves)
	}

	/// Blocks up to `number` are final: drop their consumption and the material only they used.
	/// `finalized` is the view at the finalized block; the pool keeps every payload from the
	/// cursors there, since the best chain may still sit on an earlier fork's cursor.
	pub fn on_finalized(&self, number: u64, finalized: &ChainView) {
		let mut state = self.state.lock();
		state.live.retain(|_, block| block.number > number);
		for (source, wants) in &finalized.sources {
			if let Some(pool) = state.sources.get_mut(source) {
				for (id, stored) in &wants.channels {
					pool.prune_payloads(&id.sender_data_stream(self.us).1, stored.leaf_count());
				}
			}
		}
		let sources: Vec<ParaId> = state.sources.keys().copied().collect();
		for source in sources {
			let live = state.live_endpoints(&source);
			if let Some(pool) = state.sources.get_mut(&source) {
				pool.retain_endpoints(&live);
			}
		}
	}

	/// The lifts for a candidate whose blocks have `records`, in bundle order. Per source, the
	/// newest root that all its streams' material reaches.
	pub fn assemble(&self, records: &[ConsumptionRecord]) -> Result<LiftsBySource, String> {
		let mut merged: BTreeMap<ParaId, SourceStreams> = BTreeMap::new();
		for record in records {
			for (source, streams) in &record.entries {
				let by_stream = merged.entry(*source).or_default();
				for (stream, interval) in streams {
					by_stream.entry(*stream).or_default().push(interval.clone());
				}
			}
		}
		let state = self.state.lock();
		let mut lifts = BTreeMap::new();
		for (source, streams) in &merged {
			let pool = state
				.sources
				.get(source)
				.ok_or_else(|| format!("nothing fetched from {source:?}"))?;
			let found = pool.roots.iter().rev().find_map(|root| {
				let source_lifts = streams
					.iter()
					.map(|(stream, intervals)| lift(pool, stream, intervals, root))
					.collect::<Option<Vec<_>>>()?;
				(build_requires_entry(streams, &source_lifts) == Ok(*root)).then_some(source_lifts)
			});
			let source_lifts =
				found.ok_or_else(|| format!("no lift material for {source:?} under any root"))?;
			lifts.insert(*source, source_lifts);
		}
		LiftsBySource::try_from(lifts).map_err(|e| format!("{e:?}"))
	}

	/// Fetch and store the binding of `stream` at `endpoint` under `root`: a proofs-only request
	/// whose response must start from the endpoint.
	async fn fetch_binding(
		&self,
		source: ParaId,
		stream: StreamId,
		endpoint: &MmrFrontier,
		root: StreamsRoot,
	) -> bool {
		let count = endpoint.leaf_count();
		let request = MessagesRequest {
			stream,
			under: root,
			start: Some(MessagePosition(count)),
			max_bytes: 0,
		};
		let Ok(response) = fetch(&self.transport, source, &request, Some(endpoint)).await else {
			return false;
		};
		let binding = Binding { extension: response.extension, tree_proof: response.tree_proof };
		let mut state = self.state.lock();
		state
			.sources
			.entry(source)
			.or_default()
			.insert_binding(stream, count, root, binding);
		true
	}

	/// Read the head of `stream` under `root`. When the first budget is too small for the payload,
	/// the response proves only the position, and the payload is asked for there.
	async fn fetch_head(
		&self,
		source: ParaId,
		stream: StreamId,
		root: StreamsRoot,
	) -> Result<(HeadRead, StreamProof), RequestError> {
		let request = MessagesRequest {
			stream,
			under: root,
			start: None,
			max_bytes: Register::max_encoded_len() as u32,
		};
		let mut response = fetch(&self.transport, source, &request, None).await?;
		if response.payloads.is_empty() {
			let request = MessagesRequest {
				start: Some(MessagePosition(response.frontier.leaf_count())),
				max_bytes: MAX_SPECULATIVE_MESSAGE_LEN,
				..request
			};
			response = fetch(&self.transport, source, &request, Some(&response.frontier)).await?;
		}
		let [payload] = <[Vec<u8>; 1]>::try_from(response.payloads.into_inner())
			.map_err(|_| RequestError::Unserved { refusals: Vec::new() })?;
		Ok((HeadRead { under: root, frontier: response.frontier, payload }, response.tree_proof))
	}
}

/// The last endpoint of each stream in `records`, a bundle's in order, by source.
fn last_endpoints(records: &[ConsumptionRecord]) -> BTreeMap<(ParaId, StreamId), MmrFrontier> {
	records
		.iter()
		.flat_map(|record| &record.entries)
		.flat_map(|(source, streams)| {
			streams
				.iter()
				.map(move |(stream, interval)| ((*source, *stream), interval.end.clone()))
		})
		.collect()
}

/// Whether a failed fetch was refused because the stream does not exist under the root.
fn unknown_stream(error: &RequestError) -> bool {
	matches!(
		error,
		RequestError::Unserved { refusals }
			if refusals.iter().any(|(_, refusal)| *refusal == FetchError::UnknownStream)
	)
}

fn buffer_full(count: u64, bytes: u64) -> bool {
	count >= MAX_BUFFERED_PAYLOADS as u64 || bytes >= MAX_BUFFERED_BYTES
}

/// Whether the pool holds `stream`'s payloads from `cursor` up to what the source had at the last
/// fetch, or enough that fetching pauses.
fn covers(pool: &SourcePool, stream: &StreamId, cursor: u64) -> bool {
	let (count, bytes) = pool.buffered(stream, cursor);
	buffer_full(count, bytes) ||
		pool.reach.get(stream).is_some_and(|reach| cursor + count >= *reach)
}

/// `frontier` with the leaves of `payloads` appended.
fn extended<'a>(
	frontier: &MmrFrontier,
	payloads: impl IntoIterator<Item = &'a Vec<u8>>,
) -> Option<MmrFrontier> {
	let mut frontier = frontier.clone();
	for payload in payloads {
		frontier.try_append(leaf_hash(payload)).ok()?;
	}
	Some(frontier)
}

/// MMR nodes held in memory for [`skip_proof`].
#[derive(Default)]
struct Nodes(RefCell<BTreeMap<u64, Hash>>);

impl MMRStoreReadOps<Hash> for &Nodes {
	fn get_elem(&self, pos: u64) -> mmr_lib::Result<Option<Hash>> {
		Ok(self.0.borrow().get(&pos).copied())
	}
}

impl MMRStoreWriteOps<Hash> for &Nodes {
	fn append(&mut self, pos: u64, elems: Vec<Hash>) -> mmr_lib::Result<()> {
		let mut nodes = self.0.borrow_mut();
		for (i, elem) in elems.into_iter().enumerate() {
			nodes.insert(pos + i as u64, elem);
		}
		Ok(())
	}
}

fn mmr_size(leaf_count: u64) -> u64 {
	if leaf_count == 0 {
		0
	} else {
		leaf_index_to_mmr_size(leaf_count - 1)
	}
}

/// The extension from `from` to the stream after `leaves` are appended, built from `from`'s peaks
/// and the leaves alone.
pub(crate) fn skip_proof(from: &MmrFrontier, leaves: &[Hash]) -> Option<MMRExtensionProof> {
	let to = from.leaf_count() + leaves.len() as u64;
	if leaves.is_empty() {
		return None;
	}
	if from.leaf_count() == 0 {
		let mut frontier = from.clone();
		for leaf in leaves {
			frontier.try_append(*leaf).ok()?;
		}
		return Some(MMRExtensionProof {
			leaf_count: to,
			connecting_nodes: frontier.peaks().to_vec().try_into().ok()?,
		});
	}
	let nodes = Nodes::default();
	let old_size = mmr_size(from.leaf_count());
	nodes
		.0
		.borrow_mut()
		.extend(get_peaks(old_size).into_iter().zip(from.peaks().iter().copied()));
	let mut mmr = MMR::<Hash, SpecMerge, _>::new(old_size, &nodes);
	for leaf in leaves {
		mmr.push(*leaf).ok()?;
	}
	mmr.commit().ok()?;
	let proof = MMR::<Hash, SpecMerge, _>::new(mmr_size(to), &nodes)
		.gen_ancestry_proof(old_size)
		.ok()?;
	Some(MMRExtensionProof {
		leaf_count: to,
		connecting_nodes: proof
			.prev_peaks_proof
			.proof_items()
			.iter()
			.map(|(_, hash)| *hash)
			.collect::<Vec<_>>()
			.try_into()
			.ok()?,
	})
}

/// The lift of one stream's `intervals` to `root`, from the pool's material.
fn lift(
	pool: &SourcePool,
	stream: &StreamId,
	intervals: &[Interval],
	root: &StreamsRoot,
) -> Option<RequiresLift> {
	let advances = intervals
		.windows(2)
		.filter(|pair| pair[1].start != pair[0].end.root())
		.map(|pair| {
			let key =
				(*stream, pair[0].end.leaf_count(), pair[1].end.leaf_count(), pair[1].start.0);
			pool.advances.get(&key).cloned()
		})
		.collect::<Option<Vec<_>>>()?;
	let end = &intervals.last()?.end;
	let binding = match pool.binding(stream, end.leaf_count(), root) {
		Some(binding) => binding.clone(),
		None => rebased_binding(pool, stream, end, root)?,
	};
	Some(RequiresLift { advances, extension: binding.extension, tree_proof: binding.tree_proof })
}

/// Whether the pool's material lifts `stream`'s endpoint `end` to `root`, as the PVF checks it. A
/// lift rebased over payloads of another fork of the source does not.
fn lifts_to(pool: &SourcePool, stream: &StreamId, end: &MmrFrontier, root: &StreamsRoot) -> bool {
	let interval = Interval { start: end.root(), end: end.clone() };
	let Some(lift) = lift(pool, stream, std::slice::from_ref(&interval), root) else {
		return false;
	};
	let streams = SourceStreams::from([(*stream, vec![interval])]);
	build_requires_entry(&streams, &[lift]) == Ok(*root)
}

/// The binding of `stream` at `end` under `root`, computed from a later one and the payloads in
/// between. `create_inherent` may admit only a prefix of a planned channel item, so a block can
/// end before the endpoint the node bound.
fn rebased_binding(
	pool: &SourcePool,
	stream: &StreamId,
	end: &MmrFrontier,
	root: &StreamsRoot,
) -> Option<Binding> {
	let count = end.leaf_count();
	pool.bindings_above(stream, count, root).find_map(|(later, binding)| {
		let wanted = usize::try_from(later - count).ok()?;
		let leaves: Vec<_> =
			pool.contiguous_from(stream, count).take(wanted).map(|p| leaf_hash(p)).collect();
		if leaves.len() != wanted {
			return None;
		}
		let extension = rebase(end, &leaves, &binding.extension)?;
		Some(Binding { extension, tree_proof: binding.tree_proof.clone() })
	})
}

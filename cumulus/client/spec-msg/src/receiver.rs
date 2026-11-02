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
//! - [`Receiver::on_block`]: note a block's record and fetch the advance for a read-context gap.
//! - [`Receiver::assemble`]: one lift per recorded stream, checked with `build_requires_entry`.

use crate::{
	fetch::{fetch, Transport, Verified},
	pool::{peaks_before_head, Binding, HeadRead, SourcePool},
	LOG_TARGET,
};
use codec::DecodeAll;
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	lift::{build_requires_entry, SourceStreams},
	ChannelId, ConsumeItem, ConsumedStream, ConsumptionRecord, EventRequest, ExchangeRequest,
	ExchangeResponse, ExchangeVerified, Interval, LiftsBySource, MessagePosition, MessagesRequest,
	MessagingInherentData, MmrFrontier, MmrRoot, OutChannelState, Register, RequiresLift, StreamId,
	StreamProof, StreamsRoot, VerifiedEvent,
};
use futures::FutureExt;
use parking_lot::Mutex;
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fmt::Debug,
	hash::Hash as StdHash,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};

/// Payload bytes asked for per channel fetch. The server may cap harder.
pub const FETCH_MAX_BYTES: u32 = 1024 * 1024;

/// Payloads buffered per channel beyond its cursor before fetching pauses.
pub const MAX_BUFFERED_PAYLOADS: usize = 4096;

/// Payload bytes buffered per channel beyond its cursor before fetching pauses. A sender that
/// ignores our grant could otherwise send faster than the budget consumes and fill memory.
pub const MAX_BUFFERED_BYTES: u64 = 8 * 1024 * 1024;

/// How long building the inherent waits for a missing binding before it drops the item.
pub const INHERENT_FETCH_TIMEOUT: Duration = Duration::from_millis(500);

/// Per-block consumption limits. `enact_messages` is a mandatory inherent, so its cost must fit the
/// block whatever arrived. `max_streams` and `max_reads` are clamped to the runtime's caps
/// ([`ChainView::caps`]): an inherent over them fails the block.
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
	/// The source's channel streams to us, with their cursors (`consumed_streams()`).
	pub channels: Vec<ConsumedStream>,
	/// The source's `Ack` streams to us, with the register last read from each
	/// (`out_channels()`).
	pub registers: Vec<(StreamId, Option<Register>)>,
}

/// The chain's consumption view at a block, per source.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ChainView {
	/// What is consumed from each source.
	pub sources: BTreeMap<ParaId, SourceWants>,
	/// The runtime's caps on streams and register reads per block (`consumption_caps()`). `None`
	/// leaves the [`Budget`] alone.
	pub caps: Option<(u32, u32)>,
}

impl ChainView {
	/// The view from `consumed_streams()` and `out_channels()` of chain `us`.
	pub fn new(
		us: ParaId,
		consumed: BTreeMap<ParaId, Vec<ConsumedStream>>,
		out_channels: BTreeMap<ChannelId, OutChannelState>,
	) -> Self {
		let mut sources: BTreeMap<ParaId, SourceWants> = consumed
			.into_iter()
			.map(|(source, channels)| (source, SourceWants { channels, registers: Vec::new() }))
			.collect();
		for (channel, state) in out_channels {
			let stream = StreamId::Ack { recipient: us, domain: channel.domain, num: channel.num };
			sources
				.entry(channel.peer)
				.or_default()
				.registers
				.push((stream, state.register.map(|read| read.register)));
		}
		Self { sources, caps: None }
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
			if let Some(interval) = block.record.entries.get(source).and_then(|s| s.get(stream)) {
				return Some(interval);
			}
			from = block.parent;
		}
		None
	}

	/// The `(stream, leaf count)` endpoints of `source` in live blocks.
	fn live_endpoints(&self, source: &ParaId) -> BTreeSet<(StreamId, u64)> {
		self.live
			.values()
			.filter_map(|block| block.record.entries.get(source))
			.flat_map(|streams| {
				streams.iter().map(|(stream, interval)| (*stream, interval.end.leaf_count()))
			})
			.collect()
	}
}

/// One planned inherent item and the endpoint it creates.
struct Planned {
	source: ParaId,
	stream: StreamId,
	item: ConsumeItem,
	endpoint: u64,
	root: StreamsRoot,
}

/// A gap between two reads of a stream, and where its advance comes from.
struct Gap {
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
	transport: Arc<T>,
	state: Mutex<State<H>>,
	/// Where the next block starts handing out channel budget.
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
			transport,
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

	/// Whether everything under `root`, `source`'s newest root, has been fetched.
	pub fn is_settled(&self, source: &ParaId, root: &StreamsRoot) -> bool {
		self.state
			.lock()
			.sources
			.get(source)
			.is_some_and(|pool| pool.newest_root().as_ref() == Some(root) && pool.settled)
	}

	/// `root` is `source`'s newest included root: fetch under it what `wants` names, and bind the
	/// live endpoints to it. Under a root already settled this does nothing; under one whose last
	/// round had a failed fetch, it fetches again.
	pub async fn on_root(&self, source: ParaId, root: StreamsRoot, wants: &SourceWants) {
		if self.is_settled(&source, &root) {
			return;
		}
		self.state.lock().sources.entry(source).or_default().push_root(root);
		log::debug!(target: LOG_TARGET, "New root {root:?} of {source:?}: fetching");
		let started = std::time::Instant::now();
		let mut settled = true;

		for consumed in &wants.channels {
			let stream = consumed.stream_id(self.us);
			let start = {
				let mut state = self.state.lock();
				let pool = state.sources.entry(source).or_default();
				let from = consumed.from().0;
				pool.prune_payloads(&stream, from);
				let (count, bytes) = pool
					.contiguous_from(&stream, from)
					.fold((0u64, 0u64), |(n, b), p| (n + 1, b + p.len() as u64));
				(count < MAX_BUFFERED_PAYLOADS as u64 && bytes < MAX_BUFFERED_BYTES)
					.then_some(from + count)
			};
			let Some(start) = start else { continue };
			let request = MessagesRequest {
				stream,
				start: MessagePosition(start),
				under: root,
				max_bytes: FETCH_MAX_BYTES,
			};
			match self.fetch_messages(source, request).await {
				Some((payloads, binding)) => {
					let end = start + payloads.len() as u64;
					let mut state = self.state.lock();
					let pool = state.sources.entry(source).or_default();
					pool.insert_payloads(stream, start, payloads);
					pool.insert_binding(stream, end, root, binding);
				},
				None => settled = false,
			}
		}

		for (stream, _) in &wants.registers {
			match self.fetch_head(source, *stream, root).await {
				Some((head, tree_proof)) => {
					let mut state = self.state.lock();
					state.sources.entry(source).or_default().insert_head(*stream, head, tree_proof);
				},
				None => settled = false,
			}
		}

		let unbound: Vec<_> = {
			let state = self.state.lock();
			let pool = state.sources.get(&source);
			state
				.live_endpoints(&source)
				.into_iter()
				.filter(|(stream, count)| {
					pool.and_then(|pool| pool.binding(stream, *count, &root)).is_none()
				})
				.collect()
		};
		for (stream, count) in unbound {
			settled &= self.fetch_binding(source, stream, count, root).await;
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

	/// The messaging inherent for a block whose parent has `view`.
	pub async fn inherent_data(&self, view: &ChainView) -> MessagingInherentData {
		let planned = self.plan(view);
		let bound = |p: &Planned| {
			self.state
				.lock()
				.sources
				.get(&p.source)
				.is_some_and(|pool| pool.binding(&p.stream, p.endpoint, &p.root).is_some())
		};
		// Fetch every missing binding at once, under one deadline: authoring waits at most
		// `INHERENT_FETCH_TIMEOUT`, however many items there are and whatever their peers do.
		let missing: Vec<_> = planned
			.iter()
			.filter(|p| !bound(p))
			.map(|p| self.fetch_binding(p.source, p.stream, p.endpoint, p.root))
			.collect();
		if !missing.is_empty() {
			futures::select! {
				_ = futures::future::join_all(missing).fuse() => {},
				_ = futures_timer::Delay::new(INHERENT_FETCH_TIMEOUT).fuse() => {},
			}
		}
		let mut items = Vec::with_capacity(planned.len());
		for p in planned {
			if bound(&p) {
				items.push((p.source, p.stream, p.item));
			} else {
				log::debug!(
					target: LOG_TARGET,
					"Not consuming {:?} of {:?}: no lift material at {}",
					p.stream,
					p.source,
					p.endpoint,
				);
			}
		}
		if !items.is_empty() {
			log::debug!(target: LOG_TARGET, "Messaging inherent: {} items", items.len());
		}
		MessagingInherentData { items }
	}

	/// Choose the items within the budget, from what the pool holds under each source's newest
	/// root. Register reads go first: one leaf each, so channel backlogs cannot take every stream
	/// slot and stall our own sends. Channels then fill the budget from a start that rotates each
	/// block, so no source's backlog takes every block.
	fn plan(&self, view: &ChainView) -> Vec<Planned> {
		let Budget { mut max_streams, mut max_reads, max_payloads, max_bytes } = self.budget;
		if let Some((streams, reads)) = view.caps {
			max_streams = max_streams.min(streams);
			max_reads = max_reads.min(reads);
		}
		let (mut streams, mut reads, mut payloads, mut bytes) = (0u32, 0u32, 0u32, 0u64);
		let mut planned = Vec::new();
		let state = self.state.lock();
		let newest = |source: &ParaId| {
			let pool = state.sources.get(source)?;
			Some((pool, pool.newest_root()?))
		};

		for (source, wants) in &view.sources {
			let Some((pool, root)) = newest(source) else { continue };
			for (stream, last) in &wants.registers {
				if streams >= max_streams || reads >= max_reads {
					break;
				}
				let Some(head) = pool.heads.get(stream) else { continue };
				// The pallet rejects the whole block on an undecodable register; an unchanged one
				// would cost a read for nothing.
				let Ok(register) = Register::decode_all(&mut &head.payload[..]) else { continue };
				if Some(register) == *last {
					continue;
				}
				streams += 1;
				reads += 1;
				planned.push(Planned {
					source: *source,
					stream: *stream,
					endpoint: head.end_count(),
					item: ConsumeItem::Events {
						base: head.base,
						start_peaks: head
							.start_peaks
							.clone()
							.try_into()
							.expect("a verified head's peaks fit the bound; qed"),
						payloads: vec![head.payload.clone()],
					},
					root,
				});
			}
		}

		let channels: Vec<_> = view
			.sources
			.iter()
			.flat_map(|(source, wants)| wants.channels.iter().map(move |c| (*source, c)))
			.collect();
		let offset = self.rotation.fetch_add(1, Ordering::Relaxed) % channels.len().max(1);
		let (after, before) = channels.split_at(offset);
		for (source, consumed) in before.iter().chain(after) {
			if streams >= max_streams {
				break;
			}
			let Some((pool, root)) = newest(source) else { continue };
			let stream = consumed.stream_id(self.us);
			let from = consumed.from().0;
			let mut taken = Vec::new();
			for payload in pool.contiguous_from(&stream, from) {
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
			streams += 1;
			planned.push(Planned {
				source: *source,
				stream,
				endpoint: from + taken.len() as u64,
				item: ConsumeItem::Channel { payloads: taken },
				root,
			});
		}
		planned
	}

	/// Note an imported block's consumption record, and fetch the advances its reads need.
	pub async fn on_block(&self, hash: H, parent: H, number: u64, record: ConsumptionRecord) {
		let gaps = {
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
					if pool.advances.contains_key(&(*stream, from.leaf_count(), to_count)) {
						continue;
					}
					// Only a read context the pool fetched has a root to fetch the advance under.
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
		};
		for Gap { source, stream, from, to_start, to_count, under } in gaps {
			let request = MessagesRequest {
				stream,
				start: MessagePosition(from.leaf_count()),
				under,
				max_bytes: 0,
			};
			let Some((_, Binding { extension, .. })) = self.fetch_messages(source, request).await
			else {
				continue;
			};
			// Under the read's own root, the extension ends exactly at the read's context.
			if extension.verify(&from) == Ok(to_start) {
				let mut state = self.state.lock();
				state
					.sources
					.entry(source)
					.or_default()
					.advances
					.insert((stream, from.leaf_count(), to_count), extension);
			}
		}
	}

	/// Blocks up to `number` are final: drop their consumption and the material only they used.
	pub fn on_finalized(&self, number: u64) {
		let mut state = self.state.lock();
		state.live.retain(|_, block| block.number > number);
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

	/// A verified `MessagesResponse`: its payloads, and the binding of its end under its root.
	async fn fetch_messages(
		&self,
		source: ParaId,
		request: MessagesRequest,
	) -> Option<(Vec<Vec<u8>>, Binding)> {
		match fetch(&*self.transport, source, &ExchangeRequest::Messages(request)).await {
			Ok(Verified {
				response: ExchangeResponse::Messages(response),
				outcome: ExchangeVerified::Messages,
			}) => Some((
				response.payloads.into_inner(),
				Binding { extension: response.extension, tree_proof: response.tree_proof },
			)),
			_ => None,
		}
	}

	/// Fetch and store the binding of `stream` at `count` under `root`.
	async fn fetch_binding(
		&self,
		source: ParaId,
		stream: StreamId,
		count: u64,
		root: StreamsRoot,
	) -> bool {
		let request =
			MessagesRequest { stream, start: MessagePosition(count), under: root, max_bytes: 0 };
		let Some((_, binding)) = self.fetch_messages(source, request).await else { return false };
		let mut state = self.state.lock();
		state
			.sources
			.entry(source)
			.or_default()
			.insert_binding(stream, count, root, binding);
		true
	}

	/// Read the head of `stream` under `root`.
	async fn fetch_head(
		&self,
		source: ParaId,
		stream: StreamId,
		root: StreamsRoot,
	) -> Option<(HeadRead, StreamProof)> {
		let request = ExchangeRequest::Event(EventRequest { stream, under: root, at: None });
		let Verified { response, outcome } =
			fetch(&*self.transport, source, &request).await.ok()?;
		let (
			ExchangeResponse::Event(response),
			ExchangeVerified::Event(VerifiedEvent::Head { position, frontier }),
		) = (response, outcome)
		else {
			return None;
		};
		let start_peaks = peaks_before_head(&response, position, &frontier)?;
		let payload = response.payload;
		Some((HeadRead { under: root, base: position, start_peaks, payload }, response.tree_proof))
	}
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
			pool.advances
				.get(&(*stream, pair[0].end.leaf_count(), pair[1].end.leaf_count()))
				.cloned()
		})
		.collect::<Option<Vec<_>>>()?;
	let binding = pool.binding(stream, intervals.last()?.end.leaf_count(), root)?;
	Some(RequiresLift {
		advances,
		extension: binding.extension.clone(),
		tree_proof: binding.tree_proof.clone(),
	})
}

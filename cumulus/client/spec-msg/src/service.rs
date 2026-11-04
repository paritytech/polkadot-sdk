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

//! The receiver's ties to the own chain and the relay chain.
//!
//! - [`run_receiver`]: per relay best block, pass each source's newest included root to
//!   [`Receiver::on_root`] until it settles, one round per source at a time; note own blocks; prune
//!   on finality. The loop never waits on the network.
//! - [`inherent_data`] and [`assembler`]: the inherent provider and the collator's
//!   [`SpecMsgAssembler`].
//!
//! Idle without `SpecMsgApi` or `ParachainHost` v17.

use crate::{
	fetch::Transport,
	receiver::{ChainView, Receiver, SourceWants},
	relay::{newest_included_provides, relay_has_api},
	LOG_TARGET,
};
use cumulus_client_collator::service::{SpecMsgAssembler, SpecMsgInputs};
use cumulus_primitives_core::{CumulusDigestItem, ParaId, SpecMsgApi};
use cumulus_primitives_spec_messaging::{ConsumptionRecord, MessagingInherentData, StreamsRoot};
use cumulus_relay_chain_interface::{PHash, RelayChainInterface};
use futures::{select, stream::FuturesUnordered, Stream, StreamExt};
use sc_client_api::BlockchainEvents;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	SaturatedConversion,
};
use std::{
	collections::BTreeMap, fmt::Debug, hash::Hash as StdHash, marker::PhantomData, sync::Arc,
};

/// The consumption view at `at`, or `None` if its runtime has no `SpecMsgApi`.
pub fn chain_view<Block, C>(client: &C, at: Block::Hash) -> Option<ChainView>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	if !api.has_api::<dyn SpecMsgApi<Block>>(at).ok()? {
		return None;
	}
	let consumed = api
		.consumed_streams(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "consumed_streams at {at:?} failed: {e}"))
		.ok()?;
	let max_touched = api
		.max_touched_streams(at)
		.map_err(|e| log::warn!(target: LOG_TARGET, "max_touched_streams at {at:?} failed: {e}"))
		.ok()?;
	Some(ChainView::new(consumed, max_touched))
}

/// The consumption record of block `at`; empty if its runtime has no `SpecMsgApi`.
pub fn consumption_record<Block, C>(
	client: &C,
	at: Block::Hash,
) -> Result<ConsumptionRecord, String>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	if !api.has_api::<dyn SpecMsgApi<Block>>(at).map_err(|e| e.to_string())? {
		return Ok(ConsumptionRecord::default());
	}
	api.consumption_record(at).map_err(|e| e.to_string())
}

/// What the receiver loop reads from the own chain and the relay chain.
#[async_trait::async_trait]
pub(crate) trait Chains: Send + Sync {
	/// The consumption view at the own chain's best block; `None` without `SpecMsgApi`.
	fn view(&self) -> Option<ChainView>;

	/// `source`'s newest included root as of relay block `head`; `None` also when the relay
	/// runtime lacks the API.
	async fn root(&self, head: PHash, source: ParaId) -> Option<StreamsRoot>;
}

/// [`Chains`] read from the node's client and relay chain interface.
struct ClientChains<Block, C> {
	client: Arc<C>,
	relay: Arc<dyn RelayChainInterface>,
	_block: PhantomData<fn() -> Block>,
}

#[async_trait::async_trait]
impl<Block, C> Chains for ClientChains<Block, C>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync,
	C::Api: SpecMsgApi<Block>,
{
	fn view(&self) -> Option<ChainView> {
		chain_view(&*self.client, self.client.info().best_hash)
	}

	async fn root(&self, head: PHash, source: ParaId) -> Option<StreamsRoot> {
		if !relay_has_api(&*self.relay, head).await {
			return None;
		}
		newest_included_provides(&*self.relay, head, source).await
	}
}

/// An imported block's consumption.
pub(crate) struct Imported<H> {
	pub(crate) hash: H,
	pub(crate) parent: H,
	pub(crate) number: u64,
	pub(crate) record: ConsumptionRecord,
}

/// Keep `receiver` fed from `client` and `relay` until a notification stream ends.
pub async fn run_receiver<Block, C, T>(
	client: Arc<C>,
	relay: Arc<dyn RelayChainInterface>,
	receiver: Arc<Receiver<Block::Hash, T>>,
) where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + BlockchainEvents<Block> + HeaderBackend<Block> + Send + Sync,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized,
{
	let heads = match relay.new_best_notification_stream().await {
		Ok(stream) => stream.map(|header| header.hash()),
		Err(e) => {
			log::error!(target: LOG_TARGET, "No relay best-block stream; receiver stopped: {e}");
			return;
		},
	};
	let imports = client.import_notification_stream().filter_map({
		let client = client.clone();
		move |import| {
			let imported = match consumption_record(&*client, import.hash) {
				Ok(record) => Some(Imported {
					hash: import.hash,
					parent: *import.header.parent_hash(),
					number: (*import.header.number()).saturated_into(),
					record,
				}),
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"consumption_record at {:?} failed: {e}",
						import.hash,
					);
					None
				},
			};
			futures::future::ready(imported)
		}
	});
	let finality = client.finality_notification_stream().map({
		let client = client.clone();
		move |finalized| {
			let view = chain_view(&*client, finalized.hash).unwrap_or_default();
			((*finalized.header.number()).saturated_into(), view)
		}
	});
	let chains = ClientChains { client, relay, _block: PhantomData };
	drive(&chains, &receiver, heads, imports, finality).await
}

/// The receiver loop over `heads` (relay best blocks), `imports` and `finality` (the finalized
/// block's number and view), until one of them ends.
///
/// Nothing in it waits on the network. Each source's round runs beside the loop, one at a time
/// per source; a source still busy when a newer relay block arrives gets one more round, on the
/// newest, once done. The leaves of a block's read gaps are fetched beside it as well. A source
/// whose peers stall thus delays only its own rounds, never another source's, an import or
/// finality.
pub(crate) async fn drive<H, T, S>(
	chains: &S,
	receiver: &Receiver<H, T>,
	heads: impl Stream<Item = PHash> + Unpin,
	imports: impl Stream<Item = Imported<H>> + Unpin,
	finality: impl Stream<Item = (u64, ChainView)> + Unpin,
) where
	H: Copy + Eq + StdHash + Debug,
	T: Transport + ?Sized,
	S: Chains + ?Sized,
{
	let (mut heads, mut imports, mut finality) = (heads.fuse(), imports.fuse(), finality.fuse());
	let mut slots = RoundSlots::default();
	let mut rounds = FuturesUnordered::new();
	let mut gap_fetches = FuturesUnordered::new();
	let round = |source: ParaId, head: PHash, wants: SourceWants| async move {
		if let Some(root) = chains.root(head, source).await {
			receiver.on_root(source, root, &wants).await;
		}
		source
	};
	loop {
		select! {
			head = heads.next() => {
				let Some(head) = head else { return };
				let mut view = chains.view().unwrap_or_default();
				for source in slots.on_head(head, view.sources.keys().copied()) {
					if let Some(wants) = view.sources.remove(&source) {
						rounds.push(round(source, head, wants));
					}
				}
			},
			import = imports.next() => {
				let Some(Imported { hash, parent, number, record }) = import else { return };
				let gaps = receiver.note_block(hash, parent, number, record);
				if !gaps.is_empty() {
					gap_fetches.push(receiver.fetch_gaps(gaps));
				}
			},
			finalized = finality.next() => {
				let Some((number, view)) = finalized else { return };
				receiver.on_finalized(number, &view);
			},
			source = rounds.select_next_some() => {
				let Some(head) = slots.on_done(source) else { continue };
				let wants = chains.view().and_then(|mut view| view.sources.remove(&source));
				if let Some(wants) = wants {
					slots.start(source, head);
					rounds.push(round(source, head, wants));
				}
			},
			() = gap_fetches.select_next_some() => {},
		}
	}
}

/// Which sources get a round: one at a time per source, on the newest relay block.
#[derive(Default)]
struct RoundSlots {
	/// The newest relay block.
	newest: Option<PHash>,
	/// The sources with a round running, with the relay block it runs on.
	running: BTreeMap<ParaId, PHash>,
}

impl RoundSlots {
	/// Relay block `head` is the newest. Of `sources`, those to start a round for on it: the
	/// others have one running.
	fn on_head(&mut self, head: PHash, sources: impl IntoIterator<Item = ParaId>) -> Vec<ParaId> {
		self.newest = Some(head);
		let mut started = Vec::new();
		for source in sources {
			if !self.running.contains_key(&source) {
				self.running.insert(source, head);
				started.push(source);
			}
		}
		started
	}

	/// `source`'s round ended. The newest relay block, if newer than the one the round ran on:
	/// `source` needs another round, on it.
	fn on_done(&mut self, source: ParaId) -> Option<PHash> {
		let ran_on = self.running.remove(&source)?;
		self.newest.filter(|newest| *newest != ran_on)
	}

	/// A round of `source` runs on `head`.
	fn start(&mut self, source: ParaId, head: PHash) {
		self.running.insert(source, head);
	}
}

/// The messaging inherent for a block built on `parent`. Empty when `parent`'s runtime has no
/// `SpecMsgApi`; an empty inherent places nothing.
pub async fn inherent_data<Block, C, T>(
	client: &C,
	receiver: &Receiver<Block::Hash, T>,
	parent: Block::Hash,
) -> MessagingInherentData
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized,
{
	let Some(view) = chain_view(client, parent) else {
		return MessagingInherentData::default();
	};
	match bundle_records(client, parent) {
		Ok(bundle) => receiver.inherent_data(&view, &bundle).await,
		Err(e) => {
			log::warn!(target: LOG_TARGET, "Bundle of a block on {parent:?} unknown: {e}");
			MessagingInherentData::default()
		},
	}
}

/// The consumption records of the blocks before a block built on `parent` in its bundle, oldest
/// first. None when `parent` ends its bundle (or bundles nothing): the block starts the next one.
fn bundle_records<Block, C>(
	client: &C,
	parent: Block::Hash,
) -> Result<Vec<ConsumptionRecord>, String>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	C::Api: SpecMsgApi<Block>,
{
	let header_of = |hash: Block::Hash| {
		client
			.header(hash)
			.map_err(|e| e.to_string())?
			.ok_or_else(|| format!("no header of {hash:?}"))
	};
	let mut at = parent;
	let mut header = header_of(at)?;
	let mut records = Vec::new();
	if CumulusDigestItem::is_last_block_in_core(header.digest()) != Some(false) {
		return Ok(records);
	}
	loop {
		records.push(consumption_record(client, at)?);
		if !extends_bundle(&header) {
			break;
		}
		at = *header.parent_hash();
		header = header_of(at)?;
	}
	records.reverse();
	Ok(records)
}

/// Whether the block of `header` is not the first of its bundle: its candidate holds its parent.
fn extends_bundle<H: HeaderT>(header: &H) -> bool {
	CumulusDigestItem::find_block_bundle_info(header.digest()).is_some_and(|info| info.index > 0)
}

/// The collator's assembler: each block's consumption record, and the lifts for them from the
/// receiver's pool. An error skips the collation; the next attempt assembles again.
pub fn assembler<Block, C, T>(
	client: Arc<C>,
	receiver: Arc<Receiver<Block::Hash, T>>,
) -> SpecMsgAssembler<Block>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: SpecMsgApi<Block>,
	T: Transport + ?Sized + 'static,
{
	Arc::new(move |blocks: &[Block]| {
		let records = blocks
			.iter()
			.map(|block| consumption_record(&*client, block.header().hash()))
			.collect::<Result<Vec<_>, _>>()?;
		let lifts = receiver.assemble(&records)?;
		Ok(SpecMsgInputs { records, lifts })
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		protocol,
		tests::{ack, channel, frontier_at, import, payloads, Model, TestArchive},
		Archive, Budget, MemoryStore,
	};
	use codec::{Decode, Encode};
	use cumulus_primitives_spec_messaging::{
		ConsumedStreams, InChannelId, Interval, MessagePosition, MessagesRequest, MmrFrontier,
		OutChannelId, Register, RegisterStatus, StreamId,
	};
	use futures::{
		channel::{mpsc, oneshot},
		executor::LocalPool,
		task::LocalSpawnExt,
	};
	use parking_lot::{Mutex, RwLock};
	use sc_network::PeerId;
	use std::sync::atomic::{AtomicBool, Ordering};

	const US: u32 = 2000;

	fn stalled() -> ParaId {
		1000.into()
	}

	fn live() -> ParaId {
		1001.into()
	}

	fn head(n: u8) -> PHash {
		PHash::repeat_byte(n)
	}

	/// A source chain's collator.
	struct Source {
		peer: PeerId,
		archive: RwLock<TestArchive>,
		model: RwLock<Model>,
		number: RwLock<u64>,
		/// While set, each request waits until released, as on a peer that accepts requests and
		/// never answers them.
		hold: AtomicBool,
		held: Mutex<Vec<oneshot::Sender<()>>>,
		/// Every request, in order.
		log: Mutex<Vec<MessagesRequest>>,
	}

	impl Source {
		fn new() -> Self {
			Self {
				peer: PeerId::random(),
				archive: RwLock::new(Archive::open(Arc::new(MemoryStore::default()))),
				model: RwLock::new(Model::default()),
				number: RwLock::new(0),
				hold: AtomicBool::new(false),
				held: Mutex::new(Vec::new()),
				log: Mutex::new(Vec::new()),
			}
		}

		/// Author the next block with `sends`; return its root.
		fn author(&self, sends: Vec<(StreamId, Vec<Vec<u8>>)>) -> StreamsRoot {
			let mut number = self.number.write();
			*number += 1;
			import(&mut self.archive.write(), &mut self.model.write(), *number, sends)
		}

		/// Let every held request through.
		fn release(&self) {
			self.held.lock().clear();
		}

		/// The roots the requests so far were made under.
		fn unders(&self) -> Vec<StreamsRoot> {
			self.log.lock().iter().map(|request| request.under).collect()
		}
	}

	/// The two sources' collators.
	struct Peers(BTreeMap<ParaId, Source>);

	#[async_trait::async_trait]
	impl Transport for Peers {
		fn peers(&self, source: ParaId) -> Vec<PeerId> {
			self.0.get(&source).map(|source| vec![source.peer]).unwrap_or_default()
		}

		async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
			let source = self.0.values().find(|source| source.peer == peer).ok_or("unknown")?;
			source.log.lock().push(MessagesRequest::decode(&mut &request[..]).unwrap());
			if source.hold.load(Ordering::SeqCst) {
				let (release, released) = oneshot::channel();
				source.held.lock().push(release);
				let _ = released.await;
			}
			protocol::answer(&source.archive.read(), &request).ok_or_else(|| "refused".into())
		}
	}

	/// The chains as the test sets them: a channel from each source and a register of our
	/// channel to it, and the sources' roots per relay head.
	struct TestChains {
		view: ChainView,
		roots: Mutex<BTreeMap<(PHash, ParaId), StreamsRoot>>,
	}

	#[async_trait::async_trait]
	impl Chains for TestChains {
		fn view(&self) -> Option<ChainView> {
			Some(self.view.clone())
		}

		async fn root(&self, head: PHash, source: ParaId) -> Option<StreamsRoot> {
			self.roots.lock().get(&(head, source)).copied()
		}
	}

	/// [`drive`] on a local pool, fed by the test.
	struct Harness {
		pool: LocalPool,
		peers: Arc<Peers>,
		chains: Arc<TestChains>,
		receiver: Arc<Receiver<u64, Peers>>,
		heads: mpsc::UnboundedSender<PHash>,
		imports: mpsc::UnboundedSender<Imported<u64>>,
		finality: mpsc::UnboundedSender<(u64, ChainView)>,
	}

	impl Harness {
		fn new() -> Self {
			let sources = [stalled(), live()];
			let consumed = ConsumedStreams {
				channels: sources
					.iter()
					.map(|sender| {
						(InChannelId { sender: *sender, domain: 0, num: 0 }, MmrFrontier::new())
					})
					.collect(),
				registers: sources
					.iter()
					.map(|recipient| {
						(
							OutChannelId { recipient: *recipient, domain: 0, num: 0 },
							MessagePosition(0),
						)
					})
					.collect(),
				events: BTreeMap::new(),
			};
			let chains = Arc::new(TestChains {
				view: ChainView::new(consumed, 16),
				roots: Mutex::new(BTreeMap::new()),
			});
			let peers = Arc::new(Peers(sources.iter().map(|id| (*id, Source::new())).collect()));
			let receiver = Arc::new(Receiver::new(US.into(), Budget::default(), peers.clone()));
			let (heads, heads_rx) = mpsc::unbounded();
			let (imports, imports_rx) = mpsc::unbounded();
			let (finality, finality_rx) = mpsc::unbounded();
			let pool = LocalPool::new();
			let (driven_chains, driven_receiver) = (chains.clone(), receiver.clone());
			pool.spawner()
				.spawn_local(async move {
					drive(&*driven_chains, &*driven_receiver, heads_rx, imports_rx, finality_rx)
						.await
				})
				.unwrap();
			Self { pool, peers, chains, receiver, heads, imports, finality }
		}

		fn source(&self, id: ParaId) -> &Source {
			&self.peers.0[&id]
		}

		fn wants(&self, id: ParaId) -> &SourceWants {
			&self.chains.view.sources[&id]
		}

		/// Relay block `n` becomes best, with `roots` as the sources' newest included roots; run
		/// until nothing can progress.
		fn relay_block(&mut self, n: u8, roots: &[(ParaId, StreamsRoot)]) {
			for (source, root) in roots {
				self.chains.roots.lock().insert((head(n), *source), *root);
			}
			self.heads.unbounded_send(head(n)).unwrap();
			self.pool.run_until_stalled();
		}

		fn import(&mut self, hash: u64, record: ConsumptionRecord) {
			let imported = Imported { hash, parent: hash - 1, number: hash, record };
			self.imports.unbounded_send(imported).unwrap();
			self.pool.run_until_stalled();
		}
	}

	fn register(up_to: u64) -> Vec<u8> {
		Register { up_to: MessagePosition(up_to), status: RegisterStatus::Opened }.encode()
	}

	#[test]
	fn a_source_whose_peers_never_answer_does_not_hold_up_the_others() {
		let mut h = Harness::new();
		h.source(stalled()).hold.store(true, Ordering::SeqCst);
		let stalled_root = StreamsRoot(PHash::repeat_byte(0xEE));
		let live1 = h.source(live()).author(vec![(channel(US), payloads(1, 2))]);
		h.relay_block(1, &[(stalled(), stalled_root), (live(), live1)]);
		assert!(h.receiver.is_settled(&live(), &live1, h.wants(live())));
		assert_eq!(h.source(stalled()).held.lock().len(), 1, "the stalled round waits");

		// The live source moves on while the stalled source's round still waits.
		let live2 = h.source(live()).author(vec![(channel(US), payloads(2, 2))]);
		h.relay_block(2, &[(stalled(), stalled_root), (live(), live2)]);
		assert!(h.receiver.is_settled(&live(), &live2, h.wants(live())), "fetched at once");
		assert_eq!(h.source(stalled()).log.lock().len(), 1, "one round per source at a time");

		// Imports and finality are handled meanwhile as well.
		h.import(1, ConsumptionRecord::default());
		assert!(h.receiver.is_live(&1));
		h.finality.unbounded_send((1, h.chains.view.clone())).unwrap();
		h.pool.run_until_stalled();
		assert!(!h.receiver.is_live(&1));
	}

	#[test]
	fn a_busy_source_catches_up_on_the_newest_relay_block_only() {
		let mut h = Harness::new();
		h.source(stalled()).hold.store(true, Ordering::SeqCst);
		let roots: Vec<_> = (1..=3u8).map(|n| StreamsRoot(PHash::repeat_byte(0xE0 + n))).collect();
		for (n, root) in (1..=3).zip(&roots) {
			h.relay_block(n, &[(stalled(), *root)]);
		}
		assert_eq!(h.source(stalled()).unders(), vec![roots[0]], "only the first round runs");

		// The first round ends; one more runs, under the newest root. The middle one is skipped.
		h.source(stalled()).hold.store(false, Ordering::SeqCst);
		h.source(stalled()).release();
		h.pool.run_until_stalled();
		let unders = h.source(stalled()).unders();
		assert!(!unders.contains(&roots[1]), "{unders:?}");
		assert_eq!(unders.last(), Some(&roots[2]));
	}

	#[test]
	fn a_gap_fetch_that_never_completes_does_not_hold_up_the_loop() {
		let mut h = Harness::new();
		// The stalled source serves at first: its register head is read under two roots, at
		// positions 0 and 3.
		let registers: Vec<_> = (0..4).map(register).collect();
		let root1 = h.source(stalled()).author(vec![(ack(US), registers[..1].to_vec())]);
		h.relay_block(1, &[(stalled(), root1)]);
		let root2 = h.source(stalled()).author(vec![(ack(US), registers[1..].to_vec())]);
		h.relay_block(2, &[(stalled(), root2)]);
		assert!(h.receiver.is_settled(&stalled(), &root2, h.wants(stalled())));

		// Then it stops answering. Two blocks read its register at positions 0 and 3: the
		// second read skipped two leaves, and fetching them waits.
		h.source(stalled()).hold.store(true, Ordering::SeqCst);
		let read = |start: usize, end: usize| ConsumptionRecord {
			entries: BTreeMap::from([(
				stalled(),
				vec![(
					ack(US),
					Interval {
						start: frontier_at(&registers, start).root(),
						end: frontier_at(&registers, end),
					},
				)],
			)]),
		};
		h.import(1, read(0, 1));
		h.import(2, read(3, 4));
		assert_eq!(h.source(stalled()).held.lock().len(), 1, "the gap fetch waits");

		// The other source's new root is fetched meanwhile.
		let live1 = h.source(live()).author(vec![(channel(US), payloads(1, 2))]);
		h.relay_block(3, &[(stalled(), root2), (live(), live1)]);
		assert!(h.receiver.is_settled(&live(), &live1, h.wants(live())));

		// Once the peer answers, the advance over the gap is kept.
		h.source(stalled()).hold.store(false, Ordering::SeqCst);
		h.source(stalled()).release();
		h.pool.run_until_stalled();
		assert!(h.receiver.has_advance(&stalled(), ack(US), 1, 4));
	}
}

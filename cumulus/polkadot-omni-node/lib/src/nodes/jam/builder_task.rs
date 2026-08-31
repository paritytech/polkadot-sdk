// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The JAM block-builder task (phase 5).
//!
//! A wall-clock **para-slot timer** drives authoring — one tick per parachain slot (the Aura slot
//! duration, 6 s today) — instead of one iteration per JAM best-block notification, which could
//! never pace a parachain producing several blocks per JAM slot. The JAM best-block subscription
//! is demoted to a tip cache that the tick reads synchronously; the anchor is that cached tip.
//!
//! Per tick: prune the [`UnincludedSegment`] against the para head JAM has accumulated, ask the
//! runtime whether one more block fits on top (`can_build_upon` — capacity and velocity are
//! runtime-owned), author on the deepest in-flight block, import it, and hand
//! `(block, proof, context)` to the collation task. Blocks stay in the segment until JAM
//! accumulates them, so authoring no longer waits for inclusion.
//!
//! Phases 1–5 inject a **mocked** parachain inherent (the runtime still requires
//! `set_validation_data`); its fake relay slot follows the wall-clock JAM slot, and it advertises
//! the head JAM has accumulated as the relay chain's included para head — that is what makes the
//! runtime's own segment tracking, and therefore its capacity limit, real. Both the mocked
//! validation data and the timestamp travel *inside* the block, so importers validate them
//! instead of recomputing them from their own clock — exactly as in relay mode.

use super::{
	JamCollatorMessage, LOG_TARGET, ParentLink, fetch_anchor_state_proof, jam_slot_as_relay_slot,
	jam_slot_at, segments::export_of,
};
use crate::common::{
	ConstructNodeRuntimeApi, NodeBlock,
	aura::{AuraIdT, AuraRuntimeApi},
	types::{ParachainBackend, ParachainClient},
};
use codec::Decode;
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{CollectCollationInfo, RelayParentOffsetApi};
use futures::{FutureExt, StreamExt, channel::mpsc};
use jam_cumulus_facade::service_state::{ParaInfo, para_info_key};
use jam_interface::{
	BlockDesc, HeaderHash, JamChainSource, JamStateSource, ServiceId, Slot as JamSlot,
	StateRootHash, WorkPackageHash, WorkReport,
};
use jam_state_helpers::StateProof;
use jam_types::{RefineContext, SegmentTreeRoot};
use polkadot_primitives::{HeadData, Id as ParaId, UpgradeGoAhead};
use sc_client_api::Backend as _;
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::standalone as aura_internal;
use sp_api::{ProofRecorder, ProvideRuntimeApi};
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_consensus::{Environment, ProposeArgs, Proposer};
use sp_consensus_aura::{AuraApi, Slot, SlotDuration};
use sp_externalities::Extensions;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Header as HeaderT, UniqueSaturatedInto};
use sp_timestamp::Timestamp;
use sp_trie::proof_size_extension::ProofSizeExt;
use std::{
	collections::{HashSet, VecDeque},
	future::Future,
	sync::Arc,
	time::{Duration, Instant},
};

const PROPOSAL_DURATION: Duration = Duration::from_millis(2000);
/// Phase-1 PoV budget; generous for a mostly-empty test chain, small enough for any JAM
/// work-package size limit.
const MAX_POV_SIZE: usize = 3 * 1024 * 1024;
/// How many JAM blocks back from the cached tip the anchor is taken.
///
/// Zero anchors at the tip itself, which buys a slot of proof freshness; the knob mirrors the
/// relay collator's `relay_parent_offset` for the day a fork-prone JAM chain wants distance.
const ANCHOR_OFFSET: u32 = 0;
/// How far the cached JAM tip may lag the wall clock before a tick is skipped.
///
/// Notification driving could not build on a stalled JAM chain — no notification, no block. A
/// wall-clock timer keeps ticking regardless, so it needs to be told when the tip it caches went
/// stale (JAM stalled, or the subscription died silently) rather than anchoring against it.
const MAX_TIP_LAG_SLOTS: JamSlot = 2;
/// Sanity bound on the number of in-flight blocks.
///
/// The runtime's consensus hook owns capacity for real (`can_build_upon`); this only stops a
/// runaway should that ever answer wrongly, and tripping it is a bug worth a loud log.
const MAX_UNINCLUDED: usize = 8;

pub(crate) struct BuilderTaskParams<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	/// Needed for the block tree alone: correlating JAM's in-flight reports means walking the
	/// children of the accumulated head, which only the backend's blockchain can enumerate.
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub block_import: BI,
	pub proposer_factory: PF,
	pub keystore: KeystorePtr,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub jam: Arc<Jam>,
	pub message_sender: mpsc::Sender<JamCollatorMessage<Block>>,
	pub rebuild_receiver: mpsc::Receiver<()>,
}

/// The in-flight work package JAM's reports say is carrying a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReportedPackage {
	wp_hash: WorkPackageHash,
	segroot: SegmentTreeRoot,
}

/// One block in flight, and how its package is known.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentEntry<Header> {
	header: Header,
	/// `None` for a block authored this session — the collation manager holds its package and
	/// knows its hash first-hand. `Some` for one taken from JAM's in-flight reports, which is
	/// the only way a restarted collator, or one extending another collator's block, can name
	/// the parent package at all.
	reported: Option<ReportedPackage>,
}

/// The parachain blocks that are in flight — authored, or read out of JAM's in-flight reports —
/// but not accumulated by JAM yet.
///
/// The JAM twin of Cumulus' unincluded segment: `included` is the para head proven in JAM state
/// at the anchor, `entries` are the blocks chained on top of it, oldest first, each one the child
/// of its predecessor.
struct UnincludedSegment<Header> {
	/// The para head accumulated in JAM state; `None` until the para's first block lands.
	included: Option<Header>,
	entries: VecDeque<SegmentEntry<Header>>,
}

/// What folding JAM's in-flight reports into the segment did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Adoption {
	/// In-flight blocks taken from JAM's reports.
	from_state: usize,
	/// In-flight blocks authored this session, which always end up on top.
	from_local: usize,
	/// The reported chain and the blocks we authored are on different forks. Ours won and the
	/// reported chain was ignored — the priority order says our own tip beats state.
	forked: bool,
}

/// What reading the accumulated para head did to the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PruneOutcome {
	/// The included head moved and `popped` in-flight blocks became accumulated. Zero means it
	/// moved while we had nothing in flight — another collator authored that block.
	Advanced { popped: usize },
	/// The included head is still where it was; the segment is untouched.
	Unchanged,
	/// The included head is a block we do not have in flight.
	Diverged,
}

impl<Header: HeaderT> UnincludedSegment<Header> {
	fn new() -> Self {
		Self { included: None, entries: VecDeque::new() }
	}

	/// Fold a freshly read accumulated head into the segment.
	///
	/// The head JAM accepted either is one of our in-flight blocks (everything up to and
	/// including it is now accumulated and leaves the segment) or it is not — in which case
	/// another collator's chain won and everything we still hold is orphaned. Dropping it all is
	/// the correct move: the next tick rebuilds from the head JAM accepted (drop-tail self-heal),
	/// whereas keeping the orphans would only produce blocks the service refuses to chain on.
	fn prune(&mut self, included: Option<Header>) -> PruneOutcome {
		let new_head = included.as_ref().map(|header| header.hash());
		if new_head == self.included.as_ref().map(|header| header.hash()) {
			return PruneOutcome::Unchanged;
		}

		let position = new_head
			.and_then(|hash| self.entries.iter().position(|entry| entry.header.hash() == hash));
		self.included = included;
		match position {
			Some(index) => {
				self.entries.drain(..=index);
				PruneOutcome::Advanced { popped: index + 1 }
			},
			None if self.entries.is_empty() => PruneOutcome::Advanced { popped: 0 },
			None => {
				self.entries.clear();
				PruneOutcome::Diverged
			},
		}
	}

	/// Rebuild the in-flight part of the segment around the chain JAM's reports describe.
	///
	/// The reported chain goes underneath: those packages exist on chain, so a block chaining
	/// onto them can name one as its prerequisite. The blocks authored this session stay on top
	/// and stay ours — a package submitted a moment ago is not reported yet and is therefore
	/// invisible in JAM state, while the collation manager is already tracking it. That is the
	/// whole reason the priority order puts our own tip above anything read from state.
	fn adopt(&mut self, reported: Vec<Correlated<Header>>) -> Adoption {
		let local: VecDeque<SegmentEntry<Header>> =
			self.entries.drain(..).filter(|entry| entry.reported.is_none()).collect();
		let ours: HashSet<Header::Hash> = local.iter().map(|entry| entry.header.hash()).collect();
		// A reported block we authored ourselves is already in `local`, and so is everything
		// after it on the reported chain — the two lists describe the same line of blocks, so the
		// reported one is cut where ours takes over.
		let taken = reported
			.iter()
			.position(|block| ours.contains(&block.header.hash()))
			.unwrap_or(reported.len());
		let mut adopted: VecDeque<SegmentEntry<Header>> = reported
			.into_iter()
			.take(taken)
			.map(|block| SegmentEntry { header: block.header, reported: Some(block.package) })
			.collect();

		let base = adopted
			.back()
			.map(|entry| entry.header.hash())
			.or_else(|| self.included.as_ref().map(|header| header.hash()));
		// With no accumulated head and nothing adopted, our blocks start at genesis and there is
		// nothing here to check them against.
		let joins = match (local.front(), base) {
			(None, _) | (Some(_), None) => true,
			(Some(first), Some(base)) => *first.header.parent_hash() == base,
		};

		let (from_state, from_local) = (adopted.len(), local.len());
		if joins {
			adopted.extend(local);
			self.entries = adopted;
			Adoption { from_state, from_local, forked: false }
		} else {
			self.entries = local;
			Adoption { from_state: 0, from_local, forked: true }
		}
	}

	/// The block to author on: the deepest in-flight block, else the accumulated head. `None`
	/// means the para has no head at all yet and the next block is its first.
	fn tip(&self) -> Option<&Header> {
		self.entries.back().map(|entry| &entry.header).or(self.included.as_ref())
	}

	/// How the next block's package attaches to the block [`Self::tip`] names.
	fn parent_link(&self) -> ParentLink {
		match self.entries.back().and_then(|entry| entry.reported) {
			None if self.entries.is_empty() => ParentLink::Included,
			None => ParentLink::Tip,
			Some(ReportedPackage { wp_hash, segroot }) => ParentLink::Reported { wp_hash, segroot },
		}
	}

	fn push(&mut self, header: Header) {
		self.entries.push_back(SegmentEntry { header, reported: None });
	}

	/// Forget everything in flight, keeping the accumulated head. Sent by the collation task when
	/// it gives up on the packages carrying those blocks.
	fn reset(&mut self) {
		self.entries.clear();
	}

	fn depth(&self) -> usize {
		self.entries.len()
	}

	fn is_full(&self) -> bool {
		self.entries.len() >= MAX_UNINCLUDED
	}

	fn entry_hashes(&self) -> Vec<Header::Hash> {
		self.entries.iter().map(|entry| entry.header.hash()).collect()
	}
}

/// One work package JAM currently has in flight, reduced to what correlation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InFlightReport {
	wp_hash: WorkPackageHash,
	/// The root of the package's export tree. The only handle the collator has on "which block
	/// is this package carrying": the work output that would say so outright is the parachain
	/// service's own format, and decoding it would tie the collator to one service.
	segroot: SegmentTreeRoot,
	source: ReportSource,
}

/// Which of JAM's two in-flight state entries a report was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportSource {
	/// `C(10)`: reported by guarantors, data still being made available.
	Availability,
	/// `C(14)`: available, queued for accumulation behind its dependencies.
	ReadyQueue,
}

/// A block we hold whose header reproduces an in-flight report's export root.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Correlated<Header> {
	header: Header,
	package: ReportedPackage,
}

/// What one tick's in-flight reports say about the blocks we hold.
#[derive(Debug, PartialEq, Eq)]
struct Correlation<Header: HeaderT> {
	/// The reported blocks forming an unbroken line from the included head, oldest first.
	chain: Vec<Correlated<Header>>,
	/// Reports matching a block we hold that is not on that line: someone is building a fork.
	off_chain: Vec<(WorkPackageHash, Header::Hash)>,
	/// Reports matching no block we hold. Someone's chain is ahead of ours and we are waiting
	/// for the block itself to be announced — never an error, just something to wait for.
	unmatched: Vec<InFlightReport>,
}

/// Match JAM's in-flight reports to blocks we hold, by export root alone.
///
/// The collator never decodes a report's work output, so the only correlation available is: take
/// a block we hold, recompute the export its package must have published ([`export_of`], the same
/// helper the collation task builds the real export with), and see whether a report commits to
/// that root. The matches are then threaded into a chain by parent-hash linkage from the included
/// head, because that is the order the packages depend on each other in.
fn correlate<Header: HeaderT>(
	reports: &[InFlightReport],
	candidates: &[Header],
	included_hash: Header::Hash,
) -> Correlation<Header> {
	let roots: Vec<(SegmentTreeRoot, &Header)> = candidates
		.iter()
		.filter_map(|header| match export_of(&header.encode()) {
			Ok(export) => Some((export.segroot, header)),
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					block = ?header.hash(),
					error,
					"Cannot recompute the export root of a block we hold; no report will ever be \
					 correlated to it.",
				);
				None
			},
		})
		.collect();

	let mut matched: Vec<(&InFlightReport, &Header)> = Vec::new();
	let mut unmatched = Vec::new();
	for report in reports {
		match roots.iter().find(|(segroot, _)| *segroot == report.segroot) {
			Some((_, header)) => matched.push((report, header)),
			None => unmatched.push(*report),
		}
	}

	let mut chain = Vec::new();
	let mut parent = included_hash;
	while chain.len() < MAX_UNINCLUDED {
		let Some(position) = matched.iter().position(|(_, header)| *header.parent_hash() == parent)
		else {
			break;
		};
		let (report, header) = matched.remove(position);
		parent = header.hash();
		chain.push(Correlated {
			header: header.clone(),
			package: ReportedPackage { wp_hash: report.wp_hash, segroot: report.segroot },
		});
	}

	let off_chain =
		matched.into_iter().map(|(report, header)| (report.wp_hash, header.hash())).collect();
	Correlation { chain, off_chain, unmatched }
}

/// The blocks we hold that descend from `included`, at most [`MAX_UNINCLUDED`] generations down.
///
/// Every block an in-flight report could be carrying is somewhere in this tree: a package in
/// flight was built on the accumulated head or on another in-flight block, and the segment can be
/// no deeper than that bound. Sibling forks are kept — several collators may have reported
/// children of the same block, and working out which of them is on the reported chain is exactly
/// what [`correlate`] does.
fn known_descendants<Block: NodeBlock>(
	backend: &ParachainBackend<Block>,
	included: Block::Hash,
) -> Vec<Block::Header> {
	let blockchain = backend.blockchain();
	let mut frontier = vec![included];
	let mut descendants = Vec::new();
	for _ in 0..MAX_UNINCLUDED {
		let mut next = Vec::new();
		for hash in frontier {
			let children = match blockchain.children(hash) {
				Ok(children) => children,
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?hash,
						?error,
						"Cannot list a block's children; the in-flight search stops here.",
					);
					continue;
				},
			};
			for child in children {
				if let Ok(Some(header)) = blockchain.header(child) {
					descendants.push(header);
					next.push(child);
				}
			}
		}
		if next.is_empty() {
			break;
		}
		frontier = next;
	}
	descendants
}

/// The work packages JAM has in flight for our service, from both places state keeps them.
///
/// Filtering is by service id — a protocol-level field of a report's per-item digest — and
/// nothing else is taken from the digest: the work output's format belongs to the service, and
/// reading it would make this collator work against one service only.
async fn read_in_flight_reports<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	service_id: ServiceId,
) -> Result<Vec<InFlightReport>, String> {
	let started = Instant::now();
	let availability =
		jam.availability(anchor).await.map_err(|error| format!("availability: {error}"))?;
	let availability_ms = started.elapsed().as_millis();
	let started = Instant::now();
	let ready = jam.ready_queue(anchor).await.map_err(|error| format!("readyQueue: {error}"))?;
	let ready_queue_ms = started.elapsed().as_millis();

	let mut reports = Vec::new();
	for assignment in availability.iter().flatten() {
		push_report(&mut reports, &assignment.report, ReportSource::Availability, service_id);
	}
	let from_availability = reports.len();
	for record in ready.iter().flatten() {
		push_report(&mut reports, &record.report, ReportSource::ReadyQueue, service_id);
	}

	tracing::debug!(
		target: LOG_TARGET,
		method = "availability + readyQueue",
		at = ?anchor,
		service_id,
		cores = availability.len(),
		epoch_phases = ready.len(),
		from_availability,
		from_ready_queue = reports.len() - from_availability,
		?reports,
		availability_ms,
		ready_queue_ms,
		"JAM read: the work packages in flight for our service.",
	);
	Ok(reports)
}

/// Keep a report if it refines something for our service and is not already listed.
///
/// The same package can sit in both state entries across a tick's two reads, and JAM keys them by
/// package hash, so the hash is what deduplicates.
fn push_report(
	reports: &mut Vec<InFlightReport>,
	report: &WorkReport,
	source: ReportSource,
	service_id: ServiceId,
) {
	if !report.results.iter().any(|digest| digest.service == service_id) {
		return;
	}
	let wp_hash = report.package_spec.hash;
	if reports.iter().any(|seen| seen.wp_hash == wp_hash) {
		return;
	}
	reports.push(InFlightReport { wp_hash, segroot: report.package_spec.exports_root, source });
}

/// What the builder remembers between ticks.
struct BuilderState<Header> {
	/// Aura guard: the parachain slot last claimed.
	last_claimed_slot: Option<Slot>,
	segment: UnincludedSegment<Header>,
}

/// Everything one tick reads from JAM at its anchor.
struct AnchorReads {
	anchor: BlockDesc,
	context: RefineContext,
	state_root: StateRootHash,
	/// The para's entry in the service's state, exactly as stored; `None` = no head yet.
	included: Option<Vec<u8>>,
	proof: StateProof,
	/// Our service's work packages that JAM has in flight at this anchor.
	reports: Vec<InFlightReport>,
}

/// How long until the next para slot starts.
///
/// Mirrors the relay slot-based collator's timer (`slot_based/slot_timer.rs`): slots are aligned
/// to the unix epoch, so all collators tick on the same boundaries.
fn time_until_next_para_slot(now: Duration, slot_duration: Duration) -> Duration {
	let now = now.as_millis();
	let slot_millis = slot_duration.as_millis().max(1);
	let next_slot_start = ((now + slot_millis) / slot_millis) * slot_millis;
	Duration::from_millis((next_slot_start - now) as u64)
}

/// One block per para slot, the Aura guard carried over from phase 1 — now on the wall-clock
/// para slot rather than the anchor's timeslot.
fn para_slot_claimed(last_claimed: Option<Slot>, para_slot: Slot) -> bool {
	last_claimed.is_some_and(|last| para_slot <= last)
}

/// Whether the cached JAM tip is recent enough to anchor against.
fn tip_is_fresh(wall_jam_slot: JamSlot, tip_slot: JamSlot) -> bool {
	wall_jam_slot.saturating_sub(tip_slot) <= MAX_TIP_LAG_SLOTS
}

/// Run one JAM read, logging the method, the block it was read at, what came back and the
/// round-trip: latency is a first-class debugging signal on this path.
async fn jam_read<T, F>(method: &'static str, at: HeaderHash, read: F) -> Result<T, String>
where
	F: Future<Output = jam_interface::Result<T>>,
	T: std::fmt::Debug,
{
	let started = Instant::now();
	let result = read.await;
	let elapsed_ms = started.elapsed().as_millis();
	match &result {
		Ok(value) => tracing::debug!(
			target: LOG_TARGET,
			method,
			?at,
			?value,
			elapsed_ms,
			"JAM read.",
		),
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			method,
			?at,
			?error,
			elapsed_ms,
			"JAM read failed.",
		),
	}
	result.map_err(|error| format!("{method}: {error}"))
}

/// Run the builder task. Ends (taking the node down, it is an essential task) only if the JAM
/// best-block stream cannot be (re-)established or the collation task is gone.
pub(crate) async fn run_builder_task<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	params: BuilderTaskParams<Block, RuntimeApi, BI, PF, Jam>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource + JamStateSource,
{
	let BuilderTaskParams {
		para_client,
		para_backend,
		mut block_import,
		mut proposer_factory,
		keystore,
		para_id,
		service_id,
		jam,
		mut message_sender,
		mut rebuild_receiver,
	} = params;

	let slot_duration = match sc_consensus_aura::slot_duration(&*para_client) {
		Ok(slot_duration) => slot_duration,
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to read the Aura slot duration.");
			return;
		},
	};

	let mut best_blocks = match jam.best_block_stream().await {
		Ok(stream) => stream.fuse(),
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				"Unable to subscribe to JAM best blocks."
			);
			return;
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		slot_duration = slot_duration.as_millis(),
		anchor_offset = ANCHOR_OFFSET,
		max_tip_lag_slots = MAX_TIP_LAG_SLOTS,
		"JAM builder task started; building one block per parachain slot.",
	);

	// The segment starts empty and every tick rebuilds it: the blocks JAM's in-flight reports
	// name go underneath, ours on top. That is what carries a restart across (nothing was
	// authored this session, so the whole segment comes from state) and what lets this collator
	// extend a block another one authored.
	let mut state = BuilderState { last_claimed_slot: None, segment: UnincludedSegment::new() };
	let mut cached_tip: Option<BlockDesc> = None;
	loop {
		let mut next_tick = futures_timer::Delay::new(time_until_next_para_slot(
			Timestamp::current().as_duration(),
			slot_duration.as_duration(),
		))
		.fuse();
		loop {
			futures::select! {
				_ = next_tick => break,
				jam_best = best_blocks.next() => match jam_best {
					Some(jam_best) => {
						tracing::debug!(target: LOG_TARGET, ?jam_best, "JAM tip cached.");
						cached_tip = Some(jam_best);
					},
					None => {
						tracing::error!(target: LOG_TARGET, "JAM best-block stream ended.");
						return;
					},
				},
				rebuild = rebuild_receiver.next() => match rebuild {
					Some(()) => {
						tracing::info!(
							target: LOG_TARGET,
							dropped = state.segment.depth(),
							entries = ?state.segment.entry_hashes(),
							"Segment reset requested by the collation task; dropping the \
							 in-flight blocks and rebuilding from the accumulated head.",
						);
						state.segment.reset();
					},
					None => {
						tracing::error!(
							target: LOG_TARGET,
							"Collation task is gone; stopping the builder task."
						);
						return;
					},
				},
			}
		}

		// Captured once, before anything else: the block's Aura slot, its timestamp and the
		// mocked inherent's fake relay slot all derive from this instant and have to agree, so
		// re-reading the clock after an await would let a tick claim the following slot.
		let now = Timestamp::current();

		let Some(tip) = cached_tip else {
			tracing::debug!(
				target: LOG_TARGET,
				"No JAM best block seen yet; skipping this parachain slot.",
			);
			continue;
		};

		match run_tick::<Block, RuntimeApi, AuraId, _, _, _>(
			&para_client,
			&para_backend,
			&mut block_import,
			&mut proposer_factory,
			&keystore,
			para_id,
			service_id,
			&*jam,
			slot_duration,
			tip,
			now,
			&mut state,
		)
		.await
		{
			Ok(Some(message)) => {
				let block_hash = message.block.hash();
				if let Err(error) = message_sender.try_send(message) {
					if error.is_disconnected() {
						tracing::error!(
							target: LOG_TARGET,
							"Collation task is gone; stopping the builder task."
						);
						return;
					}
					tracing::warn!(
						target: LOG_TARGET,
						?block_hash,
						"Collation task is backlogged; dropping this block's collation.",
					);
				}
			},
			Ok(None) => {},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					error,
					?tip,
					"Failed to build a block in this parachain slot.",
				);
			},
		}
	}
}

/// One para-slot tick: guards, JAM reads, segment maintenance, capacity gate, authoring.
///
/// `Ok(None)` means the tick was skipped; the reason is always logged where it was decided.
async fn run_tick<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_backend: &ParachainBackend<Block>,
	block_import: &mut BI,
	proposer_factory: &mut PF,
	keystore: &KeystorePtr,
	para_id: ParaId,
	service_id: ServiceId,
	jam: &Jam,
	slot_duration: SlotDuration,
	tip: BlockDesc,
	now: Timestamp,
	state: &mut BuilderState<Block::Header>,
) -> Result<Option<JamCollatorMessage<Block>>, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource + JamStateSource,
{
	// Everything time-derived comes from this one instant: the block's Aura slot and the mocked
	// inherent's fake relay slot have to agree, and the consensus hook panics if they do not.
	let para_slot = Slot::from_timestamp(now, slot_duration);
	let wall_jam_slot = jam_slot_at(now);
	tracing::debug!(
		target: LOG_TARGET,
		?para_slot,
		wall_jam_slot,
		timestamp = now.as_millis(),
		?tip,
		tip_lag_slots = wall_jam_slot.saturating_sub(tip.slot),
		tip_fresh = tip_is_fresh(wall_jam_slot, tip.slot),
		segment_depth = state.segment.depth(),
		"Parachain slot tick.",
	);

	if para_slot_claimed(state.last_claimed_slot, para_slot) {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			last_claimed_slot = ?state.last_claimed_slot,
			"Parachain slot already claimed; skipping this tick.",
		);
		return Ok(None);
	}

	if !tip_is_fresh(wall_jam_slot, tip.slot) {
		tracing::warn!(
			target: LOG_TARGET,
			?tip,
			wall_jam_slot,
			lag_slots = wall_jam_slot.saturating_sub(tip.slot),
			max_tip_lag_slots = MAX_TIP_LAG_SLOTS,
			"The cached JAM tip lags the wall clock; skipping this tick.",
		);
		return Ok(None);
	}

	let para_id_u32: u32 = para_id.into();
	// The tick is the reads' budget: a JAM node that cannot answer within one parachain slot has
	// nothing fresh to say, and the next tick supersedes this one anyway.
	let reads = match tokio::time::timeout(
		slot_duration.as_duration(),
		read_anchor(jam, tip, service_id, para_id_u32),
	)
	.await
	{
		Ok(reads) => reads?,
		Err(_) => {
			tracing::warn!(
				target: LOG_TARGET,
				?tip,
				budget_ms = slot_duration.as_millis(),
				"JAM reads did not finish within the parachain slot; skipping this tick.",
			);
			return Ok(None);
		},
	};
	let Some(AnchorReads {
		anchor,
		context,
		state_root,
		included,
		proof: anchor_state_proof,
		reports,
	}) = reads
	else {
		return Ok(None);
	};

	let included_head = match &included {
		Some(bytes) => {
			let info =
				ParaInfo::decode(&mut &bytes[..]).map_err(|e| format!("included ParaInfo: {e}"))?;
			let head = info.head_data.into_inner();
			Some(
				<Block::Header as Decode>::decode(&mut &head[..])
					.map_err(|e| format!("included head data: {e}"))?,
			)
		},
		None => None,
	};
	tracing::debug!(
		target: LOG_TARGET,
		anchor = ?anchor.header_hash,
		included_head = ?included_head.as_ref().map(|header| header.hash()),
		included_number = ?included_head.as_ref().map(|header| *header.number()),
		"Para head accumulated in JAM state at the anchor.",
	);

	let head_before = state.segment.included.as_ref().map(|header| header.hash());
	let entries_before = state.segment.entry_hashes();
	let outcome = state.segment.prune(included_head);
	// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
	macro_rules! log_prune {
		($level:ident) => {
			tracing::$level!(
				target: LOG_TARGET,
				?outcome,
				?head_before,
				head_after = ?state.segment.included.as_ref().map(|header| header.hash()),
				entries_before = ?entries_before,
				entries_after = ?state.segment.entry_hashes(),
				depth = state.segment.depth(),
				"Unincluded segment pruned against the accumulated head.",
			)
		};
	}
	match outcome {
		PruneOutcome::Diverged => log_prune!(warn),
		PruneOutcome::Advanced { popped } if popped > 0 => log_prune!(info),
		_ => log_prune!(debug),
	}

	let genesis_hash = para_client.info().genesis_hash;
	// The whole header, not just its hash: the mocked inherent advertises it to the runtime as
	// the relay chain's included para head, which is what `parachain-system` prunes its own
	// unincluded segment against.
	let included_header = match state.segment.included.clone() {
		Some(header) => header,
		// Nothing accumulated yet, so the para's included head is still its genesis block.
		None => para_client
			.header(genesis_hash)
			.map_err(|e| format!("genesis header: {e}"))?
			.ok_or_else(|| format!("genesis header {genesis_hash:?} not found"))?,
	};
	let included_hash = included_header.hash();
	let candidates = known_descendants::<Block>(para_backend, included_hash);
	let correlation = correlate(&reports, &candidates, included_hash);
	for report in &correlation.unmatched {
		tracing::info!(
			target: LOG_TARGET,
			wp_hash = ?report.wp_hash,
			segroot = ?report.segroot,
			source = ?report.source,
			?included_hash,
			"A work package is in flight for a block we do not hold; someone's chain is ahead of \
			 ours and we are waiting for the block announcement.",
		);
	}
	let reported_chain = correlation.chain.len();
	let adoption = state.segment.adopt(correlation.chain);
	tracing::debug!(
		target: LOG_TARGET,
		?included_hash,
		known_descendants = candidates.len(),
		reports = reports.len(),
		reported_chain,
		off_chain = ?correlation.off_chain,
		unmatched = correlation.unmatched.len(),
		from_state = adoption.from_state,
		from_local = adoption.from_local,
		depth = state.segment.depth(),
		entries = ?state.segment.entry_hashes(),
		"Unincluded segment reconstructed from JAM's in-flight reports.",
	);
	if adoption.forked {
		tracing::warn!(
			target: LOG_TARGET,
			?included_hash,
			reported_chain,
			from_local = adoption.from_local,
			entries = ?state.segment.entry_hashes(),
			"The reported chain does not join the blocks we authored; keeping ours and ignoring \
			 it — another collator is building a fork of the accumulated head.",
		);
	}

	let parent_source = match (state.segment.depth(), state.segment.included.is_some()) {
		(0, false) => "genesis",
		(0, true) => "accumulated head",
		_ => "deepest in-flight block",
	};
	let parent_header = match state.segment.tip() {
		Some(header) => header.clone(),
		// Nothing accumulated and nothing in flight, so the next block is the para's first one.
		None => included_header.clone(),
	};
	let parent_hash = parent_header.hash();
	let parent_link = state.segment.parent_link();
	tracing::debug!(
		target: LOG_TARGET,
		parent_source,
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		?parent_link,
		depth = state.segment.depth(),
		"Parent selected.",
	);
	// In-flight entries are blocks we authored and imported, so this only ever fires for a head
	// accumulated from a block we have not seen (another collator's, before it reached us).
	if para_client.header(parent_hash).map_err(|e| format!("parent header: {e}"))?.is_none() {
		tracing::warn!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			parent_number = %parent_header.number(),
			"The selected parent is not known locally; waiting for import/sync.",
		);
		return Ok(None);
	}

	if state.segment.is_full() {
		tracing::error!(
			target: LOG_TARGET,
			depth = state.segment.depth(),
			max_unincluded = MAX_UNINCLUDED,
			"The unincluded segment hit the local sanity bound; the runtime's capacity gate \
			 should have stopped us long before. Skipping this tick.",
		);
		return Ok(None);
	}

	let started = Instant::now();
	let can_build = can_build_upon::<Block, RuntimeApi, AuraId>(
		para_client,
		parent_hash,
		included_hash,
		para_slot,
	)?;
	tracing::debug!(
		target: LOG_TARGET,
		parent = ?parent_hash,
		?included_hash,
		?para_slot,
		depth = state.segment.depth(),
		can_build,
		elapsed_ms = started.elapsed().as_millis(),
		"Asked the runtime whether the unincluded segment has room.",
	);
	if !can_build {
		tracing::info!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			?included_hash,
			?para_slot,
			depth = state.segment.depth(),
			"The runtime refuses another block on this parent (capacity or velocity); \
			 skipping this tick.",
		);
		return Ok(None);
	}

	let authorities = para_client
		.runtime_api()
		.authorities(parent_hash)
		.map_err(|e| format!("authorities: {e}"))?;
	let Some(author_pub) = aura_internal::claim_slot::<<AuraId as AuraIdT>::BoundedPair>(
		para_slot,
		&authorities,
		keystore,
	)
	.await
	else {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			authorities = authorities.len(),
			"Slot not ours; skipping.",
		);
		return Ok(None);
	};
	let slot_claim =
		SlotClaim::unchecked::<<AuraId as AuraIdT>::BoundedPair>(author_pub, para_slot, now);

	tracing::info!(
		target: LOG_TARGET,
		?tip,
		?anchor,
		?para_slot,
		wall_jam_slot,
		timestamp = now.as_millis(),
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		?parent_link,
		depth = state.segment.depth(),
		"Building a parachain block against the JAM anchor.",
	);

	let inherent_data = create_inherent_data::<Block, RuntimeApi>(
		para_client,
		para_id,
		&parent_header,
		&included_header,
		wall_jam_slot,
		now,
		slot_duration,
	)
	.await?;

	let proposer = proposer_factory
		.init(&parent_header)
		.await
		.map_err(|e| format!("proposer init: {e}"))?;
	let storage_proof_recorder = ProofRecorder::<Block>::default();
	let mut extra_extensions = Extensions::new();
	extra_extensions.register(ProofSizeExt::new(storage_proof_recorder.clone()));

	let proposal = proposer
		.propose(ProposeArgs {
			inherent_data,
			inherent_digests: sp_runtime::generic::Digest {
				logs: vec![slot_claim.pre_digest().clone()],
			},
			max_duration: PROPOSAL_DURATION,
			block_size_limit: Some(MAX_POV_SIZE),
			extra_extensions,
			storage_proof_recorder: Some(storage_proof_recorder.clone()),
		})
		.await
		.map_err(|e| format!("propose: {e}"))?;

	let sealed_importable =
		cumulus_client_consensus_aura::collator::seal::<_, <AuraId as AuraIdT>::BoundedPair>(
			proposal.block,
			proposal.storage_changes,
			slot_claim.author_pub(),
			keystore,
		)
		.map_err(|e| format!("seal: {e}"))?;

	let block = Block::new(
		sealed_importable.post_header(),
		sealed_importable
			.body
			.clone()
			.ok_or_else(|| "sealed block has no body".to_string())?,
	);
	if !matches!(sealed_importable.state_action, StateAction::ApplyChanges(_)) {
		return Err("Building a block should return storage changes".into());
	}
	let proof = storage_proof_recorder.drain_storage_proof();

	block_import
		.import_block(sealed_importable)
		.await
		.map_err(|e| format!("import: {e}"))?;

	state.last_claimed_slot = Some(para_slot);
	state.segment.push(block.header().clone());
	tracing::info!(
		target: LOG_TARGET,
		block_hash = ?block.hash(),
		block_number = %block.header().number(),
		extrinsics = block.extrinsics().len(),
		proof_nodes = proof.iter_nodes().count(),
		anchor_proof_nodes = anchor_state_proof.nodes.len(),
		depth = state.segment.depth(),
		"Built and imported a parachain block.",
	);

	Ok(Some(JamCollatorMessage {
		parent_header,
		parent_link,
		block,
		proof,
		context,
		anchor_state_root: *state_root,
		anchor_state_proof,
		anchor_included_head: included_hash,
		anchor_slot: anchor.slot,
		triggered_by: tip,
	}))
}

/// The JAM reads one tick makes at its anchor, plus the refine context built around it.
///
/// `Ok(None)` means the reads disagree with each other and the tick has to be skipped.
async fn read_anchor<Jam: JamChainSource + JamStateSource + ?Sized>(
	jam: &Jam,
	tip: BlockDesc,
	service_id: ServiceId,
	para_id: u32,
) -> Result<Option<AnchorReads>, String> {
	// Anchoring at the cached tip itself is what buys back the freshness the phase-4 "parent of
	// best" convention gave away; `ANCHOR_OFFSET` walks back from it when distance is wanted.
	let mut anchor = tip;
	for _ in 0..ANCHOR_OFFSET {
		anchor = jam_read("parent", anchor.header_hash, jam.parent(anchor.header_hash)).await?;
	}
	let state_root =
		jam_read("stateRoot", anchor.header_hash, jam.state_root(anchor.header_hash)).await?;
	let beefy_root =
		jam_read("beefyRoot", anchor.header_hash, jam.beefy_root(anchor.header_hash)).await?;
	let finalized = jam_read("finalizedBlock", anchor.header_hash, jam.finalized_block()).await?;
	let lookup_anchor =
		jam_read("parent", finalized.header_hash, jam.parent(finalized.header_hash)).await?;
	let included = jam_read(
		"serviceValue",
		anchor.header_hash,
		jam.service_value(anchor.header_hash, service_id, &para_info_key(para_id.into())),
	)
	.await?;
	// Reconstruction is an addition to what the builder could already do, so losing it must not
	// cost a block: without reports the tick simply keeps whatever it is already holding, which
	// is exactly 5.3's behaviour.
	let reports = read_in_flight_reports(jam, anchor.header_hash, service_id)
		.await
		.unwrap_or_else(|error| {
			tracing::warn!(
				target: LOG_TARGET,
				anchor = ?anchor.header_hash,
				error,
				"Unable to read JAM's in-flight reports; this tick reconstructs nothing and \
				 builds on the blocks it already holds.",
			);
			Vec::new()
		});

	let started = Instant::now();
	let (proof, proved_head) =
		fetch_anchor_state_proof(jam, anchor.header_hash, &state_root, service_id, para_id).await?;
	tracing::debug!(
		target: LOG_TARGET,
		method = "stateProof",
		at = ?anchor.header_hash,
		nodes = proof.nodes.len(),
		values = proof.values.len(),
		proved = proved_head.is_some(),
		elapsed_ms = started.elapsed().as_millis(),
		"JAM read.",
	);
	// The proof and the head read above describe the same key at the same anchor, so anything
	// but equality means one of the two reads is stale — shipping it would only earn a refine
	// rejection.
	if proved_head != included {
		tracing::error!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			proved_head = proved_head.is_some(),
			read_head = included.is_some(),
			"The anchor state proof disagrees with the para head read at the same anchor; \
			 skipping this tick.",
		);
		return Ok(None);
	}

	Ok(Some(AnchorReads {
		anchor,
		context: RefineContext {
			anchor: anchor.header_hash,
			state_root,
			beefy_root,
			lookup_anchor: lookup_anchor.header_hash,
			lookup_anchor_slot: lookup_anchor.slot,
			prerequisites: Default::default(),
		},
		state_root,
		included,
		proof,
		reports,
	}))
}

/// Ask the runtime whether one more block may be chained onto `parent`.
///
/// Capacity and velocity are runtime-owned (`FixedVelocityConsensusHook`), and on JAM there is no
/// second, relay-side depth cap behind them — this answer is the only hard limit there is.
/// Mirrors `cumulus_client_consensus_aura::collators::can_build_upon`: with an empty segment the
/// runtime cannot recognise the parent as the included block (it never sees its own hash), so
/// that case is decided here. The para slot doubles as the API's relay slot because under the
/// mocked inherent both are the same wall-clock 6 s slot number.
fn can_build_upon<Block, RuntimeApi, AuraId>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	parent_hash: Block::Hash,
	included_hash: Block::Hash,
	para_slot: Slot,
) -> Result<bool, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	if parent_hash == included_hash {
		return Ok(true);
	}
	para_client
		.runtime_api()
		.can_build_upon(parent_hash, included_hash, para_slot)
		.map_err(|e| format!("can_build_upon: {e}"))
}

/// The mocked relay state one block is built against.
///
/// `current_para_block_head` is what the mock puts under the relay chain's para-head key, and
/// therefore what `parachain-system` reads as the *included* head and prunes its own unincluded
/// segment against. It has to be the head JAM has accumulated: advertising the parent — which is
/// what a dev-mode collator does, having no notion of inclusion at all — tells the runtime that
/// every block is included the moment it is authored, so its segment never grows and the
/// `size_after_included >= C` half of `can_build_upon` can never refuse anything.
///
/// The mocked relay parent number stays a pure function of the wall-clock JAM slot.
/// `FixedVelocityConsensusHook::on_state_proof` derives the block's Aura slot from it and panics
/// on a mismatch, so it must not move with the included head.
fn mocked_relay_state<Header: HeaderT>(
	para_id: ParaId,
	parent_header: &Header,
	included_header: &Header,
	wall_jam_slot: JamSlot,
	slot_duration: SlotDuration,
	relay_parent_offset: u32,
	upgrade_go_ahead: Option<UpgradeGoAhead>,
) -> MockValidationDataInherentDataProvider<()> {
	const RELAY_CHAIN_SLOT_DURATION_MILLIS: u64 = 6000;

	let current_para_block =
		UniqueSaturatedInto::<u32>::unique_saturated_into(*parent_header.number()) + 1;
	let relay_blocks_per_para_block =
		(slot_duration.as_millis() / RELAY_CHAIN_SLOT_DURATION_MILLIS).max(1) as u32;
	let target_relay_slot = jam_slot_as_relay_slot(wall_jam_slot);
	let relay_offset =
		(target_relay_slot as u32).saturating_sub(relay_blocks_per_para_block * current_para_block);

	MockValidationDataInherentDataProvider::<()> {
		current_para_block,
		para_id,
		current_para_block_head: Some(HeadData(included_header.encode())),
		relay_blocks_per_para_block,
		relay_offset,
		relay_parent_offset,
		para_blocks_per_relay_epoch: 10,
		upgrade_go_ahead,
		..Default::default()
	}
}

/// Timestamp + mocked parachain inherent, both derived from the wall clock.
///
/// Mirrors omni-node's relay-less dev mode, except the fake relay slot is the wall-clock JAM
/// timeslot instead of a plain wall-clock relay slot. Both values end up *inside* the block (as
/// the timestamp and `set_validation_data` extrinsics), so importers validate them rather than
/// recomputing them from their own clock — the same contract as in relay mode.
async fn create_inherent_data<Block, RuntimeApi>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_id: ParaId,
	parent_header: &Block::Header,
	included_header: &Block::Header,
	wall_jam_slot: JamSlot,
	timestamp: Timestamp,
	slot_duration: SlotDuration,
) -> Result<InherentData, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: CollectCollationInfo<Block> + RelayParentOffsetApi<Block>,
{
	let parent_hash = parent_header.hash();
	let should_send_go_ahead = para_client
		.runtime_api()
		.collect_collation_info(parent_hash, parent_header)
		.map(|info| info.new_validation_code.is_some())
		.unwrap_or_default();
	let relay_parent_offset =
		para_client.runtime_api().relay_parent_offset(parent_hash).unwrap_or_default();

	let mocked_parachain = mocked_relay_state(
		para_id,
		parent_header,
		included_header,
		wall_jam_slot,
		slot_duration,
		relay_parent_offset,
		should_send_go_ahead.then(|| {
			tracing::info!(
				target: LOG_TARGET,
				"Detected pending validation code, sending go-ahead signal."
			);
			UpgradeGoAhead::GoAhead
		}),
	);

	tracing::debug!(
		target: LOG_TARGET,
		current_para_block = mocked_parachain.current_para_block,
		target_relay_slot = jam_slot_as_relay_slot(wall_jam_slot),
		relay_offset = mocked_parachain.relay_offset,
		relay_blocks_per_para_block = mocked_parachain.relay_blocks_per_para_block,
		relay_parent_offset,
		included_head = ?included_header.hash(),
		"Mocked parachain inherent parameters.",
	);

	let timestamp_provider = sp_timestamp::InherentDataProvider::new(timestamp);

	let mut inherent_data = InherentData::new();
	timestamp_provider
		.provide_inherent_data(&mut inherent_data)
		.await
		.map_err(|e| format!("timestamp inherent: {e}"))?;
	mocked_parachain
		.provide_inherent_data(&mut inherent_data)
		.await
		.map_err(|e| format!("mocked parachain inherent: {e}"))?;

	Ok(inherent_data)
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_client_parachain_inherent::{INHERENT_IDENTIFIER, ParachainInherentData};
	use cumulus_pallet_parachain_system::RelayChainStateProof;
	use sp_core::H256;
	use sp_runtime::traits::{BlakeTwo256, Hash as _};

	type TestHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

	const TEST_PARA_ID: u32 = 1000;

	/// A chain of headers, each one the child of its predecessor — the shape the segment holds.
	fn chain(length: u32) -> Vec<TestHeader> {
		let mut parent = H256::zero();
		(1..=length)
			.map(|number| {
				let header = TestHeader::new(
					number,
					Default::default(),
					Default::default(),
					parent,
					Default::default(),
				);
				parent = header.hash();
				header
			})
			.collect()
	}

	/// A segment whose in-flight blocks were all authored by this collator.
	fn segment_of(
		included: Option<TestHeader>,
		entries: &[TestHeader],
	) -> UnincludedSegment<TestHeader> {
		UnincludedSegment {
			included,
			entries: entries
				.iter()
				.cloned()
				.map(|header| SegmentEntry { header, reported: None })
				.collect(),
		}
	}

	/// The mocked provider the builder hands to the block proposer.
	fn provider_for(
		parent: &TestHeader,
		included: &TestHeader,
		now: Timestamp,
	) -> MockValidationDataInherentDataProvider<()> {
		mocked_relay_state(
			ParaId::from(TEST_PARA_ID),
			parent,
			included,
			jam_slot_at(now),
			SlotDuration::from_millis(6000),
			0,
			None,
		)
	}

	/// The validation data the mocked provider actually produces — what travels inside the block
	/// and what `set_validation_data` is handed.
	async fn produced_by(
		provider: MockValidationDataInherentDataProvider<()>,
	) -> ParachainInherentData {
		let mut inherent_data = InherentData::new();
		provider
			.provide_inherent_data(&mut inherent_data)
			.await
			.expect("the mocked provider cannot fail");
		inherent_data
			.get_data(&INHERENT_IDENTIFIER)
			.expect("the parachain inherent data decodes")
			.expect("the mocked provider always supplies it")
	}

	/// The included para head read back the way `parachain-system` reads it before pruning its
	/// unincluded segment against it.
	fn included_head_of(data: &ParachainInherentData) -> HeadData {
		RelayChainStateProof::new(
			ParaId::from(TEST_PARA_ID),
			data.validation_data.relay_parent_storage_root,
			data.relay_chain_state.clone(),
		)
		.expect("the mocked relay state proof is well formed")
		.read_included_para_head()
		.expect("the mocked relay state carries an included para head")
	}

	fn reported_package(byte: u8, header: &TestHeader) -> ReportedPackage {
		ReportedPackage {
			wp_hash: WorkPackageHash::from([byte; 32]),
			segroot: export_of(&header.encode()).expect("a header fits a segment").segroot,
		}
	}

	/// A report for `header`, as a collator that holds the block would have to recognise it:
	/// nothing but the export root ties the two together.
	fn report_for(byte: u8, header: &TestHeader) -> InFlightReport {
		let package = reported_package(byte, header);
		InFlightReport {
			wp_hash: package.wp_hash,
			segroot: package.segroot,
			source: ReportSource::Availability,
		}
	}

	fn correlated(byte: u8, header: &TestHeader) -> Correlated<TestHeader> {
		Correlated { header: header.clone(), package: reported_package(byte, header) }
	}

	/// A work report as JAM state holds it, carrying one item refined by `service`.
	fn work_report(byte: u8, service: ServiceId, segroot: SegmentTreeRoot) -> WorkReport {
		let digest = jam_types::WorkDigest {
			service,
			code_hash: Default::default(),
			payload_hash: Default::default(),
			accumulate_gas: 0,
			result: Ok(Default::default()),
			refine_load: Default::default(),
		};
		WorkReport {
			package_spec: jam_std_common::WorkPackageSpec {
				hash: WorkPackageHash::from([byte; 32]),
				len: 0,
				erasure_root: Default::default(),
				exports_root: segroot,
				exports_count: 1,
			},
			context: RefineContext {
				anchor: Default::default(),
				state_root: Default::default(),
				beefy_root: Default::default(),
				lookup_anchor: Default::default(),
				lookup_anchor_slot: 0,
				prerequisites: Default::default(),
			},
			core_index: 0,
			authorizer_hash: Default::default(),
			auth_gas_used: 0,
			auth_output: Default::default(),
			sr_lookup: Default::default(),
			results: vec![digest].try_into().expect("a single digest always fits; qed"),
		}
	}

	/// The normal pipelined case: JAM accumulated the oldest in-flight block, so exactly that one
	/// block leaves the segment and the rest stay in flight.
	#[test]
	fn the_accumulated_block_leaves_the_segment() {
		let chain = chain(3);
		let mut segment = segment_of(None, &chain);

		assert_eq!(segment.prune(Some(chain[0].clone())), PruneOutcome::Advanced { popped: 1 });
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
	}

	/// Accumulation can catch up by several blocks at once (one JAM block accumulating a chain of
	/// work reports); everything up to and including the new head has to go.
	#[test]
	fn catching_up_pops_every_block_up_to_the_accumulated_head() {
		let chain = chain(4);
		let mut segment = segment_of(None, &chain);

		assert_eq!(segment.prune(Some(chain[2].clone())), PruneOutcome::Advanced { popped: 3 });
		assert_eq!(segment.entry_hashes(), vec![chain[3].hash()]);
	}

	/// The common tick: nothing was accumulated since the last one, so the segment must be left
	/// exactly as it was or the builder would re-author blocks that are still in flight.
	#[test]
	fn an_unmoved_head_leaves_the_segment_alone() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[1..]);

		assert_eq!(segment.prune(Some(chain[0].clone())), PruneOutcome::Unchanged);
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
	}

	/// A head we have nothing in flight against is another collator's block, not a divergence:
	/// with slot rotation this is what every tick after someone else's slot looks like.
	#[test]
	fn an_empty_segment_just_adopts_the_new_head() {
		let chain = chain(2);
		let mut segment = segment_of(Some(chain[0].clone()), &[]);

		assert_eq!(segment.prune(Some(chain[1].clone())), PruneOutcome::Advanced { popped: 0 });
		assert_eq!(segment.included.map(|header| header.hash()), Some(chain[1].hash()));
	}

	/// A head that is none of our in-flight blocks means another chain won; our blocks are
	/// orphaned and keeping them would only produce blocks the service refuses to chain on.
	#[test]
	fn a_foreign_head_diverges_and_clears_the_segment() {
		let ours = chain(3);
		let theirs = chain(2)
			.into_iter()
			.map(|mut header| {
				header.state_root = H256::repeat_byte(0xaa);
				header
			})
			.collect::<Vec<_>>();
		let mut segment = segment_of(Some(ours[0].clone()), &ours[1..]);

		assert_eq!(segment.prune(Some(theirs[1].clone())), PruneOutcome::Diverged);
		assert!(segment.entry_hashes().is_empty());
		assert_eq!(segment.included.map(|header| header.hash()), Some(theirs[1].hash()));
	}

	/// Authoring always extends the deepest block in flight — that is what pipelining is — and
	/// falls back to the accumulated head once the segment drains.
	#[test]
	fn the_tip_is_the_deepest_in_flight_block_else_the_accumulated_head() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[1..]);
		assert_eq!(segment.tip().map(|header| header.hash()), Some(chain[2].hash()));

		segment.reset();
		assert_eq!(segment.tip().map(|header| header.hash()), Some(chain[0].hash()));

		assert!(segment_of(None, &[]).tip().is_none());
	}

	/// The local bound is a backstop for a runtime that answers `can_build_upon` wrongly, so it
	/// must trip exactly at the depth it names.
	#[test]
	fn the_sanity_bound_trips_at_max_unincluded() {
		let chain = chain(MAX_UNINCLUDED as u32);
		assert!(!segment_of(None, &chain[..MAX_UNINCLUDED - 1]).is_full());
		assert!(segment_of(None, &chain).is_full());
	}

	/// Authoring twice in one parachain slot is equivocation, so the guard must reject a slot it
	/// has already claimed while letting the next one through.
	#[test]
	fn one_block_per_parachain_slot() {
		assert!(!para_slot_claimed(None, Slot::from(7)));
		assert!(para_slot_claimed(Some(Slot::from(7)), Slot::from(7)));
		assert!(para_slot_claimed(Some(Slot::from(7)), Slot::from(6)));
		assert!(!para_slot_claimed(Some(Slot::from(7)), Slot::from(8)));
	}

	/// The guard exists to catch a JAM chain that stalled, so the boundary matters: a tip that is
	/// exactly `MAX_TIP_LAG_SLOTS` behind is still usable, one slot older is not.
	#[test]
	fn a_tip_lagging_more_than_the_bound_is_stale() {
		assert!(tip_is_fresh(100, 100));
		assert!(tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS));
		assert!(!tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS - 1));
	}

	/// The timer has to land on the next slot boundary; being early would make the tick derive
	/// the previous slot's number and skip itself on the one-block-per-slot guard.
	#[test]
	fn the_timer_waits_for_the_next_slot_boundary() {
		let slot = Duration::from_millis(6000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(0), slot).as_millis(), 6000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(1000), slot).as_millis(), 5000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(5999), slot).as_millis(), 1);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(6000), slot).as_millis(), 6000);
	}

	/// The one handle correlation has: a report is recognised as carrying a block only because
	/// the block's header, laid out as an export segment, hashes to the root the report commits
	/// to. Nothing in the report's work output is read.
	#[test]
	fn a_report_matches_the_block_whose_header_reproduces_its_export_root() {
		let chain = chain(2);
		let correlation = correlate(&[report_for(1, &chain[1])], &chain[1..], chain[0].hash());

		assert_eq!(correlation.chain, vec![correlated(1, &chain[1])]);
		assert!(correlation.unmatched.is_empty());
		assert!(correlation.off_chain.is_empty());
	}

	/// A package in flight for a block nobody has sent us is the normal multi-collator race, not
	/// an error: it has to come back as "unmatched" so the tick can say it is waiting for the
	/// block announcement rather than dropping the report on the floor.
	#[test]
	fn a_report_for_a_block_we_do_not_hold_is_unmatched() {
		let chain = chain(2);
		let report = report_for(1, &chain[1]);

		let correlation = correlate::<TestHeader>(&[report], &[], chain[0].hash());

		assert!(correlation.chain.is_empty());
		assert_eq!(correlation.unmatched, vec![report]);
	}

	/// Packages depend on each other in block order, so the matches have to come back in that
	/// order — and the only ordering available is parent-hash linkage from the included head,
	/// which must hold whatever order the reports and the block tree arrive in.
	#[test]
	fn reports_are_threaded_into_a_chain_by_parent_linkage() {
		let chain = chain(4);
		let reports =
			[report_for(3, &chain[3]), report_for(1, &chain[1]), report_for(2, &chain[2])];
		let candidates = [chain[2].clone(), chain[3].clone(), chain[1].clone()];

		let correlation = correlate(&reports, &candidates, chain[0].hash());

		assert_eq!(
			correlation.chain,
			vec![correlated(1, &chain[1]), correlated(2, &chain[2]), correlated(3, &chain[3])],
		);
	}

	/// The multi-collator case 5.4 exists for: another collator's block is in flight and ours is
	/// its child. Both are reported, both must land on the same chain, and nothing distinguishes
	/// them here — correlation does not know or care who authored what.
	#[test]
	fn a_foreign_block_and_ours_thread_into_one_chain() {
		let chain = chain(3);
		let (theirs, ours) = (&chain[1], &chain[2]);

		let correlation = correlate(
			&[report_for(9, theirs), report_for(8, ours)],
			&[theirs.clone(), ours.clone()],
			chain[0].hash(),
		);

		assert_eq!(correlation.chain, vec![correlated(9, theirs), correlated(8, ours)]);
	}

	/// A block we hold that does not descend from the included head in an unbroken line is on
	/// somebody's fork; it must stay off the chain (its package cannot be a parent of ours) but
	/// still be reported as matched, or a fork would look exactly like a missing block.
	#[test]
	fn a_matched_block_off_the_included_line_does_not_join_the_chain() {
		let chain = chain(3);
		let orphan = &chain[2];

		let correlation = correlate(&[report_for(5, orphan)], &[orphan.clone()], chain[0].hash());

		assert!(correlation.chain.is_empty());
		assert!(correlation.unmatched.is_empty());
		assert_eq!(correlation.off_chain, vec![(WorkPackageHash::from([5u8; 32]), orphan.hash())]);
	}

	/// The segment is bounded, so the reconstructed chain must be too — a JAM state showing more
	/// in-flight blocks than the bound allows would otherwise rebuild a segment the capacity gate
	/// was never asked about.
	#[test]
	fn the_reconstructed_chain_is_depth_capped() {
		let chain = chain(MAX_UNINCLUDED as u32 + 3);
		let reports: Vec<_> =
			chain[1..].iter().enumerate().map(|(i, h)| report_for(i as u8, h)).collect();

		let correlation = correlate(&reports, &chain[1..], chain[0].hash());

		assert_eq!(correlation.chain.len(), MAX_UNINCLUDED);
	}

	/// Only our service's packages may be correlated: another service's report commits to its own
	/// export root, and matching one would chain our block onto a package that carries something
	/// else entirely.
	#[test]
	fn only_reports_for_our_service_are_kept() {
		let segroot = SegmentTreeRoot::from([1u8; 32]);
		let mut reports = Vec::new();

		push_report(&mut reports, &work_report(1, 7, segroot), ReportSource::Availability, 42);
		push_report(&mut reports, &work_report(2, 42, segroot), ReportSource::Availability, 42);

		assert_eq!(
			reports,
			vec![InFlightReport {
				wp_hash: WorkPackageHash::from([2u8; 32]),
				segroot,
				source: ReportSource::Availability,
			}],
		);
	}

	/// The same package sits in availability at one moment and in the ready queue the next, and a
	/// tick reads both; listing it twice would put the same block in the segment twice.
	#[test]
	fn a_package_listed_in_both_state_entries_is_kept_once() {
		let segroot = SegmentTreeRoot::from([1u8; 32]);
		let mut reports = Vec::new();

		push_report(&mut reports, &work_report(1, 42, segroot), ReportSource::Availability, 42);
		push_report(&mut reports, &work_report(1, 42, segroot), ReportSource::ReadyQueue, 42);

		assert_eq!(reports.len(), 1);
		assert_eq!(reports[0].source, ReportSource::Availability);
	}

	/// The restart case. Nothing was authored this session, so the entire segment comes from
	/// JAM's reports, and the next block is built on the deepest of them.
	#[test]
	fn a_restarted_collator_rebuilds_its_whole_segment_from_the_reports() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &[]);

		let adoption = segment.adopt(vec![correlated(1, &chain[1]), correlated(2, &chain[2])]);

		assert_eq!(adoption, Adoption { from_state: 2, from_local: 0, forked: false });
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
		assert_eq!(segment.tip().map(|header| header.hash()), Some(chain[2].hash()));
	}

	/// Our own packages are invisible in JAM state for a slot or two after submission, so a
	/// reconstruction that replaced the blocks we authored with what state can see would keep
	/// re-rooting the chain one block short of where it actually is.
	#[test]
	fn adopting_keeps_the_blocks_we_authored_on_top() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[2..]);

		let adoption = segment.adopt(vec![correlated(1, &chain[1])]);

		assert_eq!(adoption, Adoption { from_state: 1, from_local: 1, forked: false });
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
		assert_eq!(segment.parent_link(), ParentLink::Tip);
	}

	/// A block of ours that JAM has now reported is still ours: adopting it as well would put it
	/// in the segment twice and, worse, make the collation manager re-root onto a package it is
	/// already tracking.
	#[test]
	fn a_reported_block_we_authored_is_not_adopted_a_second_time() {
		let chain = chain(2);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[1..]);

		let adoption = segment.adopt(vec![correlated(1, &chain[1])]);

		assert_eq!(adoption, Adoption { from_state: 0, from_local: 1, forked: false });
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash()]);
		assert_eq!(segment.parent_link(), ParentLink::Tip);
	}

	/// Reported chain and our own blocks on different children of the included head: the priority
	/// order says our own tip wins, and abandoning our blocks for someone else's fork would throw
	/// away packages that are still perfectly able to accumulate.
	#[test]
	fn a_reported_fork_that_does_not_join_our_blocks_is_ignored() {
		let ours = chain(2);
		let mut theirs = ours[1].clone();
		theirs.state_root = H256::repeat_byte(0xaa);
		let mut segment = segment_of(Some(ours[0].clone()), &ours[1..]);

		let adoption = segment.adopt(vec![correlated(4, &theirs)]);

		assert_eq!(adoption, Adoption { from_state: 0, from_local: 1, forked: true });
		assert_eq!(segment.entry_hashes(), vec![ours[1].hash()]);
	}

	/// The priority order the timing gap forces: a package we submitted a moment ago is not
	/// reported yet, so our own tip has to outrank anything state can show; state in turn
	/// outranks the accumulated head, which is only the root case.
	#[test]
	fn the_parent_link_follows_the_priority_order() {
		let chain = chain(3);

		assert_eq!(segment_of(Some(chain[0].clone()), &[]).parent_link(), ParentLink::Included);

		let mut state_only = segment_of(Some(chain[0].clone()), &[]);
		state_only.adopt(vec![correlated(1, &chain[1])]);
		assert_eq!(
			state_only.parent_link(),
			ParentLink::Reported {
				wp_hash: WorkPackageHash::from([1u8; 32]),
				segroot: export_of(&chain[1].encode()).unwrap().segroot,
			},
			"with nothing of our own in flight, the reported tip is the parent",
		);

		let mut ours_on_top = segment_of(Some(chain[0].clone()), &chain[2..]);
		ours_on_top.adopt(vec![correlated(1, &chain[1])]);
		assert_eq!(
			ours_on_top.parent_link(),
			ParentLink::Tip,
			"a block we authored outranks anything read from state",
		);
	}

	/// The mocked relay state is the only thing telling `parachain-system` what has been
	/// included, and the pallet prunes its own unincluded segment against it. It must name the
	/// head JAM accumulated: naming the parent — what a dev-mode collator advertises, having no
	/// notion of inclusion — makes the runtime drop its whole segment every block, so the segment
	/// never grows and the capacity half of `can_build_upon` can never refuse a block.
	#[tokio::test]
	async fn the_mocked_relay_state_advertises_the_accumulated_head_as_included() {
		let blocks = chain(4);
		let (included, parent) = (&blocks[0], &blocks[3]);
		let now = Timestamp::new(jam_types::JAM_COMMON_ERA * 1000);

		let provider = provider_for(parent, included, now);
		// Only what counts as included moved; the block being authored is still the parent's
		// child, which is what the mocked relay parent number is computed for.
		assert_eq!(provider.current_para_block, parent.number() + 1);

		let head = included_head_of(&produced_by(provider).await);
		// The pallet hashes those bytes and compares the result to its segment entries, which
		// hold block hashes — so this is the comparison the runtime will actually make.
		assert_eq!(BlakeTwo256::hash(&head.0), included.hash());
		assert_ne!(BlakeTwo256::hash(&head.0), parent.hash());
	}

	/// `FixedVelocityConsensusHook::on_state_proof` derives the block's Aura slot from the relay
	/// parent number the mock advertises and panics unless the two agree, so that number has to
	/// stay a pure function of the wall clock. Advertising a lagging included head must not shift
	/// it, whichever block is included.
	#[tokio::test]
	async fn the_mocked_relay_parent_number_does_not_move_with_the_included_head() {
		let blocks = chain(4);
		let slot_duration = SlotDuration::from_millis(6000);

		for offset_ms in [0, 1, 5999, 6000, 123_456] {
			let now = Timestamp::new(jam_types::JAM_COMMON_ERA * 1000 + offset_ms);
			for included in [&blocks[0], &blocks[2], &blocks[3]] {
				let data = produced_by(provider_for(&blocks[3], included, now)).await;
				assert_eq!(
					u64::from(data.validation_data.relay_parent_number),
					*Slot::from_timestamp(now, slot_duration),
					"included {:?}, {offset_ms} ms into the JAM common era",
					included.hash(),
				);
			}
		}
	}

	/// The consensus hook panics unless the block's Aura slot equals the slot it derives from the
	/// mocked relay slot, so both wall-clock derivations must agree at every instant in a slot.
	#[test]
	fn the_aura_slot_matches_the_mocked_relay_slot() {
		let slot_duration = SlotDuration::from_millis(6000);
		for offset_ms in [0, 1, 2999, 5999, 6000, 6001, 123_456] {
			let now = Timestamp::new(jam_types::JAM_COMMON_ERA * 1000 + offset_ms);
			assert_eq!(
				*Slot::from_timestamp(now, slot_duration),
				jam_slot_as_relay_slot(jam_slot_at(now)),
				"disagreement {offset_ms} ms into the JAM common era",
			);
		}
	}
}

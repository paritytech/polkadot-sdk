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

//! The JAM block-builder task (phase 5a).
//!
//! A wall-clock **para-slot timer** drives authoring — one tick per parachain slot (the Aura slot
//! duration, 6 s today) — instead of one iteration per JAM best-block notification, which could
//! never pace a parachain producing several blocks per JAM slot. The JAM best-block subscription
//! is demoted to a tip cache that the tick reads synchronously; the anchor is that cached tip.
//!
//! Per tick: read the para head JAM has accumulated, pick the parent to author on out of the
//! blocks the local database holds below that head ([`select_parent`]), ask the runtime whether
//! one more block fits on top (`can_build_upon` — capacity and velocity are runtime-owned),
//! author, import, and hand `(block, proof, context)` to the collation task. Authoring never
//! waits for inclusion and, since phase 5a, never waits on JAM state for its parent either: work
//! packages carry no links to each other, so the only thing the builder needs from JAM is the
//! accumulated head it prunes and anchors against.
//!
//! JAM's in-flight work reports are still read every tick, but purely as a **monitor**: with no
//! links left, that log line is the only pre-accumulate view of the pipeline. Nothing in the tick
//! branches on it.
//!
//! Phases 1–5 inject a **mocked** parachain inherent (the runtime still requires
//! `set_validation_data`); its fake relay slot follows the wall-clock JAM slot, and it advertises
//! the head JAM has accumulated as the relay chain's included para head — that is what makes the
//! runtime's own segment tracking, and therefore its capacity limit, real. Both the mocked
//! validation data and the timestamp travel *inside* the block, so importers validate them
//! instead of recomputing them from their own clock — exactly as in relay mode.

use super::{
	JamCollatorMessage, LOG_TARGET, fetch_anchor_state_proof, jam_slot_as_relay_slot, jam_slot_at,
};
use crate::common::{
	ConstructNodeRuntimeApi, NodeBlock,
	aura::{AuraIdT, AuraRuntimeApi},
	types::{ParachainBackend, ParachainClient},
};
use codec::{Decode, DecodeAll};
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
use jam_types::RefineContext;
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
	collections::VecDeque,
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
/// How long the non-blocking in-flight monitor's reads may take before a tick gives up on them.
///
/// Comfortably under a parachain slot: the monitor exists to log the pipeline, and a tick that
/// waited on it would have made it a dependency of authoring, which phase 5a explicitly is not.
const MONITOR_BUDGET: Duration = Duration::from_secs(2);
/// Sanity bound on the number of in-flight blocks.
///
/// The runtime's consensus hook owns capacity for real (`can_build_upon`); this only stops a
/// runaway should that ever answer wrongly, and tripping it is a bug worth a loud log.
const MAX_UNINCLUDED: usize = 8;

pub(crate) struct BuilderTaskParams<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	/// Needed for the block tree alone: picking a parent means walking the children of the
	/// accumulated head, and `children` lives on the backend's blockchain, not on the client.
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub block_import: BI,
	pub proposer_factory: PF,
	pub keystore: KeystorePtr,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub jam: Arc<Jam>,
	pub message_sender: mpsc::Sender<JamCollatorMessage<Block>>,
}

/// What reading the accumulated para head did to the builder's view of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadMove {
	/// The head is still where it was; nothing of ours accumulated since the last tick.
	Unchanged,
	/// The head moved to a block this collator authored.
	Ours,
	/// The head moved to a block this collator did not author: another collator's — the common
	/// case under slot rotation — or one of ours from further back than [`MAX_UNINCLUDED`].
	Foreign,
}

/// Classify a change of the accumulated head, for the log alone.
///
/// Nothing branches on the answer any more: phase 5a picks the next parent from the local block
/// tree below whatever head JAM accepted, so a head this collator did not author is not a
/// divergence to heal but simply the block the next one is built on.
fn head_move<Header: HeaderT>(
	before: Option<&Header>,
	after: Option<&Header>,
	ours: &VecDeque<Header::Hash>,
) -> HeadMove {
	let hash = |header: Option<&Header>| header.map(|header| header.hash());
	match hash(after) {
		after if after == hash(before) => HeadMove::Unchanged,
		Some(after) if ours.contains(&after) => HeadMove::Ours,
		_ => HeadMove::Foreign,
	}
}

/// One block the local database holds below the accumulated head.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Descendant<Header> {
	header: Header,
	/// Generations below the accumulated head; `1` is a direct child of it.
	depth: usize,
}

/// The blocks the local database holds below `included`, breadth-first and depth-capped.
///
/// Everything the next block could possibly be built on is in here: the blocks this collator
/// authored (it imports them before submitting their packages) and the blocks other collators
/// announced and this node imported. Sibling forks are kept — several collators may have
/// authored children of the same block, and choosing between them is [`select_parent`]'s job.
///
/// The cap is [`MAX_UNINCLUDED`] generations: a deeper block could never be built on anyway, and
/// the walk must stay bounded when a fork storm widens the tree.
fn local_descendants<Block: NodeBlock>(
	backend: &ParachainBackend<Block>,
	included: Block::Hash,
) -> Vec<Descendant<Block::Header>> {
	let blockchain = backend.blockchain();
	let mut frontier = vec![included];
	let mut descendants = Vec::new();
	for depth in 1..=MAX_UNINCLUDED {
		let mut next = Vec::new();
		for hash in frontier {
			let children = match blockchain.children(hash) {
				Ok(children) => children,
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?hash,
						?error,
						"Cannot list a block's children; the parent search stops on this branch.",
					);
					continue;
				},
			};
			for child in children {
				if let Ok(Some(header)) = blockchain.header(child) {
					descendants.push(Descendant { header, depth });
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

/// The block to author on, out of what the local database holds below the accumulated head.
///
/// This is the whole of phase 5a's parent choice, and the order it applies is deterministic:
///
/// 1. **The deepest block wins.** That is what pipelining is — the collator keeps extending its
///    own unaccumulated tip instead of waiting for JAM to accept it.
/// 2. **At equal depth, a block this collator authored beats one imported from a peer.** Its
///    package is already in flight from here, and preferring it keeps consecutive ticks on one
///    line rather than hopping between siblings every slot.
/// 3. **Otherwise the first one seen**, which is the database's own order of children within a
///    generation, walked breadth-first by [`local_descendants`] — stable across ticks for as long
///    as the database does not change.
///
/// `None` means the local database holds nothing below the accumulated head, so the next block is
/// a direct child of it.
fn select_parent<'a, Header: HeaderT>(
	descendants: &'a [Descendant<Header>],
	ours: &VecDeque<Header::Hash>,
) -> Option<&'a Descendant<Header>> {
	let mut best: Option<&Descendant<Header>> = None;
	for candidate in descendants {
		let better = match best {
			None => true,
			Some(best) if candidate.depth != best.depth => candidate.depth > best.depth,
			Some(best) =>
				ours.contains(&candidate.header.hash()) && !ours.contains(&best.header.hash()),
		};
		if better {
			best = Some(candidate);
		}
	}
	best
}

/// One work package JAM currently has in flight for our service.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InFlightReport<Header: HeaderT> {
	wp_hash: WorkPackageHash,
	source: ReportSource,
	/// The parachain block the report's work digest says its package carries, read best-effort.
	/// `None` means the digest is in a shape this collator cannot read, which is never an error
	/// here — see [`decode_digest_head`].
	head: Option<ReportedHead<Header>>,
}

/// The parachain block a work digest names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReportedHead<Header: HeaderT> {
	hash: Header::Hash,
	number: <Header as HeaderT>::Number,
}

/// Read the parachain head out of a work digest's output, best-effort.
///
/// The work output belongs to the parachain service and its shape moves with the service — 5a.1
/// appends the block's number to it — so only the prefix every shape shares is parsed: the para
/// id and the head data, which *is* the encoded header, and therefore carries both the block hash
/// and its number without any of the rest being understood.
///
/// Anything that does not parse comes back as `None`. This is a logging aid: it must tolerate an
/// unknown format, a failed refine's error output, and outright garbage, and it must never fail a
/// tick or feed a decision.
fn decode_digest_head<Header: HeaderT>(output: &[u8]) -> Option<Header> {
	let mut input = output;
	let _para_id = u32::decode(&mut input).ok()?;
	let head_data = Vec::<u8>::decode(&mut input).ok()?;
	// Every shape of the output carries the parent head hash behind the head data. Requiring it
	// stops a short byte string that happens to decode as a vector from being read as a header.
	if input.len() < 32 {
		return None;
	}
	Header::decode_all(&mut &head_data[..]).ok()
}

/// Which of JAM's two in-flight state entries a report was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportSource {
	/// `C(10)`: reported by guarantors, data still being made available.
	Availability,
	/// `C(14)`: available, queued for accumulation behind its dependencies.
	ReadyQueue,
}

/// The monitor read, bounded and infallible.
///
/// It can neither fail a tick nor hold one up past its own budget: whatever goes wrong is logged
/// and comes back as "nothing seen". That is the whole contract phase 5a gives the monitor —
/// building never blocks on JAM state, and the pipeline view is a debugging instrument, not an
/// input to any decision.
///
/// The read is taken at the cached JAM tip rather than at the package anchor: it belongs to no
/// package, so the freshest block is the right one to ask (and with `ANCHOR_OFFSET` at zero the
/// two are the same block anyway).
async fn monitor_in_flight<Header: HeaderT, Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	at: HeaderHash,
	service_id: ServiceId,
) -> Vec<InFlightReport<Header>> {
	let read = read_in_flight_reports::<Header, Jam>(jam, at, service_id);
	match tokio::time::timeout(MONITOR_BUDGET, read).await {
		Ok(Ok(reports)) => reports,
		Ok(Err(error)) => {
			tracing::warn!(
				target: LOG_TARGET,
				?at,
				error,
				"Unable to read JAM's in-flight reports; this tick sees nothing of the pipeline.",
			);
			Vec::new()
		},
		Err(_) => {
			tracing::warn!(
				target: LOG_TARGET,
				?at,
				budget_ms = MONITOR_BUDGET.as_millis(),
				"The in-flight monitor read did not finish within its budget; giving up on it \
				 for this tick rather than holding the tick up.",
			);
			Vec::new()
		},
	}
}

/// The work packages JAM has in flight for our service, from both places state keeps them.
///
/// Phase 5a reads these for the log alone: with no links between packages, this is the only view
/// of the pipeline there is before accumulation, and it is what debugging and — later — recovery
/// consume. Nothing in a tick branches on the answer.
///
/// Filtering is by service id, a protocol-level field of a report's per-item digest.
async fn read_in_flight_reports<Header: HeaderT, Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	service_id: ServiceId,
) -> Result<Vec<InFlightReport<Header>>, String> {
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

	// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
	macro_rules! log_read {
		($message:literal) => {
			tracing::debug!(
				target: LOG_TARGET,
				method = "availability + readyQueue",
				at = ?anchor,
				service_id,
				cores = availability.len(),
				epoch_phases = ready.len(),
				from_availability,
				from_ready_queue = reports.len() - from_availability,
				unreadable_digests = reports.iter().filter(|report| report.head.is_none()).count(),
				?reports,
				availability_ms,
				ready_queue_ms,
				$message,
			)
		};
	}
	if reports.is_empty() {
		log_read!("JAM read: nothing of ours is in flight — no work package for our service is \
		           in availability or in the ready queue at this anchor.");
	} else {
		log_read!("JAM read: the work packages in flight for our service.");
	}
	Ok(reports)
}

/// Keep a report if it refines something for our service and is not already listed.
///
/// The same package can sit in both state entries across a tick's two reads, and JAM keys them by
/// package hash, so the hash is what deduplicates. A report whose digest cannot be read is still
/// kept: the package is in flight either way, and that is the fact the monitor is there to show.
fn push_report<Header: HeaderT>(
	reports: &mut Vec<InFlightReport<Header>>,
	report: &WorkReport,
	source: ReportSource,
	service_id: ServiceId,
) {
	let Some(digest) = report.results.iter().find(|digest| digest.service == service_id) else {
		return;
	};
	let wp_hash = report.package_spec.hash;
	if reports.iter().any(|seen| seen.wp_hash == wp_hash) {
		return;
	}
	let head = digest
		.result
		.as_ref()
		.ok()
		.and_then(|output| decode_digest_head::<Header>(&output.0))
		.map(|header| ReportedHead { hash: header.hash(), number: *header.number() });
	reports.push(InFlightReport { wp_hash, source, head });
}

/// What the builder remembers between ticks.
struct BuilderState<Header: HeaderT> {
	/// Aura guard: the parachain slot last claimed.
	last_claimed_slot: Option<Slot>,
	/// The para head JAM has accumulated, as of the last tick that managed to read it.
	included: Option<Header>,
	/// The blocks this collator authored most recently, oldest first, capped at
	/// [`MAX_UNINCLUDED`]. Its only job is to let [`select_parent`] prefer our own block over a
	/// peer's at the same depth, so it needs no pruning beyond that cap: a block below the
	/// accumulated head never turns up among its descendants again.
	own_recent: VecDeque<Header::Hash>,
}

impl<Header: HeaderT> BuilderState<Header> {
	fn remember_authored(&mut self, block_hash: Header::Hash) {
		self.own_recent.push_back(block_hash);
		while self.own_recent.len() > MAX_UNINCLUDED {
			self.own_recent.pop_front();
		}
	}
}

/// Everything one tick reads from JAM at its anchor.
struct AnchorReads {
	anchor: BlockDesc,
	context: RefineContext,
	state_root: StateRootHash,
	/// The para's entry in the service's state, exactly as stored; `None` = no head yet.
	included: Option<Vec<u8>>,
	proof: StateProof,
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

	// Nothing here survives a restart, and nothing needs to: every tick reads the accumulated
	// head from JAM and finds the blocks above it in the local database, which is also how this
	// collator extends a block another one authored.
	let mut state =
		BuilderState { last_claimed_slot: None, included: None, own_recent: VecDeque::new() };
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
		included_head = ?state.included.as_ref().map(|header| header.hash()),
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
	// nothing fresh to say, and the next tick supersedes this one anyway. The monitor read runs
	// alongside the anchor reads rather than after them, so watching the pipeline costs the tick
	// no latency of its own, and it carries its own bound: nothing here may skip a tick.
	let (reads, reports) = match tokio::time::timeout(
		slot_duration.as_duration(),
		futures::future::join(
			read_anchor(jam, tip, service_id, para_id_u32),
			monitor_in_flight::<Block::Header, _>(jam, tip.header_hash, service_id),
		),
	)
	.await
	{
		Ok((reads, reports)) => (reads?, reports),
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
	let Some(AnchorReads { anchor, context, state_root, included, proof: anchor_state_proof }) =
		reads
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

	let head_before = state.included.as_ref().map(|header| header.hash());
	let moved = head_move(state.included.as_ref(), included_head.as_ref(), &state.own_recent);
	state.included = included_head;
	// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
	macro_rules! log_head_move {
		($level:ident) => {
			tracing::$level!(
				target: LOG_TARGET,
				?moved,
				?head_before,
				head_after = ?state.included.as_ref().map(|header| header.hash()),
				head_number = ?state.included.as_ref().map(|header| *header.number()),
				own_recent = ?state.own_recent,
				"The para head JAM has accumulated.",
			)
		};
	}
	match moved {
		HeadMove::Unchanged => log_head_move!(debug),
		_ => log_head_move!(info),
	}

	let genesis_hash = para_client.info().genesis_hash;
	// The whole header, not just its hash: the mocked inherent advertises it to the runtime as
	// the relay chain's included para head, which is what `parachain-system` prunes its own
	// unincluded segment against.
	let included_header = match state.included.clone() {
		Some(header) => header,
		// Nothing accumulated yet, so the para's included head is still its genesis block.
		None => para_client
			.header(genesis_hash)
			.map_err(|e| format!("genesis header: {e}"))?
			.ok_or_else(|| format!("genesis header {genesis_hash:?} not found"))?,
	};
	let included_hash = included_header.hash();

	let descendants = local_descendants::<Block>(para_backend, included_hash);
	let selected = select_parent(&descendants, &state.own_recent);
	let (parent_header, depth) = match selected {
		Some(parent) => (parent.header.clone(), parent.depth),
		None => (included_header.clone(), 0),
	};
	let parent_hash = parent_header.hash();
	let parent_source = match (depth, state.included.is_some()) {
		(0, false) => "genesis",
		(0, true) => "accumulated head",
		_ if state.own_recent.contains(&parent_hash) => "our own in-flight block",
		_ => "another collator's in-flight block",
	};
	tracing::debug!(
		target: LOG_TARGET,
		parent_source,
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		depth,
		?included_hash,
		local_descendants = descendants.len(),
		siblings_at_depth = descendants.iter().filter(|other| other.depth == depth).count(),
		"Parent selected out of the blocks the local database holds below the accumulated head.",
	);
	// Only the accumulated-head fallback can name a block we do not have: everything the walk
	// returns came out of the local database in the first place.
	if para_client.header(parent_hash).map_err(|e| format!("parent header: {e}"))?.is_none() {
		tracing::warn!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			parent_number = %parent_header.number(),
			"The accumulated head is not known locally; waiting for import/sync.",
		);
		return Ok(None);
	}

	// The monitor's derived events. Nothing branches on them; they are the pre-accumulation view
	// of the pipeline, which with no links left is the only one there is.
	for report in &reports {
		let known = match &report.head {
			Some(head) => para_client.header(head.hash).ok().flatten().is_some(),
			None => false,
		};
		match &report.head {
			Some(head) if !known => tracing::warn!(
				target: LOG_TARGET,
				wp_hash = ?report.wp_hash,
				source = ?report.source,
				head = ?head.hash,
				head_number = %head.number,
				?included_hash,
				"A work package is in flight for a parachain block we do not hold. Somebody's \
				 chain is ahead of ours and we have not been told about the block — normally the \
				 announcement is still on its way, but a block withheld on purpose looks exactly \
				 like this.",
			),
			Some(head) => tracing::debug!(
				target: LOG_TARGET,
				wp_hash = ?report.wp_hash,
				source = ?report.source,
				head = ?head.hash,
				head_number = %head.number,
				"A work package is in flight for a block we hold.",
			),
			None => tracing::debug!(
				target: LOG_TARGET,
				wp_hash = ?report.wp_hash,
				source = ?report.source,
				"A work package is in flight; its work digest is in a shape we cannot read, so \
				 which block it carries is unknown here.",
			),
		}
	}

	if depth >= MAX_UNINCLUDED {
		tracing::error!(
			target: LOG_TARGET,
			depth,
			max_unincluded = MAX_UNINCLUDED,
			parent = ?parent_hash,
			"The chain of unaccumulated blocks hit the local sanity bound; the runtime's capacity \
			 gate should have stopped us long before. Skipping this tick.",
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
		depth,
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
			depth,
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
		parent_source,
		depth,
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
	state.remember_authored(block.hash());
	tracing::info!(
		target: LOG_TARGET,
		block_hash = ?block.hash(),
		block_number = %block.header().number(),
		extrinsics = block.extrinsics().len(),
		proof_nodes = proof.iter_nodes().count(),
		anchor_proof_nodes = anchor_state_proof.nodes.len(),
		depth = depth + 1,
		"Built and imported a parachain block.",
	);

	Ok(Some(JamCollatorMessage {
		parent_header,
		block,
		proof,
		context,
		anchor_state_root: *state_root,
		anchor_state_proof,
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

	/// A work output in the shape the parachain service produces: para id, head data (the
	/// encoded header itself), the parent head hash, and — since 5a.1 — the block's number.
	fn work_output(header: &TestHeader, with_number: bool) -> Vec<u8> {
		let mut output = (0u32, header.encode(), [7u8; 32]).encode();
		if with_number {
			output.extend(header.number().encode());
		}
		output
	}

	/// A work report as JAM state holds it, carrying one item refined by `service`.
	fn work_report(byte: u8, service: ServiceId) -> WorkReport {
		work_report_with(byte, service, Ok(Default::default()))
	}

	/// The same, with the refinement result spelled out.
	fn work_report_with(
		byte: u8,
		service: ServiceId,
		result: Result<jam_types::WorkOutput, jam_types::WorkError>,
	) -> WorkReport {
		let digest = jam_types::WorkDigest {
			service,
			code_hash: Default::default(),
			payload_hash: Default::default(),
			accumulate_gas: 0,
			result,
			refine_load: Default::default(),
		};
		WorkReport {
			package_spec: jam_std_common::WorkPackageSpec {
				hash: WorkPackageHash::from([byte; 32]),
				len: 0,
				erasure_root: Default::default(),
				exports_root: Default::default(),
				exports_count: 0,
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

	/// The descendants of the accumulated head, as [`local_descendants`] would hand them over:
	/// breadth-first, so shallower blocks come first and siblings keep the order they were seen.
	fn descendants(blocks: &[(&TestHeader, usize)]) -> Vec<Descendant<TestHeader>> {
		blocks
			.iter()
			.map(|(header, depth)| Descendant { header: (*header).clone(), depth: *depth })
			.collect()
	}

	fn ours(headers: &[&TestHeader]) -> VecDeque<H256> {
		headers.iter().map(|header| header.hash()).collect()
	}

	/// A sibling of `header`: same parent and number, different state root — the shape two
	/// collators authoring in the same slot produce.
	fn sibling(header: &TestHeader) -> TestHeader {
		let mut sibling = header.clone();
		sibling.state_root = H256::repeat_byte(0xaa);
		sibling
	}

	/// Pipelining in one line: the collator extends the deepest block it holds rather than
	/// re-rooting on the accumulated head every slot.
	#[test]
	fn the_deepest_local_descendant_is_the_parent() {
		let chain = chain(3);
		let candidates = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);

		let parent = select_parent(&candidates, &VecDeque::new()).expect("the tree is not empty");

		assert_eq!(parent.header.hash(), chain[2].hash());
		assert_eq!(parent.depth, 3);
	}

	/// Nothing below the accumulated head means the next block is a direct child of it, which is
	/// the root case and the state a collator restarts into.
	#[test]
	fn an_empty_local_tree_selects_no_parent() {
		assert!(select_parent::<TestHeader>(&[], &VecDeque::new()).is_none());
	}

	/// Two collators authored children of the same block. Ours is the one whose package this
	/// collator already has in flight, so extending it keeps consecutive ticks on one line
	/// instead of hopping between siblings every slot.
	#[test]
	fn our_own_block_wins_a_tie_at_the_same_depth() {
		let chain = chain(2);
		let theirs = sibling(&chain[1]);

		let theirs_first =
			descendants(&[(&chain[0], 1), (&theirs, 2), (&chain[1], 2)]);
		let parent = select_parent(&theirs_first, &ours(&[&chain[1]])).expect("not empty");

		assert_eq!(parent.header.hash(), chain[1].hash());
	}

	/// A deeper block of somebody else's still beats a shallower one of ours: depth is the first
	/// rule, and preferring our own shallower block would throw away a slot of pipelining.
	#[test]
	fn depth_beats_ownership() {
		let chain = chain(3);
		let candidates = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);

		let parent = select_parent(&candidates, &ours(&[&chain[0]])).expect("not empty");

		assert_eq!(parent.header.hash(), chain[2].hash());
	}

	/// With neither block ours, the tie is broken by arrival: the first one the walk saw. The
	/// database enumerates a block's children in import order, so this keeps a collator on the
	/// same branch across ticks instead of flipping between two strangers' siblings.
	#[test]
	fn a_tie_between_two_foreign_blocks_goes_to_the_first_seen() {
		let chain = chain(2);
		let theirs = sibling(&chain[1]);
		let ours_none = VecDeque::new();

		let first = descendants(&[(&chain[1], 1), (&theirs, 1)]);
		let reversed = descendants(&[(&theirs, 1), (&chain[1], 1)]);

		assert_eq!(select_parent(&first, &ours_none).unwrap().header.hash(), chain[1].hash());
		assert_eq!(select_parent(&reversed, &ours_none).unwrap().header.hash(), theirs.hash());
	}

	/// The head standing still is the common tick and must stay quiet in the log; the two ways it
	/// moves are worth a line each, and only the log distinguishes them.
	#[test]
	fn a_head_that_moved_says_whether_it_was_ours() {
		let chain = chain(2);
		let mine = ours(&[&chain[1]]);

		assert_eq!(head_move(Some(&chain[0]), Some(&chain[0]), &mine), HeadMove::Unchanged);
		assert_eq!(head_move(Some(&chain[0]), Some(&chain[1]), &mine), HeadMove::Ours);
		assert_eq!(
			head_move(Some(&chain[0]), Some(&sibling(&chain[1])), &mine),
			HeadMove::Foreign,
		);
		assert_eq!(
			head_move(Some(&chain[0]), None, &mine),
			HeadMove::Foreign,
			"a head that vanished is nothing we authored — it cannot happen, and if it did the \
			 log is where it should show up",
		);
	}

	/// The para's very first block accumulating: there was no head before, and it is ours.
	#[test]
	fn the_first_head_of_all_is_a_move() {
		let chain = chain(1);
		assert_eq!(head_move(None, Some(&chain[0]), &ours(&[&chain[0]])), HeadMove::Ours);
		assert_eq!(head_move::<TestHeader>(None, None, &VecDeque::new()), HeadMove::Unchanged);
	}

	/// The remembered blocks are a tie-breaker, not a ledger, so the list stays bounded — and the
	/// bound is the same depth the walk and the sanity tripwire use.
	#[test]
	fn the_remembered_blocks_stay_bounded() {
		let chain = chain(MAX_UNINCLUDED as u32 + 2);
		let mut state = BuilderState::<TestHeader> {
			last_claimed_slot: None,
			included: None,
			own_recent: VecDeque::new(),
		};

		for header in &chain {
			state.remember_authored(header.hash());
		}

		assert_eq!(state.own_recent.len(), MAX_UNINCLUDED);
		assert!(!state.own_recent.contains(&chain[0].hash()), "the oldest is forgotten first");
		assert!(state.own_recent.contains(&chain[chain.len() - 1].hash()));
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

	/// Only our service's packages belong in the monitor: another service's report says nothing
	/// about this parachain, and logging it as ours would make the pipeline view a lie.
	#[test]
	fn only_reports_for_our_service_are_kept() {
		let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

		push_report(&mut reports, &work_report(1, 7), ReportSource::Availability, 42);
		push_report(&mut reports, &work_report(2, 42), ReportSource::Availability, 42);

		assert_eq!(
			reports,
			vec![InFlightReport {
				wp_hash: WorkPackageHash::from([2u8; 32]),
				source: ReportSource::Availability,
				head: None,
			}],
		);
	}

	/// The same package sits in availability at one moment and in the ready queue the next, and a
	/// tick reads both; listing it twice would double every package in the monitor's view.
	#[test]
	fn a_package_listed_in_both_state_entries_is_kept_once() {
		let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

		push_report(&mut reports, &work_report(1, 42), ReportSource::Availability, 42);
		push_report(&mut reports, &work_report(1, 42), ReportSource::ReadyQueue, 42);

		assert_eq!(reports.len(), 1);
		assert_eq!(reports[0].source, ReportSource::Availability);
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

	/// The monitor reads which block a package carries out of the work digest, and the shape of
	/// that output belongs to the service: 5a.1 appends the block number to it. Both shapes have
	/// to read the same, or the collator would go blind against one version of the service.
	#[test]
	fn both_shapes_of_the_work_output_yield_the_same_block() {
		let header = chain(1).remove(0);

		for with_number in [false, true] {
			let decoded = decode_digest_head::<TestHeader>(&work_output(&header, with_number))
				.expect("the shared prefix parses in either shape");
			assert_eq!(decoded.hash(), header.hash());
			assert_eq!(*decoded.number(), *header.number());
		}
	}

	/// ...and anything else must come back as "unknown", never as a panic and never as an error
	/// that could reach the tick: a service this collator was not built against, a refine that
	/// failed, or plain garbage all end up here.
	#[test]
	fn an_unreadable_work_output_is_simply_unknown() {
		let header = chain(1).remove(0);
		let well_formed = work_output(&header, false);

		for (what, output) in [
			("empty", Vec::new()),
			("garbage", vec![0xff; 64]),
			("truncated head data", well_formed[..well_formed.len() - 40].to_vec()),
			("a vector that is no header", (0u32, vec![1u8, 2, 3], [0u8; 32]).encode()),
		] {
			assert!(
				decode_digest_head::<TestHeader>(&output).is_none(),
				"{what} must not be read as a block",
			);
		}
	}

	/// A package whose digest cannot be read is still in flight, and that is the fact the monitor
	/// exists to show — dropping the report would hide a package from the only pre-accumulation
	/// view there is. Same for one whose refinement failed outright.
	#[test]
	fn a_package_with_an_unreadable_digest_is_still_reported() {
		let header = chain(1).remove(0);
		let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

		let output = jam_types::WorkOutput(work_output(&header, true));
		let readable = work_report_with(1, 42, Ok(output));
		push_report(&mut reports, &readable, ReportSource::Availability, 42);
		let unreadable = work_report_with(2, 42, Ok(jam_types::WorkOutput(vec![0xff; 8])));
		push_report(&mut reports, &unreadable, ReportSource::ReadyQueue, 42);
		let failed = work_report_with(3, 42, Err(jam_types::WorkError::Panic));
		push_report(&mut reports, &failed, ReportSource::ReadyQueue, 42);

		assert_eq!(reports.len(), 3);
		assert_eq!(
			reports[0].head,
			Some(ReportedHead { hash: header.hash(), number: *header.number() }),
		);
		assert_eq!(reports[1].head, None);
		assert_eq!(reports[2].head, None, "a failed refinement names no block");
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

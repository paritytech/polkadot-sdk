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

//! The verified pool: everything the fetch pipeline proved under included
//! source roots, held for the inherent provider and the lift assembler.
//!
//! Per consumed channel stream the pool keeps a *ledger*: one contiguous run
//! of verified payloads with their leaf hashes, the frontier at its base,
//! and the *binding* — the newest included `StreamsRoot` the run verified
//! under, with the stream's head under it, the extension proof from the
//! ledger's end to that head and the tree proof of the stream's entry.
//! Because every chunk's verification appends onto the previously verified
//! frontier, the newest chunk's root binds the entire run: re-requesting
//! under a newer root (even payload-free, when caught up) refreshes the
//! binding of all pooled history at once.
//!
//! Per read ack register the pool keeps the latest verified head reads
//! (payload + inclusion proof for the inherent; frontier + tree proof for
//! the lift), keyed by their read-context leaf count — the consumption
//! record's `end.leaf_count` is how the assembler finds the read a block
//! acted on. Reads are handed to at most one *live* block; a hand-out to a
//! block that never lands is reconciled back to handable state against the
//! authoring parent's applied register view (see [`SpecMsgPool::build_inherent`]).
//!
//! Nothing in here is trusted beyond what verification proved: the pool
//! stores only what [`crate::verify`] derived, tagged with the root it was
//! derived under. Staleness is handled by regeneration, not invalidation —
//! streams are append-only, so a run bound under an old included root stays
//! a valid prefix under every newer one; the fetcher's next round under the
//! newer root re-binds it.
//!
//! The pool also tracks which sources have a fetch round *in flight*
//! ([`SpecMsgPool::begin_round`] / [`RoundGuard`]): the fetcher and the
//! proposer both react to the same relay-chain import, and the proposer's
//! pool snapshot usually beats a round completing milliseconds later. The
//! fetch loop marks the source the moment it receives the trigger — before
//! any round work, so even a snapshot firing right off the relay import
//! finds the marker — and [`SpecMsgPool::wait_for_in_flight_rounds`] is
//! the authoring side's bounded grace window over it.
//!
//! Two photo-finish edges sit either side of that in-flight marker, and the
//! grace window covers both (issue 10):
//! - *late offer*: the monitor has PUSHED an included offer but the fetcher has not yet dequeued
//!   it, so no round is marked at the snapshot instant. [`SpecMsgPool::note_pending_offer`] records
//!   a per-source "offer pushed at <instant>" the moment the monitor sends, and the window waits on
//!   it exactly like an in-flight round (same `bound` expiry); [`SpecMsgPool::begin_round`]
//!   supersedes it when the real round starts.
//! - *store visibility*: a round completed and its guard already dropped a hair before the
//!   snapshot, so nothing is marked yet the just-fetched material has not settled into the read
//!   view. [`SpecMsgPool::end_round`] retains a per-source "completed at <instant>" for a brief
//!   [`COMPLETION_RETENTION`], so a snapshot landing microseconds later waits a beat and re-reads
//!   rather than sealing an empty inherent.

use std::{
	collections::{BTreeMap, VecDeque},
	time::{Duration, Instant},
};

use codec::DecodeAll;
use futures::{channel::oneshot, FutureExt};
use parking_lot::Mutex;

use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	hash_leaf, MMRExtensionProof, MmrFrontier, MmrInclusionProof, Register, RequiresLift,
	SpecHasher, SpecMsgInherentData, StreamId, StreamsRoot, TreeInclusionProof, LEAF_VERSION,
};
use polkadot_core_primitives::Hash;

use crate::{nodes::HistoricNodes, LOG_TARGET};

/// How many verified register head reads are retained per ack stream: the
/// newest read feeds the next block, the older ones remain addressable for
/// lift assembly of blocks built against them (a couple of authoring rounds
/// of slack is plenty — regeneration refetches anyway).
const RETAINED_REGISTER_READS: usize = 4;

/// What the inherent provider hands the runtime per block, bounded so an
/// honest provider never hits the runtime's per-block caps (issue 06); the
/// leftover backlog stays pooled for the next block — partial consumption
/// is a lift case, fully supported.
#[derive(Clone, Copy, Debug)]
pub struct InherentBudget {
	/// Total payload bytes across all streams.
	pub max_bytes: usize,
	/// Total items (channel items + register reads) — must stay below the
	/// runtime's `MaxTouchedStreams`.
	pub max_streams: usize,
}

impl Default for InherentBudget {
	fn default() -> Self {
		// Conservative MVP defaults: well under the PoV budget and any
		// sensible `MaxTouchedStreams` configuration.
		Self { max_bytes: 256 * 1024, max_streams: 8 }
	}
}

/// Errors of pool bookkeeping. All of them are local bugs (the fetcher
/// violating the ledger contract), never network-induced.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PoolError {
	/// A chunk was appended that does not continue the ledger's run.
	#[error("chunk does not continue the pooled run")]
	NotContiguous,
}

/// Why lift material for a recorded stream could not be produced from the
/// pool. The candidate then goes out without lifts — and a non-empty record
/// without lifts fails validation, so these are loud errors, not fallbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LiftMaterialError {
	/// The stream has no pooled ledger / read covering the record.
	#[error("no pooled material covers the recorded stream state")]
	NotCovered,
}

/// The binding of a channel ledger to the newest included root it verified
/// under (see the module docs).
#[derive(Clone, Debug)]
pub struct ChannelBinding {
	/// The included `StreamsRoot` the run verified under.
	pub root: StreamsRoot,
	/// The stream's proven leaf count under `root`.
	pub head: u64,
	/// Extension from the ledger's end to `head` (empty when caught up).
	pub extension: MMRExtensionProof,
	/// The stream entry's tree proof under `root`.
	pub tree_proof: TreeInclusionProof,
}

/// One consumed channel stream's contiguous verified run.
struct ChannelLedger {
	/// Frontier at the first retained payload (`base.leaf_count` is its
	/// position).
	base: MmrFrontier,
	/// Frontier after the last retained payload — the fetch resume state.
	end: MmrFrontier,
	/// The payloads at positions `base.leaf_count..end.leaf_count`.
	payloads: VecDeque<Vec<u8>>,
	/// Their leaf hashes (kept even where payloads were handed over — lift
	/// generation needs hashes, not payloads).
	leaves: VecDeque<Hash>,
	/// The newest included root binding the run; `None` only transiently
	/// (a ledger is created by a verified chunk, which brings one).
	binding: Option<ChannelBinding>,
}

impl ChannelLedger {
	fn base_position(&self) -> u64 {
		self.base.leaf_count
	}

	fn end_position(&self) -> u64 {
		self.end.leaf_count
	}

	/// Leaf-hash view for proof generation over the retained run.
	fn nodes(&self) -> HistoricNodes<LedgerLeaves<'_>> {
		HistoricNodes::new(
			LedgerLeaves { base: self.base_position(), leaves: &self.leaves },
			&self.base,
		)
	}
}

/// Global-position leaf access over a ledger's retained run.
struct LedgerLeaves<'a> {
	base: u64,
	leaves: &'a VecDeque<Hash>,
}

impl crate::nodes::LeafHashes for LedgerLeaves<'_> {
	fn leaf_hash(&self, index: u64) -> Option<Hash> {
		index
			.checked_sub(self.base)
			.and_then(|local| usize::try_from(local).ok())
			.and_then(|local| self.leaves.get(local).copied())
	}
}

/// One verified register head read.
#[derive(Clone, Debug)]
pub struct RegisterRead {
	/// The included root the read verified under.
	pub root: StreamsRoot,
	/// The register leaf's payload (a SCALE-encoded `Register`).
	pub payload: Vec<u8>,
	/// The inclusion proof pinning the leaf as the head — what the inherent
	/// carries.
	pub inclusion: MmrInclusionProof,
	/// The ack stream's full frontier under `root` — the read context the
	/// consumption record ends at.
	pub frontier: MmrFrontier,
	/// The stream entry's tree proof under `root` — the lift's tail.
	pub tree_proof: TreeInclusionProof,
}

/// One ack stream's retained head reads.
#[derive(Default)]
struct RegisterReads {
	/// Keyed by the read context's leaf count (newest last).
	reads: BTreeMap<u64, RegisterRead>,
	/// Context count of the newest read not yet handed to a block.
	fresh: Option<u64>,
	/// Context count of a read handed to exactly one block whose landing is
	/// not yet confirmed — reconciled against the authoring parent's applied
	/// register view at the next hand-out opportunity
	/// ([`Self::reconcile_handed`]).
	handed: Option<u64>,
}

impl RegisterReads {
	/// Reconciles the outstanding hand-out (if any) against the authoring
	/// parent's applied register view — the same non-destructive fork
	/// discipline the channel cursors get: a hand-out whose information the
	/// parent state reflects landed on the branch being built on and stays
	/// consumed *there*; one it does not reflect was handed to a block not
	/// on this branch (dropped, unincluded or reorged away) and returns to
	/// handable state — unless a newer read superseded it meanwhile.
	///
	/// The hand-out marker is deliberately KEPT when the parent reflects the
	/// read: the parent itself may be an unincluded block of a branch that is
	/// later abandoned wholesale — the collator chains proposals ahead of
	/// backing, so "my parent applied it" proves landing on *this branch*
	/// only, never on chain. Consuming the marker on that verdict loses the
	/// hand-out for every other branch: exactly the run-3 watermark stall
	/// (the read of B's `up_to = 3` register was handed to A#125, a build on
	/// A#125 itself judged it landed, and when the branch died the re-author
	/// had nothing left to hand — and no new source root ever re-triggered).
	/// A hand-out is only forgotten when its read is evicted by newer ones
	/// or can never be reflected (undecodable payload).
	///
	/// Reflection is judged on the register's monotonic fields (`up_to`,
	/// `version`, plus the close latch): `grant` is advisory and
	/// non-monotonic, so by value it cannot distinguish "not yet applied"
	/// from "superseded by a newer read" — a dropped grant-only change
	/// simply waits for the next root's refresh, the pre-existing cadence.
	fn reconcile_handed(&mut self, parent: Option<&Register>) {
		let Some(context) = self.handed else { return };
		let Some(read) = self.reads.get(&context) else {
			// Evicted by newer reads — nothing left worth re-handing.
			self.handed = None;
			return;
		};
		let Ok(handed) = Register::decode_all(&mut read.payload.as_slice()) else {
			// The runtime rejects undecodable reads (`BadRegister`), so one
			// can never be reflected: abandon it rather than re-hand it to
			// every block forever.
			self.handed = None;
			return;
		};
		let reflected = parent.map_or(false, |parent| {
			parent.up_to >= handed.up_to &&
				parent.version >= handed.version &&
				(parent.closed || !handed.closed)
		});
		if reflected {
			// Landed on this branch: hand nothing here — not even a refresh
			// of the same read under a newer root, which carries no new
			// information for a branch that already applied it.
			if self.fresh == Some(context) {
				self.fresh = None;
			}
		} else if self.fresh.map_or(true, |fresh| fresh <= context) {
			self.fresh = Some(context);
		}
	}
}

#[derive(Default)]
struct SourcePool {
	/// The root of the source's last *completed* fetch round — the target
	/// every lift of this source aims at.
	target: Option<StreamsRoot>,
	channels: BTreeMap<StreamId, ChannelLedger>,
	registers: BTreeMap<StreamId, RegisterReads>,
}

/// How long a just-completed round is retained past its guard drop
/// ([`SpecMsgPool::end_round`]) so a snapshot landing microseconds later
/// still waits a beat and re-reads the pool instead of sealing an empty
/// inherent — the store-visibility photo-finish (issue 10). A few
/// milliseconds comfortably covers the ~1–2 ms publish→observe gaps the soak
/// runs saw, while the retention only ever delays a proposal that arrives
/// inside that window (idle authoring stays free).
pub(crate) const COMPLETION_RETENTION: Duration = Duration::from_millis(5);

/// The fetch rounds currently in flight, plus the authoring tasks waiting
/// out the grace window for them
/// ([`SpecMsgPool::wait_for_in_flight_rounds`]).
#[derive(Default)]
struct RoundsInFlight {
	/// Sources with a running fetch round, by the round's start instant.
	started: BTreeMap<ParaId, Instant>,
	/// Sources the monitor has offered an included root for but whose round
	/// the fetcher has not yet begun, by the offer's push instant — the
	/// late-offer window ([`SpecMsgPool::note_pending_offer`]). Superseded by
	/// [`SpecMsgPool::begin_round`]; expired under the same `bound` as a
	/// round so a never-dequeued offer never wedges authoring.
	pending_offers: BTreeMap<ParaId, Instant>,
	/// Sources whose round just completed, by the completion instant — the
	/// store-visibility window ([`SpecMsgPool::end_round`]), honoured for
	/// [`COMPLETION_RETENTION`] and only by a proposal that arrives on top of
	/// the completion (one already parked on the round was woken by it and
	/// its writes are visible).
	completed: BTreeMap<ParaId, Instant>,
	/// Woken (and drained) whenever any round ends or a fresh offer is
	/// recorded; every waiter re-checks what is still in flight or imminent.
	waiters: Vec<oneshot::Sender<()>>,
}

/// How a time-bounded [`SpecMsgPool::wait_for_in_flight_rounds`] ended,
/// distinguished for logging (issue 12): the two paths share the same
/// mechanics but mean very different things, and reusing one message made a
/// benign 3–4 ms completion-retention settle read in the logs as a stalled
/// 250 ms round.
#[derive(Debug, PartialEq, Eq)]
enum GraceExit {
	/// The wait was held open only by a just-completed round's brief re-read
	/// retention ([`COMPLETION_RETENTION`]), which has now lapsed. The
	/// fetched material is in the read view; this is the normal settle a
	/// snapshot landing on top of a completion takes, NOT a hard-bound stall.
	RetentionElapsed,
	/// Live work — a round in flight, or an offer the monitor pushed but the
	/// fetcher has not yet turned into a round — was still outstanding when
	/// the hard `bound` (a full `ROUND_GRACE_WINDOW`) elapsed. This is the
	/// genuine "authoring proceeded without the material" case.
	BoundExpired,
}

impl GraceExit {
	/// Classifies a time-bounded grace exit from what remained at the exit
	/// instant: any live work (a round in flight or a pushed-but-undequeued
	/// offer) means the hard bound was hit; neither means only the completion
	/// retention lapsed.
	fn classify(rounds_started: usize, pending_offers: usize) -> Self {
		if rounds_started == 0 && pending_offers == 0 {
			GraceExit::RetentionElapsed
		} else {
			GraceExit::BoundExpired
		}
	}
}

/// Logs a time-bounded grace exit with the message matching its
/// classification (issue 12), so completion-retention settles and genuine
/// hard-bound expiries are never conflated.
fn log_grace_time_exit(
	exit: GraceExit,
	rounds: usize,
	pending_offers: usize,
	retained: usize,
	waited: Duration,
) {
	let waited_ms = waited.as_millis() as u64;
	match exit {
		GraceExit::RetentionElapsed => tracing::debug!(
			target: LOG_TARGET,
			retained,
			waited_ms,
			"Grace window over: completion-retention elapsed",
		),
		GraceExit::BoundExpired => tracing::debug!(
			target: LOG_TARGET,
			rounds,
			pending_offers,
			waited_ms,
			"Grace window over: bound expired with rounds still in flight",
		),
	}
}

/// The verified pool shared between the fetch pipeline (writer), the
/// inherent provider and the lift assembler (readers). See the module docs.
#[derive(Default)]
pub struct SpecMsgPool {
	sources: Mutex<BTreeMap<ParaId, SourcePool>>,
	rounds: Mutex<RoundsInFlight>,
}

/// The in-flight marker of one running fetch round, created by
/// [`SpecMsgPool::begin_round`]. Dropping it — on completion, failure and
/// cancellation alike — clears the marker and wakes every grace-window
/// waiter ([`SpecMsgPool::wait_for_in_flight_rounds`]).
pub struct RoundGuard<'a> {
	pool: &'a SpecMsgPool,
	source: ParaId,
}

impl Drop for RoundGuard<'_> {
	fn drop(&mut self) {
		self.pool.end_round(self.source);
	}
}

impl SpecMsgPool {
	/// The fetch resume state of `stream`: position and frontier after the
	/// pooled run, or `None` when nothing is pooled (fetch trust-free from
	/// the runtime cursor).
	pub fn resume(&self, source: ParaId, stream: &StreamId) -> Option<(u64, MmrFrontier)> {
		let sources = self.sources.lock();
		let ledger = sources.get(&source)?.channels.get(stream)?;
		Some((ledger.end_position(), ledger.end.clone()))
	}

	/// Appends one verified chunk to `stream`'s ledger and (re)binds the
	/// run under `binding.root`.
	///
	/// `start` is the frontier at the chunk's base (the requester's resume
	/// state, or the frontier the trust-free verification derived from the
	/// response's `start_peaks`); `end` the frontier after the chunk's
	/// payloads ([`crate::VerifiedMessages::end`]). A fresh ledger adopts
	/// `start` as its base; an existing one requires exact continuation.
	pub fn note_chunk(
		&self,
		source: ParaId,
		stream: StreamId,
		start: MmrFrontier,
		payloads: Vec<Vec<u8>>,
		end: MmrFrontier,
		binding: ChannelBinding,
	) -> Result<(), PoolError> {
		let mut sources = self.sources.lock();
		let channels = &mut sources.entry(source).or_default().channels;
		let leaves: Vec<Hash> = payloads
			.iter()
			.map(|payload| hash_leaf::<SpecHasher>(LEAF_VERSION, payload))
			.collect();

		match channels.get_mut(&stream) {
			Some(ledger) => {
				if ledger.end != start {
					return Err(PoolError::NotContiguous);
				}
				ledger.leaves.extend(leaves);
				ledger.payloads.extend(payloads);
				ledger.end = end;
				ledger.binding = Some(binding);
			},
			None => {
				channels.insert(
					stream,
					ChannelLedger {
						base: start,
						end,
						payloads: payloads.into(),
						leaves: leaves.into(),
						binding: Some(binding),
					},
				);
			},
		}
		Ok(())
	}

	/// Stores one verified register head read (newest-wins per context;
	/// older contexts are retained for lift assembly, bounded).
	pub fn note_register(&self, source: ParaId, stream: StreamId, read: RegisterRead) {
		if let Ok(register) = Register::decode_all(&mut read.payload.as_slice()) {
			tracing::info!(
				target: LOG_TARGET,
				source = %u32::from(source),
				?stream,
				version = register.version,
				up_to = register.up_to.0,
				grant = ?register.grant,
				"Register read verified and stored",
			);
		}
		let mut sources = self.sources.lock();
		let registers = sources.entry(source).or_default().registers.entry(stream).or_default();
		let context = read.frontier.leaf_count;
		registers.reads.insert(context, read);
		// The newest read is the one worth handing to the next block.
		if registers.fresh.map_or(true, |fresh| fresh <= context) {
			registers.fresh = Some(context);
		}
		while registers.reads.len() > RETAINED_REGISTER_READS {
			let oldest = *registers.reads.keys().next().expect("len > 0; qed");
			registers.reads.remove(&oldest);
			if registers.fresh == Some(oldest) {
				registers.fresh = None;
			}
			if registers.handed == Some(oldest) {
				registers.handed = None;
			}
		}
	}

	/// Marks a fetch round of `source` under `root` complete: every lift of
	/// the source now targets `root`.
	pub fn complete_round(&self, source: ParaId, root: StreamsRoot) {
		self.sources.lock().entry(source).or_default().target = Some(root);
	}

	/// Marks a fetch round of `source` as in flight until the returned guard
	/// is dropped. While a round is in flight,
	/// [`Self::wait_for_in_flight_rounds`] grants it a bounded grace window
	/// before the inherent snapshot is taken. Call at trigger receipt,
	/// before any round work (see `crate::fetch`): an unmarked round cannot
	/// be awaited, and the proposer's snapshot can fire within milliseconds
	/// of the trigger.
	///
	/// Starting the real round supersedes the source's pending-offer hint
	/// ([`Self::note_pending_offer`]) and clears any stale just-completed
	/// retention: from here the in-flight marker is the source's grace-window
	/// anchor.
	pub fn begin_round(&self, source: ParaId) -> RoundGuard<'_> {
		let mut rounds = self.rounds.lock();
		rounds.started.insert(source, Instant::now());
		rounds.pending_offers.remove(&source);
		rounds.completed.remove(&source);
		RoundGuard { pool: self, source }
	}

	/// Records that the monitor has just PUSHED an included offer for
	/// `source` to the fetcher: a fetch round for it is imminent even though
	/// the offer may still be traversing the monitor→fetcher channel and no
	/// round is marked yet. [`Self::wait_for_in_flight_rounds`] waits on this
	/// hint exactly like an in-flight round — closing the late-offer
	/// photo-finish (issue 10) — and it is superseded by [`Self::begin_round`]
	/// or expired under the wait's `bound`, so a never-dequeued offer (wedged
	/// or dropped fetcher) never wedges authoring.
	///
	/// This is a bare per-source instant, deliberately NOT a guard handed
	/// through the channel: a guard could sit queued behind a stalled fetcher
	/// (bounded channel) and the per-source map cannot hold queued
	/// duplicates — the reasons monitor-side guards were rejected for the
	/// in-flight marker (issue 7).
	pub fn note_pending_offer(&self, source: ParaId) {
		let mut rounds = self.rounds.lock();
		rounds.pending_offers.insert(source, Instant::now());
		// Wake any proposal already in its grace window so it picks up the
		// now-imminent round; registered under this same lock, so a waiter
		// arriving after the offer still sees it on its own check.
		for waiter in rounds.waiters.drain(..) {
			let _ = waiter.send(());
		}
	}

	/// Clears `source`'s round-in-flight marker, retains a brief
	/// just-completed marker ([`COMPLETION_RETENTION`]) so a snapshot landing
	/// microseconds later still waits and re-reads, and wakes every
	/// grace-window waiter to re-check (the [`RoundGuard`] drop path).
	fn end_round(&self, source: ParaId) {
		let mut rounds = self.rounds.lock();
		if rounds.started.remove(&source).is_some() {
			rounds.completed.insert(source, Instant::now());
		}
		for waiter in rounds.waiters.drain(..) {
			let _ = waiter.send(());
		}
	}

	/// Waits for the in-flight fetch rounds to end, bounded by `bound` —
	/// call before snapshotting the pool for a block's inherent.
	///
	/// The fetcher and the proposer both react to the same relay-chain
	/// import notification, and the proposer's pool snapshot sits a handful
	/// of milliseconds after it — a round completing just behind the
	/// snapshot misses the first eligible block and rides the next one, +6 s
	/// on a boundary-free path. The grace window closes that race: when a
	/// round is in flight the snapshot waits for it (rounds complete in
	/// ~15–40 ms on loopback), and when none is — the common idle case —
	/// this returns immediately without arming a timer.
	///
	/// The bound is hard on two axes:
	/// - the caller never waits longer than `bound` from entry, even if rounds keep starting or
	///   never finish — on timeout the snapshot proceeds with whatever is pooled (the pre-grace
	///   behavior);
	/// - each round (and each pending offer) is granted `bound` from *its own start*, never from
	///   the wait's: a hung round or never-dequeued offer can only delay proposals overlapping its
	///   first `bound`, and authoring on a later relay parent or another fork (whole seconds away)
	///   finds it expired and does not wait at all.
	///
	/// Beyond an in-flight round the wait also honours the two photo-finish
	/// edges around it (issue 10): a *pending offer* the monitor pushed but
	/// the fetcher has not yet turned into a round (waited on under `bound`,
	/// like a round), and a round that *just completed* (retained for
	/// [`COMPLETION_RETENTION`] so a snapshot arriving right on top of the
	/// guard drop settles and re-reads). A completion the caller was itself
	/// woken by needs no retention — being woken by [`Self::end_round`]
	/// synchronises the round's writes into view — so the retention only
	/// holds a proposal that had not yet parked when it entered.
	pub async fn wait_for_in_flight_rounds(&self, bound: Duration) {
		let entered = Instant::now();
		// 0 until the wait path is taken, then the number of rounds, pending
		// offers and just-completed rounds the wait started on. The idle path
		// stays log-free; a taken wait logs its start and how it ended —
		// run-4 validation of the window was inferential because it logged
		// nothing.
		let mut waited_on = 0usize;
		// Set once the caller has parked: a completion it is then woken for
		// has its writes visible to the read that follows, so the
		// just-completed retention only holds a proposal that arrives on top
		// of a completion, never one already parked on the round producing it.
		let mut parked = false;
		loop {
			let (ended, remaining) = {
				let mut rounds = self.rounds.lock();
				// In-flight rounds and imminent (pushed-but-not-dequeued)
				// offers are granted `bound` from their start/offer instant;
				// a just-completed round a short retention from its
				// completion, but only for a not-yet-parked caller.
				let live = rounds
					.started
					.values()
					.chain(rounds.pending_offers.values())
					.map(|since| *since + bound);
				let retained = rounds
					.completed
					.values()
					.filter(|_| !parked)
					.map(|at| *at + COMPLETION_RETENTION);
				let Some(deadline) = live.chain(retained).max() else {
					if waited_on > 0 {
						tracing::debug!(
							target: LOG_TARGET,
							rounds = waited_on,
							waited_ms = entered.elapsed().as_millis() as u64,
							"Grace window over: in-flight fetch rounds completed",
						);
					}
					return;
				};
				let remaining =
					deadline.min(entered + bound).saturating_duration_since(Instant::now());
				if remaining.is_zero() {
					if waited_on > 0 {
						log_grace_time_exit(
							GraceExit::classify(rounds.started.len(), rounds.pending_offers.len()),
							rounds.started.len(),
							rounds.pending_offers.len(),
							rounds.completed.len(),
							entered.elapsed(),
						);
					}
					return;
				}
				if waited_on == 0 {
					let pending = rounds.pending_offers.len();
					let completing = if parked { 0 } else { rounds.completed.len() };
					waited_on = rounds.started.len() + pending + completing;
					tracing::debug!(
						target: LOG_TARGET,
						rounds = rounds.started.len(),
						pending_offers = pending,
						completing,
						bound_ms = bound.as_millis() as u64,
						"Waiting out the grace window for in-flight fetch rounds",
					);
				}
				// Registered under the same lock `end_round` /
				// `note_pending_offer` take: a round ending or an offer
				// arriving after this check always finds the waiter.
				let (sender, receiver) = oneshot::channel();
				rounds.waiters.push(sender);
				(receiver, remaining)
			};
			parked = true;
			let mut ended = ended.fuse();
			let mut timeout = futures_timer::Delay::new(remaining).fuse();
			futures::select_biased! {
				// A round ended, an offer arrived, or the pool went away:
				// re-check.
				_ = ended => (),
				_ = timeout => {
					// The `bound` elapsed. Whether this is a benign
					// completion-retention settle (the 5 ms COMPLETION_RETENTION
					// path, waited_ms ≈ 3–5, nothing live) or a genuine
					// hard-bound expiry with a round still in flight is
					// classified from what remains — they share this exit but
					// must NOT share a log message (issue 12).
					let rounds = self.rounds.lock();
					log_grace_time_exit(
						GraceExit::classify(
							rounds.started.len(),
							rounds.pending_offers.len(),
						),
						rounds.started.len(),
						rounds.pending_offers.len(),
						rounds.completed.len(),
						entered.elapsed(),
					);
					return
				},
			}
		}
	}

	/// Diagnostic: number of fetch rounds currently marked in flight (tests).
	pub fn rounds_in_flight(&self) -> usize {
		self.rounds.lock().started.len()
	}

	/// Diagnostic: number of monitor-pushed offers awaiting a round the
	/// fetcher has not yet begun — the late-offer grace window (tests).
	pub fn pending_offers_in_flight(&self) -> usize {
		self.rounds.lock().pending_offers.len()
	}

	/// Diagnostic: number of just-completed rounds still inside their
	/// [`COMPLETION_RETENTION`] re-read window (tests).
	pub fn retained_completions(&self) -> usize {
		self.rounds.lock().completed.len()
	}

	/// The source's current lift target root.
	pub fn target(&self, source: ParaId) -> Option<StreamsRoot> {
		self.sources.lock().get(&source).and_then(|pool| pool.target)
	}

	/// Drops the pooled state of sources no longer consumed.
	pub fn retain_sources(&self, keep: impl Fn(&ParaId) -> bool) {
		self.sources.lock().retain(|source, _| keep(source));
	}

	/// Reconciles `stream`'s ledger with the runtime's cursor. A cursor
	/// beyond the pooled run drops the ledger entirely (a block of another
	/// collator consumed material we never pooled; the fetcher restarts
	/// trust-free from the new cursor).
	///
	/// Payloads *below* the cursor are deliberately retained: the cursor is
	/// read at an unfinalized block, and a consuming ancestor may yet be
	/// reorged away — dropping the payloads would strand the surviving
	/// branch on an unservable gap (ordered streams admit no skips).
	/// Retention discipline against *finalized* consumption is future work;
	/// pooled runs are bounded by the fetch chunking meanwhile.
	pub fn prune_channel(&self, source: ParaId, stream: &StreamId, cursor: u64) {
		let mut sources = self.sources.lock();
		let Some(pool) = sources.get_mut(&source) else { return };
		let Some(ledger) = pool.channels.get(stream) else { return };
		if cursor > ledger.end_position() {
			pool.channels.remove(stream);
		}
	}

	/// Builds the block's inherent data from the pooled runs.
	///
	/// `channel_cursors` are the consumed channel streams with the runtime's
	/// resume cursor at the authoring parent (`consumed_streams()`);
	/// `register_streams` the ack streams to read, each with the parent's
	/// applied register view (`out_channels()` keys and values). What is
	/// handed over is the contiguous continuation from the cursor, within
	/// `budget`. Handing is non-destructive: the authoring parent may sit on
	/// a fork that never survives, so payloads its ancestors consumed stay
	/// pooled for the branches that did not (see [`Self::prune_channel`]).
	///
	/// Only material bound to the source's current target root is handed
	/// over — the guarantee that lift assembly for the resulting block
	/// succeeds from local material and converges per source. Register
	/// reads are additionally handed to at most one live block: before a
	/// read is handed, the stream's outstanding hand-out (if any) is
	/// reconciled against the parent's applied register view
	/// ([`RegisterReads::reconcile_handed`]) — a hand-out the parent state
	/// reflects landed and stays consumed, one it does not reflect was
	/// handed to a block that never made it onto this branch and becomes
	/// handable again. A new included root refreshes reads as before.
	pub fn build_inherent(
		&self,
		channel_cursors: &[(ParaId, StreamId, u64)],
		register_streams: &[(ParaId, StreamId, Option<Register>)],
		budget: InherentBudget,
	) -> SpecMsgInherentData {
		let mut sources = self.sources.lock();
		let mut data = SpecMsgInherentData::default();
		let mut bytes_left = budget.max_bytes;

		for (source, stream, cursor) in channel_cursors {
			if data.messages.len() + data.register_reads.len() >= budget.max_streams {
				break;
			}
			let Some(pool) = sources.get_mut(source) else { continue };
			let target = pool.target;
			let Some(ledger) = pool.channels.get(stream) else { continue };
			if *cursor > ledger.end_position() {
				// Another collator's block consumed material we never pooled;
				// the fetcher restarts trust-free from the new cursor.
				pool.channels.remove(stream);
				continue;
			}
			if *cursor < ledger.base_position() {
				// The continuation from the cursor is not retained.
				continue;
			}
			if ledger.binding.as_ref().map(|binding| binding.root) != target {
				// A fetch round is mid-flight for this source; withhold the
				// run for one block rather than risk an unliftable record.
				continue;
			}
			let skip = usize::try_from(*cursor - ledger.base_position())
				.expect("pooled runs are memory-resident; qed");
			let mut take = 0usize;
			let mut taken_bytes = 0usize;
			for payload in ledger.payloads.iter().skip(skip) {
				if taken_bytes + payload.len() > bytes_left {
					break;
				}
				taken_bytes += payload.len();
				take += 1;
			}
			if take == 0 {
				continue;
			}
			bytes_left -= taken_bytes;
			let payloads: Vec<Vec<u8>> =
				ledger.payloads.iter().skip(skip).take(take).cloned().collect();
			data.messages.push((*source, *stream, payloads));
		}

		for (source, stream, parent_view) in register_streams {
			if data.messages.len() + data.register_reads.len() >= budget.max_streams {
				break;
			}
			let Some(pool) = sources.get_mut(source) else { continue };
			let target = pool.target;
			let Some(registers) = pool.registers.get_mut(stream) else { continue };
			registers.reconcile_handed(parent_view.as_ref());
			let Some(context) = registers.fresh else { continue };
			let Some(read) = registers.reads.get(&context) else { continue };
			if target != Some(read.root) {
				continue;
			}
			data.register_reads.push((
				*source,
				*stream,
				read.payload.clone(),
				read.inclusion.clone(),
			));
			registers.fresh = None;
			registers.handed = Some(context);
		}

		data
	}

	/// The lift of a recorded channel stream whose bundle-stitched endpoint
	/// is `endpoint` messages in, plus the root it lands on (the run's
	/// binding — the assembler checks per-source convergence).
	///
	/// Local material only: the extension is either generated over the
	/// retained leaf hashes (the run reaches the head — the partial
	/// consumption path included) or the stored server-side one (endpoint
	/// == run end of a mid-backlog run).
	pub fn channel_lift(
		&self,
		source: ParaId,
		stream: &StreamId,
		endpoint: u64,
	) -> Result<(RequiresLift, StreamsRoot), LiftMaterialError> {
		let sources = self.sources.lock();
		let pool = sources.get(&source).ok_or(LiftMaterialError::NotCovered)?;
		let ledger = pool.channels.get(stream).ok_or(LiftMaterialError::NotCovered)?;
		let binding = ledger.binding.as_ref().ok_or(LiftMaterialError::NotCovered)?;

		let extension = if binding.head <= ledger.end_position() {
			// The run reaches the stream's head under the binding root:
			// generate from `endpoint` over the retained leaf hashes.
			if endpoint < ledger.base_position() || endpoint > binding.head {
				return Err(LiftMaterialError::NotCovered);
			}
			ledger
				.nodes()
				.extension(endpoint, binding.head)
				.ok_or(LiftMaterialError::NotCovered)?
		} else if endpoint == ledger.end_position() {
			// Mid-backlog run (fetch incomplete): only the run's endpoint is
			// liftable, via the stored server-side extension.
			binding.extension.clone()
		} else {
			return Err(LiftMaterialError::NotCovered);
		};

		Ok((
			RequiresLift {
				advances: Vec::new(),
				extension,
				tree_proof: binding.tree_proof.clone(),
			},
			binding.root,
		))
	}

	/// The lift of a recorded register read whose context is `context`
	/// messages in, plus the root it lands on.
	///
	/// A head read's frontier IS the stream's state under the root it was
	/// read under, so the lift is an empty extension and the stored tree
	/// proof — the caught-up hot path by construction.
	pub fn register_lift(
		&self,
		source: ParaId,
		stream: &StreamId,
		context: u64,
	) -> Result<(RequiresLift, StreamsRoot), LiftMaterialError> {
		let sources = self.sources.lock();
		let pool = sources.get(&source).ok_or(LiftMaterialError::NotCovered)?;
		let registers = pool.registers.get(stream).ok_or(LiftMaterialError::NotCovered)?;
		let read = registers.reads.get(&context).ok_or(LiftMaterialError::NotCovered)?;
		Ok((
			RequiresLift {
				advances: Vec::new(),
				extension: MMRExtensionProof::empty(),
				tree_proof: read.tree_proof.clone(),
			},
			read.root,
		))
	}

	/// Diagnostic: number of pooled payloads of `stream` (tests).
	pub fn pooled_payloads(&self, source: ParaId, stream: &StreamId) -> usize {
		self.sources
			.lock()
			.get(&source)
			.and_then(|pool| pool.channels.get(stream))
			.map_or(0, |ledger| ledger.payloads.len())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn grace_time_exit_classifies_retention_apart_from_a_hard_bound() {
		// Issue 12: the completion-retention settle and the hard-bound expiry
		// share the one time-based exit of `wait_for_in_flight_rounds` but
		// must log distinctly. The classification is purely "was any live
		// work still outstanding at the exit instant" — a round in flight or
		// a pushed-but-undequeued offer means the hard bound was hit; neither
		// means only the brief completion retention lapsed (a benign settle
		// that must NOT read as a stalled round).
		assert_eq!(GraceExit::classify(0, 0), GraceExit::RetentionElapsed);
		assert_eq!(GraceExit::classify(1, 0), GraceExit::BoundExpired);
		assert_eq!(GraceExit::classify(0, 1), GraceExit::BoundExpired);
		assert_eq!(GraceExit::classify(2, 3), GraceExit::BoundExpired);
	}
}

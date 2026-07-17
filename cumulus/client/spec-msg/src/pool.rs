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
//! acted on.
//!
//! Nothing in here is trusted beyond what verification proved: the pool
//! stores only what [`crate::verify`] derived, tagged with the root it was
//! derived under. Staleness is handled by regeneration, not invalidation —
//! streams are append-only, so a run bound under an old included root stays
//! a valid prefix under every newer one; the fetcher's next round under the
//! newer root re-binds it.

use std::collections::{BTreeMap, VecDeque};

use parking_lot::Mutex;

use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	hash_leaf, MMRExtensionProof, MmrFrontier, MmrInclusionProof, RequiresLift, SpecHasher,
	SpecMsgInherentData, StreamId, StreamsRoot, TreeInclusionProof, LEAF_VERSION,
};
use polkadot_core_primitives::Hash;

use crate::nodes::HistoricNodes;

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

	/// Drops payloads (and advances the base frontier) below `position` —
	/// the runtime's cursor moved past them. `position` beyond the ledger
	/// clears it entirely (another collator's block consumed material we
	/// never pooled; the fetcher restarts trust-free from the new cursor).
	///
	/// Returns `false` when the ledger became useless and should be dropped.
	fn prune_below(&mut self, position: u64) -> bool {
		if position > self.end_position() {
			return false;
		}
		while self.base_position() < position {
			let leaf = self.leaves.pop_front().expect("base < end implies leaves remain; qed");
			self.payloads.pop_front();
			self.base.append_leaf(leaf);
		}
		true
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
}

#[derive(Default)]
struct SourcePool {
	/// The root of the source's last *completed* fetch round — the target
	/// every lift of this source aims at.
	target: Option<StreamsRoot>,
	channels: BTreeMap<StreamId, ChannelLedger>,
	registers: BTreeMap<StreamId, RegisterReads>,
}

/// The verified pool shared between the fetch pipeline (writer), the
/// inherent provider and the lift assembler (readers). See the module docs.
#[derive(Default)]
pub struct SpecMsgPool {
	sources: Mutex<BTreeMap<ParaId, SourcePool>>,
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
		}
	}

	/// Marks a fetch round of `source` under `root` complete: every lift of
	/// the source now targets `root`.
	pub fn complete_round(&self, source: ParaId, root: StreamsRoot) {
		self.sources.lock().entry(source).or_default().target = Some(root);
	}

	/// The source's current lift target root.
	pub fn target(&self, source: ParaId) -> Option<StreamsRoot> {
		self.sources.lock().get(&source).and_then(|pool| pool.target)
	}

	/// Drops the pooled state of sources no longer consumed.
	pub fn retain_sources(&self, keep: impl Fn(&ParaId) -> bool) {
		self.sources.lock().retain(|source, _| keep(source));
	}

	/// Drops pooled payloads of `stream` below the runtime's cursor. A
	/// cursor beyond the pooled run drops the ledger entirely (a block of
	/// another collator consumed material we never pooled; the fetcher
	/// restarts trust-free from the new cursor).
	pub fn prune_channel(&self, source: ParaId, stream: &StreamId, cursor: u64) {
		let mut sources = self.sources.lock();
		let Some(pool) = sources.get_mut(&source) else { return };
		let Some(ledger) = pool.channels.get_mut(stream) else { return };
		if !ledger.prune_below(cursor) {
			pool.channels.remove(stream);
		}
	}

	/// Builds the block's inherent data from the pooled runs.
	///
	/// `channel_cursors` are the consumed channel streams with the runtime's
	/// resume cursor at the authoring parent (`consumed_streams()`);
	/// `register_streams` the ack streams to read (`out_channels()` keys).
	/// Pooled payloads below a cursor were consumed by ancestors and are
	/// pruned; what is handed over is the contiguous continuation from the
	/// cursor, within `budget`.
	///
	/// Only material bound to the source's current target root is handed
	/// over — the guarantee that lift assembly for the resulting block
	/// succeeds from local material and converges per source. Register
	/// reads are additionally handed at most once (a new included root
	/// refreshes them).
	pub fn build_inherent(
		&self,
		channel_cursors: &[(ParaId, StreamId, u64)],
		register_streams: &[(ParaId, StreamId)],
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
			let Some(ledger) = pool.channels.get_mut(stream) else { continue };
			if !ledger.prune_below(*cursor) {
				pool.channels.remove(stream);
				continue;
			}
			if ledger.binding.as_ref().map(|binding| binding.root) != target {
				// A fetch round is mid-flight for this source; withhold the
				// run for one block rather than risk an unliftable record.
				continue;
			}
			let mut take = 0usize;
			let mut taken_bytes = 0usize;
			for payload in &ledger.payloads {
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
			let payloads: Vec<Vec<u8>> = ledger.payloads.iter().take(take).cloned().collect();
			data.messages.push((*source, *stream, payloads));
		}

		for (source, stream) in register_streams {
			if data.messages.len() + data.register_reads.len() >= budget.max_streams {
				break;
			}
			let Some(pool) = sources.get_mut(source) else { continue };
			let target = pool.target;
			let Some(registers) = pool.registers.get_mut(stream) else { continue };
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

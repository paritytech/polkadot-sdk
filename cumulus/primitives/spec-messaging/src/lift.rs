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

//! Requires-lift and consumption-record primitives (spec-msg v0.5).
//!
//! Blocks never emit `Requires` and never see a `StreamsRoot`. Processing the messaging inherent
//! writes a [`ConsumptionRecord`]: which streams the block touched and the [`Interval`] consumed on
//! each. The `validate_block` wrapper then synthesizes the candidate's `Requires` set from the
//! records plus PoV-carried lifts ([`RequiresLift`]), one per recorded stream, binding that
//! stream's consumption to a current committed `StreamsRoot`. The relay chain sees only entries it
//! can match.
//!
//! The invariant [`build_requires`] enforces, per stream and candidate: the recorded intervals form
//! a proven forward chain (each block continues where the previous ended, or a PoV proof shows the
//! gap is a forward extension, see [`stitch`]), and one lift binds the chain's endpoint to a
//! committed root, which binds everything consumed.
//!
//! The record is STF output and trusted; the lifts are PoV data and verified here. Lifts are
//! matched to a source's `StreamId`-sorted record streams by position. A mispaired lift cannot
//! verify, because the `tree_proof` walk binds the record's key
//! ([`crate::streams_root::streams_root_from_proof`]).

use alloc::{collections::BTreeMap, vec, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use mmr_lib::{
	helper::{get_peak_map, get_peaks, is_valid_mmr_size},
	leaf_index_to_mmr_size, leaf_index_to_pos, MerkleProof, NodeMerkleProof,
};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::{v9::MAX_COMMITMENT_ENTRIES, RequiresSet};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

use crate::{
	mmr::{MessagePosition, MmrFrontier, MmrRoot, SpecMerge, MAX_MMR_LEAF_COUNT},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
};

/// The keyed-tree proof a lift walks from a stream root to the `StreamsRoot`: the design's
/// `TreeInclusionProof`, concretely [`StreamProof`].
pub type TreeInclusionProof = StreamProof;

/// Upper bound on an [`MMRExtensionProof`]'s connecting nodes: the design's ceiling, which the
/// lift's PoV reservation assumes. A valid ancestry proof needs at most one node per bit of the
/// leaf count, so at most 48 under [`MAX_MMR_LEAF_COUNT`]; a proof with any other count than the
/// one derived from the two leaf counts fails anyway. Checked before positions are derived.
pub const MAX_EXTENSION_CONNECTING_NODES: usize = 64;

/// Upper bound on an [`MmrInclusionProof`]'s items. A single-leaf proof needs at most the subtree
/// height plus the other peaks, each at most 63; `2 * 64` is a ceiling no valid proof reaches.
/// Bounds `calculate_root`'s work on untrusted input.
pub const MAX_INCLUSION_PROOF_ITEMS: usize = 2 * 64;

/// Why an MMR proof verification rejected its input. Typed so callers can tell a malformed or
/// forged proof from an out-of-range or non-forward one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProofError {
	/// The proof is structurally malformed: an invalid `mmr_size`, a wrong item/node count, or
	/// `mmr_lib` could not reconstruct a root from it.
	InvalidProof,
	/// A leaf position is at or past the proven MMR's leaf count (an empty MMR has no head).
	PositionOutOfRange,
	/// An extension does not go strictly forward (its target is not newer than the frontier).
	NotForward,
	/// An inclusion proof carries more items than [`MAX_INCLUSION_PROOF_ITEMS`].
	ItemLimitExceeded,
	/// An extension proof carries more connecting nodes than [`MAX_EXTENSION_CONNECTING_NODES`].
	NodeLimitExceeded,
	/// A leaf count exceeds [`MAX_MMR_LEAF_COUNT`].
	LeafCountTooLarge,
}

/// An append-only MMR extension proof: O(log n) connecting nodes that, with a frontier's own peaks,
/// reconstruct a newer root. The `mmr_lib` `gen_ancestry_proof` form of the design's
/// `connecting_nodes`. Verification computes the new root; it is never declared alongside the
/// proof. Payloads are not needed.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MMRExtensionProof {
	/// Leaf count of the extended MMR. The `mmr_lib` node count is derived from it, so no
	/// independent size can be smuggled in; the derivation is total up to [`MAX_MMR_LEAF_COUNT`],
	/// which `verify` checks first. `0` with empty `connecting_nodes` is the identity extension.
	/// Compact-encoded, since this is PoV-carried.
	#[codec(compact)]
	pub leaf_count: u64,
	/// The connecting-node hashes, in `mmr_lib`'s proof order. Positions are not stored: an MMR's
	/// shape is fixed by its size, so they are derived at verify time from the two leaf counts
	/// (`ancestry_positions`). Empty for the identity.
	pub connecting_nodes: Vec<Hash>,
}

impl MMRExtensionProof {
	/// The identity extension: `leaf_count = 0`, no connecting nodes. Yields a frontier's own root,
	/// for the caught-up case.
	pub fn identity() -> Self {
		Self { leaf_count: 0, connecting_nodes: Vec::new() }
	}

	/// Whether this is the identity extension.
	pub fn is_identity(&self) -> bool {
		self.leaf_count == 0 && self.connecting_nodes.is_empty()
	}

	/// Extend `from` and return the new root, computed from `from`'s peaks and the connecting
	/// nodes; `Err` if the proof does not fit that placement. The identity extension yields
	/// `from`'s own root. `from`'s contents are not re-checked: a forged peak yields a root no
	/// committed `StreamsRoot` can bind. The proof's own leaf count is checked against
	/// [`MAX_MMR_LEAF_COUNT`] first; within it, the derived `mmr_lib` size is always valid.
	pub fn verify(&self, from: &MmrFrontier) -> Result<MmrRoot, ProofError> {
		// The proof's leaf count arrives raw. `from` is already in range and consistent by
		// construction.
		if self.leaf_count > MAX_MMR_LEAF_COUNT {
			return Err(ProofError::LeafCountTooLarge);
		}
		if self.is_identity() {
			return Ok(from.root());
		}
		// Strictly forward: the extended MMR must have more leaves than the frontier. `0` is
		// covered by this, but reject it by name: the derivation below subtracts one.
		if self.leaf_count == 0 || self.leaf_count <= from.leaf_count() {
			return Err(ProofError::NotForward);
		}
		// Bound the connecting nodes before `calculate_root` does O(n) work on them.
		if self.connecting_nodes.len() > MAX_EXTENSION_CONNECTING_NODES {
			return Err(ProofError::NodeLimitExceeded);
		}
		let new_mmr_size = leaf_index_to_mmr_size(self.leaf_count - 1);

		// Extending from the empty stream: nothing binds, so the connecting nodes must be the new
		// MMR's peaks in `get_peaks` order.
		if from.leaf_count() == 0 {
			// The nodes must form a frontier of exactly `self.leaf_count` leaves; `from_parts`
			// checks that.
			return MmrFrontier::from_parts(self.connecting_nodes.clone(), self.leaf_count)
				.map(|f| f.root())
				.ok_or(ProofError::InvalidProof);
		}

		// `from`'s peaks are complete subtrees of the new MMR at unchanged positions; with the
		// connecting nodes they reconstruct the new root under `SpecMerge`.
		let old_mmr_size = from.mmr_size();
		let old_positions = get_peaks(old_mmr_size);
		// Positions are not carried: derive them from the two sizes and pair them with the hashes
		// in `mmr_lib`'s proof order. A length mismatch means the proof does not fit the claimed
		// extension.
		let positions = ancestry_positions(old_mmr_size, new_mmr_size);
		if positions.len() != self.connecting_nodes.len() {
			return Err(ProofError::InvalidProof);
		}
		let proof: Vec<(u64, Hash)> =
			positions.into_iter().zip(self.connecting_nodes.iter().copied()).collect();
		let nodes: Vec<(u64, Hash)> =
			old_positions.into_iter().zip(from.peaks().iter().copied()).collect();
		NodeMerkleProof::<Hash, SpecMerge>::new(new_mmr_size, proof)
			.calculate_root(nodes)
			.map(|bag| MmrRoot::from_bag(self.leaf_count, bag))
			.map_err(|_| ProofError::InvalidProof)
	}
}

/// The connecting-node positions of an MMR ancestry proof, from the two sizes alone; this is what
/// lets [`MMRExtensionProof::connecting_nodes`] be bare hashes. Derived by `mmr_lib`. Both sizes
/// must be valid MMR sizes with a non-empty old MMR; the empty and identity extensions are handled
/// before this is reached.
fn ancestry_positions(old_mmr_size: u64, new_mmr_size: u64) -> Vec<u64> {
	mmr_lib::ancestry_proof::ancestry_proof_positions(old_mmr_size, new_mmr_size)
		.expect("callers pass valid sizes and a non-empty old MMR within the new one; qed")
}

/// A single-leaf MMR inclusion proof: `mmr_lib`'s [`MerkleProof`] items (the sibling path plus the
/// other peaks) and the node count it was generated against. Used by the event and register head
/// read. Where [`MMRExtensionProof`] moves a frontier forward, this places one leaf under a fixed
/// MMR. The root is derived, never declared.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MmrInclusionProof {
	/// `mmr_lib` node count of the MMR the proof was generated against. With the leaf count it
	/// derives (`get_peak_map`), it pins a head read.
	pub mmr_size: u64,
	/// The leaf's sibling path plus the other peaks, in `mmr_lib` order.
	pub items: Vec<Hash>,
}

impl MmrInclusionProof {
	/// Leaf count of the proven MMR: the peak map of a valid MMR size.
	pub fn leaf_count(&self) -> u64 {
		get_peak_map(self.mmr_size)
	}

	/// Verify `leaf` (already hashed by [`crate::message::leaf_hash`]) as the last leaf of the
	/// proven MMR. Returns the head position and the frontier of that MMR; `Err` on any structural
	/// defect.
	///
	/// The expected shape is what `gen_proof` produces for the last leaf: the other peaks left to
	/// right, then the head's `leaf_count.trailing_zeros()` left siblings bottom up. The item count
	/// is checked exactly.
	///
	/// Only reconstructs a frontier; does not bind it to any committed `StreamsRoot`. `Ok` is not
	/// authentication. Prefer [`crate::message::verify_event`], which binds the result to a trusted
	/// `under`.
	pub fn verify_head(&self, leaf: Hash) -> Result<(MessagePosition, MmrFrontier), ProofError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(ProofError::InvalidProof);
		}
		let leaf_count = self.leaf_count();
		// Already in range under `is_valid_mmr_size`; stated so every entry point reads the same.
		if leaf_count > MAX_MMR_LEAF_COUNT {
			return Err(ProofError::LeafCountTooLarge);
		}
		if leaf_count == 0 {
			return Err(ProofError::PositionOutOfRange);
		}
		let path_len = leaf_count.trailing_zeros() as usize;
		let other_peaks = leaf_count.count_ones() as usize - 1;
		if self.items.len() != other_peaks + path_len {
			return Err(ProofError::InvalidProof);
		}

		let (peak_items, path) = self.items.split_at(other_peaks);
		let mut last_peak = leaf;
		for sibling in path {
			last_peak = <SpecMerge as mmr_lib::Merge>::merge(sibling, &last_peak)
				.expect("SpecMerge::merge is infallible; qed");
		}
		let peaks: Vec<Hash> =
			peak_items.iter().copied().chain(core::iter::once(last_peak)).collect();
		// `other_peaks + 1 == leaf_count.count_ones()` and the ceiling was checked, so this is
		// `Some`.
		let frontier =
			MmrFrontier::from_parts(peaks, leaf_count).ok_or(ProofError::InvalidProof)?;

		Ok((MessagePosition(leaf_count - 1), frontier))
	}

	/// Verify `leaf` (already hashed by [`crate::message::leaf_hash`]) at `position` and return the
	/// implied [`MmrRoot`]. `Err` if the position is out of range or the proof malformed. Only
	/// reconstructs the root; `Ok` is not authentication. Prefer [`crate::message::verify_event`],
	/// which binds the result to a trusted `under`.
	pub fn verify_leaf(
		&self,
		position: MessagePosition,
		leaf: Hash,
	) -> Result<MmrRoot, ProofError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(ProofError::InvalidProof);
		}
		// Redundant under `is_valid_mmr_size`; stated so every entry point reads the same.
		if self.leaf_count() > MAX_MMR_LEAF_COUNT {
			return Err(ProofError::LeafCountTooLarge);
		}
		// Bound `items` before `calculate_root` does O(n) work on them. `verify_head` gets the same
		// from its exact item count.
		if self.items.len() > MAX_INCLUSION_PROOF_ITEMS {
			return Err(ProofError::ItemLimitExceeded);
		}
		if position.0 >= self.leaf_count() {
			return Err(ProofError::PositionOutOfRange);
		}
		let pos = leaf_index_to_pos(position.0);
		MerkleProof::<Hash, SpecMerge>::new(self.mmr_size, self.items.clone())
			.calculate_root(vec![(pos, leaf)])
			.map(|bag| MmrRoot::from_bag(self.leaf_count(), bag))
			.map_err(|_| ProofError::InvalidProof)
	}
}

/// Per stream and block: consumption entered the block at `start` and left it at `end`.
///
/// The candidate's lift binds only the last state to a committed root; the intervals extend that
/// guarantee back over the bundle. Each block must start where the previous ended, or prove the
/// jump moved forward ([`stitch`]).
///
/// - Channels: `start` is the frontier root before the block's incoming messages, `end` the
/// frontier after. `start == previous end` holds by construction, so the chain check is free.
/// - Register and event reads: `end` is the context the reads were verified against, `start` its
/// root; nothing advances. Contexts can jump between blocks, so a fabricated context breaks the
/// chain instead of hiding behind a later genuine one.
///
/// `end` is a full frontier because the next gap check, or the lift, extends from it.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct Interval {
	/// Root the block's consumption on this stream started from.
	pub start: MmrRoot,
	/// Frontier the block's consumption on this stream ended at.
	pub end: MmrFrontier,
}

/// Written per block: the streams this block touched and the interval consumed on each, grouped by
/// source. One interval per stream, keyed so the type enforces it; per-source order is the
/// canonical `StreamId` order the lifts match against. The API view of a flat, host-append-only
/// outbox vec. Encodes as the sorted vec of pairs would.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct ConsumptionRecord {
	/// `source -> stream -> interval`.
	pub entries: BTreeMap<ParaId, BTreeMap<StreamId, Interval>>,
}

/// One source's streams with their intervals across a bundle, in block order. [`build_requires`]
/// merges the per-block records into this and [`build_requires_entry`] lifts it.
pub type SourceStreams = BTreeMap<StreamId, Vec<Interval>>;

/// One lift, carried in the PoV. Matched by position to a source's `StreamId`-sorted record
/// streams; a mispaired lift cannot verify because `tree_proof` binds the record's key.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct RequiresLift {
	/// One extension per gap in the stream's interval chain, in gap order. Empty when there are no
	/// gaps.
	pub advances: Vec<MMRExtensionProof>,
	/// Extends the chain's endpoint to the stream's current root. The identity extension when the
	/// endpoint already is that root.
	pub extension: MMRExtensionProof,
	/// Keyed walk from the current stream root to the `StreamsRoot` the requires entry becomes.
	pub tree_proof: TreeInclusionProof,
}

/// The candidate's lifts, grouped per source: the PoV transport form (encoding spec §8). `Decode`
/// enforces strictly increasing `ParaId`s and bounds the source count at
/// [`MAX_COMMITMENT_ENTRIES`], so more sources than a `RequiresSet` can hold are rejected before
/// any lift is verified. [`TryFrom<BTreeMap>`](Self::try_from) is canonical by construction.
/// [`build_requires`] zips these against the record's sources by position.
#[derive(Clone, Encode, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct LiftsBySource(BoundedVec<(ParaId, Vec<RequiresLift>), ConstU32<MAX_COMMITMENT_ENTRIES>>);

impl LiftsBySource {
	/// The sources, in strictly increasing order.
	pub fn sources(&self) -> impl Iterator<Item = &ParaId> {
		self.0.iter().map(|(source, _)| source)
	}

	/// `(source, lifts)` pairs, in source order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, Vec<RequiresLift>)> {
		self.0.iter()
	}

	/// The number of sources.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Whether there are no sources.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}
}

impl TryFrom<BTreeMap<ParaId, Vec<RequiresLift>>> for LiftsBySource {
	type Error = LiftError;

	/// A map iterates in `ParaId` order with unique keys, so only the bound can fail.
	fn try_from(map: BTreeMap<ParaId, Vec<RequiresLift>>) -> Result<Self, LiftError> {
		BoundedVec::try_from(map.into_iter().collect::<Vec<_>>())
			.map(Self)
			.map_err(|_| LiftError::TooManySources)
	}
}

/// Matches the derived `Encode`. The sequence must be strictly increasing by `ParaId`.
impl Decode for LiftsBySource {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner =
			BoundedVec::<(ParaId, Vec<RequiresLift>), ConstU32<MAX_COMMITMENT_ENTRIES>>::decode(
				input,
			)?;
		if inner.windows(2).any(|w| w[0].0 >= w[1].0) {
			return Err("LiftsBySource: sources must be strictly increasing".into());
		}
		Ok(Self(inner))
	}
}

impl DecodeWithMemTracking for LiftsBySource {}

/// Why requires synthesis failed. All are deterministic in the records and PoV, so every validator
/// reaches the same verdict.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiftError {
	/// A stream's interval list was empty.
	EmptyRecord,
	/// A gap needed a forward-extension proof but `advances` was exhausted.
	MissingAdvance,
	/// `advances` carried more proofs than there were gaps.
	StrayAdvance,
	/// A gap's proof did not forward-extend the previous endpoint to the next start.
	BrokenChain,
	/// The endpoint extension did not verify against the stitched endpoint.
	BadExtension,
	/// The tree proof did not fold to a `StreamsRoot` (malformed / wrong key).
	BadTreeProof,
	/// A source's stream count and lift count disagreed.
	LiftCountMismatch,
	/// A source's streams lifted to more than one `StreamsRoot`.
	DivergentRoots,
	/// The recorded sources and the provided lifts' sources did not match exactly.
	LiftSourceMismatch,
	/// More sources than the `RequiresSet` bound allows.
	TooManySources,
}

/// Stitch one stream's intervals, in bundle order, into its endpoint frontier. `advances` must hold
/// exactly one extension per gap (a `start` that is not the previous `end`'s root), in gap order.
/// Gaps are proven forward, so within a candidate verified states cannot regress; across candidates
/// that is the STF's to hold.
pub fn stitch(
	intervals: &[Interval],
	advances: &[MMRExtensionProof],
) -> Result<MmrFrontier, LiftError> {
	let (first, rest) = intervals.split_first().ok_or(LiftError::EmptyRecord)?;
	let mut gaps = advances.iter();
	// `first.start` is the boundary with the previous candidate; nothing here can check it, and
	// nothing needs to: everything consumed is bound through the chain from `first.end`.
	// Cross-candidate continuity is the STF's (the stored frontier for channels; for register
	// reads, the last applied position and the register's monotonic fields).
	let mut current = first.end.clone();
	for next in rest {
		let current_root = current.root();
		if next.start != current_root {
			// A gap must be a proven forward extension of where we ended.
			let proof = gaps.next().ok_or(LiftError::MissingAdvance)?;
			let extended = proof.verify(&current).map_err(|_| LiftError::BrokenChain)?;
			if extended != next.start {
				return Err(LiftError::BrokenChain);
			}
		}
		current = next.end.clone();
	}
	if gaps.next().is_some() {
		return Err(LiftError::StrayAdvance);
	}
	Ok(current)
}

/// Lift one source's streams to a single committed `StreamsRoot`: stitch each stream's intervals
/// into an endpoint, extend it to the stream's current root, walk the tree to the `StreamsRoot`,
/// and require all of the source's streams to reach the same root. `streams` iterates in `StreamId`
/// order; `lifts` matches it by position.
pub fn build_requires_entry(
	streams: &SourceStreams,
	lifts: &[RequiresLift],
) -> Result<StreamsRoot, LiftError> {
	if streams.len() != lifts.len() {
		return Err(LiftError::LiftCountMismatch);
	}
	let mut entry: Option<StreamsRoot> = None;
	for ((stream, intervals), lift) in streams.iter().zip(lifts) {
		let endpoint = stitch(intervals, &lift.advances)?;
		// The endpoint is contained in the stream's current root, computed rather than declared,
		let current = lift.extension.verify(&endpoint).map_err(|_| LiftError::BadExtension)?;
		// and the keyed walk from it yields the `StreamsRoot`. The walk binds the record's `stream`
		// key, so a mispaired lift cannot verify.
		let root = streams_root_from_proof(*stream, current.0, &lift.tree_proof)
			.ok_or(LiftError::BadTreeProof)?;
		match entry {
			None => entry = Some(root),
			Some(prev) if prev != root => return Err(LiftError::DivergentRoots),
			Some(_) => {},
		}
	}
	entry.ok_or(LiftError::EmptyRecord)
}

/// Synthesize the candidate's [`RequiresSet`] from the per-block records, in bundle order, and the
/// PoV lifts. Sources must match exactly; a recorded source without lifts, or lifts for an
/// unrecorded source, invalidates the candidate.
///
/// `None` when the candidate consumed nothing: there is no entry the relay could match, and the
/// candidate emits no `Requires` signal. `RequiresSet` cannot be empty.
pub fn build_requires(
	records: &[ConsumptionRecord],
	lifts: &LiftsBySource,
) -> Result<Option<RequiresSet>, LiftError> {
	// Merge per (source, stream) intervals across the bundle, preserving block order.
	let mut merged: BTreeMap<ParaId, SourceStreams> = BTreeMap::new();
	for record in records {
		for (source, streams) in &record.entries {
			let by_stream = merged.entry(*source).or_default();
			for (stream, interval) in streams {
				by_stream.entry(*stream).or_default().push(interval.clone());
			}
		}
	}
	// Recorded sources and lift sources must be equal; both are ParaId-sorted.
	if !merged.keys().eq(lifts.sources()) {
		return Err(LiftError::LiftSourceMismatch);
	}
	if merged.is_empty() {
		return Ok(None);
	}
	let entries = merged
		.iter()
		.zip(lifts.iter())
		.map(|((source, by_stream), (_, source_lifts))| {
			build_requires_entry(by_stream, source_lifts).map(|root| (*source, root))
		})
		.collect::<Result<Vec<_>, _>>()?;
	RequiresSet::try_from_iter(entries)
		.map(Some)
		.map_err(|_| LiftError::TooManySources)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::streams_root::{gen_stream_proof, streams_root, TreeStep};
	use mmr_lib::util::{MemMMR, MemStore};
	use polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES;
	use sp_core::H256;

	fn leaves(n: u64) -> Vec<Hash> {
		// Distinct for any `n` (the old `repeat_byte(i as u8 + 1)` wrapped past 255 leaves).
		(0..n).map(|i| H256::from_low_u64_be(i + 1)).collect()
	}

	/// Peaks-only frontier after the first `k` leaves.
	fn frontier_at(all: &[Hash], k: usize) -> MmrFrontier {
		let mut frontier = MmrFrontier::new();
		for l in &all[..k] {
			frontier.append(*l);
		}
		frontier
	}

	/// Root over the first `k` leaves.
	fn root_at(all: &[Hash], k: usize) -> MmrRoot {
		frontier_at(all, k).root()
	}

	/// `mmr_lib` node count for `k` leaves.
	fn mmr_size(k: usize) -> u64 {
		if k == 0 {
			0
		} else {
			leaf_index_to_mmr_size((k - 1) as u64)
		}
	}

	/// O(log n) extension proof from `k` leaves to `n` leaves (`0 < k < n`), via
	/// `gen_ancestry_proof`.
	fn extension(all: &[Hash], k: usize, n: usize) -> MMRExtensionProof {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for l in &all[..n] {
			mmr.push(*l).unwrap();
		}
		let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
		MMRExtensionProof {
			leaf_count: n as u64,
			connecting_nodes: ap.prev_peaks_proof.proof_items().iter().map(|(_, h)| *h).collect(),
		}
	}

	fn ch(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}

	#[test]
	fn extension_computes_root_and_rejects_tampering() {
		let all = leaves(5);
		let ext = extension(&all, 2, 5);
		// Computes root@5 from frontier@2 + O(log n) connecting nodes (never a declared root).
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Ok(root_at(&all, 5)));
		// Wrong starting frontier → does not reconstruct the extended root.
		assert_ne!(ext.verify(&frontier_at(&all, 3)), Ok(root_at(&all, 5)));
		// Tampered connecting node → a different computed root.
		let mut bad = ext.clone();
		bad.connecting_nodes[0] = H256::repeat_byte(0xff);
		assert_ne!(bad.verify(&frontier_at(&all, 2)), Ok(root_at(&all, 5)));
		// Identity extension yields the frontier's own root.
		let id = MMRExtensionProof::identity();
		assert_eq!(id.verify(&frontier_at(&all, 3)), Ok(root_at(&all, 3)));
	}

	/// The derived connecting-node positions must equal what `gen_ancestry_proof` actually emits,
	/// for every `(k, n)` shape: the invariant that lets `connecting_nodes`
	/// store bare hashes. Exhaustive over every `(old, new)` pair up to 256 leaves, which covers
	/// the all-ones sizes 127 and 255 (seven and eight peaks) and the single-peak powers of two
	/// beside them. If `mmr_lib`'s node set or order drifts, this trips; `Cargo.lock` makes a drift
	/// a deliberate bump.
	#[test]
	fn ancestry_positions_matches_mmr_lib() {
		for n in 2..=256usize {
			let all = leaves(n as u64);
			let store = MemStore::<Hash>::default();
			let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
			for l in &all {
				mmr.push(*l).unwrap();
			}
			for k in 1..n {
				let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
				let expected: Vec<u64> =
					ap.prev_peaks_proof.proof_items().iter().map(|(p, _)| *p).collect();
				let got = super::ancestry_positions(mmr_size(k), mmr_size(n));
				assert_eq!(got, expected, "position mismatch at k={k}, n={n}");
			}
		}
	}

	#[test]
	fn extension_leaf_count_disambiguates_equal_node_counts() {
		// Encoding spec §12, item 4: extending one leaf to three and to four both take two
		// connecting nodes, placed differently. That is why `leaf_count` is carried (the verifier
		// cannot derive it from the node count) and why the nodes carry no positions (the count
		// fixes them).
		let all = leaves(4);
		let to3 = extension(&all, 1, 3);
		let to4 = extension(&all, 1, 4);
		assert_eq!(to3.connecting_nodes.len(), 2);
		assert_eq!(to4.connecting_nodes.len(), 2);
		let from1 = frontier_at(&all, 1);
		assert_eq!(to3.verify(&from1), Ok(root_at(&all, 3)));
		assert_eq!(to4.verify(&from1), Ok(root_at(&all, 4)));
		// The same two nodes under the other count place differently and reach neither root.
		let relabelled =
			MMRExtensionProof { leaf_count: 4, connecting_nodes: to3.connecting_nodes };
		let got = relabelled.verify(&from1);
		assert_ne!(got, Ok(root_at(&all, 3)));
		assert_ne!(got, Ok(root_at(&all, 4)));
	}

	#[test]
	fn extension_verify_rejects_non_forward() {
		let all = leaves(5);
		// A real (non-identity) extension pointed *back* to the frontier's own size is not strictly
		// forward → rejected up front, before any crypto work.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = 2;
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::NotForward));
	}

	#[test]
	fn extension_verify_bounds_untrusted_leaf_counts() {
		let all = leaves(5);
		// The proof's own count. A value whose derived node count would overflow `u64` is rejected
		// before any position is computed from it.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = u64::MAX;
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::LeafCountTooLarge));
		ext.leaf_count = MAX_MMR_LEAF_COUNT + 1;
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::LeafCountTooLarge));
		// The frontier's count is not this function's to check. An out-of-range `MmrFrontier`
		// cannot be built, from `from_parts` or from `Decode`.
		assert!(MmrFrontier::from_parts(vec![all[0]], 1 << 63).is_none());
	}

	#[test]
	fn extension_verify_rejects_zero_leaf_count() {
		let all = leaves(5);
		// `0` with connecting nodes is not the identity. It must reject before the `leaf_count - 1`
		// the node-count derivation needs.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = 0;
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::NotForward));
	}

	#[test]
	fn extension_identity_on_empty_frontier_yields_empty_root() {
		// The empty frontier has the defined root `empty_root()` (encoding spec §3.2); the
		// identity extension yields it like any other frontier's own root. Nothing binds
		// it to a committed entry; an empty stream is never in the StreamsRoot tree, so
		// a downstream tree walk fails naturally.
		assert_eq!(
			MMRExtensionProof::identity().verify(&MmrFrontier::default()),
			Ok(MmrFrontier::default().root()),
		);
	}

	#[test]
	fn the_longest_valid_extension_fits_the_node_ceiling() {
		// The worst shape: one leaf extended to the largest count with every bit set, one
		// connecting node per bit. The lift's PoV reservation assumes this stays within the
		// ceiling.
		let nodes =
			ancestry_positions(mmr_size(1), mmr_size((MAX_MMR_LEAF_COUNT - 1) as usize)).len();
		assert_eq!(nodes, 48);
		assert!(nodes <= MAX_EXTENSION_CONNECTING_NODES);
	}

	#[test]
	fn over_long_extension_proof_is_rejected() {
		let all = leaves(5);
		let mut ext = extension(&all, 2, 5);
		// Pad the connecting nodes beyond the defense-in-depth ceiling: must reject, not do the
		// work.
		ext.connecting_nodes = vec![H256::zero(); MAX_EXTENSION_CONNECTING_NODES + 1];
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::NodeLimitExceeded));
	}

	#[test]
	fn stitch_caught_up_needs_no_advance() {
		let all = leaves(5);
		let i1 = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) };
		let i2 = Interval { start: root_at(&all, 2), end: frontier_at(&all, 5) };
		assert_eq!(stitch(&[i1, i2], &[]).unwrap(), frontier_at(&all, 5));
	}

	#[test]
	fn stitch_bridges_a_gap_and_flags_bad_advance_counts() {
		let all = leaves(5);
		let i1 = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) };
		// Block 2 jumped forward to context @4 before continuing to @5.
		let i2 = Interval { start: root_at(&all, 4), end: frontier_at(&all, 5) };
		let advance = extension(&all, 2, 4);

		assert_eq!(
			stitch(&[i1.clone(), i2.clone()], &[advance.clone()]).unwrap(),
			frontier_at(&all, 5)
		);
		// Missing the gap proof.
		assert_eq!(stitch(&[i1.clone(), i2.clone()], &[]), Err(LiftError::MissingAdvance));
		// A stray advance where the chain was already continuous.
		let cont = Interval { start: root_at(&all, 2), end: frontier_at(&all, 5) };
		assert_eq!(stitch(&[i1, cont], &[advance]), Err(LiftError::StrayAdvance));
	}

	#[test]
	fn stitch_rejects_a_gap_bridged_to_the_wrong_context() {
		// The advance is all that binds the earlier read to the stream: a context the chain does
		// not reach must fail, or a fabricated read could hide behind a later genuine one.
		let all = leaves(5);
		let i1 = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) };
		let i2 = Interval { start: root_at(&all, 4), end: frontier_at(&all, 5) };
		// A genuine extension of @2, but to @3, not to the context @4 block 2 started from.
		assert_eq!(
			stitch(&[i1.clone(), i2.clone()], &[extension(&all, 2, 3)]),
			Err(LiftError::BrokenChain)
		);
		// An extension that does not fit @2 at all (built from @1).
		assert_eq!(stitch(&[i1, i2], &[extension(&all, 1, 4)]), Err(LiftError::BrokenChain));
	}

	#[test]
	fn nothing_recorded_lifts_nothing() {
		assert_eq!(stitch(&[], &[]), Err(LiftError::EmptyRecord));
		assert_eq!(build_requires_entry(&SourceStreams::new(), &[]), Err(LiftError::EmptyRecord));
	}

	#[test]
	fn consumption_record_encodes_as_the_sorted_vec_of_pairs() {
		// The record crosses the runtime-API boundary; keying it must not move a byte relative to
		// the design's `Vec<(StreamId, Interval)>` view of the same sorted, unique entries.
		let all = leaves(3);
		let (a, b) = (ch(1), ch(2));
		let (ia, ib) = (
			Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 1) },
			Interval { start: root_at(&all, 1), end: frontier_at(&all, 3) },
		);
		let keyed = ConsumptionRecord {
			entries: BTreeMap::from([(
				ParaId::from(7),
				BTreeMap::from([(b, ib.clone()), (a, ia.clone())]),
			)]),
		};
		let as_vec: BTreeMap<ParaId, Vec<(StreamId, Interval)>> =
			BTreeMap::from([(ParaId::from(7), vec![(a, ia), (b, ib)])]);
		assert_eq!(keyed.encode(), as_vec.encode());
	}

	#[test]
	fn hot_path_single_block_single_stream() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let stream = ch(2000);
		// StreamsRoot over the single active stream at root@3.
		let entries = BTreeMap::from([(stream, root3.0)]);
		let expected = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// One interval, endpoint = frontier@3; identity extension (caught up).
		let interval = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) };
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let streams = SourceStreams::from([(stream, vec![interval])]);
		let root = build_requires_entry(&streams, &[lift]).unwrap();
		assert_eq!(root, expected);
	}

	#[test]
	fn build_requires_end_to_end_and_source_match() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let source = ParaId::from(1000);
		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, root3.0)]);
		let expected = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				BTreeMap::from([(
					stream,
					Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) },
				)]),
			)]),
		};
		let mut lifts = BTreeMap::new();
		lifts.insert(
			source,
			vec![RequiresLift {
				advances: Vec::new(),
				extension: MMRExtensionProof::identity(),
				tree_proof,
			}],
		);

		let lifts_by_source = LiftsBySource::try_from(lifts.clone()).unwrap();
		let requires = build_requires(&[record.clone()], &lifts_by_source).unwrap().unwrap();
		assert_eq!(requires.get(source), Some(&expected));

		// A lift for an unrecorded source (or a missing one) is rejected.
		let mut extra = lifts.clone();
		extra.insert(ParaId::from(9999), Vec::new());
		let extra = LiftsBySource::try_from(extra).unwrap();
		assert_eq!(build_requires(&[record], &extra), Err(LiftError::LiftSourceMismatch));

		// Nothing consumed, nothing lifted: no set, not an empty one.
		assert_eq!(build_requires(&[], &LiftsBySource::default()), Ok(None));
	}

	#[test]
	fn lifts_by_source_decode_enforces_canonical_form() {
		let lift = || RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		let pair = |id: u32| (ParaId::from(id), vec![lift()]);
		// Canonical round-trips …
		let good = vec![pair(1), pair(2)].encode();
		assert!(LiftsBySource::decode(&mut &good[..]).is_ok());
		// … out of order and duplicate sources do not (spec §8) …
		let unsorted = vec![pair(2), pair(1)].encode();
		assert!(LiftsBySource::decode(&mut &unsorted[..]).is_err());
		let duplicate = vec![pair(1), pair(1)].encode();
		assert!(LiftsBySource::decode(&mut &duplicate[..]).is_err());
		// … and more sources than a RequiresSet could hold are rejected before any verification.
		let too_many: Vec<_> = (0..=MAX_COMMITMENT_ENTRIES).map(pair).collect();
		assert!(LiftsBySource::decode(&mut &too_many.encode()[..]).is_err());
		// A map is canonical by construction; only the bound can fail it.
		let map: BTreeMap<_, _> = (0..=MAX_COMMITMENT_ENTRIES).map(pair).collect();
		assert_eq!(LiftsBySource::try_from(map).err(), Some(LiftError::TooManySources));
	}

	#[test]
	fn late_block_non_identity_extension_end_to_end() {
		// The core of the unified lift: the block ended at frontier@2, but the stream's current
		// committed root is @5. A non-identity `extension` proves @2 forward to @5, and the tree
		// walk from @5 yields the committed StreamsRoot. Every other build_requires test uses the
		// identity extension; this drives a real forward proof through the whole path.
		let all = leaves(5);
		let stream = ch(2000);
		let committed = root_at(&all, 5);
		let entries = BTreeMap::from([(stream, committed.0)]);
		let expected = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let interval = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) };
		let lift =
			RequiresLift { advances: Vec::new(), extension: extension(&all, 2, 5), tree_proof };
		let streams = SourceStreams::from([(stream, vec![interval])]);
		assert_eq!(build_requires_entry(&streams, &[lift]).unwrap(), expected);
	}

	#[test]
	fn multi_stream_source_converges_on_one_root() {
		// A source commits ONE StreamsRoot over ALL its streams. Two streams at different frontiers
		// must both lift to that same root — the happy path the divergent-roots test only shows
		// failing.
		let all = leaves(5);
		let (a, b) = (ch(2000), ch(3000));
		let (ra, rb) = (root_at(&all, 3), root_at(&all, 5));
		let entries = BTreeMap::from([(a, ra.0), (b, rb.0)]);
		let expected = streams_root(&entries).unwrap();
		let (_pa, proof_a) = gen_stream_proof(&entries, a).unwrap();
		let (_pb, proof_b) = gen_stream_proof(&entries, b).unwrap();

		let lift = |tree_proof| RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		// SourceStreams is keyed, so it iterates a, b — the order the lifts must match.
		let streams = SourceStreams::from([
			(a, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }]),
			(b, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 5) }]),
		]);
		assert_eq!(
			build_requires_entry(&streams, &[lift(proof_a), lift(proof_b)]).unwrap(),
			expected
		);
	}

	#[test]
	fn build_requires_covers_multiple_sources() {
		// Two sources, each one stream → a two-entry RequiresSet, each entry the source's own root.
		let all = leaves(3);
		let committed = root_at(&all, 3);
		let (src_a, src_b) = (ParaId::from(1000), ParaId::from(2000));
		let stream = ch(4000);
		let entries = BTreeMap::from([(stream, committed.0)]);
		let expected = streams_root(&entries).unwrap();

		let record_for = |source| {
			let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
			let record = ConsumptionRecord {
				entries: BTreeMap::from([(
					source,
					BTreeMap::from([(
						stream,
						Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) },
					)]),
				)]),
			};
			let lift = RequiresLift {
				advances: Vec::new(),
				extension: MMRExtensionProof::identity(),
				tree_proof,
			};
			(record, lift)
		};
		let (ra, la) = record_for(src_a);
		let (rb, lb) = record_for(src_b);
		let lifts = LiftsBySource::try_from(BTreeMap::from([(src_a, vec![la]), (src_b, vec![lb])]))
			.unwrap();

		let requires = build_requires(&[ra, rb], &lifts).unwrap().unwrap();
		assert_eq!(requires.len(), 2);
		assert_eq!(requires.get(src_a), Some(&expected));
		assert_eq!(requires.get(src_b), Some(&expected));
	}

	#[test]
	fn build_requires_entry_stitches_a_bundle_gap() {
		// Two intervals in one stream (a bundle where block 2 jumped context @2 → @4), bridged by
		// an advance, then extended @5 → committed root. stitch is unit-tested alone; this composes
		// it into the full entry path.
		let all = leaves(5);
		let stream = ch(2000);
		let committed = root_at(&all, 5);
		let entries = BTreeMap::from([(stream, committed.0)]);
		let expected = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let intervals = vec![
			Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) },
			Interval { start: root_at(&all, 4), end: frontier_at(&all, 5) }, // gap @2 → @4
		];
		let lift = RequiresLift {
			advances: vec![extension(&all, 2, 4)],    // bridges the gap
			extension: MMRExtensionProof::identity(), // endpoint @5 already the committed root
			tree_proof,
		};
		let streams = SourceStreams::from([(stream, intervals)]);
		assert_eq!(build_requires_entry(&streams, &[lift]).unwrap(), expected);
	}

	#[test]
	fn lift_count_must_match_stream_count() {
		let all = leaves(3);
		let stream = ch(2000);
		let streams = SourceStreams::from([(
			stream,
			vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }],
		)]);
		let lift = || RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		// One stream, no lifts, and one stream, two lifts — both mismatch.
		assert_eq!(build_requires_entry(&streams, &[]), Err(LiftError::LiftCountMismatch));
		assert_eq!(
			build_requires_entry(&streams, &[lift(), lift()]),
			Err(LiftError::LiftCountMismatch)
		);
	}

	#[test]
	fn build_requires_entry_rejects_a_backward_extension() {
		// A forged extension that does not go strictly forward fails verification → BadExtension,
		// before any root is synthesized.
		let all = leaves(5);
		let stream = ch(2000);
		let (_r, tree_proof) =
			gen_stream_proof(&BTreeMap::from([(stream, root_at(&all, 5).0)]), stream).unwrap();
		let streams = SourceStreams::from([(
			stream,
			vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }],
		)]);
		// leaf_count 1 <= the endpoint's 3 leaves → NotForward inside verify → BadExtension.
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof { leaf_count: 1, connecting_nodes: Vec::new() },
			tree_proof,
		};
		assert_eq!(build_requires_entry(&streams, &[lift]), Err(LiftError::BadExtension));
	}

	#[test]
	fn stitch_ignores_the_first_interval_start() {
		// `first.start` is the boundary with the previous candidate and is deliberately unchecked;
		// garbage there does not affect the stitched endpoint.
		let all = leaves(5);
		let garbage =
			Interval { start: MmrRoot(H256::repeat_byte(0xEE)), end: frontier_at(&all, 2) };
		let cont = Interval { start: root_at(&all, 2), end: frontier_at(&all, 5) };
		assert_eq!(stitch(&[garbage, cont], &[]).unwrap(), frontier_at(&all, 5));
	}

	#[test]
	fn build_requires_entry_rejects_a_malformed_tree_proof() {
		// stitch and extension succeed, but a tree_proof with an out-of-range split bit cannot
		// fold to any StreamsRoot → BadTreeProof. (A single-stream tree's real proof is empty, so
		// the malformed step has to be hand-built.)
		let all = leaves(3);
		let stream = ch(2000);
		let streams = SourceStreams::from([(
			stream,
			vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }],
		)]);
		// `split_bit == KEY_BITS` (64) is past the key; the fold rejects it.
		let bad = StreamProof {
			steps: BoundedVec::try_from(vec![TreeStep { split_bit: 64, sibling: H256::zero() }])
				.unwrap(),
		};
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: bad,
		};
		assert_eq!(build_requires_entry(&streams, &[lift]), Err(LiftError::BadTreeProof));
	}

	#[test]
	fn build_requires_entry_flags_missing_and_stray_advances() {
		// stitch's advance-count errors, reached through the entry path rather than in isolation.
		let all = leaves(5);
		let stream = ch(2000);
		let (_r, tree_proof) =
			gen_stream_proof(&BTreeMap::from([(stream, root_at(&all, 5).0)]), stream).unwrap();
		let mk = |intervals, advances| {
			let lift = RequiresLift {
				advances,
				extension: MMRExtensionProof::identity(),
				tree_proof: tree_proof.clone(),
			};
			build_requires_entry(&SourceStreams::from([(stream, intervals)]), &[lift])
		};

		// Gap @2 → @4 with no advance to bridge it.
		let gapped = vec![
			Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) },
			Interval { start: root_at(&all, 4), end: frontier_at(&all, 5) },
		];
		assert_eq!(mk(gapped, Vec::new()), Err(LiftError::MissingAdvance));

		// Continuous @2 → @2 but an advance supplied anyway.
		let continuous = vec![
			Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 2) },
			Interval { start: root_at(&all, 2), end: frontier_at(&all, 5) },
		];
		assert_eq!(mk(continuous, vec![extension(&all, 2, 4)]), Err(LiftError::StrayAdvance));
	}

	#[test]
	fn divergent_roots_rejected() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let (sa, sb) = (ch(2000), ch(3000));

		// Two single-stream trees → each stream's tree_proof folds to a DIFFERENT StreamsRoot.
		let (_ra, proof_a) = gen_stream_proof(&BTreeMap::from([(sa, root3.0)]), sa).unwrap();
		let (_rb, proof_b) = gen_stream_proof(&BTreeMap::from([(sb, root3.0)]), sb).unwrap();

		let mk = |tree_proof| RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let streams = SourceStreams::from([
			(sa, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }]),
			(sb, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }]),
		]);
		assert_eq!(
			build_requires_entry(&streams, &[mk(proof_a), mk(proof_b)]),
			Err(LiftError::DivergentRoots),
		);
	}

	/// Single-leaf inclusion proof for leaf `index` of an `n`-leaf MMR (`mmr_lib`'s `gen_proof`).
	fn inclusion(all: &[Hash], n: usize, index: usize) -> MmrInclusionProof {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for l in &all[..n] {
			mmr.push(*l).unwrap();
		}
		let proof = mmr.gen_proof(vec![leaf_index_to_pos(index as u64)]).unwrap();
		MmrInclusionProof { mmr_size: mmr_size(n), items: proof.proof_items().to_vec() }
	}

	#[test]
	fn inclusion_verify_head_pins_head_and_derives_root() {
		let all = leaves(6);
		// 6 leaves → head = index 5 (a lone smallest peak with a 1-step sibling path AND one other
		// peak: count_ones(6) - 1 = 1), exercising both halves of the item split.
		let proof = inclusion(&all, 6, 5);

		// Correct head leaf → head position 5 + the frontier of exactly these 6 leaves.
		let (pos, frontier) = proof.verify_head(all[5]).expect("well-formed head proof");
		assert_eq!(pos, MessagePosition(5));
		assert_eq!(frontier.leaf_count(), 6);
		assert_eq!(frontier.root(), root_at(&all, 6));

		// A different leaf under the same proof derives a frontier whose root does not match, an
		// old leaf cannot be forged as the head (the derived-root mismatch is what downstream
		// `under` comparison rejects).
		let (_, forged) = proof.verify_head(all[0]).expect("shape still parses");
		assert_ne!(forged.root(), root_at(&all, 6));

		// Wrong item count (one extra item) → structural reject.
		let mut bad = proof.clone();
		bad.items.push(H256::zero());
		assert_eq!(bad.verify_head(all[5]), Err(ProofError::InvalidProof));

		// Invalid `mmr_size` (2 is not a valid MMR node count) → reject, no panic.
		assert_eq!(
			MmrInclusionProof { mmr_size: 2, items: Vec::new() }.verify_head(all[5]),
			Err(ProofError::InvalidProof),
		);
	}

	#[test]
	fn inclusion_verify_head_boundary_shapes() {
		// The `path_len = trailing_zeros` / `other_peaks = count_ones - 1` formulas at their
		// extremes.

		// n = 1: a single-leaf MMR. The head proof has zero items (no sibling path, no other
		// peak).
		let all1 = leaves(1);
		let (pos1, f1) = inclusion(&all1, 1, 0).verify_head(all1[0]).expect("n=1 head");
		assert_eq!(pos1, MessagePosition(0));
		assert_eq!(f1.leaf_count(), 1);
		assert_eq!(f1.root(), root_at(&all1, 1));

		// n = 8: a perfect single-peak MMR. A full 3-step sibling path with no other peaks.
		let all8 = leaves(8);
		let (pos8, f8) = inclusion(&all8, 8, 7).verify_head(all8[7]).expect("n=8 head");
		assert_eq!(pos8, MessagePosition(7));
		assert_eq!(f8.leaf_count(), 8);
		assert_eq!(f8.root(), root_at(&all8, 8));
	}

	#[test]
	fn inclusion_verify_leaf_binds_position_and_value() {
		let all = leaves(6);
		let want = root_at(&all, 6);
		let proof = inclusion(&all, 6, 3);

		// Correct leaf at its position → the committed stream root.
		assert_eq!(proof.verify_leaf(MessagePosition(3), all[3]), Ok(want));
		// Tampered leaf value → a different root (never the committed one).
		assert_ne!(proof.verify_leaf(MessagePosition(3), H256::repeat_byte(0xAA)), Ok(want));
		// Correct leaf claimed at the wrong position → a different root.
		assert_ne!(proof.verify_leaf(MessagePosition(4), all[3]), Ok(want));
		// Position at/after the leaf count (6 leaves → valid 0..=5) → reject.
		assert_eq!(
			proof.verify_leaf(MessagePosition(6), all[3]),
			Err(ProofError::PositionOutOfRange)
		);
		// Invalid `mmr_size` → reject, no panic.
		assert_eq!(
			MmrInclusionProof { mmr_size: 2, items: Vec::new() }
				.verify_leaf(MessagePosition(0), all[0]),
			Err(ProofError::InvalidProof),
		);
	}

	#[test]
	fn inclusion_verify_leaf_rejects_oversized_items() {
		let all = leaves(6);
		// A valid proof, then bloated one item past the ceiling → rejected before `calculate_root`
		// touches the untrusted `items` (defense-in-depth against a crafted, oversized proof).
		let mut proof = inclusion(&all, 6, 3);
		proof.items = vec![H256::zero(); MAX_INCLUSION_PROOF_ITEMS + 1];
		assert_eq!(
			proof.verify_leaf(MessagePosition(3), all[3]),
			Err(ProofError::ItemLimitExceeded)
		);
	}

	#[test]
	fn inclusion_verify_head_rejects_non_head_proof() {
		let all = leaves(6);
		// A proof generated for a non-head leaf (index 2) has a different item count than the
		// head's expected shape (3 vs 2 for a 6-leaf MMR), so `verify_head`'s exact item-count
		// gate rejects it: a non-head leaf cannot be passed off as the head.
		let non_head = inclusion(&all, 6, 2);
		assert_eq!(non_head.verify_head(all[2]), Err(ProofError::InvalidProof));
	}

	#[test]
	fn inclusion_head_and_leaf_agree_on_root() {
		// Cross-check the two crypto paths: `verify_head`'s manual peak walk and `verify_leaf`'s
		// `mmr_lib::calculate_root` must derive the *same* root for the head leaf (and the
		// committed stream root). Guards against the two implementations diverging.
		let all = leaves(6);
		let proof = inclusion(&all, 6, 5);
		let (pos, frontier) = proof.verify_head(all[5]).expect("head verifies");
		assert_eq!(pos, MessagePosition(5));
		let via_head = frontier.root();
		let via_leaf = proof.verify_leaf(pos, all[5]).expect("leaf verifies");
		assert_eq!(via_head, via_leaf);
		assert_eq!(via_head, root_at(&all, 6));
	}
}

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

//! Requires-lift & consumption-record primitives (spec-msg v0.5).
//!
//! Blocks never emit `Requires` and never see a `StreamsRoot`. Processing the messaging inherent
//! writes a [`ConsumptionRecord`] — which streams the block touched, and the MMR [`Interval`] it
//! consumed on each. The `validate_block` wrapper then **synthesizes** the candidate's `Requires`
//! set from the records plus POV-carried **lifts** ([`RequiresLift`]): one per recorded stream,
//! binding that stream's consumption to a *current* committed `StreamsRoot`. The relay chain only
//! ever sees entries it can match. This unifies 0.3's in-block catch-up and late-block proofs into
//! one mechanism.
//!
//! The invariant [`build_requires`] enforces, per stream and candidate:
//!
//! > the recorded intervals form a **proven forward chain** — each block continues where the
//! > previous ended, or a POV proof shows the gap is a forward extension ([`stitch`]) — and one
//! > lift binds the chain's endpoint to a committed root, transitively binding everything consumed.
//!
//! Trust split: the **record** is authoritative (STF output — consumption can't be hidden); the
//! **lifts** are untrusted POV data, verified here against the record's key and a committed root.
//! Lifts are matched *positionally* to a source's `StreamId`-sorted record streams; a mispaired
//! lift cannot verify because the `tree_proof` walk binds the record's key (see
//! [`crate::streams_root::streams_root_from_proof`]).

use alloc::{collections::BTreeMap, vec, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use mmr_lib::{
	helper::{
		get_peak_map, get_peaks, is_valid_mmr_size, parent_offset, pos_height_in_tree,
		sibling_offset,
	},
	leaf_index_to_mmr_size, leaf_index_to_pos, MerkleProof, NodeMerkleProof,
};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::RequiresSet;
use scale_info::TypeInfo;

use crate::{
	mmr::{empty_root, root_from_peaks, MessagePosition, SpecMerge},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
	SpecHasher,
};

/// The keyed `StreamsRoot`-tree inclusion proof a lift walks from a stream root up to the
/// `StreamsRoot` (the design's `TreeInclusionProof`; concretely the keyed Patricia
/// [`StreamProof`]).
pub type TreeInclusionProof = StreamProof;

/// Upper bound on an [`MMRExtensionProof`]'s connecting nodes. A valid append-only ancestry proof
/// over an MMR whose leaf count fits in `u64` needs O(log n) nodes — never more than a small
/// multiple of the 64-bit height. `4 * 64` is a generous ceiling no valid proof reaches; rejecting
/// beyond it is pure defense-in-depth (PoV size already bounds the decoded length loosely).
const MAX_EXTENSION_CONNECTING_NODES: usize = 4 * 64;

/// Upper bound on an [`MmrInclusionProof`]'s items (the leaf's sibling path plus the other peaks).
/// A single-leaf proof over a `u64`-leaf-count MMR needs at most the subtree height (≤ 63) plus the
/// other peaks (≤ 63); `2 * 64` is a generous ceiling no valid proof reaches. Rejecting beyond it
/// bounds `calculate_root`'s work (and the items clone) on untrusted input — the same
/// defense-in-depth `MMRExtensionProof::verify` applies via [`MAX_EXTENSION_CONNECTING_NODES`].
const MAX_INCLUSION_PROOF_ITEMS: usize = 2 * 64;

/// Why an MMR proof verification (`MMRExtensionProof::verify`, `MmrInclusionProof::verify_head` /
/// `verify_leaf`) rejected its input. Typed so callers (peer scoring, retry logic) can tell a
/// malformed/forged proof from a merely out-of-range or non-forward one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProofError {
	/// The proof is structurally malformed: an invalid `mmr_size`, a wrong item/node count, or
	/// `mmr_lib` could not reconstruct a root from it.
	InvalidProof,
	/// A leaf position is at or past the proven MMR's leaf count (an empty MMR has no head).
	PositionOutOfRange,
	/// An extension does not go strictly forward (its target is not newer than the frontier).
	NotForward,
	/// The frontier is self-inconsistent (its peak count does not match its leaf count).
	InconsistentFrontier,
	/// An inclusion proof carries more items than any valid single-leaf proof
	/// ([`MAX_INCLUSION_PROOF_ITEMS`]) — rejected before doing untrusted-input work.
	ItemLimitExceeded,
	/// An extension proof carries more connecting nodes than any valid ancestry proof
	/// ([`MAX_EXTENSION_CONNECTING_NODES`]) — rejected before doing untrusted-input work.
	NodeLimitExceeded,
}

/// A bagged MMR root — a stream's committed root at some point in its history. Newtyped so it can
/// never be confused with a [`StreamsRoot`] (the commitment-tree root): "confusing roots must not
/// typecheck".
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MmrRoot(pub Hash);

/// A peaks-only MMR frontier — the O(log n) state a stream continues from (`mmr::Mmr::into_parts`).
/// `leaf_count` fixes the placement of an extension proof's connecting nodes relative to `peaks`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MmrFrontier {
	/// MMR peaks, highest to lowest.
	pub peaks: Vec<Hash>,
	/// Number of leaves these peaks summarize.
	pub leaf_count: u64,
}

impl MmrFrontier {
	/// The bagged root of this frontier; the empty frontier has the *defined* root
	/// `H(EMPTY_TAG)` ([`crate::mmr::empty_root`], encoding spec §3.4) — a comparable
	/// value, so an `Interval.start` of a stream's first-ever consumption stitches
	/// like any other.
	pub fn root(&self) -> MmrRoot {
		if self.peaks.is_empty() {
			MmrRoot(empty_root::<SpecHasher>())
		} else {
			MmrRoot(root_from_peaks::<SpecHasher>(&self.peaks))
		}
	}

	/// The `mmr_lib` node count (size) of this frontier's MMR.
	fn mmr_size(&self) -> u64 {
		if self.leaf_count == 0 {
			0
		} else {
			leaf_index_to_mmr_size(self.leaf_count - 1)
		}
	}
}

/// An append-only MMR extension proof: **O(log n) connecting nodes** that, with a frontier's own
/// peaks, reconstruct a newer root — the concrete `polkadot-ckb-merkle-mountain-range`
/// (`gen_ancestry_proof`) form of the design's `connecting_nodes`. Verification **computes and
/// returns** the new root (never declared alongside the proof), treating the frontier's peaks as
/// opaque fixed subtrees whose placement is fixed by its `leaf_count`. Payloads / appended leaves
/// are never needed.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MMRExtensionProof {
	/// Leaf count of the extended (newer) MMR. The `mmr_lib` node count is *derived* from it
	/// (`leaf_index_to_mmr_size`), so an out-of-range size can never be smuggled in. `0` with
	/// empty `connecting_nodes` is the identity extension (the endpoint already is the current
	/// root).
	pub leaf_count: u64,
	/// The O(log n) connecting-node **hashes** — the extended MMR's witnesses not derivable from
	/// the frontier's own peaks (from `gen_ancestry_proof`), in `mmr_lib`'s proof order. Their
	/// positions are *not* stored: an MMR's shape is fixed by its size, so they are re-derived at
	/// verify time from `(from.leaf_count, self.leaf_count)` (see `ancestry_positions`). Empty for
	/// the identity.
	pub connecting_nodes: Vec<Hash>,
}

impl MMRExtensionProof {
	/// The identity extension: `leaf_count = 0`, no connecting nodes — yields a frontier's own root
	/// unchanged (the common caught-up case where the endpoint already *is* the current stream
	/// root).
	pub fn identity() -> Self {
		Self { leaf_count: 0, connecting_nodes: Vec::new() }
	}

	/// Whether this is the identity (empty) extension.
	pub fn is_identity(&self) -> bool {
		self.leaf_count == 0 && self.connecting_nodes.is_empty()
	}

	/// Extend `from` and **return** the new root, computed from `from`'s peaks + the connecting
	/// nodes; an [`Err`] if the proof is not well-formed for that placement. The identity extension
	/// yields `from`'s own root. `from` is the verifier's own state — its self-consistency is
	/// asserted but its contents are not re-checked. The `mmr_lib` size is *derived* from
	/// `leaf_count`, so it is always a valid MMR size.
	pub fn verify(&self, from: &MmrFrontier) -> Result<MmrRoot, ProofError> {
		// Frontier self-consistency: exactly one peak per set bit of its leaf count.
		if from.peaks.len() != from.leaf_count.count_ones() as usize {
			return Err(ProofError::InconsistentFrontier);
		}
		if self.is_identity() {
			return Ok(from.root());
		}
		// Strictly forward: the extended MMR must have more leaves than the frontier.
		if self.leaf_count <= from.leaf_count {
			return Err(ProofError::NotForward);
		}
		// Defense-in-depth: bound an untrusted proof's connecting nodes before doing O(n) work in
		// `calculate_root`. A valid ancestry proof is far below this ceiling.
		if self.connecting_nodes.len() > MAX_EXTENSION_CONNECTING_NODES {
			return Err(ProofError::NodeLimitExceeded);
		}
		let new_mmr_size = leaf_index_to_mmr_size(self.leaf_count - 1);

		// Extending from the empty stream: nothing binds (a prefix of nothing is vacuous), so the
		// connecting nodes must simply BE the new MMR's peaks, in canonical (`get_peaks`) order.
		if from.leaf_count == 0 {
			let peaks = get_peaks(new_mmr_size);
			if self.connecting_nodes.len() != peaks.len() {
				return Err(ProofError::InvalidProof);
			}
			return Ok(MmrFrontier {
				peaks: self.connecting_nodes.clone(),
				leaf_count: self.leaf_count,
			}
			.root());
		}

		// General: `from`'s peaks are complete subtrees of the new MMR at their unchanged
		// positions; combined with the connecting nodes they reconstruct the new root
		// (`calculate_root` merges them under `SpecMerge`, matching the sender's MMR).
		let old_mmr_size = from.mmr_size();
		let old_positions = get_peaks(old_mmr_size);
		if old_positions.len() != from.peaks.len() {
			return Err(ProofError::InconsistentFrontier);
		}
		// The connecting-node positions aren't carried: derive them from the two MMR sizes (an
		// MMR's shape is fixed by its size) and re-pair them with the stored hashes, in `mmr_lib`'s
		// proof order. A length mismatch means the proof doesn't fit the claimed extension.
		let positions = ancestry_positions(old_mmr_size, new_mmr_size);
		if positions.len() != self.connecting_nodes.len() {
			return Err(ProofError::InvalidProof);
		}
		let proof: Vec<(u64, Hash)> =
			positions.into_iter().zip(self.connecting_nodes.iter().copied()).collect();
		let nodes: Vec<(u64, Hash)> =
			old_positions.into_iter().zip(from.peaks.iter().copied()).collect();
		NodeMerkleProof::<Hash, SpecMerge<SpecHasher>>::new(new_mmr_size, proof)
			.calculate_root(nodes)
			.map(MmrRoot)
			.map_err(|_| ProofError::InvalidProof)
	}
}

/// Derive the connecting-node positions of an MMR ancestry proof from the old and new MMR sizes
/// alone — the key to storing [`MMRExtensionProof::connecting_nodes`] as bare hashes. An MMR's
/// shape is fully determined by its size, so the set and order of nodes `gen_ancestry_proof` emits
/// (proving the old peaks under the new root) is a pure function of the two sizes. This mirrors
/// `polkadot-ckb-merkle-mountain-range` 0.8.2's `gen_ancestry_proof` / `gen_node_proof_for_peak`
/// position bookkeeping — the per-peak sibling walk, the RHS-peak bagging collapse, and the final
/// position sort — so the derived positions line up with the prover's hash order exactly. It is
/// pinned to that version; `ancestry_positions_matches_mmr_lib` cross-checks it against the
/// library. Both sizes must be valid MMR sizes with `old_mmr_size < new_mmr_size` and a non-empty
/// old MMR (the empty and identity extensions are handled before this is reached).
// TODO: workaround — upstream this into `paritytech/merkle-mountain-range` as a store-free
// `ancestry_proof_positions(prev_mmr_size, mmr_size)` factored out of `gen_ancestry_proof` (single
// source of truth, no version pinning), then replace this + `gen_node_positions_for_peak` with a
// call to it once released. Cf. the existing `expected_ancestry_proof_size`.
fn ancestry_positions(old_mmr_size: u64, new_mmr_size: u64) -> Vec<u64> {
	let mut pos_list = get_peaks(old_mmr_size);
	let mut proof: Vec<u64> = Vec::new();
	// A run of trailing new-MMR peaks with no proven descendants is bagged into one item.
	let mut bagging_track = 0usize;
	for peak_pos in get_peaks(new_mmr_size) {
		// The old peaks under this new peak (`pos_list` is ascending, so a leading prefix).
		let cut = pos_list.iter().position(|&pos| pos > peak_pos).unwrap_or(pos_list.len());
		let sub: Vec<u64> = pos_list.drain(..cut).collect();
		if sub.is_empty() {
			bagging_track += 1;
		} else {
			bagging_track = 0;
		}
		gen_node_positions_for_peak(&mut proof, sub, peak_pos);
	}
	if bagging_track > 1 {
		// The bagged item takes the leftmost bagged peak's position; drop the rest.
		let rhs = proof.split_off(proof.len() - bagging_track);
		proof.push(rhs[0]);
	}
	proof.sort_unstable();
	proof
}

/// Position-only mirror of `mmr_lib`'s `gen_node_proof_for_peak`: append the proof-node positions
/// that witness `pos_list` (nodes to prove, all `<= peak_pos`) up to `peak_pos`. The `queue` is a
/// `(height, pos)` min-heap kept as an ascending `Vec` (positions are unique, so no dedup is
/// needed); `remove(0)` is `pop_front`. Pure structure — no hashes.
fn gen_node_positions_for_peak(proof: &mut Vec<u64>, pos_list: Vec<u64>, peak_pos: u64) {
	// The peak itself is proven: no witnesses under it.
	if pos_list.len() == 1 && pos_list[0] == peak_pos {
		return;
	}
	// Nothing proven under this peak: the peak's own hash is a witness.
	if pos_list.is_empty() {
		proof.push(peak_pos);
		return;
	}
	let mut queue: Vec<(u8, u64)> =
		pos_list.into_iter().map(|pos| (pos_height_in_tree(pos), pos)).collect();
	queue.sort_unstable();
	while !queue.is_empty() {
		let (height, pos) = queue.remove(0);
		if pos == peak_pos {
			if queue.is_empty() {
				break;
			}
			continue;
		}
		let (sib_pos, parent_pos) = {
			let next_height = pos_height_in_tree(pos + 1);
			let offset = sibling_offset(height);
			if next_height > height {
				// pos is a right sibling.
				(pos - offset, pos + 1)
			} else {
				// pos is a left sibling.
				(pos + offset, pos + parent_offset(height))
			}
		};
		if queue.first().map(|(_, p)| *p) == Some(sib_pos) {
			// The sibling is also being proven: it cancels, no witness needed.
			queue.remove(0);
		} else {
			proof.push(sib_pos);
		}
		if parent_pos < peak_pos {
			let entry = (height + 1, parent_pos);
			let idx = queue.partition_point(|x| x < &entry);
			queue.insert(idx, entry);
		}
	}
}

/// An MMR inclusion proof for a **single leaf** — `mmr_lib`'s [`MerkleProof`] items (the leaf's
/// sibling path plus the other peaks, for bagging) and the node count they were generated against.
/// The concrete `polkadot-ckb-merkle-mountain-range` (`gen_proof`) form used by the lossy
/// event/register head read: where [`MMRExtensionProof`] bridges a frontier *forward*, this places
/// one leaf under a *fixed* stream MMR. As everywhere, the root is **derived**, never declared — a
/// tampered item yields a root no `tree_proof` can bind.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MmrInclusionProof {
	/// `mmr_lib` node count (size) of the MMR the proof was generated against; with the leaf count
	/// (`get_peak_map`) it pins a head read.
	pub mmr_size: u64,
	/// Proof items — the leaf's sibling path plus the other peaks, in `mmr_lib` order.
	pub items: Vec<Hash>,
}

impl MmrInclusionProof {
	/// Leaf count of the proven MMR (the peak map of a valid MMR size *is* the leaf count).
	pub fn leaf_count(&self) -> u64 {
		get_peak_map(self.mmr_size)
	}

	/// Verify `leaf` (already hashed via [`crate::message::leaf_hash`]) as the **head** — the last
	/// leaf — of the proven MMR, returning the head position and the full frontier (peaks + leaf
	/// count) of exactly that MMR. An [`Err`] on any structural defect.
	///
	/// This is the register/event head-read verification: lossy consumers need the peaks, not just
	/// the root, because the read enters the consumption record as an interval whose `end` frontier
	/// the next gap check — or the lift's extension proof — extends from.
	///
	/// The expected shape is what `gen_proof` for the last leaf produces: the other peaks
	/// left-to-right, then the head's `leaf_count.trailing_zeros()` LEFT siblings bottom-up. The
	/// item count is checked exactly — the bytes have one valid form.
	///
	/// **Only reconstructs** a frontier from `(leaf, items)`; it does **not** bind the result to
	/// any committed `StreamsRoot`. An `Ok` here is *not* authentication — a caller must bind the
	/// derived root to a trusted `under`. Prefer [`crate::message::verify_event`] /
	/// [`crate::message::verify_event_response`], which do that; never trust a raw `verify_head`.
	pub fn verify_head(&self, leaf: Hash) -> Result<(MessagePosition, MmrFrontier), ProofError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(ProofError::InvalidProof);
		}
		let leaf_count = self.leaf_count();
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
			last_peak = <SpecMerge<SpecHasher> as mmr_lib::Merge>::merge(sibling, &last_peak)
				.expect("SpecMerge::merge is infallible; qed");
		}
		let peaks: Vec<Hash> =
			peak_items.iter().copied().chain(core::iter::once(last_peak)).collect();

		Ok((MessagePosition(leaf_count - 1), MmrFrontier { peaks, leaf_count }))
	}

	/// Verify `leaf` (already hashed via [`crate::message::leaf_hash`]) at `position`, returning
	/// the implied stream [`MmrRoot`]. An [`Err`] if the position is out of range or the proof is
	/// malformed.
	///
	/// **Only reconstructs** the implied root; it does **not** bind it to any committed
	/// `StreamsRoot` — an `Ok` is not authentication. Prefer
	/// [`crate::message::verify_positional_event_response`], which binds the result to a trusted
	/// `under`.
	pub fn verify_leaf(
		&self,
		position: MessagePosition,
		leaf: Hash,
	) -> Result<MmrRoot, ProofError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(ProofError::InvalidProof);
		}
		// Defense-in-depth: bound untrusted `items` before the O(n) `calculate_root` (and its
		// clone). A valid single-leaf proof is far below this ceiling; `verify_head` gets the
		// same bound for free from its exact item-count check.
		if self.items.len() > MAX_INCLUSION_PROOF_ITEMS {
			return Err(ProofError::ItemLimitExceeded);
		}
		if position.0 >= self.leaf_count() {
			return Err(ProofError::PositionOutOfRange);
		}
		let pos = leaf_index_to_pos(position.0);
		MerkleProof::<Hash, SpecMerge<SpecHasher>>::new(self.mmr_size, self.items.clone())
			.calculate_root(vec![(pos, leaf)])
			.map(MmrRoot)
			.map_err(|_| ProofError::InvalidProof)
	}
}

/// Per stream and block: consumption entered the block at `start` and left it at `end`.
///
/// The candidate's lift binds only the LAST state to a committed root; the intervals stretch that
/// guarantee back over the whole bundle — each block must start where the previous ended, or prove
/// the jump moved forward ([`stitch`]).
///
/// - **Channels**: `start` = the frontier root before the block's incoming messages, `end` = the
///   frontier after. `start == previous end` holds by construction, so the chain check is free.
/// - **Register / event reads**: `end` = the context the reads were verified against, `start` = its
///   root (nothing advances). Contexts *can* jump between blocks (a fresher read mid-bundle is the
///   point), so a fabricated context breaks the chain instead of hiding behind a later genuine one.
///
/// `end` is a full frontier because the next gap check, or the lift, extends from it.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct Interval {
	/// Root the block's consumption on this stream started from.
	pub start: MmrRoot,
	/// Frontier the block's consumption on this stream ended at.
	pub end: MmrFrontier,
}

/// Written per block: the streams this block touched and the interval it consumed on each, grouped
/// by source. Per source the entries are `StreamId`-sorted and unique (the messaging inherent
/// carries at most one item per stream). This is the API view of a flat, host-append-only outbox
/// vec, grouped and sorted at read time.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct ConsumptionRecord {
	/// `source -> [(stream, interval)]`; each source's streams `StreamId`-sorted and unique.
	pub entries: BTreeMap<ParaId, Vec<(StreamId, Interval)>>,
}

/// One lift, carried in the POV (never in the block or commitments). Matched positionally to a
/// source's consumption-record streams (`StreamId`-sorted); a mispaired lift cannot verify because
/// `tree_proof` binds the record's key.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct RequiresLift {
	/// One extension per gap in the stream's interval chain, in gap order; empty for a caught-up
	/// single-context stream.
	pub advances: Vec<MMRExtensionProof>,
	/// Extends the chain's endpoint to the stream's current root; verification yields that root
	/// (the identity extension when the endpoint already is the current root).
	pub extension: MMRExtensionProof,
	/// Keyed walk from the current stream root to the `StreamsRoot` the requires entry becomes.
	pub tree_proof: TreeInclusionProof,
}

/// Everything that can invalidate requires synthesis. All are deterministic functions of the
/// records and POV, so every validator reaches the same verdict.
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

/// Stitch one stream's intervals (bundle order) into its endpoint frontier. `advances` must supply
/// exactly one extension per gap — a `start` that is not the previous `end`'s bagged root — in gap
/// order. Gaps are proven *forward* (extension proofs only exist forward), so verified states can
/// never regress.
pub fn stitch(
	intervals: &[Interval],
	advances: &[MMRExtensionProof],
) -> Result<MmrFrontier, LiftError> {
	let (first, rest) = intervals.split_first().ok_or(LiftError::EmptyRecord)?;
	let mut gaps = advances.iter();
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

/// Lift one source's streams to their single committed `StreamsRoot`: stitch each stream's
/// intervals into an endpoint, extend it to the stream's current root, walk the tree to the
/// `StreamsRoot`, and require *all* of the source's streams to lift to the **same** root. Returns
/// that root; the caller ([`build_requires`]) already holds the source and pairs it. `streams`
/// iterates in `StreamId` order; `lifts` matches it positionally.
pub fn build_requires_entry(
	streams: &[(StreamId, Vec<Interval>)],
	lifts: &[RequiresLift],
) -> Result<StreamsRoot, LiftError> {
	if streams.len() != lifts.len() {
		return Err(LiftError::LiftCountMismatch);
	}
	let mut entry: Option<StreamsRoot> = None;
	for ((stream, intervals), lift) in streams.iter().zip(lifts) {
		let endpoint = stitch(intervals, &lift.advances)?;
		// The endpoint is contained in the stream's current root (computed, not declared)...
		let current = lift.extension.verify(&endpoint).map_err(|_| LiftError::BadExtension)?;
		// ...and the keyed tree walk from it yields the StreamsRoot this stream lifts to. The walk
		// binds the record's `stream` key, so a mispaired lift cannot verify.
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

/// Synthesize the candidate's [`RequiresSet`] from the per-block consumption records (bundle order)
/// and the POV lifts (grouped per source). Sources must match exactly — a recorded source without
/// lifts, or lifts for an unrecorded source, invalidates the candidate.
pub fn build_requires(
	records: &[ConsumptionRecord],
	lifts: &BTreeMap<ParaId, Vec<RequiresLift>>,
) -> Result<RequiresSet, LiftError> {
	// Merge per (source, stream) intervals across the bundle, preserving block order.
	let mut merged: BTreeMap<ParaId, BTreeMap<StreamId, Vec<Interval>>> = BTreeMap::new();
	for record in records {
		for (source, streams) in &record.entries {
			let by_stream = merged.entry(*source).or_default();
			for (stream, interval) in streams {
				by_stream.entry(*stream).or_default().push(interval.clone());
			}
		}
	}
	// Recorded sources and lift sources must be exactly equal (both iterate ParaId-sorted).
	if !merged.keys().eq(lifts.keys()) {
		return Err(LiftError::LiftSourceMismatch);
	}
	let entries = merged
		.iter()
		.zip(lifts.values())
		.map(|((source, by_stream), source_lifts)| {
			let streams: Vec<(StreamId, Vec<Interval>)> =
				by_stream.iter().map(|(s, i)| (*s, i.clone())).collect();
			build_requires_entry(&streams, source_lifts).map(|root| (*source, root))
		})
		.collect::<Result<Vec<_>, _>>()?;
	RequiresSet::try_from_iter(entries).map_err(|_| LiftError::TooManySources)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mmr::{Mmr, MmrAccumulator},
		streams_root::{gen_stream_proof, streams_root},
	};
	use mmr_lib::util::{MemMMR, MemStore};
	use polkadot_primitives::{v9::MAX_COMMITMENT_ENTRIES, MAX_POV_SIZE};
	use sp_core::H256;

	type H = SpecHasher;

	fn leaves(n: u64) -> Vec<Hash> {
		(0..n).map(|i| H256::repeat_byte(i as u8 + 1)).collect()
	}

	/// Peaks-only frontier after the first `k` leaves.
	fn frontier_at(all: &[Hash], k: usize) -> MmrFrontier {
		let mut mmr = Mmr::<H>::new();
		for l in &all[..k] {
			mmr.append(*l);
		}
		let (peaks, leaf_count) = mmr.into_parts();
		MmrFrontier { peaks, leaf_count }
	}

	/// Bagged root over the first `k` leaves.
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
		let mut mmr = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
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

	/// A keyed `StreamsRoot`-trie membership proof for one stream out of `n_streams` active
	/// streams. Recipients are clustered (`2000..2000+n`), so keys diverge only in the low bits —
	/// the realistic worst case for Patricia depth.
	fn tree_proof(n_streams: u32) -> StreamProof {
		let entries: Vec<(StreamId, Hash)> =
			(0..n_streams).map(|i| (ch(2_000 + i), H256::repeat_byte(i as u8))).collect();
		let target = ch(2_000 + n_streams / 2);
		gen_stream_proof(entries, target).expect("target stream is present; qed").1
	}

	/// An extension proof of a given connecting-node count, for sizing the design's *day-scale*
	/// worst case (≈10⁹ leaves → ~30 nodes) without building a 10⁹-leaf MMR: PoV cost is the
	/// *encoded* size, which is a pure function of the node count, so synthetic nodes size
	/// identically to real ones. Each node is a bare 32 B `Hash` (positions are derived at verify
	/// time, not carried), matching the design's hash-only count.
	fn synthetic_extension(nodes: usize) -> MMRExtensionProof {
		MMRExtensionProof {
			leaf_count: u64::MAX / 2, // a day-scale count; value is irrelevant to encoded size
			connecting_nodes: (0..nodes as u64).map(|i| H256::repeat_byte(i as u8)).collect(),
		}
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
	/// for every `(k, n)` shape — the consensus-critical invariant that lets `connecting_nodes`
	/// store bare hashes. Sweeps enough leaf counts to exercise multi-peak proofs and the
	/// RHS-peak bagging collapse; if `mmr_lib`'s node set/order ever drifts, this trips.
	#[test]
	fn ancestry_positions_matches_mmr_lib() {
		for n in 2..=64usize {
			let all = leaves(n as u64);
			let store = MemStore::<Hash>::default();
			let mut mmr = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
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
	fn extension_verify_rejects_non_forward() {
		let all = leaves(5);
		// A real (non-identity) extension pointed *back* to the frontier's own size is not strictly
		// forward → rejected up front, before any crypto work.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = 2;
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Err(ProofError::NotForward));
	}

	#[test]
	fn extension_identity_on_empty_frontier_yields_empty_root() {
		// The empty frontier has the defined root H(EMPTY_TAG) (encoding spec §3.4); the
		// identity extension yields it like any other frontier's own root. Nothing binds
		// it to a committed entry — an empty stream is never in the StreamsRoot tree, so
		// a downstream tree walk fails naturally.
		assert_eq!(
			MMRExtensionProof::identity().verify(&MmrFrontier::default()),
			Ok(MmrFrontier::default().root()),
		);
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
	fn hot_path_single_block_single_stream() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let stream = ch(2000);
		// StreamsRoot over the single active stream at root@3.
		let entries = vec![(stream, root3.0)];
		let expected = streams_root(entries.clone()).unwrap();
		let (_r, tree_proof) = gen_stream_proof(entries, stream).unwrap();

		// One interval, endpoint = frontier@3; identity extension (caught up).
		let interval = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) };
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let root = build_requires_entry(&[(stream, vec![interval])], &[lift]).unwrap();
		assert_eq!(root, expected);
	}

	#[test]
	fn build_requires_end_to_end_and_source_match() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let source = ParaId::from(1000);
		let stream = ch(2000);
		let entries = vec![(stream, root3.0)];
		let expected = streams_root(entries.clone()).unwrap();
		let (_r, tree_proof) = gen_stream_proof(entries, stream).unwrap();

		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				vec![(
					stream,
					Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) },
				)],
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

		let requires = build_requires(&[record.clone()], &lifts).unwrap();
		assert_eq!(requires.get(source), Some(&expected));

		// A lift for an unrecorded source (or a missing one) is rejected.
		let mut extra = lifts.clone();
		extra.insert(ParaId::from(9999), Vec::new());
		assert_eq!(build_requires(&[record], &extra), Err(LiftError::LiftSourceMismatch));
	}

	#[test]
	fn divergent_roots_rejected() {
		let all = leaves(3);
		let root3 = root_at(&all, 3);
		let (sa, sb) = (ch(2000), ch(3000));

		// Two single-stream trees → each stream's tree_proof folds to a DIFFERENT StreamsRoot.
		let (_ra, proof_a) = gen_stream_proof(vec![(sa, root3.0)], sa).unwrap();
		let (_rb, proof_b) = gen_stream_proof(vec![(sb, root3.0)], sb).unwrap();

		let mk = |tree_proof| RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let streams = vec![
			(sa, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }]),
			(sb, vec![Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 3) }]),
		];
		assert_eq!(
			build_requires_entry(&streams, &[mk(proof_a), mk(proof_b)]),
			Err(LiftError::DivergentRoots),
		);
	}

	/// Empirical PoV-cost report for the requires-lift (run with `-- --nocapture`).
	///
	/// v0.5 replaces the pre-v0.5 point-3 two-level `sp_trie` read (relay `paras::Heads` proof + a
	/// sender-state proof) with the POV-carried [`RequiresLift`], committing against the sender's
	/// own keyed binary Patricia trie — no `sp_trie` witnesses. This measures the real
	/// SCALE-encoded size along its two dimensions — the `tree_proof` ([`StreamProof`], `O(log S)`
	/// in active streams) and the `extension` ([`MMRExtensionProof`], `O(log n)` in messages) —
	/// and asserts both stay logarithmic and the protocol's bounded worst case sits well under
	/// `MAX_POV_SIZE`.
	#[test]
	fn pov_cost_report() {
		println!("\n===== spec-msg v0.5 requires-lift PoV cost (SCALE-encoded, real proofs) =====");

		// tree_proof: keyed StreamsRoot Patricia trie, O(log S) in a source's active streams.
		println!("\n[tree_proof]  StreamProof over S active streams (clustered keys)");
		println!("        S | steps | encoded");
		for s in [1u32, 4, 16, 64, 256, 1024] {
			let proof = tree_proof(s);
			// Patricia-compressed: depth is O(log S), never the 64-bit key width.
			assert!(
				proof.steps.len() <= 20,
				"tree proof depth {} too large for S={s}",
				proof.steps.len()
			);
			println!("     {s:>4} | {:>5} | {:>5} B", proof.steps.len(), proof.encoded_size());
		}

		// extension: MMR ancestry, O(log n) in the stream's total messages. Build one MMR to N
		// leaves; measure the ancestry proof bridging an endpoint `behind` leaves back from the
		// tip (the unconsumed tail a lagging receiver must extend over).
		const N: usize = 100_000;
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
		for i in 0..N as u64 {
			mmr.push(H256::from_low_u64_be(i)).unwrap();
		}
		// NB: each connecting node is a bare 32 B `Hash` (positions derived from the two leaf
		// counts at verify time), so measured `encoded` tracks the design's "N hashes × 32 B"
		// estimate.
		println!("\n[extension]  MMRExtensionProof to tip N={N}, endpoint `behind` leaves back");
		println!("     behind | nodes | encoded (32 B/node)");
		let mut sample_ext = MMRExtensionProof::identity();
		for behind in [1usize, 16, 256, 4_096, N / 2] {
			let k = N - behind;
			let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
			let ext = MMRExtensionProof {
				leaf_count: N as u64,
				connecting_nodes: ap
					.prev_peaks_proof
					.proof_items()
					.iter()
					.map(|(_, h)| *h)
					.collect(),
			};
			// O(log N) connecting nodes — independent of how far the tail stretches.
			assert!(
				ext.connecting_nodes.len() <= 64,
				"extension not O(log N): {}",
				ext.connecting_nodes.len()
			);
			println!(
				"   {behind:>7} | {:>5} | {:>5} B",
				ext.connecting_nodes.len(),
				ext.encoded_size()
			);
			sample_ext = ext;
		}

		// combined: one RequiresLift = extension + tree_proof (advances empty in the hot path; each
		// interval-chain gap would add one more MMRExtensionProof of the same order).
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: sample_ext.clone(), // endpoint N/2 behind on an N-message stream
			tree_proof: tree_proof(64),    // source with 64 active streams
		};
		let per_lift = lift.encoded_size();
		println!(
			"\n[lift]  one RequiresLift (ext@N/2 + tree_proof@S=64, advances=0): {per_lift} B"
		);

		// Per source: one lift per stream consumed from it (all fold to the SAME StreamsRoot). Per
		// candidate: summed across sources.
		println!("\n[candidate]  requires-lift PoV = Σ sources × streams/source × per_lift");
		for (streams_per_source, sources) in [(1usize, 1usize), (2, 10), (4, 50)] {
			let candidate = sources * streams_per_source * per_lift;
			println!("   {sources:>3} sources × {streams_per_source} stream(s) = {candidate} B");
		}
		let budget = MAX_POV_SIZE as usize;

		// --- Angle 1: empirical PoV share (real proofs, ≤100k messages, no gaps) ---
		// A full-cap candidate — `MaxTouchedStreams` touched streams, itself capped at
		// MAX_COMMITMENT_ENTRIES (one lift per touched stream, regardless of how they spread
		// over sources) — with a conservative *measured* lift (deepest 100k-message extension +
		// a 1024-stream tree_proof): what fraction of the budget do real proofs at
		// representative scale consume? Guards the typical footprint at <10%.
		let empirical_lift = RequiresLift {
			advances: Vec::new(),
			extension: sample_ext, // deepest 100k-message extension measured above
			tree_proof: tree_proof(1024),
		};
		let empirical_per_lift = empirical_lift.encoded_size();
		let full_candidate = MAX_COMMITMENT_ENTRIES as usize * empirical_per_lift;
		println!(
			"\n[empirical share]  {} touched streams × {empirical_per_lift} B \
			 = {full_candidate} B ({:.2}% of MAX_POV_SIZE = {} MiB)",
			MAX_COMMITMENT_ENTRIES,
			100.0 * full_candidate as f64 / budget as f64,
			budget / (1024 * 1024),
		);
		assert!(
			full_candidate < budget / 10,
			"empirical full-cap candidate {full_candidate} B exceeds 10% of MAX_POV_SIZE",
		);

		// --- Angle 2: day-scale worst-case lift (the design's "Proof Size Considerations" sizing)
		// --- The measurements above top out at N=100k messages; the design sizes against ~24 h
		// of lag (≈10⁹ leaves → ~30 connecting nodes) *plus* one read-context gap (an advance
		// proof). We can't build a 10⁹-leaf MMR, but PoV cost is the encoded size, so synthesize
		// proofs of the worst-case shape. Ceiling per the design: ~4 KB/stream + ~2 KB/gap.
		const DAY_SCALE_EXT_NODES: usize = 30; // log₂(10⁹) ≈ 30 (design's "~30 hashes")
		let worst_lift = RequiresLift {
			advances: vec![synthetic_extension(DAY_SCALE_EXT_NODES)], // one read-context gap
			extension: synthetic_extension(DAY_SCALE_EXT_NODES),      // day-scale unconsumed tail
			tree_proof: tree_proof(1024),                             // 1024 active streams
		};
		let worst_per_lift = worst_lift.encoded_size();
		println!(
			"\n[worst-case lift]  day-scale (~10⁹ leaves, {DAY_SCALE_EXT_NODES}-node ext + 1 gap + tree@S=1024): {worst_per_lift} B"
		);
		// Regression guard: one worst-case lift stays bounded. The design ceilings a touched stream
		// at ~4 KB + ~2 KB/gap ≈ 6 KB; guard at 8 KB so an O(n) extension (non-logarithmic) or an
		// uncompressed tree walk trips this instead of silently eating the PoV.
		assert!(worst_per_lift < 8 * 1024, "worst-case lift {worst_per_lift} B exceeds ~8 KB");

		// Authoring-time budget (design: "the block must guarantee at authoring time that the worst
		// case still fits"): the bound is *how many touched streams fit*, not a fixed per-candidate
		// total. Even at the day-scale worst case the PoV must comfortably hold the source cap.
		let max_streams = budget / worst_per_lift;
		println!(
			"[headroom]  MAX_POV_SIZE {} MiB / {worst_per_lift} B ≈ {max_streams} day-scale touched streams \
			 (source cap = {MAX_COMMITMENT_ENTRIES})",
			budget / (1024 * 1024),
		);
		// A candidate names ≤ MAX_COMMITMENT_ENTRIES sources; even at day-scale the PoV must hold
		// at least that many touched streams, or authoring couldn't fill a block. (Ample margin
		// here.)
		assert!(
			max_streams >= MAX_COMMITMENT_ENTRIES as usize,
			"day-scale worst case leaves room for only {max_streams} streams (< {MAX_COMMITMENT_ENTRIES})",
		);
		println!("===== end =====\n");
	}

	/// Single-leaf inclusion proof for leaf `index` of an `n`-leaf MMR (`mmr_lib`'s `gen_proof`).
	fn inclusion(all: &[Hash], n: usize, index: usize) -> MmrInclusionProof {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
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
		// peak — count_ones(6) - 1 = 1), exercising both halves of the item split.
		let proof = inclusion(&all, 6, 5);

		// Correct head leaf → head position 5 + the frontier of exactly these 6 leaves.
		let (pos, frontier) = proof.verify_head(all[5]).expect("well-formed head proof");
		assert_eq!(pos, MessagePosition(5));
		assert_eq!(frontier.leaf_count, 6);
		assert_eq!(frontier.root(), root_at(&all, 6));

		// A different leaf under the same proof derives a frontier whose root does NOT match — an
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

		// n = 1: a single-leaf MMR — the head proof has ZERO items (no sibling path, no other
		// peak).
		let all1 = leaves(1);
		let (pos1, f1) = inclusion(&all1, 1, 0).verify_head(all1[0]).expect("n=1 head");
		assert_eq!(pos1, MessagePosition(0));
		assert_eq!(f1.leaf_count, 1);
		assert_eq!(f1.root(), root_at(&all1, 1));

		// n = 8: a perfect single-peak MMR — a full 3-step sibling path with NO other peaks.
		let all8 = leaves(8);
		let (pos8, f8) = inclusion(&all8, 8, 7).verify_head(all8[7]).expect("n=8 head");
		assert_eq!(pos8, MessagePosition(7));
		assert_eq!(f8.leaf_count, 8);
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
		// gate rejects it outright — a non-head leaf can't be passed off as the head.
		let non_head = inclusion(&all, 6, 2);
		assert_eq!(non_head.verify_head(all[2]), Err(ProofError::InvalidProof));
	}

	#[test]
	fn inclusion_head_and_leaf_agree_on_root() {
		// Cross-check the two crypto paths: `verify_head`'s manual peak walk and `verify_leaf`'s
		// `mmr_lib::calculate_root` must derive the *same* root for the head leaf (and the
		// committed stream root). Guards against the two implementations silently diverging.
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

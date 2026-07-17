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

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size, NodeMerkleProof};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::RequiresSet;
use scale_info::TypeInfo;

use crate::{
	mmr::{root_from_peaks, SpecMerge},
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
	/// The bagged root of this frontier, or `None` for an empty frontier (which has no root).
	pub fn root(&self) -> Option<MmrRoot> {
		(!self.peaks.is_empty()).then(|| MmrRoot(root_from_peaks::<SpecHasher>(&self.peaks)))
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
	/// `mmr_lib` node count of the extended MMR (with the frontier's `leaf_count`, this places the
	/// connecting nodes). `0` for the identity extension.
	pub new_mmr_size: u64,
	/// The O(log n) connecting nodes `(position, hash)` — the extended MMR's peaks / witnesses not
	/// derivable from the frontier's own peaks (from `gen_ancestry_proof`). Empty for the
	/// identity.
	pub connecting_nodes: Vec<(u64, Hash)>,
}

impl MMRExtensionProof {
	/// The identity extension: no connecting nodes, yielding a frontier's own root unchanged (the
	/// common caught-up case where the endpoint already *is* the current stream root).
	pub fn identity() -> Self {
		Self { new_mmr_size: 0, connecting_nodes: Vec::new() }
	}

	/// Whether this is an identity (empty) extension.
	pub fn is_identity(&self) -> bool {
		self.connecting_nodes.is_empty()
	}

	/// Extend `from` and **return** the new root, computed from `from`'s peaks + the connecting
	/// nodes; `None` if the proof is not well-formed for that placement. The identity extension
	/// yields `from`'s own root. `from` is the verifier's own state — trusted, not re-checked.
	pub fn verify(&self, from: &MmrFrontier) -> Option<MmrRoot> {
		let from_root = from.root()?;
		if self.is_identity() {
			return Some(from_root);
		}
		// Defense-in-depth: bound an untrusted proof's connecting nodes before doing O(n) work in
		// `calculate_root`. A valid ancestry proof is far below this ceiling.
		if self.connecting_nodes.len() > MAX_EXTENSION_CONNECTING_NODES {
			return None;
		}
		// Place `from`'s peaks at their canonical positions in the *previous* (pre-extension) MMR.
		let prev_positions = get_peaks(from.mmr_size());
		if prev_positions.len() != from.peaks.len() {
			return None;
		}
		let nodes: Vec<(u64, Hash)> =
			prev_positions.into_iter().zip(from.peaks.iter().copied()).collect();
		// Reconstruct the extended root from those peaks + the connecting nodes (`calculate_root`
		// merges them under `SpecMerge`, matching the sender's MMR).
		let proof = NodeMerkleProof::<Hash, SpecMerge<SpecHasher>>::new(
			self.new_mmr_size,
			self.connecting_nodes.clone(),
		);
		proof.calculate_root(nodes).ok().map(MmrRoot)
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
		let current_root = current.root().ok_or(LiftError::BrokenChain)?;
		if next.start != current_root {
			// A gap must be a proven forward extension of where we ended.
			let proof = gaps.next().ok_or(LiftError::MissingAdvance)?;
			let extended = proof.verify(&current).ok_or(LiftError::BrokenChain)?;
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

/// Build the single requires entry for one source: stitch each stream's intervals into an endpoint,
/// extend it to the stream's current root, walk the tree to the `StreamsRoot`, and require *all* of
/// the source's streams to lift to the **same** root (one entry per source). `streams` iterates in
/// `StreamId` order; `lifts` matches it positionally.
pub fn build_requires_entry(
	source: ParaId,
	streams: &[(StreamId, Vec<Interval>)],
	lifts: &[RequiresLift],
) -> Result<(ParaId, StreamsRoot), LiftError> {
	if streams.len() != lifts.len() {
		return Err(LiftError::LiftCountMismatch);
	}
	let mut entry: Option<StreamsRoot> = None;
	for ((stream, intervals), lift) in streams.iter().zip(lifts) {
		let endpoint = stitch(intervals, &lift.advances)?;
		// The endpoint is contained in the stream's current root (computed, not declared)...
		let current = lift.extension.verify(&endpoint).ok_or(LiftError::BadExtension)?;
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
	entry.map(|root| (source, root)).ok_or(LiftError::EmptyRecord)
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
			build_requires_entry(*source, &streams, source_lifts)
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
	use polkadot_primitives::{v9::MAX_SOURCES_PER_BLOCK, MAX_POV_SIZE};
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
		frontier_at(all, k).root().unwrap()
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
			new_mmr_size: ap.prev_peaks_proof.mmr_size(),
			connecting_nodes: ap.prev_peaks_proof.proof_items().to_vec(),
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
	/// identically to real ones. Each node is `(u64 position, Hash)` = 40 B — 8 B more than the
	/// design's hash-only (32 B) count.
	fn synthetic_extension(nodes: usize) -> MMRExtensionProof {
		MMRExtensionProof {
			new_mmr_size: u64::MAX / 2, // a day-scale size; placement is irrelevant to encoded size
			connecting_nodes: (0..nodes as u64).map(|i| (i, H256::repeat_byte(i as u8))).collect(),
		}
	}

	#[test]
	fn extension_computes_root_and_rejects_tampering() {
		let all = leaves(5);
		let ext = extension(&all, 2, 5);
		// Computes root@5 from frontier@2 + O(log n) connecting nodes (never a declared root).
		assert_eq!(ext.verify(&frontier_at(&all, 2)), Some(root_at(&all, 5)));
		// Wrong starting frontier → does not reconstruct the extended root.
		assert_ne!(ext.verify(&frontier_at(&all, 3)), Some(root_at(&all, 5)));
		// Tampered connecting node → a different computed root.
		let mut bad = ext.clone();
		bad.connecting_nodes[0].1 = H256::repeat_byte(0xff);
		assert_ne!(bad.verify(&frontier_at(&all, 2)), Some(root_at(&all, 5)));
		// Identity extension yields the frontier's own root.
		let id = MMRExtensionProof::identity();
		assert_eq!(id.verify(&frontier_at(&all, 3)), Some(root_at(&all, 3)));
	}

	#[test]
	fn over_long_extension_proof_is_rejected() {
		let all = leaves(5);
		let mut ext = extension(&all, 2, 5);
		// Pad the connecting nodes beyond the defense-in-depth ceiling: must reject, not do the
		// work.
		ext.connecting_nodes = vec![(0u64, H256::zero()); MAX_EXTENSION_CONNECTING_NODES + 1];
		assert_eq!(ext.verify(&frontier_at(&all, 2)), None);
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
		let entry =
			build_requires_entry(ParaId::from(1000), &[(stream, vec![interval])], &[lift]).unwrap();
		assert_eq!(entry, (ParaId::from(1000), expected));
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
		let source = ParaId::from(1000);
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
			build_requires_entry(source, &streams, &[mk(proof_a), mk(proof_b)]),
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
		// NB: each connecting node is `(u64 position, Hash)` = 40 B, vs the design's hash-only 32 B
		// count — so measured `encoded` runs ~25% above a "N hashes × 32 B" estimate.
		println!("\n[extension]  MMRExtensionProof to tip N={N}, endpoint `behind` leaves back");
		println!("     behind | nodes | encoded (40 B/node)");
		let mut sample_ext = MMRExtensionProof::identity();
		for behind in [1usize, 16, 256, 4_096, N / 2] {
			let k = N - behind;
			let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
			let ext = MMRExtensionProof {
				new_mmr_size: ap.prev_peaks_proof.mmr_size(),
				connecting_nodes: ap.prev_peaks_proof.proof_items().to_vec(),
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
		// A full-source candidate (MAX_SOURCES_PER_BLOCK sources) with a conservative *measured*
		// lift (deepest 100k-message extension + a 1024-stream tree_proof): what fraction of the
		// budget do real proofs at representative scale consume? Guards the typical footprint at
		// <10%.
		const STREAMS_PER_SOURCE: usize = 4;
		let empirical_lift = RequiresLift {
			advances: Vec::new(),
			extension: sample_ext, // deepest 100k-message extension measured above
			tree_proof: tree_proof(1024),
		};
		let empirical_per_lift = empirical_lift.encoded_size();
		let full_candidate =
			MAX_SOURCES_PER_BLOCK as usize * STREAMS_PER_SOURCE * empirical_per_lift;
		println!(
			"\n[empirical share]  {} sources × {STREAMS_PER_SOURCE} streams × {empirical_per_lift} B \
			 = {full_candidate} B ({:.2}% of MAX_POV_SIZE = {} MiB)",
			MAX_SOURCES_PER_BLOCK,
			100.0 * full_candidate as f64 / budget as f64,
			budget / (1024 * 1024),
		);
		assert!(
			full_candidate < budget / 10,
			"empirical full-source candidate {full_candidate} B exceeds 10% of MAX_POV_SIZE",
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
			 (source cap = {MAX_SOURCES_PER_BLOCK})",
			budget / (1024 * 1024),
		);
		// A candidate names ≤ MAX_SOURCES_PER_BLOCK sources; even at day-scale the PoV must hold at
		// least that many touched streams, or authoring couldn't fill a block. (Ample margin here.)
		assert!(
			max_streams >= MAX_SOURCES_PER_BLOCK as usize,
			"day-scale worst case leaves room for only {max_streams} streams (< {MAX_SOURCES_PER_BLOCK})",
		);
		println!("===== end =====\n");
	}
}

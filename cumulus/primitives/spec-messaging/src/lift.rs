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

//! Requires-lift and consumption-record primitives.
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

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::{v9::MAX_COMMITMENT_ENTRIES, RequiresSet};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

use crate::{
	mmr::{MMRExtensionProof, MmrFrontier, MmrRoot},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
};

/// Per stream and block: consumption entered the block at `start` and left it at `end`.
///
/// The candidate's lift binds only the last state to a committed root; the intervals extend that
/// guarantee back over the bundle. Each block must start where the previous ended, or prove the
/// jump moved forward ([`stitch`]).
///
/// - Channels: `start` is the frontier root before the block's incoming messages, `end` the
/// frontier after. `start == previous end` holds by construction, so the chain check is free.
/// - Register and event reads: `start` is the root of the frontier before the read's payloads, and
/// `end` is the frontier after appending them. Contexts can skip leaves between blocks, so a skip
/// is a gap and must be proven forward.
///
/// `end` is a full frontier because the next gap check, or the lift, extends from it.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
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
	pub tree_proof: StreamProof,
}

/// The candidate's lifts, grouped per source: the PoV transport form. `Decode` enforces strictly
/// increasing `ParaId`s and bounds the source count at [`MAX_COMMITMENT_ENTRIES`], so more sources
/// than a `RequiresSet` can hold are rejected before any lift is verified.
/// [`TryFrom<BTreeMap>`](Self::try_from) is canonical by construction. [`build_requires`] zips
/// these against the record's sources by position.
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

	pub fn len(&self) -> usize {
		self.0.len()
	}

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
	let mut current = &first.end;
	for next in rest {
		let current_root = current.root();
		if next.start != current_root {
			// A gap must be a proven forward extension of where we ended.
			let proof = gaps.next().ok_or(LiftError::MissingAdvance)?;
			let extended = proof.calculate_root(current).map_err(|_| LiftError::BrokenChain)?;
			if extended != next.start {
				return Err(LiftError::BrokenChain);
			}
		}
		current = &next.end;
	}
	if gaps.next().is_some() {
		return Err(LiftError::StrayAdvance);
	}
	Ok(current.clone())
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
		let current =
			lift.extension.calculate_root(&endpoint).map_err(|_| LiftError::BadExtension)?;
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
	use crate::{
		mmr::{ProofError, MAX_MMR_LEAF_COUNT},
		streams_root::{gen_stream_proof, streams_root, TreeStep},
		test_utils::{ch, extension, frontier_at, leaves},
	};
	use polkadot_core_primitives::Hash;
	use polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES;
	use sp_core::H256;

	/// Root over the first `k` leaves.
	fn root_at(all: &[Hash], k: usize) -> MmrRoot {
		frontier_at(all, k).root()
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
	fn stitch_rejects_a_backward_step() {
		// A later block read an older context. Extensions only go forward, so nothing can bridge
		// it; the pallet does not record an ignored stale read for this reason.
		let all = leaves(5);
		let i1 = Interval { start: MmrRoot(H256::zero()), end: frontier_at(&all, 4) };
		let i2 = Interval { start: root_at(&all, 2), end: frontier_at(&all, 2) };
		assert_eq!(stitch(&[i1.clone(), i2.clone()], &[]), Err(LiftError::MissingAdvance));
		assert_eq!(stitch(&[i1, i2], &[extension(&all, 4, 5)]), Err(LiftError::BrokenChain));
	}

	#[test]
	fn nothing_recorded_lifts_nothing() {
		assert_eq!(stitch(&[], &[]), Err(LiftError::EmptyRecord));
		assert_eq!(build_requires_entry(&SourceStreams::new(), &[]), Err(LiftError::EmptyRecord));
	}

	#[test]
	fn an_empty_source_entry_fails_closed() {
		// The pallet records a source only with a stream. An entry with none has nothing to lift,
		// so the candidate is invalid rather than silently requiring nothing from that source.
		let source = ParaId::from(1000);
		let record = ConsumptionRecord { entries: BTreeMap::from([(source, BTreeMap::new())]) };
		let lifts = LiftsBySource::try_from(BTreeMap::from([(source, Vec::new())])).unwrap();
		assert_eq!(build_requires(&[record], &lifts), Err(LiftError::EmptyRecord));
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
		// … out of order and duplicate sources do not …
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
		// The block ended at @2 and the committed root is at @5: a real extension bridges them.
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
		// Two streams at different frontiers both lift to the source's one `StreamsRoot`.
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
		// A bundle whose second block jumped @2 → @4: an advance bridges the gap, then the
		// extension reaches the committed @5.
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
		// leaf_count 1 <= the endpoint's 3 leaves → NotForward inside `calculate_root` →
		// BadExtension.
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof { leaf_count: 1, connecting_nodes: BoundedVec::new() },
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
		// A hand-built step with an out-of-range split bit (a one-stream tree has no real steps).
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

	#[test]
	fn encoded_len_for_matches_generated_proofs_and_rejects_other_node_counts() {
		// Out-of-range inputs return None without panicking.
		assert_eq!(MMRExtensionProof::encoded_len_for(0, 1), None);
		assert_eq!(MMRExtensionProof::encoded_len_for(5, 4), None);
		assert_eq!(MMRExtensionProof::encoded_len_for(1, MAX_MMR_LEAF_COUNT + 1), None);
		assert_eq!(
			MMRExtensionProof::encoded_len_for(MAX_MMR_LEAF_COUNT + 1, MAX_MMR_LEAF_COUNT + 1),
			None,
		);

		// Equal counts need no skip proof (0 bytes).
		for n in [1, 2, 64, 1 << 20, MAX_MMR_LEAF_COUNT] {
			assert_eq!(MMRExtensionProof::encoded_len_for(n, n), Some(0));
		}

		// Exhaustively check every pair up to 64 leaves against real generated proofs, and verify
		// that padded or truncated connecting_nodes are rejected by `verify`.
		let all = leaves(64);
		for n2 in 2..=64usize {
			for n1 in 1..n2 {
				let from = frontier_at(&all, n1);
				let proof = extension(&all, n1, n2);
				assert_eq!(
					MMRExtensionProof::encoded_len_for(n1 as u64, n2 as u64),
					Some(proof.encoded_size()),
					"encoded_len_for mismatch at {n1} -> {n2}"
				);
				assert!(proof.encoded_size() <= MMRExtensionProof::max_encoded_len());

				// Truncated proof must fail verify.
				let mut truncated = proof.clone();
				truncated.connecting_nodes.pop();
				assert_eq!(truncated.calculate_root(&from), Err(ProofError::InvalidProof));

				// Padded proof must fail verify.
				let mut padded = proof;
				padded.connecting_nodes.try_push(H256::zero()).expect("<= 64 leaves < 48; qed");
				assert_eq!(padded.calculate_root(&from), Err(ProofError::InvalidProof));
			}
		}

		// Power-of-two and worst-case edge pairs up to MAX_MMR_LEAF_COUNT.
		let edge_counts = [
			1,
			2,
			3,
			(1 << 16) - 1,
			1 << 16,
			(1 << 32) - 1,
			1 << 32,
			(1 << 47) - 1,
			1 << 47,
			MAX_MMR_LEAF_COUNT - 1,
			MAX_MMR_LEAF_COUNT,
		];
		for (i, &n1) in edge_counts.iter().enumerate() {
			for &n2 in &edge_counts[i + 1..] {
				let len = MMRExtensionProof::encoded_len_for(n1, n2).unwrap();
				assert!(
					len <= MMRExtensionProof::max_encoded_len(),
					"{n1} -> {n2}: {len} > {}",
					MMRExtensionProof::max_encoded_len()
				);
			}
		}
		// Worst-case extension (1 -> 2^48 - 1) takes 48 hashes: 7 + 1 + 48*32 = 1,544 bytes
		// (`Compact(2^48 - 1)` is 7 bytes; `MaxEncodedLen` reserves 8 bytes for `#[codec(compact)]
		// u64` = 1,545 bytes).
		assert_eq!(MMRExtensionProof::encoded_len_for(1, MAX_MMR_LEAF_COUNT - 1), Some(1_544));
		assert_eq!(MMRExtensionProof::max_encoded_len(), 1_545);
		assert_eq!(StreamProof::max_encoded_len(), 2_114);
	}

	/// The PoV reservation for a lossy read is `encoded_len_for(previous end, read start)`. Over
	/// random read sequences (adjacent reads, skips, cut reads), the reserved bytes must equal the
	/// encoded advances that `stitch` accepts.
	#[test]
	fn reserved_skip_bytes_equal_the_advances_stitch_accepts() {
		const N: usize = 256;
		let all = leaves(N as u64);
		let mut seed = 0x9E37_79B9_7F4A_7C15u64;
		let mut next = |bound: usize| {
			seed ^= seed << 13;
			seed ^= seed >> 7;
			seed ^= seed << 17;
			seed as usize % bound
		};

		for _ in 0..300 {
			let first = next(N / 2);
			let mut prev_end = first + 1 + next(8);
			let mut intervals =
				vec![Interval { start: root_at(&all, first), end: frontier_at(&all, prev_end) }];
			let (mut reserved, mut advances) = (0, Vec::new());
			loop {
				// A third of reads continue where the last one ended; the rest skip ahead.
				let skip_bound = 1 << next(7);
				let start = prev_end + if next(3) == 0 { 0 } else { 1 + next(skip_bound) };
				let end = start + 1 + next(8);
				if end > N {
					break;
				}
				reserved += MMRExtensionProof::encoded_len_for(prev_end as u64, start as u64)
					.expect("1 <= prev_end <= start <= N; qed");
				if start > prev_end {
					advances.push(extension(&all, prev_end, start));
				}
				intervals
					.push(Interval { start: root_at(&all, start), end: frontier_at(&all, end) });
				prev_end = end;
			}

			assert_eq!(stitch(&intervals, &advances), Ok(frontier_at(&all, prev_end)));
			assert_eq!(reserved, advances.iter().map(Encode::encoded_size).sum::<usize>());
		}
	}

	#[test]
	fn extension_proof_frozen_layout() {
		assert_eq!(MMRExtensionProof::identity().encode(), vec![0x00, 0x00]);

		let proof = MMRExtensionProof {
			leaf_count: 1 << 47,
			connecting_nodes: BoundedVec::truncate_from(vec![
				H256::repeat_byte(0x01),
				H256::repeat_byte(0x02),
			]),
		};
		// Compact `2^47` takes the big-integer mode: a 0x0B length byte (6 bytes), then LE.
		let mut expected = vec![0x0B, 0, 0, 0, 0, 0, 0x80, 0x08];
		expected.extend([0x01; 32]);
		expected.extend([0x02; 32]);
		assert_eq!(proof.encode(), expected);
		assert_eq!(MMRExtensionProof::decode(&mut &expected[..]).unwrap(), proof);
	}
}

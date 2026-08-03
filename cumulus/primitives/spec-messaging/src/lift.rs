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

//! Requires lifts: the POV-carried proofs binding a block's recorded
//! consumption to currently committed `StreamsRoot`s.
//!
//! A lift is a pure function of *public* data — the source's streams and its
//! committed roots. It carries no signature and nothing secret, so anyone
//! can generate it and assemble a valid candidate around the unaltered
//! block; every (re)submission regenerates the lifts against the
//! then-current provides, so a block never goes stale.

use alloc::{collections::BTreeMap, vec::Vec};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::{RequiresSet, StreamsRoot};

use crate::{
	mmr::{MMRExtensionProof, MmrError, MmrFrontier},
	record::{ConsumptionRecord, Interval},
	stream_id::StreamId,
	tree::{TreeError, TreeInclusionProof},
};

/// One lift, carried in the POV (never in the block or commitments), for
/// one stream of the consumption record.
///
/// Matched positionally to the record's streams within each source — the
/// record supplies the [`crate::StreamId`], and a mispaired lift cannot
/// verify: the tree walk binds the record's key, so landing on a committed
/// root means being a valid lift for exactly that stream.
///
/// On the hot path (single block, caught up) `advances` and `extension` are
/// empty and the lift is a bare tree proof, ~300 B per stream.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct RequiresLift {
	/// One proof per gap in the stream's interval chain, in gap order (see
	/// the wrapper's stitching); empty for prefix streams and
	/// single-context reads.
	pub advances: Vec<MMRExtensionProof>,
	/// Extends the chain's endpoint to the stream's current state;
	/// verification *yields* the current stream root. Empty when the
	/// endpoint already is the target root's entry.
	pub extension: MMRExtensionProof,
	/// Walked from the computed stream root, *yields* the `StreamsRoot` the
	/// requires entry becomes — validated by the relay chain's window
	/// match.
	pub tree_proof: TreeInclusionProof,
}

/// Canonical transport of a candidate's lifts: per source, positionally
/// matched to the consumption record's streams of that source (which are in
/// [`crate::StreamId`]'s canonical order).
///
/// Manual `Decode` REJECTS non-strictly-increasing `ParaId`s — same
/// canonicality discipline as `RequiresSet`: the bytes come from an
/// untrusted submitter and must have exactly one valid form.
#[derive(
	Clone,
	codec::Encode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct LiftsBySource(Vec<(ParaId, Vec<RequiresLift>)>);

impl codec::Decode for LiftsBySource {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner = Vec::<(ParaId, Vec<RequiresLift>)>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"LiftsBySource entries must be sorted by strictly increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

impl LiftsBySource {
	/// Builds from an arbitrary (possibly unordered) iterator, sorting by
	/// source and rejecting duplicates.
	pub fn try_from_iter(
		it: impl IntoIterator<Item = (ParaId, Vec<RequiresLift>)>,
	) -> Result<Self, LiftsError> {
		let mut entries: Vec<(ParaId, Vec<RequiresLift>)> = it.into_iter().collect();
		entries.sort_by_key(|(source, _)| *source);

		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(LiftsError::DuplicateSource);
		}

		Ok(Self(entries))
	}

	/// The lifts of `source`, or `None` if absent.
	pub fn get(&self, source: ParaId) -> Option<&[RequiresLift]> {
		self.0
			.binary_search_by_key(&source, |(id, _)| *id)
			.ok()
			.map(|idx| self.0[idx].1.as_slice())
	}

	/// Iterates all `(source, lifts)` entries in sorted order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, Vec<RequiresLift>)> {
		self.0.iter()
	}

	/// Number of sources.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// `true` if no source carries lifts.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}
}

/// Errors constructing a [`LiftsBySource`].
#[derive(Debug, PartialEq, Eq)]
pub enum LiftsError {
	/// The same source appears more than once.
	DuplicateSource,
}

/// Errors stitching a bundle's consumption record and lifting it to
/// committed roots ([`build_requires`]). Any of these invalidates the
/// candidate.
///
/// The verifier's rule is mechanical — one lift per recorded stream,
/// verified, roots converging per source. It cannot judge *staleness*: a
/// lift landing on a root the relay chain no longer stores verifies fine
/// here and is rejected by the window match at inclusion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiftError {
	/// The sources named by the record and by the lifts differ.
	SourceMismatch,
	/// A source's lift count differs from its recorded stream count — the
	/// lifts are matched positionally, one per stream.
	LiftCountMismatch,
	/// Consecutive intervals of one stream neither chain by equality nor by
	/// a forward advance proof landing exactly on the next start.
	BrokenChain,
	/// A lift carried more advance proofs than its stream's chain has gaps
	/// — the lift bytes have exactly one valid form.
	UnusedAdvances,
	/// The lift's extension proof does not extend the chain's endpoint.
	InvalidExtension(MmrError),
	/// The lift's tree proof is not a valid path.
	InvalidTreeProof(TreeError),
	/// Two streams of one source lifted to different [`StreamsRoot`]s.
	DivergentRoots,
	/// The synthesized requires set does not construct (more sources than
	/// `MAX_COMMITMENT_ENTRIES`; the STF's per-block caps on touched
	/// streams are what keep this unreachable for honest blocks).
	TooManySources,
}

/// Stitches one stream's per-block intervals (bundle order) into the
/// chain's endpoint.
///
/// Each block must start where the previous one ended — equality, zero
/// proofs; channel streams chain like this by statehood (consumption is a
/// stored frontier every block continues from) — or one advance proof, in
/// gap order, must show the jump is a forward extension. Gaps only occur
/// for register/event read contexts, which pick their context freely; the
/// chain is what stops a block from acting on reads against a fabricated
/// context and hiding behind a later, genuine one.
///
/// On the hot path (single block) this degenerates to the sole interval's
/// `end` without touching a single proof.
pub fn stitch(
	intervals: &[Interval],
	advances: &[MMRExtensionProof],
) -> Result<MmrFrontier, LiftError> {
	let (first, rest) = intervals.split_first().ok_or(LiftError::BrokenChain)?;
	let mut advances = advances.iter();
	let mut end = first.end.clone();
	for interval in rest {
		if interval.start != end.root() {
			let advance = advances.next().ok_or(LiftError::BrokenChain)?;
			// A backward "advance" (`NotForward`) and the empty proof (its
			// identity is `end.root()`, which mismatches here by
			// construction) both fail — the chain only ever moves forward.
			let advanced = advance.verify(&end).map_err(|_| LiftError::BrokenChain)?;
			if advanced != interval.start {
				return Err(LiftError::BrokenChain);
			}
		}
		end = interval.end.clone();
	}
	if advances.next().is_some() {
		return Err(LiftError::UnusedAdvances);
	}
	Ok(end)
}

/// Builds one source's requires entry from its recorded streams (in
/// [`StreamId`] canonical order) and their positionally matched lifts.
///
/// Per stream: [`stitch`] the intervals; the lift's `extension` extends the
/// chain's endpoint to the stream's current root (*yielded*, not declared);
/// its `tree_proof`, walked from that root and binding the record's stream
/// key, yields the [`StreamsRoot`]. All streams of the source must land on
/// the same root.
pub fn build_requires_entry(
	streams: &[(StreamId, Vec<Interval>)],
	lifts: &[RequiresLift],
) -> Result<StreamsRoot, LiftError> {
	if streams.len() != lifts.len() {
		return Err(LiftError::LiftCountMismatch);
	}
	let mut entry_root = None;
	for ((stream, intervals), lift) in streams.iter().zip(lifts) {
		let end = stitch(intervals, &lift.advances)?;
		let current = lift.extension.verify(&end).map_err(LiftError::InvalidExtension)?;
		let root = lift.tree_proof.verify(stream, &current).map_err(LiftError::InvalidTreeProof)?;
		match entry_root {
			None => entry_root = Some(root),
			Some(existing) if existing == root => (),
			Some(_) => return Err(LiftError::DivergentRoots),
		}
	}
	// A source without recorded streams has no entry to build; `build_requires`
	// never constructs one.
	entry_root.ok_or(LiftError::LiftCountMismatch)
}

/// Merges the bundle's per-block consumption records (bundle order),
/// verifies one POV-carried lift per recorded stream and synthesizes the
/// candidate's `Requires` set — one entry per source.
///
/// `Ok(None)` when the bundle consumed nothing; the lifts must then be
/// empty too. Sources in record and lifts must match exactly — a missing or
/// stray lift fails. One code path for steady state, partial consumption,
/// resubmission and bundles.
pub fn build_requires(
	records: &[ConsumptionRecord],
	lifts: &LiftsBySource,
) -> Result<Option<RequiresSet>, LiftError> {
	// Merge in bundle order: per source, per stream, the interval chain.
	let mut merged: BTreeMap<ParaId, BTreeMap<StreamId, Vec<Interval>>> = BTreeMap::new();
	for record in records {
		for (source, streams) in &record.entries {
			let source_entry = merged.entry(*source).or_default();
			for (stream, interval) in streams {
				source_entry.entry(*stream).or_default().push(interval.clone());
			}
		}
	}

	if merged.len() != lifts.len() {
		return Err(LiftError::SourceMismatch);
	}
	if merged.is_empty() {
		return Ok(None);
	}

	let mut entries = Vec::with_capacity(merged.len());
	for (source, streams) in &merged {
		let source_lifts = lifts.get(*source).ok_or(LiftError::SourceMismatch)?;
		let streams: Vec<(StreamId, Vec<Interval>)> =
			streams.iter().map(|(id, intervals)| (*id, intervals.clone())).collect();
		entries.push((*source, build_requires_entry(&streams, source_lifts)?));
	}

	Ok(Some(RequiresSet::try_from_iter(entries).map_err(|_| LiftError::TooManySources)?))
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn lift() -> RequiresLift {
		RequiresLift::default()
	}

	#[test]
	fn try_from_iter_sorts_and_rejects_duplicates() {
		let lifts = LiftsBySource::try_from_iter([
			(ParaId::from(2), alloc::vec![lift()]),
			(ParaId::from(1), alloc::vec![lift(), lift()]),
		])
		.unwrap();
		let sources: Vec<ParaId> = lifts.iter().map(|(id, _)| *id).collect();
		assert_eq!(sources, alloc::vec![ParaId::from(1), ParaId::from(2)]);
		assert_eq!(lifts.get(ParaId::from(1)).unwrap().len(), 2);
		assert_eq!(lifts.get(ParaId::from(3)), None);

		assert_eq!(
			LiftsBySource::try_from_iter([
				(ParaId::from(1), alloc::vec![]),
				(ParaId::from(1), alloc::vec![]),
			]),
			Err(LiftsError::DuplicateSource)
		);
	}

	#[test]
	fn decode_rejects_unsorted_sources() {
		let ok = LiftsBySource::try_from_iter([
			(ParaId::from(1), alloc::vec![lift()]),
			(ParaId::from(2), alloc::vec![]),
		])
		.unwrap();
		let encoded = ok.encode();
		assert_eq!(LiftsBySource::decode(&mut &encoded[..]).unwrap(), ok);

		let bad: Vec<(ParaId, Vec<RequiresLift>)> =
			alloc::vec![(ParaId::from(2), alloc::vec![]), (ParaId::from(1), alloc::vec![])];
		assert!(LiftsBySource::decode(&mut &bad.encode()[..]).is_err());

		let dup: Vec<(ParaId, Vec<RequiresLift>)> =
			alloc::vec![(ParaId::from(1), alloc::vec![]), (ParaId::from(1), alloc::vec![])];
		assert!(LiftsBySource::decode(&mut &dup.encode()[..]).is_err());
	}
}

#[cfg(test)]
mod verification_tests {
	use super::*;
	use crate::test_utils::{record, SourceFixture, StreamFixture};

	const SOURCE: u32 = 2000;

	fn para(id: u32) -> ParaId {
		ParaId::from(id)
	}

	fn channel(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}

	fn ack(recipient: u32) -> StreamId {
		StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
	}

	fn lifts_for(source: ParaId, lifts: Vec<RequiresLift>) -> LiftsBySource {
		LiftsBySource::try_from_iter([(source, lifts)]).unwrap()
	}

	#[test]
	fn single_block_caught_up_is_a_bare_tree_proof() {
		// Hot path: one block, caught up on one channel stream. The lift
		// degenerates to a bare tree proof and the synthesized entry is the
		// sender's committed root.
		let id = channel(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 10)]);
		let records = [record([(para(SOURCE), id, source.stream(&id).interval(4, 10))])];

		let lift = source.lift(&id, 10, &[]);
		assert!(lift.advances.is_empty() && lift.extension.is_empty());

		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![lift]))
			.unwrap()
			.unwrap();
		assert_eq!(set.len(), 1);
		assert_eq!(set.get(para(SOURCE)), Some(&source.streams_root()));
	}

	#[test]
	fn missing_stray_and_miscounted_lifts_fail() {
		let id = channel(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 10)]);
		let records = [record([(para(SOURCE), id, source.stream(&id).interval(0, 10))])];
		let lift = source.lift(&id, 10, &[]);

		// Missing lift: the recorded source has no lifts at all.
		assert_eq!(
			build_requires(&records, &LiftsBySource::default()),
			Err(LiftError::SourceMismatch)
		);

		// Stray lift: an extra source not in the record.
		let stray = LiftsBySource::try_from_iter([
			(para(SOURCE), alloc::vec![lift.clone()]),
			(para(SOURCE + 1), alloc::vec![lift.clone()]),
		])
		.unwrap();
		assert_eq!(build_requires(&records, &stray), Err(LiftError::SourceMismatch));

		// Right source count, wrong source.
		let wrong_source = lifts_for(para(SOURCE + 1), alloc::vec![lift.clone()]);
		assert_eq!(build_requires(&records, &wrong_source), Err(LiftError::SourceMismatch));

		// One recorded stream, two lifts — positional matching is exact.
		let doubled = lifts_for(para(SOURCE), alloc::vec![lift.clone(), lift]);
		assert_eq!(build_requires(&records, &doubled), Err(LiftError::LiftCountMismatch));

		// One recorded stream, zero lifts.
		assert_eq!(
			build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![])),
			Err(LiftError::LiftCountMismatch)
		);
	}

	#[test]
	fn mispaired_lifts_across_streams_diverge() {
		// Two streams of one source, lifts swapped: each tree walk binds the
		// RECORD's stream key, so a mispaired (individually valid) proof
		// lands on a different root — the source's roots diverge.
		let data = channel(2001);
		let register = ack(2001);
		let source = SourceFixture::new(
			para(SOURCE),
			alloc::vec![StreamFixture::new(data, 8), StreamFixture::new(register, 5)],
		);
		let records = [record([
			(para(SOURCE), data, source.stream(&data).interval(0, 8)),
			(para(SOURCE), register, source.stream(&register).interval(0, 5)),
		])];

		// Correctly paired: both streams land on the committed root.
		let paired = lifts_for(
			para(SOURCE),
			alloc::vec![source.lift(&data, 8, &[]), source.lift(&register, 5, &[])],
		);
		let set = build_requires(&records, &paired).unwrap().unwrap();
		assert_eq!(set.get(para(SOURCE)), Some(&source.streams_root()));

		// Swapped: both lifts are valid for *their* stream, but paired to
		// the wrong record slot they cannot converge.
		//
		// (The swap only makes sense here because both streams are caught up
		// and gap-free — otherwise the extension already fails.)
		let swapped = lifts_for(
			para(SOURCE),
			alloc::vec![source.lift(&register, 5, &[]), source.lift(&data, 8, &[])],
		);
		assert_eq!(build_requires(&records, &swapped), Err(LiftError::DivergentRoots));
	}

	#[test]
	fn mispaired_single_stream_lift_yields_uncommitted_root() {
		// With a single recorded stream there is nothing to diverge from:
		// the tree walk with the wrong slot's proof still *yields* a root —
		// but never the committed one, so the relay's window match rejects
		// the candidate. (This is why yielding, not declaring, matters.)
		let data = channel(2001);
		let register = ack(2001);
		let source = SourceFixture::new(
			para(SOURCE),
			// Same length so the (empty) extension still verifies.
			alloc::vec![StreamFixture::new(data, 8), StreamFixture::new(register, 8)],
		);
		let records = [record([(para(SOURCE), data, source.stream(&data).interval(0, 8))])];

		let mispaired = RequiresLift {
			advances: alloc::vec![],
			extension: MMRExtensionProof::empty(),
			tree_proof: source.tree_proof(&register),
		};
		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![mispaired]))
			.unwrap()
			.unwrap();
		assert_ne!(set.get(para(SOURCE)), Some(&source.streams_root()));
	}

	#[test]
	fn partial_consumption_lifts_through_extension() {
		// The block stopped mid-backlog (6 of 10 consumed): the lift's
		// non-empty extension proof yields the stream's CURRENT root, and
		// the entry still lands on the committed StreamsRoot.
		let id = channel(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 10)]);
		let records = [record([(para(SOURCE), id, source.stream(&id).interval(0, 6))])];

		let lift = source.lift(&id, 6, &[]);
		assert!(!lift.extension.is_empty());
		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![lift]))
			.unwrap()
			.unwrap();
		assert_eq!(set.get(para(SOURCE)), Some(&source.streams_root()));

		// A truncated extension (6 -> 8 instead of 6 -> 10) yields the root
		// at 8 messages; the tree walk from it cannot land on the committed
		// root — rejected by the relay's window match.
		let truncated = RequiresLift {
			advances: alloc::vec![],
			extension: source.stream(&id).extension_proof(6, 8),
			tree_proof: source.tree_proof(&id),
		};
		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![truncated]))
			.unwrap()
			.unwrap();
		assert_ne!(set.get(para(SOURCE)), Some(&source.streams_root()));
	}

	#[test]
	fn bundle_channel_intervals_chain_by_equality() {
		// Two blocks in a bundle; the second continues exactly where the
		// first ended — statehood equality, zero advance proofs.
		let id = channel(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 10)]);
		let stream = source.stream(&id);
		let records = [
			record([(para(SOURCE), id, stream.interval(0, 3))]),
			record([(para(SOURCE), id, stream.interval(3, 7))]),
		];

		let lift = source.lift(&id, 7, &[]);
		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![lift]))
			.unwrap()
			.unwrap();
		assert_eq!(set.get(para(SOURCE)), Some(&source.streams_root()));
	}

	#[test]
	fn read_context_gaps_need_advance_proofs() {
		// Register/event reads pick their context freely, so a bundle's
		// contexts CAN jump — but only forward, proven per gap.
		let id = ack(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 10)]);
		let stream = source.stream(&id);
		let records = [
			record([(para(SOURCE), id, stream.read_context(5))]),
			record([(para(SOURCE), id, stream.read_context(8))]),
		];

		// Gap 5 -> 8 covered by an advance proof: passes.
		let lift = source.lift(&id, 8, &[(5, 8)]);
		let set = build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![lift]))
			.unwrap()
			.unwrap();
		assert_eq!(set.get(para(SOURCE)), Some(&source.streams_root()));

		// Gap without a proof: the chain is broken.
		let unproven = source.lift(&id, 8, &[]);
		assert_eq!(
			build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![unproven])),
			Err(LiftError::BrokenChain)
		);

		// An advance landing elsewhere (5 -> 9 instead of 5 -> 8): broken.
		let overshooting = source.lift(&id, 8, &[(5, 9)]);
		assert_eq!(
			build_requires(&records, &lifts_for(para(SOURCE), alloc::vec![overshooting])),
			Err(LiftError::BrokenChain)
		);

		// Backward "advance": contexts 8 then 5 cannot chain — an extension
		// proof only ever moves forward.
		let backward_records = [
			record([(para(SOURCE), id, stream.read_context(8))]),
			record([(para(SOURCE), id, stream.read_context(5))]),
		];
		let lift = source.lift(&id, 5, &[(5, 8)]);
		assert_eq!(
			build_requires(&backward_records, &lifts_for(para(SOURCE), alloc::vec![lift])),
			Err(LiftError::BrokenChain)
		);

		// An advance proof beyond the chain's gaps: the lift has exactly
		// one valid form.
		let equality_records = [
			record([(para(SOURCE), id, stream.interval(0, 5))]),
			record([(para(SOURCE), id, stream.interval(5, 8))]),
		];
		let stray_advance = source.lift(&id, 8, &[(5, 8)]);
		assert_eq!(
			build_requires(&equality_records, &lifts_for(para(SOURCE), alloc::vec![stray_advance])),
			Err(LiftError::UnusedAdvances)
		);
	}

	#[test]
	fn divergent_roots_across_streams_fail() {
		// Two streams of one source must lift to the SAME StreamsRoot; a
		// tampered sibling in one tree proof makes its walk land elsewhere.
		let data = channel(2001);
		let register = ack(2001);
		let source = SourceFixture::new(
			para(SOURCE),
			alloc::vec![StreamFixture::new(data, 8), StreamFixture::new(register, 5)],
		);
		let records = [record([
			(para(SOURCE), data, source.stream(&data).interval(0, 8)),
			(para(SOURCE), register, source.stream(&register).interval(0, 5)),
		])];

		let mut tampered = source.lift(&register, 5, &[]);
		tampered.tree_proof.steps[0].1 = polkadot_core_primitives::Hash::repeat_byte(0xAA);
		let lifts = lifts_for(para(SOURCE), alloc::vec![source.lift(&data, 8, &[]), tampered]);
		assert_eq!(build_requires(&records, &lifts), Err(LiftError::DivergentRoots));
	}

	#[test]
	fn empty_record_synthesizes_nothing() {
		// A bundle that consumed nothing has no requires — and must not
		// carry lifts either.
		assert_eq!(build_requires(&[], &LiftsBySource::default()), Ok(None));
		assert_eq!(
			build_requires(&[ConsumptionRecord::default()], &LiftsBySource::default()),
			Ok(None),
		);

		let id = channel(2001);
		let source = SourceFixture::new(para(SOURCE), alloc::vec![StreamFixture::new(id, 3)]);
		let stray = lifts_for(para(SOURCE), alloc::vec![source.lift(&id, 3, &[])]);
		assert_eq!(build_requires(&[], &stray), Err(LiftError::SourceMismatch));
	}

	#[test]
	fn multiple_sources_build_sorted_entries() {
		// Two sources; entries come out sorted by ParaId (canonical
		// RequiresSet order), each bound to its own committed root.
		let id_a = channel(2001);
		let id_b = channel(2002);
		let source_a = SourceFixture::new(para(2107), alloc::vec![StreamFixture::new(id_a, 4)]);
		let source_b = SourceFixture::new(para(2006), alloc::vec![StreamFixture::new(id_b, 6)]);
		let records = [record([
			(source_a.para, id_a, source_a.stream(&id_a).interval(0, 4)),
			(source_b.para, id_b, source_b.stream(&id_b).interval(0, 6)),
		])];
		let lifts = LiftsBySource::try_from_iter([
			(source_a.para, alloc::vec![source_a.lift(&id_a, 4, &[])]),
			(source_b.para, alloc::vec![source_b.lift(&id_b, 6, &[])]),
		])
		.unwrap();

		let set = build_requires(&records, &lifts).unwrap().unwrap();
		let ids: Vec<ParaId> = set.iter().map(|(id, _)| *id).collect();
		assert_eq!(ids, alloc::vec![para(2006), para(2107)]);
		assert_eq!(set.get(source_a.para), Some(&source_a.streams_root()));
		assert_eq!(set.get(source_b.para), Some(&source_b.streams_root()));
	}
}

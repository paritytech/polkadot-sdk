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

//! `lift_pipeline`: `build_requires` end to end.
//!
//! Each source commits `STREAMS` streams, each a prefix of the oracle's MMR. Their history is cut
//! into per-block intervals the way the STF builds them: a base frontier (the previous block's end,
//! or a collator hint that may be forged) plus the appended leaves. The lifts start honest, then
//! take arbitrary edits.
//!
//! If `build_requires` returns a source's committed root, every interval end recorded for it must
//! be a real frontier at or below the committed count. The first interval's start isn't checked:
//! that's the STF's job, not `stitch`'s.

use crate::{Count, Edit, HashChoice, Oracle, Outcome, N};
use arbitrary::Arbitrary;
use cumulus_primitives_spec_messaging::{
	build_requires,
	streams_root::{gen_stream_proof, streams_root, TreeStep},
	ConsumptionRecord, Interval, LiftsBySource, MMRExtensionProof, MmrFrontier, RequiresLift,
	StreamId, StreamsRoot,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::H256 as Hash;
use std::collections::BTreeMap;

/// Three gives the trie a real branch and lets a record consume only some of them.
const STREAMS: usize = 3;
const SOURCES: usize = 2;
const RECIPIENT: u32 = 1;

fn stream(j: usize) -> StreamId {
	StreamId::Channel { recipient: ParaId::from(RECIPIENT), domain: 0, num: j as u16 }
}

fn source(i: usize) -> ParaId {
	ParaId::from(1000 + i as u32)
}

#[derive(Arbitrary, Debug)]
pub struct PipelineInput {
	pub sources: [SourceInput; SOURCES],
	pub edits: Vec<PipelineEdit>,
}

#[derive(Arbitrary, Debug)]
pub struct SourceInput {
	/// `None` leaves the source out. Stream `j` commits `committed[j] % N + 1` leaves.
	pub committed: Option<[u8; STREAMS]>,
	/// Interval boundaries per stream, reduced into `0..=committed`, sorted and paired up into one
	/// `(start, end)` per block.
	pub cuts: [Vec<u8>; STREAMS],
}

/// Where a block's interval starts.
#[derive(Arbitrary, Clone, Debug)]
pub enum BaseChoice {
	/// The previous block's end, or empty for the first block.
	Prev,
	/// After `k % (N + 1)` leaves.
	Real(u8),
	/// A collator hint, skipped unless `from_parts` accepts it.
	Forged { leaf_count: Count, peaks: Vec<HashChoice> },
}

/// One lie. Indices wrap: `source` over the sources taking part, `stream` over its consumed
/// streams, `item` over blocks or advances.
#[derive(Arbitrary, Clone, Debug)]
pub enum PipelineEdit {
	Base {
		source: u8,
		stream: u8,
		item: u8,
		base: BaseChoice,
	},
	Appended {
		source: u8,
		stream: u8,
		item: u8,
		edit: Edit,
	},
	AdvanceNodes {
		source: u8,
		stream: u8,
		item: u8,
		edit: Edit,
	},
	AdvanceCount {
		source: u8,
		stream: u8,
		item: u8,
		count: Count,
	},
	DropAdvance {
		source: u8,
		stream: u8,
		item: u8,
	},
	/// Inserts the honest proof from `from` to `to` leaves.
	AddAdvance {
		source: u8,
		stream: u8,
		item: u8,
		from: u8,
		to: u8,
	},
	/// The extension from the endpoint to the committed count.
	ExtensionNodes {
		source: u8,
		stream: u8,
		edit: Edit,
	},
	ExtensionCount {
		source: u8,
		stream: u8,
		count: Count,
	},
	TreeSibling {
		source: u8,
		stream: u8,
		step: u8,
		hash: HashChoice,
	},
	TreeSplit {
		source: u8,
		stream: u8,
		step: u8,
		bit: u8,
	},
	/// Gives two streams each other's lifts.
	SwapLifts {
		source: u8,
		a: u8,
		b: u8,
	},
}

/// A `None` base is the previous block's end.
struct Block {
	base: Option<MmrFrontier>,
	appended: Vec<Hash>,
}

struct Consumed {
	stream: StreamId,
	committed: u64,
	blocks: Vec<Block>,
	lift: RequiresLift,
}

struct Source {
	para: ParaId,
	committed_root: StreamsRoot,
	consumed: Vec<Consumed>,
}

fn pick<T>(items: &mut [T], i: u8) -> Option<&mut T> {
	let len = items.len();
	(len > 0).then(|| &mut items[i as usize % len])
}

fn edit_step(c: &mut Consumed, step: u8, f: impl FnOnce(&mut TreeStep)) {
	let mut steps = c.lift.tree_proof.steps.clone().into_inner();
	if let Some(s) = pick(&mut steps, step) {
		f(s);
	}
	c.lift.tree_proof.steps = steps.try_into().expect("length unchanged; qed");
}

fn consumed_mut(sources: &mut [Source], s: u8, j: u8) -> Option<&mut Consumed> {
	pick(&mut pick(sources, s)?.consumed, j)
}

impl Consumed {
	/// What the STF would record. `None` if a forged base is at the leaf ceiling: the STF's
	/// `try_append` refuses the leaf, so it records nothing.
	fn intervals(&self) -> Option<Vec<Interval>> {
		let mut prev = MmrFrontier::new();
		self.blocks
			.iter()
			.map(|block| {
				let base = block.base.clone().unwrap_or_else(|| prev.clone());
				let mut end = base.clone();
				for leaf in &block.appended {
					end.try_append(*leaf).ok()?;
				}
				prev = end.clone();
				Some(Interval { start: base.root(), end })
			})
			.collect()
	}
}

impl Oracle {
	/// Everything honest, before the edits.
	fn honest_sources(&self, input: &PipelineInput) -> Vec<Source> {
		let mut sources = Vec::new();
		for (i, s) in input.sources.iter().enumerate() {
			let Some(committed) = s.committed else { continue };
			let committed: Vec<u64> = committed.iter().map(|c| *c as u64 % N as u64 + 1).collect();
			let entries: BTreeMap<StreamId, Hash> = (0..STREAMS)
				.map(|j| (stream(j), self.frontiers[committed[j] as usize].root().0))
				.collect();
			let committed_root = streams_root(&entries).expect("STREAMS > 0 entries; qed");
			let mut consumed = Vec::new();
			for (j, &c) in committed.iter().enumerate() {
				let mut cuts: Vec<u64> = s.cuts[j].iter().map(|x| *x as u64 % (c + 1)).collect();
				cuts.sort_unstable();
				let pairs: Vec<(u64, u64)> = cuts.chunks_exact(2).map(|p| (p[0], p[1])).collect();
				let Some(&(_, endpoint)) = pairs.last() else { continue };
				let mut prev_end = 0;
				let blocks = pairs
					.iter()
					.map(|&(a, b)| {
						let base = (a != prev_end).then(|| self.frontiers[a as usize].clone());
						prev_end = b;
						Block { base, appended: self.leaves[a as usize..b as usize].to_vec() }
					})
					.collect();
				let advances = pairs
					.windows(2)
					.filter(|w| w[1].0 > w[0].1)
					.map(|w| self.extension(w[0].1, w[1].0).expect("b < a' <= N; qed").clone())
					.collect();
				let extension = if endpoint == c {
					MMRExtensionProof::identity()
				} else {
					self.extension(endpoint, c).expect("endpoint < c <= N; qed").clone()
				};
				let (_, tree_proof) =
					gen_stream_proof(&entries, stream(j)).expect("stream j is committed; qed");
				consumed.push(Consumed {
					stream: stream(j),
					committed: c,
					blocks,
					lift: RequiresLift { advances, extension, tree_proof },
				});
			}
			sources.push(Source { para: source(i), committed_root, consumed });
		}
		sources
	}

	fn base(&self, choice: &BaseChoice) -> Option<Option<MmrFrontier>> {
		match choice {
			BaseChoice::Prev => Some(None),
			BaseChoice::Real(k) => Some(Some(self.frontiers[*k as usize % (N + 1)].clone())),
			BaseChoice::Forged { leaf_count, peaks } => MmrFrontier::from_parts(
				peaks.iter().map(|h| self.hash(*h)).collect(),
				leaf_count.get(),
			)
			.map(Some),
		}
	}

	fn apply_pipeline_edit(&self, sources: &mut [Source], edit: &PipelineEdit) {
		use PipelineEdit::*;
		match edit {
			Base { source, stream, item, base } => {
				if let (Some(c), Some(base)) =
					(consumed_mut(sources, *source, *stream), self.base(base))
				{
					if let Some(block) = pick(&mut c.blocks, *item) {
						block.base = base;
					}
				}
			},
			Appended { source, stream, item, edit } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					if let Some(block) = pick(&mut c.blocks, *item) {
						self.apply(&mut block.appended, &[*edit]);
					}
				}
			},
			AdvanceNodes { source, stream, item, edit } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					if let Some(a) = pick(&mut c.lift.advances, *item) {
						self.apply_bounded(&mut a.connecting_nodes, &[*edit]);
					}
				}
			},
			AdvanceCount { source, stream, item, count } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					if let Some(a) = pick(&mut c.lift.advances, *item) {
						a.leaf_count = count.get();
					}
				}
			},
			DropAdvance { source, stream, item } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					let len = c.lift.advances.len();
					if len > 0 {
						c.lift.advances.remove(*item as usize % len);
					}
				}
			},
			AddAdvance { source, stream, item, from, to } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					let wrap = |x: u8| x as u64 % (N as u64 + 1);
					let proof = self.extension(wrap(*from), wrap(*to)).cloned().unwrap_or_default();
					let at = *item as usize % (c.lift.advances.len() + 1);
					c.lift.advances.insert(at, proof);
				}
			},
			ExtensionNodes { source, stream, edit } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					self.apply_bounded(&mut c.lift.extension.connecting_nodes, &[*edit]);
				}
			},
			ExtensionCount { source, stream, count } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					c.lift.extension.leaf_count = count.get();
				}
			},
			TreeSibling { source, stream, step, hash } => {
				let hash = self.hash(*hash);
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					edit_step(c, *step, |s| s.sibling = hash);
				}
			},
			TreeSplit { source, stream, step, bit } => {
				if let Some(c) = consumed_mut(sources, *source, *stream) {
					edit_step(c, *step, |s| s.split_bit = *bit);
				}
			},
			SwapLifts { source, a, b } => {
				if let Some(s) = pick(sources, *source) {
					let len = s.consumed.len();
					if len > 1 {
						let (a, b) = (*a as usize % len, *b as usize % len);
						let lift_a = s.consumed[a].lift.clone();
						s.consumed[a].lift = core::mem::replace(&mut s.consumed[b].lift, lift_a);
					}
				}
			},
		}
	}

	/// Fuzz entry point for `build_requires`.
	pub fn check_pipeline_input(&self, input: &PipelineInput) -> Outcome {
		let mut sources = self.honest_sources(input);
		for edit in &input.edits {
			self.apply_pipeline_edit(&mut sources, edit);
		}
		let Some(intervals) = sources
			.iter()
			.map(|s| s.consumed.iter().map(Consumed::intervals).collect::<Option<Vec<_>>>())
			.collect::<Option<Vec<Vec<Vec<Interval>>>>>()
		else {
			return Outcome::Skipped;
		};

		// One record per block; a stream's `b`-th interval goes in record `b`.
		let blocks = intervals.iter().flatten().map(Vec::len).max().unwrap_or(0);
		let records: Vec<ConsumptionRecord> = (0..blocks)
			.map(|b| ConsumptionRecord {
				entries: sources
					.iter()
					.zip(&intervals)
					.map(|(s, per_stream)| {
						let streams = s
							.consumed
							.iter()
							.zip(per_stream)
							.filter_map(|(c, ivs)| ivs.get(b).map(|i| (c.stream, i.clone())))
							.collect::<BTreeMap<_, _>>();
						(s.para, streams)
					})
					.filter(|(_, streams)| !streams.is_empty())
					.collect(),
			})
			.collect();
		let lifts: BTreeMap<ParaId, Vec<RequiresLift>> = sources
			.iter()
			.filter(|s| !s.consumed.is_empty())
			.map(|s| (s.para, s.consumed.iter().map(|c| c.lift.clone()).collect()))
			.collect();
		let Ok(lifts) = LiftsBySource::try_from(lifts) else { return Outcome::Skipped };

		let set = match build_requires(&records, &lifts) {
			Ok(Some(set)) => set,
			Ok(None) => return Outcome::Unbound,
			Err(_) => return Outcome::Rejected,
		};
		let mut any_real = false;
		for (para, root) in set.iter() {
			if !sources.iter().any(|s| s.committed_root == *root) {
				continue;
			}
			any_real = true;
			let (source, per_stream) = sources
				.iter()
				.zip(&intervals)
				.find(|(s, _)| s.para == *para)
				.expect("build_requires only lifts sources it was given; qed");
			assert_eq!(
				*root, source.committed_root,
				"FORGERY: source {para:?} lifts to another source's committed root"
			);
			for (c, ivs) in source.consumed.iter().zip(per_stream) {
				for (b, interval) in ivs.iter().enumerate() {
					let count = interval.end.leaf_count();
					assert!(
						count <= c.committed && self.frontier(count) == Some(&interval.end),
						"FORGERY: source {para:?} stream {:?} lifts to its committed root (count {}) \
						 although block {b} ends at a false frontier ({count} leaves)",
						c.stream,
						c.committed
					);
				}
			}
		}
		if any_real {
			Outcome::Real
		} else {
			Outcome::Unbound
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn input(edits: Vec<PipelineEdit>) -> PipelineInput {
		PipelineInput {
			sources: [
				// Commits 41/21/11 leaves. Stream 0 reads 3..10, 15..30 and an empty 30..30,
				// ending short of 41. Stream 1 reads 0..20. Stream 2 reads nothing.
				SourceInput {
					committed: Some([40, 20, 10]),
					cuts: [vec![3, 10, 15, 30, 30, 30], vec![0, 20], vec![]],
				},
				// One block ending right at the committed count.
				SourceInput { committed: Some([5, 5, 5]), cuts: [vec![], vec![], vec![2, 6]] },
			],
			edits,
		}
	}

	#[test]
	fn honest_bundle_lifts_to_committed_roots() {
		let oracle = Oracle::new();
		assert_eq!(oracle.check_pipeline_input(&input(vec![])), Outcome::Real);
	}

	/// With only the lied-about source taking part, no lie lifts to a committed root.
	#[test]
	fn lies_never_reach_a_committed_root() {
		let oracle = Oracle::new();
		let forged = BaseChoice::Forged {
			leaf_count: Count::Near(15),
			peaks: [1, 2, 3, 4].map(|b| HashChoice::Raw([b; 32])).to_vec(),
		};
		let set0 = |slot| Edit::Set { slot, hash: HashChoice::Raw([9; 32]) };
		let lies = [
			(0, PipelineEdit::Base { source: 0, stream: 0, item: 1, base: forged }),
			(0, PipelineEdit::Base { source: 0, stream: 0, item: 1, base: BaseChoice::Prev }),
			(0, PipelineEdit::Base { source: 0, stream: 0, item: 2, base: BaseChoice::Real(31) }),
			(0, PipelineEdit::Appended { source: 0, stream: 0, item: 0, edit: set0(0) }),
			(0, PipelineEdit::DropAdvance { source: 0, stream: 0, item: 0 }),
			(0, PipelineEdit::AdvanceNodes { source: 0, stream: 0, item: 1, edit: set0(0) }),
			(
				0,
				PipelineEdit::AdvanceCount {
					source: 0,
					stream: 0,
					item: 0,
					count: Count::Near(16),
				},
			),
			(0, PipelineEdit::ExtensionNodes { source: 0, stream: 0, edit: set0(1) }),
			(0, PipelineEdit::ExtensionCount { source: 0, stream: 1, count: Count::Near(22) }),
			(0, PipelineEdit::SwapLifts { source: 0, a: 0, b: 1 }),
			(1, PipelineEdit::TreeSplit { source: 0, stream: 0, step: 0, bit: 63 }),
			(
				1,
				PipelineEdit::TreeSibling {
					source: 0,
					stream: 0,
					step: 0,
					hash: HashChoice::Palette(7),
				},
			),
		];
		for (only, lie) in lies {
			let mut bundle = input(vec![lie.clone()]);
			bundle.sources[1 - only].committed = None;
			assert_ne!(oracle.check_pipeline_input(&bundle), Outcome::Real, "{lie:?}");
		}
	}
}

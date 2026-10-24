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

//! `mmr_frontier` and `mmr_decode`: `MmrFrontier`, `bag_peaks` and `MmrRoot` from `mmr`.
//!
//! [`Model`] is an independent peaks-with-heights MMR. It hashes with plain blake2-256 and the
//! domain tags directly and bags by the documented right fold, so it shares no code with the crate.
//! Every frontier is checked against it after every step, at any leaf count up to the ceiling. When
//! the leaves are known (grown from empty or an oracle prefix), `mmr_lib` is a second reference.
//!
//! Any mismatch is a `DIVERGENCE` panic.

use crate::{Count, HashChoice, Oracle, Outcome, N};
use arbitrary::Arbitrary;
use codec::{Decode, Encode};
use cumulus_primitives_spec_messaging::{
	leaf_hash,
	mmr::{bag_peaks, empty_root, SpecMerge, MAX_MMR_LEAF_COUNT},
	FrontierFull, MmrFrontier, MmrRoot, INNER_TAG, PEAK_TAG, ROOT_TAG,
};
use mmr_lib::{
	helper::get_peaks,
	util::{MemMMR, MemStore},
	MMRStoreReadOps,
};
use sp_core::{Blake2Hasher, Hasher, H256 as Hash};

/// Above this many known leaves the `mmr_lib` rebuild is skipped.
const MMR_LIB_LEAVES: usize = 512;

/// Each append is checked against the model in full, so this bounds an input's run time.
const MAX_APPENDS: usize = 2048;

fn tagged(tag: u8, left: &Hash, right: &Hash) -> Hash {
	let mut preimage = vec![tag];
	preimage.extend_from_slice(left.as_bytes());
	preimage.extend_from_slice(right.as_bytes());
	Blake2Hasher::hash(&preimage)
}

/// `bag([p1..pn]) = H(PEAK_TAG ++ bag([p2..pn]) ++ p1)`, `bag([p]) = p`, `bag([]) = 0`.
fn model_bag(peaks: &[Hash]) -> Hash {
	match peaks.split_first() {
		None => Hash::zero(),
		Some((first, [])) => *first,
		Some((first, rest)) => tagged(PEAK_TAG, &model_bag(rest), first),
	}
}

/// `H(ROOT_TAG ++ leaf_count[8, LE] ++ bag)`.
fn model_root(leaf_count: u64, bag: Hash) -> Hash {
	let mut preimage = vec![ROOT_TAG];
	preimage.extend_from_slice(&leaf_count.to_le_bytes());
	preimage.extend_from_slice(bag.as_bytes());
	Blake2Hasher::hash(&preimage)
}

/// Peaks with their heights, highest first. Appends merge while the two lowest peaks are equal.
#[derive(Default)]
struct Model {
	leaf_count: u64,
	peaks: Vec<(u32, Hash)>,
}

impl Model {
	fn from_parts(peaks: &[Hash], leaf_count: u64) -> Self {
		let heights = (0..u64::BITS).rev().filter(|h| leaf_count >> h & 1 == 1);
		Self { leaf_count, peaks: heights.zip(peaks.iter().copied()).collect() }
	}

	fn append(&mut self, leaf: Hash) {
		self.peaks.push((0, leaf));
		while let [.., (hl, l), (hr, r)] = self.peaks[..] {
			if hl != hr {
				break;
			}
			self.peaks.truncate(self.peaks.len() - 2);
			self.peaks.push((hl + 1, tagged(INNER_TAG, &l, &r)));
		}
		self.leaf_count += 1;
	}

	fn hashes(&self) -> Vec<Hash> {
		self.peaks.iter().map(|(_, h)| *h).collect()
	}

	fn root(&self) -> Hash {
		model_root(self.leaf_count, model_bag(&self.hashes()))
	}
}

fn check_against_model(frontier: &MmrFrontier, model: &Model) {
	let count = frontier.leaf_count();
	assert!(count <= MAX_MMR_LEAF_COUNT, "DIVERGENCE: a frontier holds {count} leaves");
	assert_eq!(
		frontier.peaks().len(),
		count.count_ones() as usize,
		"DIVERGENCE: {count} leaves but {} peaks",
		frontier.peaks().len()
	);
	assert_eq!(count, model.leaf_count, "DIVERGENCE: leaf count");
	assert_eq!(frontier.peaks(), &model.hashes()[..], "DIVERGENCE: peaks at {count} leaves");
	assert_eq!(frontier.root().0, model.root(), "DIVERGENCE: root at {count} leaves");
}

fn check_against_mmr_lib(frontier: &MmrFrontier, leaves: &[Hash]) {
	let store = MemStore::<Hash>::default();
	let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
	for leaf in leaves {
		mmr.push(*leaf).expect("MemStore never fails; qed");
	}
	let n = leaves.len() as u64;
	assert_eq!(frontier.leaf_count(), n, "DIVERGENCE: leaf count against mmr_lib");
	if n == 0 {
		assert_eq!(frontier.root(), empty_root(), "DIVERGENCE: empty root against mmr_lib");
		return;
	}
	mmr.commit().expect("MemStore never fails; qed");
	let peaks: Vec<Hash> = get_peaks(mmr.mmr_size())
		.into_iter()
		.map(|pos| (&store).get_elem(pos).expect("MemStore never fails; qed").expect("a peak; qed"))
		.collect();
	assert_eq!(frontier.peaks(), &peaks[..], "DIVERGENCE: peaks against mmr_lib at {n} leaves");
	let bag = MemMMR::<Hash, SpecMerge>::new(mmr.mmr_size(), &store)
		.get_root()
		.expect("a non-empty MMR has a root; qed");
	assert_eq!(
		frontier.root(),
		MmrRoot::from_bag(n, bag),
		"DIVERGENCE: root against mmr_lib at {n} leaves"
	);
}

fn check_round_trip(frontier: &MmrFrontier) {
	let encoded = frontier.encode();
	assert_eq!(
		encoded,
		(frontier.leaf_count(), frontier.peaks().to_vec()).encode(),
		"DIVERGENCE: wire layout"
	);
	let decoded = MmrFrontier::decode(&mut &encoded[..])
		.expect("DIVERGENCE: a frontier fails to decode its own encoding");
	assert_eq!(&decoded, frontier, "DIVERGENCE: round trip");
}

/// `from_parts` accepts exactly the shapes the docs promise, and `Decode` agrees with it.
fn check_from_parts(peaks: &[Hash], leaf_count: u64) -> Option<MmrFrontier> {
	let valid = leaf_count <= MAX_MMR_LEAF_COUNT && peaks.len() == leaf_count.count_ones() as usize;
	let built = MmrFrontier::from_parts(peaks.to_vec(), leaf_count);
	assert_eq!(
		built.is_some(),
		valid,
		"DIVERGENCE: from_parts({leaf_count}, {} peaks)",
		peaks.len()
	);
	let decoded = MmrFrontier::decode(&mut &(leaf_count, peaks.to_vec()).encode()[..]).ok();
	assert_eq!(decoded, built, "DIVERGENCE: decode disagrees with from_parts");
	built
}

/// Small counts, either side of the ceiling, and around powers of two.
#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum LeafCount {
	Small(Count),
	/// `MAX_MMR_LEAF_COUNT - 128 + x`.
	AroundCeiling(u8),
	/// `2^(exp % 64) + offset`, wrapping.
	PowerOfTwo {
		exp: u8,
		offset: i8,
	},
}

impl LeafCount {
	fn get(self) -> u64 {
		match self {
			LeafCount::Small(c) => c.get(),
			LeafCount::AroundCeiling(x) => MAX_MMR_LEAF_COUNT - 128 + x as u64,
			LeafCount::PowerOfTwo { exp, offset } => {
				(1u64 << (exp % 64)).wrapping_add(offset as i64 as u64)
			},
		}
	}
}

#[derive(Arbitrary, Debug)]
pub enum Start {
	Empty,
	/// The oracle's frontier after `k % (N + 1)` leaves.
	Real(u8),
	/// Skipped unless `from_parts` accepts it. `fit` cycles or truncates the peaks to one per set
	/// bit of the count.
	Parts {
		leaf_count: LeafCount,
		peaks: Vec<HashChoice>,
		fit: bool,
	},
}

#[derive(Arbitrary, Debug)]
pub enum FrontierOp {
	Append(HashChoice),
	/// `n` leaves `leaf_hash(seed, [j])`.
	AppendMany {
		n: u8,
		seed: u8,
	},
	RoundTrip,
	Bag(Vec<HashChoice>),
}

#[derive(Arbitrary, Debug)]
pub struct FrontierInput {
	pub start: Start,
	pub ops: Vec<FrontierOp>,
}

/// `leaves` are the frontier's leaves, when known.
fn run_ops(
	palette: &[Hash],
	mut frontier: MmrFrontier,
	mut leaves: Option<Vec<Hash>>,
	ops: &[FrontierOp],
) {
	let mut model = Model::from_parts(frontier.peaks(), frontier.leaf_count());
	check_against_model(&frontier, &model);
	let mut appends = 0;
	let mut append = |frontier: &mut MmrFrontier, leaf: Hash| {
		if appends == MAX_APPENDS {
			return;
		}
		appends += 1;
		let before = frontier.clone();
		match frontier.try_append(leaf) {
			Ok(()) => {
				assert!(
					before.leaf_count() < MAX_MMR_LEAF_COUNT,
					"DIVERGENCE: an append carried a frontier past the ceiling"
				);
				model.append(leaf);
				if let Some(known) = leaves.as_mut() {
					known.push(leaf);
				}
			},
			Err(FrontierFull) => {
				assert_eq!(
					before.leaf_count(),
					MAX_MMR_LEAF_COUNT,
					"DIVERGENCE: an append refused below the ceiling"
				);
				assert_eq!(&before, frontier, "DIVERGENCE: a refused append changed the frontier");
			},
		}
		check_against_model(frontier, &model);
	};
	for op in ops {
		match op {
			FrontierOp::Append(choice) => append(&mut frontier, choice.resolve(palette)),
			FrontierOp::AppendMany { n, seed } => {
				for j in 0..*n {
					append(&mut frontier, leaf_hash(*seed, &[j]));
				}
			},
			FrontierOp::RoundTrip => check_round_trip(&frontier),
			FrontierOp::Bag(choices) => {
				let peaks: Vec<Hash> = choices.iter().map(|c| c.resolve(palette)).collect();
				assert_eq!(bag_peaks(&peaks), model_bag(&peaks), "DIVERGENCE: bag_peaks");
			},
		}
	}
	check_round_trip(&frontier);
	if let Some(known) = leaves.filter(|l| l.len() <= MMR_LIB_LEAVES) {
		check_against_mmr_lib(&frontier, &known);
	}
}

impl Oracle {
	/// Fuzz entry point for `MmrFrontier` and the root and bagging in `mmr`.
	pub fn check_frontier_input(&self, input: &FrontierInput) -> Outcome {
		let (frontier, leaves) = match &input.start {
			Start::Empty => (MmrFrontier::new(), Some(Vec::new())),
			Start::Real(k) => {
				let k = *k as usize % (N + 1);
				(self.frontiers[k].clone(), Some(self.leaves[..k].to_vec()))
			},
			Start::Parts { leaf_count, peaks, fit } => {
				let leaf_count = leaf_count.get();
				let mut peaks: Vec<Hash> = peaks.iter().map(|c| c.resolve(&self.palette)).collect();
				if *fit {
					let have = peaks.len().max(1);
					peaks = (0..leaf_count.count_ones() as usize)
						.map(|i| peaks.get(i % have).copied().unwrap_or_default())
						.collect();
				}
				let Some(frontier) = check_from_parts(&peaks, leaf_count) else {
					return Outcome::Skipped;
				};
				(frontier, None)
			},
		};
		run_ops(&self.palette, frontier, leaves, &input.ops);
		Outcome::Real
	}

	/// Fuzz entry point for raw `MmrFrontier` bytes: decoding must accept exactly the valid
	/// `(leaf_count, peaks)` pairs, in their canonical encoding. The rest of the input is appended
	/// as 32-byte leaves.
	pub fn check_frontier_decode(&self, data: &[u8]) -> Outcome {
		let mut input = data;
		let decoded = MmrFrontier::decode(&mut input);

		let mut reference = data;
		let valid = <(u64, Vec<Hash>)>::decode(&mut reference).is_ok_and(|(count, peaks)| {
			count <= MAX_MMR_LEAF_COUNT && peaks.len() == count.count_ones() as usize
		});
		let frontier = match decoded {
			Ok(frontier) => {
				assert!(valid, "DIVERGENCE: decoded an invalid frontier {frontier:?}");
				assert_eq!(input.len(), reference.len(), "DIVERGENCE: decode length");
				// The reference read these bytes as `(leaf_count, peaks)`: equal bytes, equal
				// fields.
				let consumed = &data[..data.len() - input.len()];
				assert_eq!(consumed, &frontier.encode()[..], "DIVERGENCE: non-canonical");
				frontier
			},
			Err(e) => {
				assert!(!valid, "DIVERGENCE: refused a valid frontier: {e:?}");
				return Outcome::Rejected;
			},
		};
		let ops: Vec<FrontierOp> = input
			.chunks_exact(32)
			.map(|chunk| {
				FrontierOp::Append(HashChoice::Raw(chunk.try_into().expect("32 bytes; qed")))
			})
			.chain(core::iter::once(FrontierOp::RoundTrip))
			.collect();
		run_ops(&self.palette, frontier, None, &ops);
		Outcome::Real
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The crate's frozen vectors, so the model agrees with the protocol, not only with the crate.
	#[test]
	fn model_matches_frozen_vector() {
		let mut model = Model::default();
		for i in 1..=5u8 {
			model.append(Hash::repeat_byte(i));
		}
		assert_eq!(
			model.root(),
			Hash(hex_literal::hex!(
				"1190cb02577a8bafec9f05c38e64ca9155b65e9d862604c4f3d920dff4e5a85f"
			))
		);
		assert_eq!(model_root(0, Hash::zero()), MmrFrontier::new().root().0);
	}

	#[test]
	fn honest_growth_and_ceiling_pass() {
		let oracle = Oracle::new();
		for k in 0..=N as u8 {
			let input = FrontierInput {
				start: Start::Real(k),
				ops: vec![FrontierOp::AppendMany { n: 70, seed: 3 }, FrontierOp::RoundTrip],
			};
			assert_eq!(oracle.check_frontier_input(&input), Outcome::Real);
		}
		let input = FrontierInput {
			start: Start::Parts {
				leaf_count: LeafCount::AroundCeiling(120),
				peaks: vec![],
				fit: true,
			},
			ops: vec![FrontierOp::AppendMany { n: 20, seed: 0 }],
		};
		assert_eq!(oracle.check_frontier_input(&input), Outcome::Real);
		let raw = (MAX_MMR_LEAF_COUNT, vec![Hash::zero()]).encode();
		assert_eq!(oracle.check_frontier_decode(&raw), Outcome::Real);
		assert_eq!(
			oracle.check_frontier_decode(&(3u64, vec![Hash::zero()]).encode()),
			Outcome::Rejected
		);
	}
}

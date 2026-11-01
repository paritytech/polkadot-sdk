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

//! Fuzzer for `cumulus_primitives_spec_messaging`: the MMR verifiers in `lift`, the frontier in
//! `mmr` ([`frontier`]) and the off-chain response verifiers in `message` ([`message`]).
//!
//! [`Oracle`] keeps one real `N`-leaf MMR, every honest proof over it, and a palette of real
//! hashes for inputs to replay. Inputs are honest proofs with edits applied, or raw bytes. Whenever
//! a verifier returns a real root, the oracle checks the claim behind it:
//! - `FORGERY`: a false claim got through.
//! - `MALLEABLE`: a true claim, but from a non-canonical proof.
//!
//! Any other panic is the verifier crashing on bad input.
//!
//! # Running
//!
//! From this directory: `cargo hfuzz run <target>`, one of the binaries in `src/bin`. Pass
//! honggfuzz flags through `HFUZZ_RUN_ARGS`, e.g. `-n 8`. Needs `binutils-dev` and `libunwind-dev`.
//!
//! To debug a crash: `cargo hfuzz run-debug lift_extension hfuzz_workspace/lift_extension/*.fuzz`.

use arbitrary::Arbitrary;
use codec::Decode;
use cumulus_primitives_spec_messaging::{
	build_requires, leaf_hash_with_version,
	mmr::{bag_peaks, empty_root, SpecMerge},
	stitch, ConsumptionRecord, Interval, LiftsBySource, MMRExtensionProof, MmrFrontier,
	LEAF_VERSION,
};
use mmr_lib::{
	leaf_index_to_mmr_size,
	util::{MemMMR, MemStore},
	MMRStoreReadOps,
};
use sp_core::{bounded::BoundedVec, Get, H256 as Hash};
use std::collections::{BTreeSet, HashMap};

pub mod frontier;
pub mod message;
mod pipeline;
pub use frontier::FrontierInput;
pub use message::{check_inherent_decode, FetchInput, MessageOracle};
pub use pipeline::PipelineInput;

/// Enough for every peak layout up to six peaks, small enough to precompute every proof.
pub const N: usize = 64;

/// Only for tests and the replay tally; failures panic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
	/// The input didn't make a proof, e.g. `from_parts` refused a forged frontier.
	Skipped,
	Rejected,
	/// Verified, but to a root no real prefix has.
	Unbound,
	/// Verified to a real root, and the claim held.
	Real,
}

/// The payload behind the oracle's leaf `i`.
pub fn payload(i: u64) -> Vec<u8> {
	i.to_le_bytes().to_vec()
}

pub fn mmr_size(leaves: u64) -> u64 {
	if leaves == 0 {
		0
	} else {
		leaf_index_to_mmr_size(leaves - 1)
	}
}

/// One real MMR, built with `mmr_lib`, and every honest proof over it.
pub struct Oracle {
	pub leaves: Vec<Hash>,
	/// Indexed by leaf count.
	pub frontiers: Vec<MmrFrontier>,
	/// Root to leaf count, including the empty root.
	pub roots: HashMap<Hash, usize>,
	/// `[k][m]`: from `k` to `m` leaves, `None` unless `k < m`.
	extensions: Vec<Vec<Option<MMRExtensionProof>>>,
	/// Every real hash an attacker could replay: nodes, partial peak bags and roots.
	pub palette: Vec<Hash>,
}

impl Default for Oracle {
	fn default() -> Self {
		Self::new()
	}
}

impl Oracle {
	pub fn new() -> Self {
		Self::with_leaf_version(LEAF_VERSION)
	}

	/// Leaf `i` is `leaf_hash_with_version(leaf_version, &payload(i))`.
	pub fn with_leaf_version(leaf_version: u8) -> Self {
		let leaves: Vec<Hash> = (0..N as u64)
			.map(|i| leaf_hash_with_version(leaf_version, &payload(i)))
			.collect();

		let store = MemStore::<Hash>::default();
		{
			let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
			for leaf in &leaves {
				mmr.push(*leaf).expect("MemStore never fails; qed");
			}
			mmr.commit().expect("MemStore never fails; qed");
		}
		// A prefix's nodes are just the first `mmr_size(k)` in the full store.
		let at = |k: usize| MemMMR::<Hash, SpecMerge>::new(mmr_size(k as u64), &store);

		let mut frontiers = vec![MmrFrontier::new()];
		for leaf in &leaves {
			let mut next = frontiers[frontiers.len() - 1].clone();
			next.append(*leaf);
			frontiers.push(next);
		}

		let mut roots = HashMap::new();
		roots.insert(empty_root().0, 0);
		for (m, frontier) in frontiers.iter().enumerate().skip(1) {
			roots.insert(frontier.root().0, m);
		}

		let extensions = (0..=N)
			.map(|k| {
				(0..=N)
					.map(|m| {
						(k < m).then(|| {
							let connecting_nodes = if k == 0 {
								frontiers[m].peaks().to_vec()
							} else {
								at(m)
									.gen_ancestry_proof(mmr_size(k as u64))
									.expect("0 < k < m is a valid ancestry shape; qed")
									.prev_peaks_proof
									.proof_items()
									.iter()
									.map(|(_, hash)| *hash)
									.collect()
							};
							let connecting_nodes = BoundedVec::try_from(connecting_nodes)
								.expect("an honest proof fits the bound; qed");
							MMRExtensionProof { leaf_count: m as u64, connecting_nodes }
						})
					})
					.collect()
			})
			.collect();

		let mut palette = BTreeSet::new();
		for pos in 0..mmr_size(N as u64) {
			if let Ok(Some(node)) = (&store).get_elem(pos) {
				palette.insert(node);
			}
		}
		for frontier in &frontiers[1..] {
			let peaks = frontier.peaks();
			for start in 0..peaks.len() {
				palette.insert(bag_peaks(&peaks[start..]));
			}
		}
		palette.extend(roots.keys().copied());

		Self { leaves, frontiers, roots, extensions, palette: palette.into_iter().collect() }
	}

	pub fn extension(&self, k: u64, m: u64) -> Option<&MMRExtensionProof> {
		self.extensions.get(k as usize)?.get(m as usize)?.as_ref()
	}

	pub fn frontier(&self, k: u64) -> Option<&MmrFrontier> {
		self.frontiers.get(k as usize)
	}

	fn hash(&self, choice: HashChoice) -> Hash {
		choice.resolve(&self.palette)
	}

	fn from(&self, choice: &FromChoice) -> Option<MmrFrontier> {
		match choice {
			FromChoice::Real(k) => Some(self.frontiers[*k as usize % (N + 1)].clone()),
			FromChoice::Forged { leaf_count, peaks } => MmrFrontier::from_parts(
				peaks.iter().map(|c| self.hash(*c)).collect(),
				leaf_count.get(),
			),
		}
	}

	fn apply_bounded<S: Get<u32>>(&self, items: &mut BoundedVec<Hash, S>, edits: &[Edit]) {
		apply_bounded(&self.palette, items, edits)
	}

	fn apply(&self, items: &mut Vec<Hash>, edits: &[Edit]) {
		apply(&self.palette, items, edits)
	}

	/// Fuzz entry point for [`MMRExtensionProof::calculate_root`].
	pub fn check_extension_input(&self, input: &ExtensionInput) -> Outcome {
		let Some(from) = self.from(&input.from) else { return Outcome::Skipped };
		let mut proof = input
			.template
			.and_then(|m| self.extension(from.leaf_count(), m as u64 % (N as u64 + 1)).cloned())
			.unwrap_or_default();
		if let Some(count) = input.leaf_count {
			proof.leaf_count = count.get();
		}
		self.apply_bounded(&mut proof.connecting_nodes, &input.edits);
		self.check_extension(&from, &proof)
	}

	/// A real root must come from the honest proof off a real frontier.
	pub fn check_extension(&self, from: &MmrFrontier, proof: &MMRExtensionProof) -> Outcome {
		let Ok(root) = proof.calculate_root(from) else { return Outcome::Rejected };
		let Some(&m) = self.roots.get(&root.0) else { return Outcome::Unbound };
		let k = from.leaf_count();
		if proof.leaf_count == 0 && proof.connecting_nodes.is_empty() {
			assert!(
				self.frontier(k) == Some(from) && k == m as u64,
				"FORGERY: identity extension on a forged {k}-leaf frontier yields the real root of \
				 {m} leaves"
			);
			return Outcome::Real;
		}
		assert!(
			self.frontier(k) == Some(from),
			"FORGERY: a forged {k}-leaf frontier extends to the real root of {m} leaves: {proof:?}"
		);
		assert_eq!(
			proof.leaf_count, m as u64,
			"FORGERY: an extension claiming {} leaves yields the real root of {m} leaves: {proof:?}",
			proof.leaf_count
		);
		assert_eq!(
			Some(proof),
			self.extension(k, m as u64),
			"MALLEABLE: a non-honest extension from {k} to {m} leaves yields the real root"
		);
		Outcome::Real
	}

	/// Fuzz entry point for raw SCALE bytes. The first byte picks what to decode.
	pub fn check_decode(&self, data: &[u8]) -> Outcome {
		let Some((&selector, mut rest)) = data.split_first() else { return Outcome::Skipped };
		let input = &mut rest;
		match selector % 3 {
			0 => {
				let Ok((k, proof)) = <(u8, MMRExtensionProof)>::decode(input) else {
					return Outcome::Skipped;
				};
				self.check_extension(&self.frontiers[k as usize % (N + 1)], &proof)
			},
			1 => {
				let Ok((intervals, advances)) =
					<(Vec<Interval>, Vec<MMRExtensionProof>)>::decode(input)
				else {
					return Outcome::Skipped;
				};
				match stitch(&intervals, &advances) {
					Ok(endpoint) => {
						assert_eq!(
							Some(&endpoint),
							intervals.last().map(|i| &i.end),
							"stitch returned something other than the last interval's end"
						);
						Outcome::Real
					},
					Err(_) => Outcome::Rejected,
				}
			},
			_ => {
				let Ok((records, lifts)) = <(Vec<ConsumptionRecord>, LiftsBySource)>::decode(input)
				else {
					return Outcome::Skipped;
				};
				match build_requires(&records, &lifts) {
					Ok(_) => Outcome::Real,
					Err(_) => Outcome::Rejected,
				}
			},
		}
	}
}

/// [`apply`] on a bounded list. An edit that would pass the bound is dropped: such a list cannot
/// decode, which the `*_decode` targets cover.
pub(crate) fn apply_bounded<S: Get<u32>>(
	palette: &[Hash],
	items: &mut BoundedVec<Hash, S>,
	edits: &[Edit],
) {
	let mut edited = items.to_vec();
	apply(palette, &mut edited, edits);
	if let Ok(edited) = BoundedVec::try_from(edited) {
		*items = edited;
	}
}

pub(crate) fn apply(palette: &[Hash], items: &mut Vec<Hash>, edits: &[Edit]) {
	for edit in edits {
		let len = items.len();
		match *edit {
			Edit::Set { slot, hash } if len > 0 => {
				items[slot as usize % len] = hash.resolve(palette)
			},
			Edit::Insert { slot, hash } => {
				items.insert(slot as usize % (len + 1), hash.resolve(palette))
			},
			Edit::Remove { slot } if len > 0 => {
				items.remove(slot as usize % len);
			},
			Edit::Swap { a, b } if len > 0 => items.swap(a as usize % len, b as usize % len),
			_ => {},
		}
	}
}

/// A palette index (wraps) or raw bytes.
#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum HashChoice {
	Palette(u16),
	Raw([u8; 32]),
}

impl HashChoice {
	pub(crate) fn resolve(self, palette: &[Hash]) -> Hash {
		match self {
			HashChoice::Palette(i) => palette[i as usize % palette.len()],
			HashChoice::Raw(bytes) => Hash(bytes),
		}
	}
}

/// A count, size or position. `Near` stays in `0..=N + 1`, where the interesting values are.
#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum Count {
	Near(u8),
	Raw(u64),
	/// Hits valid MMR sizes far more often than `Raw`.
	SizeOf(u8),
}

impl Count {
	pub(crate) fn get(self) -> u64 {
		let near = |x: u8| x as u64 % (N as u64 + 2);
		match self {
			Count::Near(x) => near(x),
			Count::Raw(v) => v,
			Count::SizeOf(x) => mmr_size(near(x)),
		}
	}
}

#[derive(Arbitrary, Debug)]
pub enum FromChoice {
	/// After `k % (N + 1)` leaves.
	Real(u8),
	/// Skipped unless `from_parts` accepts it.
	Forged { leaf_count: Count, peaks: Vec<HashChoice> },
}

/// Slots wrap around the list's length.
#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum Edit {
	Set { slot: u8, hash: HashChoice },
	Insert { slot: u8, hash: HashChoice },
	Remove { slot: u8 },
	Swap { a: u8, b: u8 },
}

#[derive(Arbitrary, Debug)]
pub struct ExtensionInput {
	pub from: FromChoice,
	/// Start from the honest proof to `m % (N + 1)` leaves, or the identity if there isn't one.
	pub template: Option<u8>,
	pub leaf_count: Option<Count>,
	pub edits: Vec<Edit>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use arbitrary::Unstructured;

	#[test]
	fn honest_proofs_reach_real_roots() {
		let oracle = Oracle::new();
		for m in 1..=N as u64 {
			for k in 0..m {
				let proof = oracle.extension(k, m).unwrap();
				assert_eq!(
					oracle.check_extension(oracle.frontier(k).unwrap(), proof),
					Outcome::Real
				);
			}
		}
	}

	#[test]
	fn identity_and_edits_are_reachable() {
		let oracle = Oracle::new();
		let identity = ExtensionInput {
			from: FromChoice::Real(5),
			template: None,
			leaf_count: None,
			edits: vec![],
		};
		assert_eq!(oracle.check_extension_input(&identity), Outcome::Real);
		let edited = ExtensionInput {
			from: FromChoice::Real(5),
			template: Some(20),
			leaf_count: None,
			edits: vec![Edit::Set { slot: 0, hash: HashChoice::Raw([7; 32]) }],
		};
		assert_eq!(oracle.check_extension_input(&edited), Outcome::Unbound);
	}

	/// The harness itself doesn't panic on random bytes, and structured inputs do reach real roots.
	#[test]
	fn harness_smoke() {
		let oracle = Oracle::new();
		let messages = MessageOracle::new();
		let mut state = 0x9E37_79B9_7F4A_7C15u64;
		let mut real = 0;
		for _ in 0..20_000 {
			let bytes: Vec<u8> = (0..256)
				.map(|_| {
					state ^= state << 13;
					state ^= state >> 7;
					state ^= state << 17;
					state as u8
				})
				.collect();
			if let Ok(input) = ExtensionInput::arbitrary(&mut Unstructured::new(&bytes)) {
				real += (oracle.check_extension_input(&input) == Outcome::Real) as u32;
			}
			if let Ok(input) = PipelineInput::arbitrary(&mut Unstructured::new(&bytes)) {
				real += (oracle.check_pipeline_input(&input) == Outcome::Real) as u32;
			}
			if let Ok(input) = FrontierInput::arbitrary(&mut Unstructured::new(&bytes)) {
				real += (oracle.check_frontier_input(&input) == Outcome::Real) as u32;
			}
			if let Ok(input) = FetchInput::arbitrary(&mut Unstructured::new(&bytes)) {
				real += (messages.check_fetch_input(&input) == Outcome::Real) as u32;
			}
			oracle.check_decode(&bytes);
			oracle.check_frontier_decode(&bytes);
			messages.check_message_decode(&bytes);
		}
		assert!(real > 0);
	}

	/// Replays honggfuzz inputs without a debugger, picking the target from the path. A single file
	/// also prints the decoded input. Run under `cargo llvm-cov` for corpus coverage, e.g.
	/// `REPLAY="hfuzz_workspace/lift_extension/*.fuzz" cargo test --release replay -- --ignored
	/// --nocapture`.
	#[test]
	#[ignore]
	fn replay() {
		let oracle = Oracle::new();
		let messages = MessageOracle::new();
		let roots = std::env::var("REPLAY").expect("set REPLAY to input files or directories");
		let mut paths = Vec::new();
		for root in roots.split_whitespace().map(std::path::PathBuf::from) {
			match std::fs::read_dir(&root) {
				Ok(dir) => paths.extend(dir.map(|e| e.expect("readable directory entry").path())),
				Err(_) => paths.push(root),
			}
		}
		let verbose = paths.len() == 1;
		let mut tally: std::collections::BTreeMap<(&str, String), usize> = Default::default();
		for path in &paths {
			let bytes = std::fs::read(path).expect("readable input");
			let name = path.to_string_lossy();
			let input = &mut Unstructured::new(&bytes);
			let (target, outcome) = if name.contains("lift_extension") {
				let Ok(input) = ExtensionInput::arbitrary(input) else {
					*tally.entry(("lift_extension", "Unparsed".into())).or_default() += 1;
					continue;
				};
				if verbose {
					println!("{input:#?}");
				}
				("lift_extension", oracle.check_extension_input(&input))
			} else if name.contains("mmr_frontier") {
				let Ok(input) = FrontierInput::arbitrary(input) else {
					*tally.entry(("mmr_frontier", "Unparsed".into())).or_default() += 1;
					continue;
				};
				if verbose {
					println!("{input:#?}");
				}
				("mmr_frontier", oracle.check_frontier_input(&input))
			} else if name.contains("mmr_decode") {
				("mmr_decode", oracle.check_frontier_decode(&bytes))
			} else if name.contains("message_fetch") {
				let Ok(input) = FetchInput::arbitrary(input) else {
					*tally.entry(("message_fetch", "Unparsed".into())).or_default() += 1;
					continue;
				};
				if verbose {
					println!("{input:#?}");
				}
				("message_fetch", messages.check_fetch_input(&input))
			} else if name.contains("message_decode") {
				("message_decode", messages.check_message_decode(&bytes))
			} else if name.contains("lift_pipeline") {
				let Ok(input) = PipelineInput::arbitrary(input) else {
					*tally.entry(("lift_pipeline", "Unparsed".into())).or_default() += 1;
					continue;
				};
				if verbose {
					println!("{input:#?}");
				}
				("lift_pipeline", oracle.check_pipeline_input(&input))
			} else {
				("lift_decode", oracle.check_decode(&bytes))
			};
			*tally.entry((target, format!("{outcome:?}"))).or_default() += 1;
		}
		for ((target, outcome), count) in tally {
			println!("{target:<15} {outcome:<9} {count}");
		}
	}
}

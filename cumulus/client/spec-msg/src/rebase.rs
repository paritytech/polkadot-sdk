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

//! Lift material for an endpoint before the one it was fetched for.
//!
//! `create_inherent` may admit only a prefix of a planned channel item: it stops at the first
//! payload it cannot admit, and at the block's proof budget. The block then ends the stream before
//! the endpoint the node holds a binding for. [`rebase`] computes the extension from the earlier
//! endpoint out of that binding's extension and the leaves in between, with no fetch.

use cumulus_primitives_spec_messaging::{
	mmr::{SpecMerge, MAX_MMR_LEAF_COUNT},
	MMRExtensionProof, MmrFrontier,
};
use mmr_lib::Merge;
use polkadot_core_primitives::Hash;
use std::collections::BTreeMap;

/// A perfect subtree of a stream's MMR: its first leaf and its height.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Span {
	start: u64,
	height: u32,
}

impl Span {
	/// The leaf after its last.
	fn end(&self) -> u64 {
		self.start + (1 << self.height)
	}
}

/// The subtrees the connecting nodes of an extension stand for, in their wire order: one node per
/// sibling, then one for the run if there is one.
#[derive(Default)]
struct Layout {
	/// The right siblings on the path from the last old leaf up to the new peak over it, lowest
	/// first.
	siblings: Vec<Span>,
	/// The new peaks to the right of the old leaves, highest first. The proof carries them as one
	/// node: their bag, or the peak itself if there is only one.
	run: Vec<Span>,
}

impl Layout {
	/// The layout of an extension from `n1` to `n2` leaves, `1 <= n1 <= n2 <= MAX_MMR_LEAF_COUNT`.
	/// Empty for the identity.
	fn new(n1: u64, n2: u64) -> Self {
		let mut layout = Self::default();
		if n1 >= n2 {
			return layout;
		}
		// The highest bit the counts differ in: `n2` has a peak of that height, starting at
		// `split`, that `n1` lacks. The peaks before it are common to both.
		let height = 63 - (n1 ^ n2).leading_zeros();
		let split = (n2 >> (height + 1)) << (height + 1);
		// The old leaves under that peak.
		let held = n1 - split;
		if held == 0 {
			layout.run.push(Span { start: split, height });
		} else {
			for h in 0..height {
				// The node over the last old leaf at height `h`. A right child merges with an old
				// peak; a left child needs its sibling.
				let index = (held - 1) >> h;
				if index & 1 == 0 {
					layout.siblings.push(Span { start: split + ((index + 1) << h), height: h });
				}
			}
		}
		let mut start = split + (1 << height);
		for h in (0..height).rev() {
			if (n2 >> h) & 1 == 1 {
				layout.run.push(Span { start, height: h });
				start += 1 << h;
			}
		}
		layout
	}

	/// The number of connecting nodes.
	fn len(&self) -> usize {
		self.siblings.len() + usize::from(!self.run.is_empty())
	}
}

/// The subtrees past `first`: over `leaves` from `first` on, the known ones past those, and
/// whatever spans both, from its halves.
struct Subtrees<'a> {
	first: u64,
	leaves: &'a [Hash],
	known: BTreeMap<Span, Hash>,
}

impl Subtrees<'_> {
	fn get(&self, span: Span) -> Option<Hash> {
		let past = self.first + self.leaves.len() as u64;
		if span.start >= self.first && span.end() <= past {
			let (start, end) =
				((span.start - self.first) as usize, (span.end() - self.first) as usize);
			return merkle(&self.leaves[start..end]);
		}
		if let Some(node) = self.known.get(&span) {
			return Some(*node);
		}
		if span.height == 0 || span.start >= past || span.end() <= past {
			return None;
		}
		let half = span.height - 1;
		let left = self.get(Span { start: span.start, height: half })?;
		let right = self.get(Span { start: span.start + (1 << half), height: half })?;
		SpecMerge::merge(&left, &right).ok()
	}
}

/// The root of the perfect subtree over `leaves`, a power of two of them.
fn merkle(leaves: &[Hash]) -> Option<Hash> {
	match leaves {
		[] => None,
		[leaf] => Some(*leaf),
		_ => {
			let (left, right) = leaves.split_at(leaves.len() / 2);
			SpecMerge::merge(&merkle(left)?, &merkle(right)?).ok()
		},
	}
}

/// `extension`, which extends the frontier `leaves` past `from`, rebased to extend `from` itself
/// to the same root. `None` if `from` is empty (that extension carries the new peaks one by one,
/// and `extension` may bag some) or if the material does not fit together.
///
/// The extension from `from` needs the subtrees right of it. Those over `leaves` are hashed, those
/// past them are `extension`'s siblings, and one over both is merged from its halves. Its run of
/// new peaks ends with the run `extension` bagged, which is folded into the rest. The result is
/// checked to reach the root `extension` does before it is returned.
pub(crate) fn rebase(
	from: &MmrFrontier,
	leaves: &[Hash],
	extension: &MMRExtensionProof,
) -> Option<MMRExtensionProof> {
	let (old, count) = (from.leaf_count(), extension.leaf_count);
	let past = old.checked_add(leaves.len() as u64)?;
	if old == 0 || past > count || count > MAX_MMR_LEAF_COUNT {
		return None;
	}
	let given = Layout::new(past, count);
	if given.len() != extension.connecting_nodes.len() {
		return None;
	}
	let (siblings, run) = extension.connecting_nodes.split_at(given.siblings.len());
	let subtrees = Subtrees {
		first: old,
		leaves,
		known: given.siblings.iter().copied().zip(siblings.iter().copied()).collect(),
	};

	let wanted = Layout::new(old, count);
	let mut nodes = wanted
		.siblings
		.iter()
		.map(|span| subtrees.get(*span))
		.collect::<Option<Vec<_>>>()?;
	if let Some((lowest, higher)) = wanted.run.split_last() {
		// Bags fold from the lowest peak up, so the run `extension` bagged starts the fold.
		let (mut bag, higher) = match run.first() {
			Some(node) if wanted.run.ends_with(&given.run) => {
				(*node, &wanted.run[..wanted.run.len() - given.run.len()])
			},
			_ => (subtrees.get(*lowest)?, higher),
		};
		for span in higher.iter().rev() {
			bag = SpecMerge::merge_peaks(&bag, &subtrees.get(*span)?).ok()?;
		}
		nodes.push(bag);
	}
	let rebased = MMRExtensionProof { leaf_count: count, connecting_nodes: nodes.try_into().ok()? };

	let mut to = from.clone();
	for leaf in leaves {
		to.try_append(*leaf).ok()?;
	}
	(rebased.calculate_root(from).ok()? == extension.calculate_root(&to).ok()?).then_some(rebased)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::receiver::skip_proof;
	use cumulus_primitives_spec_messaging::leaf_hash;

	/// Every rebase from `from` past `later` leaves to `count` equals the extension built from
	/// the leaves with `mmr_lib`, for every `1 <= from < later <= count <= 120`.
	#[test]
	fn a_rebased_extension_is_the_one_built_from_the_leaves() {
		let leaves: Vec<Hash> = (0..120u32).map(|i| leaf_hash(&i.to_le_bytes())).collect();
		let mut frontiers = vec![MmrFrontier::new()];
		for leaf in &leaves {
			let mut next = frontiers[frontiers.len() - 1].clone();
			next.append(*leaf);
			frontiers.push(next);
		}
		let mut checked = 0;
		for count in 2..=leaves.len() {
			// From each `from` in `1..count`, at index `from - 1`.
			let direct: Vec<_> = (1..count)
				.map(|from| skip_proof(&frontiers[from], &leaves[from..count]).expect("a proof"))
				.collect();
			for later in 2..=count {
				let given = if later == count {
					MMRExtensionProof::identity(&frontiers[count])
				} else {
					direct[later - 1].clone()
				};
				for from in 1..later {
					let rebased = rebase(&frontiers[from], &leaves[from..later], &given);
					assert_eq!(
						rebased.as_ref(),
						Some(&direct[from - 1]),
						"{from}, {later}, {count}"
					);
					checked += 1;
				}
			}
		}
		assert_eq!(checked, (2..=120u64).map(|n| n * (n - 1) / 2).sum::<u64>());
	}

	#[test]
	fn a_rebase_needs_a_start_and_a_proof_that_fits() {
		let leaves: Vec<Hash> = (0..9u32).map(|i| leaf_hash(&i.to_le_bytes())).collect();
		let at = |n: usize| {
			let mut frontier = MmrFrontier::new();
			leaves[..n].iter().for_each(|leaf| frontier.append(*leaf));
			frontier
		};
		let given = skip_proof(&at(6), &leaves[6..]).expect("a proof");

		// No leaves: the extension itself.
		assert_eq!(rebase(&at(6), &[], &given), Some(given.clone()));
		// From the empty stream, the proof is the new peaks themselves.
		assert_eq!(rebase(&MmrFrontier::new(), &leaves[..6], &given), None);
		// Leaves past the proof's count.
		let to_seven = skip_proof(&at(5), &leaves[5..7]).expect("a proof");
		assert_eq!(rebase(&at(4), &leaves[4..9], &to_seven), None);
		// A node too many or too few for the counts.
		let mut long = given.clone();
		long.connecting_nodes.try_push(leaves[0]).expect("room for one more");
		assert_eq!(rebase(&at(4), &leaves[4..6], &long), None);
		let mut short = given.clone();
		short.connecting_nodes.truncate(given.connecting_nodes.len() - 1);
		assert_eq!(rebase(&at(4), &leaves[4..6], &short), None);
		// The same proof with every node in place rebases.
		assert_eq!(rebase(&at(4), &leaves[4..6], &given), skip_proof(&at(4), &leaves[4..]));
	}
}

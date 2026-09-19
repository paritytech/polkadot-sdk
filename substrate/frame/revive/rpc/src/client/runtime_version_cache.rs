// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Tracks which runtime versions govern which block-number ranges.

use super::SubstrateBlockNumber;
use std::{collections::BTreeMap, sync::RwLock};

/// A runtime's `(spec_version, transaction_version)`, the pair subxt's per-block lookup expects.
pub type SpecAndTransactionVersion = (u32, u32);

/// How many blocks outside the ranges are cached individually.
const MAX_STANDALONE_BLOCKS: usize = 4096;

/// How many ranges are kept; the oldest go first, and their blocks resolve on demand again.
const MAX_RANGES: usize = 64;

/// The runtime versions governing blocks from `from` (inclusive) up to the start of the next
/// range, or up to `highest_observed` for the newest range; `None` for blocks the subscriptions
/// skipped. Ranges are never modified: the newest one grows as `highest_observed` moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionRange {
	from: SubstrateBlockNumber,
	versions: Option<SpecAndTransactionVersion>,
}

/// Runtime versions by block number. `ranges` cover the blocks the subscriptions observed, from
/// the first one up to `highest_observed`; blocks resolved outside that span are kept in
/// `standalone_blocks`, since a single block says nothing about its neighbours.
#[derive(Debug, Default)]
struct VersionStore {
	/// Sorted by `from`, capped at `MAX_RANGES`. Every observed runtime upgrade adds a range, and
	/// an upgrade among skipped blocks also adds a `None` range covering those blocks.
	ranges: Vec<VersionRange>,
	/// The highest block number the subscriptions observed; the ranges end here.
	highest_observed: SubstrateBlockNumber,
	/// The versions of blocks the ranges do not cover, keyed by block number: historical blocks
	/// below the first range, blocks inside a skipped gap, and blocks ahead of `highest_observed`.
	standalone_blocks: BTreeMap<SubstrateBlockNumber, SpecAndTransactionVersion>,
}

impl VersionStore {
	fn lookup_ranges(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Option<SpecAndTransactionVersion> {
		if block_number > self.highest_observed {
			return None;
		}
		self.ranges.iter().rev().find(|range| range.from <= block_number)?.versions
	}

	fn cache_standalone_block(
		&mut self,
		block_number: SubstrateBlockNumber,
		versions: SpecAndTransactionVersion,
	) {
		if self.lookup_ranges(block_number).is_some() {
			return;
		}
		self.standalone_blocks.insert(block_number, versions);
		if self.standalone_blocks.len() > MAX_STANDALONE_BLOCKS {
			self.standalone_blocks.pop_first();
		}
	}

	fn record(&mut self, block_number: SubstrateBlockNumber, versions: SpecAndTransactionVersion) {
		if let Some(newest) = self.ranges.last() {
			// The ranges only grow forward; an older block cannot rewrite them.
			if block_number < newest.from {
				return self.cache_standalone_block(block_number, versions);
			}
			// Unchanged versions only move `highest_observed`, even across skipped blocks: an
			// upgrade in between would have raised the spec version.
			if newest.versions != Some(versions) {
				self.start_range_after_upgrade(block_number, versions);
			}
		} else {
			self.start_range(block_number, Some(versions));
		}
		self.highest_observed = self.highest_observed.max(block_number);
	}

	/// The upgrade observed at `block_number` starts a range there, whether the block is
	/// contiguous or a same-height fork block replacing what was observed at that height. Blocks
	/// skipped before it stay unknown, since the upgrade could sit anywhere among them.
	fn start_range_after_upgrade(
		&mut self,
		block_number: SubstrateBlockNumber,
		versions: SpecAndTransactionVersion,
	) {
		let next_unobserved = self.highest_observed.saturating_add(1);
		if block_number > next_unobserved {
			self.start_range(next_unobserved, None);
		}
		self.start_range(block_number, Some(versions));
	}

	fn start_range(
		&mut self,
		from: SubstrateBlockNumber,
		versions: Option<SpecAndTransactionVersion>,
	) {
		self.ranges.push(VersionRange { from, versions });
		if self.ranges.len() > MAX_RANGES {
			self.ranges.remove(0);
		}
	}
}

/// Answers which runtime versions govern the block numbers observed by the block subscriptions
/// or resolved on demand.
#[derive(Debug, Default)]
pub struct RuntimeVersionCache {
	store: RwLock<VersionStore>,
}

impl RuntimeVersionCache {
	/// The `(spec_version, transaction_version)` at `block_number`, if the ranges cover it or the
	/// block itself was resolved before.
	pub fn lookup(&self, block_number: SubstrateBlockNumber) -> Option<SpecAndTransactionVersion> {
		let store = self.store.read().expect("no code panics while holding the lock; qed");
		store
			.lookup_ranges(block_number)
			.or_else(|| store.standalone_blocks.get(&block_number).copied())
	}

	/// Bumps `highest_observed` to `block_number`, which must be contiguous with the observed
	/// span.
	pub fn bump_highest_observed(&self, block_number: SubstrateBlockNumber) -> bool {
		let mut store = self.store.write().expect("no code panics while holding the lock; qed");
		if store.ranges.is_empty() || block_number > store.highest_observed.saturating_add(1) {
			return false;
		}
		store.highest_observed = store.highest_observed.max(block_number);
		true
	}

	/// Record the versions the subscriptions observed at `block_number`: extends the newest range
	/// when they are unchanged, starts a new range at `block_number` otherwise, and leaves any
	/// skipped blocks in between unknown. Blocks below the newest range are cached like
	/// [`Self::cache_standalone`] does.
	pub fn record(&self, block_number: SubstrateBlockNumber, versions: SpecAndTransactionVersion) {
		let mut store = self.store.write().expect("no code panics while holding the lock; qed");
		store.record(block_number, versions);
	}

	/// Cache the versions resolved on demand for a single block. Unlike [`Self::record`], this
	/// never grows the ranges: the block may lie anywhere and says nothing about its neighbours.
	pub fn cache_standalone(
		&self,
		block_number: SubstrateBlockNumber,
		versions: SpecAndTransactionVersion,
	) {
		let mut store = self.store.write().expect("no code panics while holding the lock; qed");
		store.cache_standalone_block(block_number, versions);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::ops::RangeInclusive;

	/// A cache whose subscriptions observed `blocks` under unchanged versions.
	fn observed(
		blocks: RangeInclusive<SubstrateBlockNumber>,
		versions: SpecAndTransactionVersion,
	) -> RuntimeVersionCache {
		let cache = RuntimeVersionCache::default();
		cache.record(*blocks.start(), versions);
		for block_number in blocks.skip(1) {
			assert!(cache.bump_highest_observed(block_number));
		}
		cache
	}

	#[test]
	fn ranges_cover_only_the_observed_span() {
		let cache = RuntimeVersionCache::default();
		// Bumping an empty cache must not fabricate a range out of nothing.
		assert!(!cache.bump_highest_observed(100));
		assert_eq!(cache.lookup(100), None);

		let cache = observed(100..=110, (1, 1));
		assert_eq!(cache.lookup(100), Some((1, 1)));
		assert_eq!(cache.lookup(110), Some((1, 1)));
		// Blocks outside the span are unknown: an upgrade could sit anywhere before or after it,
		// so a jump past the highest observed block must be refused too.
		assert_eq!(cache.lookup(99), None);
		assert_eq!(cache.lookup(111), None);
		assert!(!cache.bump_highest_observed(112));
	}

	#[test]
	fn recording_starts_extends_or_skips_ranges() {
		let cache = observed(100..=110, (1, 1));
		// A contiguous block with new versions starts a range; older blocks keep theirs.
		cache.record(111, (2, 1));
		assert_eq!(cache.lookup(110), Some((1, 1)));
		assert_eq!(cache.lookup(111), Some((2, 1)));
		// Unchanged versions across a gap extend the newest range instead of adding one: an
		// upgrade in between would have raised the spec version.
		cache.record(200, (2, 1));
		assert_eq!(cache.lookup(150), Some((2, 1)));
		assert_eq!(cache.store.read().unwrap().ranges.len(), 2);
		// Changed versions across a gap leave the skipped blocks unknown: the upgrade could sit
		// anywhere among them.
		cache.record(300, (3, 1));
		assert_eq!(cache.lookup(200), Some((2, 1)));
		assert_eq!(cache.lookup(201), None);
		assert_eq!(cache.lookup(299), None);
		assert_eq!(cache.lookup(300), Some((3, 1)));
		assert!(cache.bump_highest_observed(301));
	}

	#[test]
	fn ranges_are_bounded_and_the_oldest_goes_first() {
		let cache = observed(100..=100, (1, 1));
		let upgrades = MAX_RANGES as u32;
		for upgrade in 1..=upgrades {
			cache.record(100 + upgrade as SubstrateBlockNumber, (1 + upgrade, 1));
		}
		// Blocks of a dropped range resolve on demand again instead of answering wrongly.
		assert_eq!(cache.lookup(100), None);
		assert_eq!(cache.lookup(101), Some((2, 1)));
		assert_eq!(cache.lookup(100 + upgrades as SubstrateBlockNumber), Some((1 + upgrades, 1)));
	}

	#[test]
	fn standalone_blocks_answer_only_for_themselves() {
		let cache = observed(100..=110, (1, 1));
		// Blocks resolved ahead of the subscriptions or below their first block say nothing
		// about their neighbours: an upgrade could sit anywhere in between.
		cache.cache_standalone(115, (2, 1));
		cache.record(50, (0, 0));
		assert_eq!(cache.lookup(115), Some((2, 1)));
		assert_eq!(cache.lookup(50), Some((0, 0)));
		for block_number in [49, 51, 99, 111, 114, 116] {
			assert_eq!(cache.lookup(block_number), None);
		}
		// The subscriptions then observe the upgrade at its actual block.
		assert!(cache.bump_highest_observed(111));
		cache.record(112, (2, 1));
		assert_eq!(cache.lookup(111), Some((1, 1)));
		assert_eq!(cache.lookup(112), Some((2, 1)));
	}

	#[test]
	fn standalone_blocks_are_bounded_and_the_lowest_goes_first() {
		let last_block = MAX_STANDALONE_BLOCKS as SubstrateBlockNumber;
		let first_observed = last_block + 10;
		let cache = observed(first_observed..=first_observed, (2, 1));
		for block_number in 0..=last_block {
			cache.cache_standalone(block_number, (1, 1));
		}
		// Requests cluster near the head, so the blocks furthest below the ranges go first.
		assert_eq!(cache.lookup(0), None);
		assert_eq!(cache.lookup(1), Some((1, 1)));
		// Blocks the ranges answer are not cached, so head traffic cannot evict the rest.
		cache.cache_standalone(first_observed, (2, 1));
		assert_eq!(cache.lookup(1), Some((1, 1)));
	}
}

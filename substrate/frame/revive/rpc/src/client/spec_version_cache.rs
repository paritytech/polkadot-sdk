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
//!
//! Subxt consults `Config::spec_and_transaction_version_for_block_number` before falling back to
//! a `Core_version` runtime call, but only supports statically configured ranges. This store
//! supplies that hook dynamically, fed by the block subscriptions, so at-block clients skip the
//! per-block `Core_version` call.

use std::sync::RwLock;

/// The runtime versions governing blocks from `from` (inclusive) up to the start of the next
/// range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
	from: u64,
	spec_version: u32,
	transaction_version: u32,
}

#[derive(Debug, Default)]
struct Inner {
	/// Sorted by `from`, one entry per observed runtime upgrade.
	ranges: Vec<Range>,
	/// The highest block number the cache can answer for.
	known_up_to: u64,
}

/// Answers which runtime versions govern the block numbers observed by the block subscriptions.
#[derive(Debug, Default)]
pub struct SpecVersionCache {
	inner: RwLock<Inner>,
}

impl SpecVersionCache {
	/// The `(spec_version, transaction_version)` at `block_number`, if the observed window
	/// covers it.
	pub fn lookup(&self, block_number: u64) -> Option<(u32, u32)> {
		let inner = self.inner.read().expect("no code panics while holding the lock; qed");
		if block_number > inner.known_up_to {
			return None;
		}
		inner
			.ranges
			.iter()
			.rev()
			.find(|range| range.from <= block_number)
			.map(|range| (range.spec_version, range.transaction_version))
	}

	/// Extends the window to a contiguous `block_number`.
	pub fn extend_to(&self, block_number: u64) -> bool {
		let mut inner = self.inner.write().expect("no code panics while holding the lock; qed");
		if inner.ranges.is_empty() || block_number > inner.known_up_to.saturating_add(1) {
			return false;
		}
		inner.known_up_to = inner.known_up_to.max(block_number);
		true
	}

	/// Record the versions resolved at `block_number`: extends the newest range when they are
	/// unchanged, or starts a new range at `block_number` otherwise.
	pub fn record(&self, block_number: u64, spec_version: u32, transaction_version: u32) {
		let mut inner = self.inner.write().expect("no code panics while holding the lock; qed");
		match inner.ranges.last() {
			// The window only grows forward; historical data cannot rewrite it.
			Some(last) if last.from > block_number => return,
			Some(last)
				if (last.spec_version, last.transaction_version) ==
					(spec_version, transaction_version) => {},
			_ => inner
				.ranges
				.push(Range { from: block_number, spec_version, transaction_version }),
		}
		inner.known_up_to = inner.known_up_to.max(block_number);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_cache_answers_nothing() {
		let cache = SpecVersionCache::default();
		assert_eq!(cache.lookup(0), None);
		// Extending an empty cache must not fabricate a window out of nothing.
		assert!(!cache.extend_to(100));
		assert_eq!(cache.lookup(50), None);
	}

	#[test]
	fn covers_only_the_observed_window() {
		let cache = SpecVersionCache::default();
		cache.record(100, 1, 1);
		for block_number in 101..=110 {
			assert!(cache.extend_to(block_number));
		}
		// A jump past the watermark could hide an unobserved upgrade and must be refused.
		assert!(!cache.extend_to(112));
		assert_eq!(cache.lookup(100), Some((1, 1)));
		assert_eq!(cache.lookup(110), Some((1, 1)));
		// Blocks before the first observation predate the window and must resolve on-chain:
		// the versions governing them are unknown.
		assert_eq!(cache.lookup(99), None);
		// Blocks past the watermark could sit behind an unobserved upgrade.
		assert_eq!(cache.lookup(111), None);
	}

	#[test]
	fn upgrade_starts_a_new_range_and_preserves_the_old_one() {
		let cache = SpecVersionCache::default();
		cache.record(100, 1, 1);
		for block_number in 101..=199 {
			cache.extend_to(block_number);
		}
		cache.record(200, 2, 1);
		for block_number in 201..=250 {
			cache.extend_to(block_number);
		}
		// Old blocks keep resolving with the versions they were authored under.
		assert_eq!(cache.lookup(150), Some((1, 1)));
		assert_eq!(cache.lookup(199), Some((1, 1)));
		assert_eq!(cache.lookup(200), Some((2, 1)));
		assert_eq!(cache.lookup(250), Some((2, 1)));
	}

	#[test]
	fn recording_unchanged_versions_extends_instead_of_growing() {
		let cache = SpecVersionCache::default();
		cache.record(100, 1, 1);
		// A gap in the stream is bridged by re-resolving; identical versions must extend the
		// window without adding a range, keeping memory bounded by upgrades.
		cache.record(500, 1, 1);
		assert_eq!(cache.lookup(300), Some((1, 1)));
		let inner = cache.inner.read().unwrap();
		assert_eq!(inner.ranges.len(), 1);
	}

	#[test]
	fn stale_historical_records_cannot_rewrite_the_window() {
		let cache = SpecVersionCache::default();
		cache.record(200, 2, 1);
		cache.record(100, 1, 1);
		assert_eq!(cache.lookup(200), Some((2, 1)));
		assert_eq!(cache.lookup(100), None);
	}
}

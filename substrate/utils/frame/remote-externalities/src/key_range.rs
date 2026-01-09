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

//! Key range management for parallel storage fetching.

use sp_core::storage::StorageKey;
use std::{
	collections::VecDeque,
	sync::{Arc, Mutex},
};

/// Default page size for fetching keys.
pub(crate) const DEFAULT_PAGE_SIZE: u32 = 1000;

/// Represents a range of keys to fetch from the remote node.
#[derive(Debug, Clone)]
pub(crate) struct KeyRange {
	/// The starting key of this range (inclusive for initial ranges, exclusive for continuations).
	pub(crate) start_key: StorageKey,
	/// The ending key of this range (exclusive), or None for open-ended range.
	pub(crate) end_key: Option<StorageKey>,
	/// The common prefix for this range.
	pub(crate) prefix: StorageKey,
	/// Page size for fetching keys in this range (decreases on failure).
	pub(crate) page_size: u32,
	/// If true, start_key was already fetched and should be excluded from results.
	pub(crate) exclude_start_key: bool,
}

impl KeyRange {
	pub(crate) fn new(
		start_key: StorageKey,
		end_key: Option<StorageKey>,
		prefix: StorageKey,
	) -> Self {
		Self { start_key, end_key, prefix, page_size: DEFAULT_PAGE_SIZE, exclude_start_key: false }
	}

	pub(crate) fn continuation(
		start_key: StorageKey,
		end_key: Option<StorageKey>,
		prefix: StorageKey,
	) -> Self {
		Self { start_key, end_key, prefix, page_size: DEFAULT_PAGE_SIZE, exclude_start_key: true }
	}

	/// Returns a new KeyRange with halved page size (minimum 10).
	pub(crate) fn with_halved_page_size(&self) -> Self {
		Self { page_size: (self.page_size / 2).max(10), ..self.clone() }
	}

	/// Filter keys returned from RPC to only include keys within this range.
	///
	/// Returns a tuple of:
	/// - The filtered keys
	/// - Whether this should be considered a "full batch" (triggers subdivision)
	///
	/// A batch is only considered "full" if AFTER filtering it still has keys.
	/// This prevents infinite subdivision when continuation ranges filter out the start_key.
	pub(crate) fn filter_keys(&self, keys: Vec<StorageKey>) -> (Vec<StorageKey>, bool) {
		let rpc_returned_full_batch = keys.len() == self.page_size as usize;

		let filtered: Vec<_> = keys
			.into_iter()
			.filter(|key| {
				// Exclude start_key if this is a continuation range
				if self.exclude_start_key && key == &self.start_key {
					return false;
				}
				// Exclude keys at or beyond end_key
				self.end_key.as_ref().map_or(true, |end| key < end)
			})
			.collect();

		// Only consider it a full batch if we still have keys after filtering
		let is_full_batch = rpc_returned_full_batch && !filtered.is_empty();

		(filtered, is_full_batch)
	}
}

/// Work queue for distributing key ranges to workers.
pub(crate) type WorkQueue = Arc<Mutex<VecDeque<KeyRange>>>;

/// Generate 16 key ranges for parallel fetching by dividing the key space.
///
/// Creates ranges by appending one nibble (4 bits) to the prefix.
/// This gives us: 0x00, 0x10, 0x20, ..., 0xF0
pub(crate) fn gen_key_ranges(prefix: &StorageKey) -> Vec<KeyRange> {
	let prefix_bytes = prefix.as_ref().to_vec();
	let mut ranges = Vec::with_capacity(16);

	for i in 0u8..16u8 {
		let mut start_key = prefix_bytes.clone();
		start_key.push(i << 4); // Shift nibble to upper 4 bits

		let end_key = if i < 15 {
			let mut end = prefix_bytes.clone();
			end.push((i + 1) << 4); // Next nibble
			Some(StorageKey(end))
		} else {
			None
		};

		ranges.push(KeyRange::new(StorageKey(start_key), end_key, prefix.clone()));
	}

	ranges
}

/// Initialize a work queue with ranges for each prefix.
pub(crate) fn initialize_work_queue(prefixes: &[StorageKey]) -> WorkQueue {
	let mut queue = VecDeque::new();

	for prefix in prefixes {
		let ranges = gen_key_ranges(prefix);
		queue.extend(ranges);
	}

	Arc::new(Mutex::new(queue))
}

/// Subdivide the key space AFTER the last_key into up to 16 new ranges.
///
/// Takes the last two keys from a batch to find where they diverge, then creates
/// ranges based on incrementing the nibble at the divergence point.
pub(crate) fn subdivide_remaining_range(
	second_last_key: &StorageKey,
	last_key: &StorageKey,
	end_key: Option<&StorageKey>,
	prefix: &StorageKey,
) -> Vec<KeyRange> {
	let second_last_bytes = second_last_key.as_ref();
	let last_key_bytes = last_key.as_ref();

	// Find the first byte position where the two keys diverge
	let divergence_pos = second_last_bytes
		.iter()
		.zip(last_key_bytes.iter())
		.position(|(a, b)| a != b)
		.unwrap_or(second_last_bytes.len().min(last_key_bytes.len()));

	let mut subdivision_nibble = None;

	for subdivision_pos in divergence_pos..last_key_bytes.len() {
		let byte = last_key_bytes[subdivision_pos];
		let nibble = byte >> 4;

		if nibble < 15 {
			subdivision_nibble = Some((subdivision_pos, nibble));
			break;
		}
	}

	let mut ranges = Vec::new();

	// If we found a position where we can subdivide
	if let Some((pos, current_nibble)) = subdivision_nibble {
		let subdivision_prefix = &last_key_bytes[..pos];

		let mut end = subdivision_prefix.to_vec();
		end.push((current_nibble + 1) << 4);
		// Use continuation() since last_key was already fetched
		ranges.push(KeyRange::continuation(
			last_key.clone(),
			Some(StorageKey(end)),
			prefix.clone(),
		));

		// Create ranges for each nibble from (current_nibble + 1) to 0xF
		for nibble in (current_nibble + 1)..16u8 {
			let mut start = subdivision_prefix.to_vec();
			start.push(nibble << 4);
			let start_key = StorageKey(start);

			// Check if this range starts at or after the end
			if end_key.map_or(false, |ek| start_key >= *ek) {
				break;
			}

			let chunk_end_key = if nibble < 15 {
				let mut end = subdivision_prefix.to_vec();
				end.push((nibble + 1) << 4);
				let computed_end = StorageKey(end);

				Some(match end_key {
					Some(actual_end) if &computed_end > actual_end => actual_end.clone(),
					_ => computed_end,
				})
			} else {
				end_key.cloned()
			};

			// These are fresh ranges, not continuations
			ranges.push(KeyRange::new(start_key, chunk_end_key, prefix.clone()));
		}
	} else if end_key.as_ref().map_or(true, |ek| last_key <= ek) {
		// We are not yet past the end - use continuation since last_key was already fetched
		ranges.push(KeyRange::continuation(last_key.clone(), end_key.cloned(), prefix.clone()));
	}

	ranges
}

#[cfg(test)]
mod tests {
	use super::*;

	fn key(bytes: &[u8]) -> StorageKey {
		StorageKey(bytes.to_vec())
	}

	#[test]
	fn gen_key_ranges_creates_16_non_overlapping_ranges() {
		let prefix = key(&[0xAB, 0xCD]);
		let ranges = gen_key_ranges(&prefix);

		assert_eq!(ranges.len(), 16);

		// Check first range
		assert_eq!(ranges[0].start_key, key(&[0xAB, 0xCD, 0x00]));
		assert_eq!(ranges[0].end_key, Some(key(&[0xAB, 0xCD, 0x10])));
		assert!(!ranges[0].exclude_start_key);

		// Check middle range
		assert_eq!(ranges[8].start_key, key(&[0xAB, 0xCD, 0x80]));
		assert_eq!(ranges[8].end_key, Some(key(&[0xAB, 0xCD, 0x90])));

		// Check last range (open-ended)
		assert_eq!(ranges[15].start_key, key(&[0xAB, 0xCD, 0xF0]));
		assert_eq!(ranges[15].end_key, None);

		// Verify ranges are contiguous (no gaps, no overlaps)
		for i in 0..15 {
			assert_eq!(ranges[i].end_key, Some(ranges[i + 1].start_key.clone()));
		}
	}

	#[test]
	fn gen_key_ranges_with_empty_prefix() {
		let prefix = key(&[]);
		let ranges = gen_key_ranges(&prefix);

		assert_eq!(ranges.len(), 16);
		assert_eq!(ranges[0].start_key, key(&[0x00]));
		assert_eq!(ranges[0].end_key, Some(key(&[0x10])));
		assert_eq!(ranges[15].start_key, key(&[0xF0]));
		assert_eq!(ranges[15].end_key, None);
	}

	#[test]
	fn subdivide_creates_continuation_ranges() {
		let prefix = key(&[0xAB]);
		let second_last = key(&[0xAB, 0x12, 0x34]);
		let last = key(&[0xAB, 0x12, 0x35]);

		let ranges = subdivide_remaining_range(&second_last, &last, None, &prefix);

		// First range should be a continuation (exclude_start_key = true)
		assert!(!ranges.is_empty());
		assert!(ranges[0].exclude_start_key, "First subdivided range should exclude start_key");
		assert_eq!(ranges[0].start_key, last);
	}

	#[test]
	fn subdivide_respects_end_key() {
		let prefix = key(&[0xAB]);
		let second_last = key(&[0xAB, 0x12, 0x34]);
		let last = key(&[0xAB, 0x12, 0x35]);
		let end = key(&[0xAB, 0x20]); // End before most subdivided ranges

		let ranges = subdivide_remaining_range(&second_last, &last, Some(&end), &prefix);

		// All ranges should respect end_key
		for range in &ranges {
			if let Some(range_end) = &range.end_key {
				assert!(range_end <= &end, "Range end should not exceed overall end_key");
			}
			assert!(range.start_key < end, "Range start should be before end_key");
		}
	}

	#[test]
	fn subdivide_when_last_key_at_nibble_boundary() {
		let prefix = key(&[0xAB]);
		let second_last = key(&[0xAB, 0x1F, 0xFF]);
		let last = key(&[0xAB, 0x20, 0x00]);

		let ranges = subdivide_remaining_range(&second_last, &last, None, &prefix);

		assert!(!ranges.is_empty());
		// First range starts from last_key and is a continuation
		assert_eq!(ranges[0].start_key, last);
		assert!(ranges[0].exclude_start_key);
	}

	#[test]
	fn subdivide_clamps_ranges_to_end_key() {
		let prefix = key(&[0xAB]);
		let second_last = key(&[0xAB, 0x12, 0x34]);
		let last = key(&[0xAB, 0x12, 0x50]);
		let end = key(&[0xAB, 0x12, 0x60]); // End within the subdivision range

		let ranges = subdivide_remaining_range(&second_last, &last, Some(&end), &prefix);

		// Should have created some ranges, but they should respect end_key
		assert!(!ranges.is_empty());

		// Check that ranges don't start at or past end_key
		for range in &ranges {
			assert!(
				range.start_key < end,
				"Range start {:?} should be before end {:?}",
				range.start_key,
				end
			);
		}
	}

	#[test]
	fn key_range_with_halved_page_size_preserves_exclude_flag() {
		let range = KeyRange::continuation(key(&[0x00]), Some(key(&[0x10])), key(&[]));

		assert!(range.exclude_start_key);
		assert_eq!(range.page_size, DEFAULT_PAGE_SIZE);

		let halved = range.with_halved_page_size();

		assert!(halved.exclude_start_key, "exclude_start_key should be preserved");
		assert_eq!(halved.page_size, DEFAULT_PAGE_SIZE / 2);
		assert_eq!(halved.start_key, range.start_key);
		assert_eq!(halved.end_key, range.end_key);
	}

	#[test]
	fn key_range_new_vs_continuation() {
		let start = key(&[0x00]);
		let end = Some(key(&[0x10]));
		let prefix = key(&[]);

		let new_range = KeyRange::new(start.clone(), end.clone(), prefix.clone());
		let cont_range = KeyRange::continuation(start.clone(), end.clone(), prefix.clone());

		assert!(!new_range.exclude_start_key, "new() should not exclude start_key");
		assert!(cont_range.exclude_start_key, "continuation() should exclude start_key");

		// Everything else should be the same
		assert_eq!(new_range.start_key, cont_range.start_key);
		assert_eq!(new_range.end_key, cont_range.end_key);
		assert_eq!(new_range.prefix, cont_range.prefix);
		assert_eq!(new_range.page_size, cont_range.page_size);
	}

	#[test]
	fn filter_keys_excludes_start_key_for_continuation() {
		let start = key(&[0xAB, 0x12, 0x34]);
		let end = key(&[0xAB, 0x20]);
		let prefix = key(&[0xAB]);

		let range = KeyRange::continuation(start.clone(), Some(end.clone()), prefix);

		// RPC returns keys including the start_key (which was already fetched)
		let keys = vec![
			start.clone(),
			key(&[0xAB, 0x12, 0x35]),
			key(&[0xAB, 0x12, 0x40]),
		];

		let (filtered, is_full_batch) = range.filter_keys(keys);

		// start_key should be excluded
		assert!(!filtered.contains(&start), "start_key should be filtered out");
		assert_eq!(filtered.len(), 2);
		// Not a full batch since we didn't get page_size keys
		assert!(!is_full_batch);
	}

	#[test]
	fn filter_keys_prevents_infinite_subdivision_on_empty_result() {
		// This test verifies the bug fix: when a continuation range returns only
		// the start_key (which gets filtered out), is_full_batch must be false
		// to prevent infinite subdivision loops.
		let start = key(&[0xAB, 0x12, 0x34]);
		let prefix = key(&[0xAB]);

		let mut range = KeyRange::continuation(start.clone(), None, prefix);
		range.page_size = 1; // Simulate a range with page_size=1

		// RPC returns exactly page_size keys (a "full batch"), but it's only the start_key
		let keys = vec![start.clone()];

		let (filtered, is_full_batch) = range.filter_keys(keys);

		// After filtering, the page is empty
		assert!(filtered.is_empty(), "start_key should be filtered out");
		// CRITICAL: is_full_batch must be false to prevent infinite subdivision
		assert!(
			!is_full_batch,
			"is_full_batch should be false when filtered result is empty"
		);
	}

	#[test]
	fn filter_keys_excludes_keys_beyond_end() {
		let start = key(&[0xAB, 0x10]);
		let end = key(&[0xAB, 0x20]);
		let prefix = key(&[0xAB]);

		let range = KeyRange::new(start.clone(), Some(end.clone()), prefix);

		let keys = vec![
			key(&[0xAB, 0x10]),
			key(&[0xAB, 0x15]),
			key(&[0xAB, 0x20]), // At end - should be excluded
			key(&[0xAB, 0x25]), // Beyond end - should be excluded
		];

		let (filtered, _) = range.filter_keys(keys);

		assert_eq!(filtered.len(), 2);
		assert!(!filtered.contains(&end), "keys at end should be excluded");
		assert!(
			!filtered.contains(&key(&[0xAB, 0x25])),
			"keys beyond end should be excluded"
		);
	}

	#[test]
	fn filter_keys_reports_full_batch_correctly() {
		let start = key(&[0x00]);
		let prefix = key(&[]);

		let mut range = KeyRange::new(start, None, prefix);
		range.page_size = 3;

		// Exactly page_size keys, all valid
		let keys = vec![key(&[0x00]), key(&[0x01]), key(&[0x02])];

		let (filtered, is_full_batch) = range.filter_keys(keys);

		assert_eq!(filtered.len(), 3);
		assert!(is_full_batch, "should be a full batch when page_size keys returned");
	}
}

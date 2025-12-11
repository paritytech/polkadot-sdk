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
	/// The starting key of this range (inclusive).
	pub(crate) start_key: StorageKey,
	/// The ending key of this range (exclusive), or None for open-ended range.
	pub(crate) end_key: Option<StorageKey>,
	/// The common prefix for this range.
	pub(crate) prefix: StorageKey,
	/// Page size for fetching keys in this range (decreases on failure).
	pub(crate) page_size: u32,
}

impl KeyRange {
	pub(crate) fn new(
		start_key: StorageKey,
		end_key: Option<StorageKey>,
		prefix: StorageKey,
	) -> Self {
		Self { start_key, end_key, prefix, page_size: DEFAULT_PAGE_SIZE }
	}

	/// Returns a new KeyRange with halved page size (minimum 10).
	pub(crate) fn with_halved_page_size(&self) -> Self {
		Self { page_size: (self.page_size / 2).max(10), ..self.clone() }
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
		ranges.push(KeyRange::new(last_key.clone(), Some(StorageKey(end)), prefix.clone()));

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

			ranges.push(KeyRange::new(start_key, chunk_end_key, prefix.clone()));
		}
	} else if end_key.as_ref().map_or(true, |ek| last_key <= ek) {
		// We are not yet past the end
		ranges.push(KeyRange::new(last_key.clone(), end_key.cloned(), prefix.clone()));
	}

	ranges
}

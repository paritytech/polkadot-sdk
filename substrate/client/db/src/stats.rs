// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Database usage statistics

use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

/// Accumulated usage statistics for state queries.
pub struct StateUsageStats {
	started: std::time::Instant,
	reads: AtomicU64,
	bytes_read: AtomicU64,
	writes: AtomicU64,
	bytes_written: AtomicU64,
	reads_cache: AtomicU64,
	bytes_read_cache: AtomicU64,
}

impl StateUsageStats {
	/// New empty usage stats.
	pub fn new() -> Self {
		Self {
			started: std::time::Instant::now(),
			reads: 0.into(),
			bytes_read: 0.into(),
			writes: 0.into(),
			bytes_written: 0.into(),
			reads_cache: 0.into(),
			bytes_read_cache: 0.into(),
		}
	}

	/// Tally one read operation, of some length.
	pub fn tally_read(&self, data_bytes: u64, cache: bool) {
		self.reads.fetch_add(1, AtomicOrdering::Relaxed);
		self.bytes_read.fetch_add(data_bytes, AtomicOrdering::Relaxed);
		if cache {
			self.reads_cache.fetch_add(1, AtomicOrdering::Relaxed);
			self.bytes_read_cache.fetch_add(data_bytes, AtomicOrdering::Relaxed);
		}
	}

	/// Tally one key read.
	pub fn tally_key_read(&self, key: &[u8], val: Option<&Vec<u8>>, cache: bool) {
		self.tally_read(key.len() as u64 + val.as_ref().map(|x| x.len() as u64).unwrap_or(0), cache);
	}

	/// Tally one child key read.
	pub fn tally_child_key_read(
		&self,
		key: &(Vec<u8>, Vec<u8>),
		val: Option<Vec<u8>>,
		cache: bool,
	) -> Option<Vec<u8>> {
		let bytes = key.0.len() + key.1.len() + val.as_ref().map(|x| x.len()).unwrap_or(0);
		self.tally_read(bytes as u64, cache);
		val
	}

	/// Tally some write operations, including their byte count.
	pub fn tally_writes(&self, ops: u64, data_bytes: u64) {
		self.writes.fetch_add(ops, AtomicOrdering::Relaxed);
		self.bytes_written.fetch_add(data_bytes, AtomicOrdering::Relaxed);
	}

	/// Merge state machine usage info.
	pub fn merge_sm(&self, info: sp_state_machine::UsageInfo) {
		self.reads.fetch_add(info.reads.ops, AtomicOrdering::Relaxed);
		self.bytes_read.fetch_add(info.reads.bytes, AtomicOrdering::Relaxed);
		self.writes.fetch_add(info.writes.ops, AtomicOrdering::Relaxed);
		self.bytes_written.fetch_add(info.writes.bytes, AtomicOrdering::Relaxed);
		self.reads_cache.fetch_add(info.cache_reads.ops, AtomicOrdering::Relaxed);
		self.bytes_read_cache.fetch_add(info.cache_reads.bytes, AtomicOrdering::Relaxed);
	}

	/// Returns the collected `UsageInfo` and resets the internal state.
	pub fn take(&self) -> sp_state_machine::UsageInfo {
		use sp_state_machine::UsageUnit;

		fn unit(ops: &AtomicU64, bytes: &AtomicU64) -> UsageUnit {
			UsageUnit {
				ops: ops.swap(0, AtomicOrdering::Relaxed),
				bytes: bytes.swap(0, AtomicOrdering::Relaxed),
			}
		}

		sp_state_machine::UsageInfo {
			reads: unit(&self.reads, &self.bytes_read),
			writes: unit(&self.writes, &self.bytes_written),
			cache_reads: unit(&self.reads_cache, &self.bytes_read_cache),
			// TODO: Proper tracking state of memory footprint here requires
			//       imposing `MallocSizeOf` requirement on half of the codebase,
			//       so it is an open question how to do it better
			memory: 0,
			started: self.started,
			span: self.started.elapsed(),
		}
	}
}

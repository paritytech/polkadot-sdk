// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use parity_scale_codec::{Decode, Encode};
use std::path::PathBuf;

/// Result from prepare worker if successful.
#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct PrepareWorkerSuccess {
	/// Checksum of the compiled PVF.
	pub checksum: String,
	/// Stats of the current preparation run.
	pub stats: PrepareStats,
}

/// Result of PVF preparation if successful.
#[derive(Debug, Clone, Default)]
pub struct PrepareSuccess {
	/// Canonical path to the compiled artifact.
	pub path: PathBuf,
	/// Stats of the current preparation run.
	pub stats: PrepareStats,
}

/// Preparation statistics, including the CPU time and memory taken.
#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct PrepareStats {
	/// The CPU time that elapsed for the preparation job.
	pub cpu_time_elapsed: std::time::Duration,
	/// The observed memory statistics for the preparation job.
	pub memory_stats: MemoryStats,
}

/// Helper struct to contain all the memory stats, including `MemoryAllocationStats` and, if
/// supported by the OS, `ru_maxrss`.
#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct MemoryStats {
	/// Memory stats from `tikv_jemalloc_ctl`, polling-based and not very precise.
	#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
	pub memory_tracker_stats: Option<MemoryAllocationStats>,
	/// `ru_maxrss` from `getrusage`. `None` if an error occurred.
	#[cfg(target_os = "linux")]
	pub max_rss: Option<i64>,
	/// Peak allocation in bytes measured by tracking allocator
	pub peak_tracked_alloc: u64,
}

/// Statistics of collected memory metrics.
#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct MemoryAllocationStats {
	/// Total resident memory, in bytes.
	pub resident: u64,
	/// Total allocated memory, in bytes.
	pub allocated: u64,
}

/// The kind of prepare job.
#[derive(Copy, Clone, Debug, Encode, Decode)]
pub enum PrepareJobKind {
	/// Compilation triggered by a candidate validation request.
	Compilation,
	/// A prechecking job.
	Prechecking,
}

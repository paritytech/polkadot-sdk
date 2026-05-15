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

//! Per-transaction cold/warm access list for storage opcodes.
//!
//! Scoped-down version targeting only SLOAD/SSTORE and the storage precompile
//! (clearStorage / containsStorage / getStorage / takeStorage). Tracks
//! per-`(address, slot)` cold/warm state with per-frame rollback semantics,
//! mirroring EIP-2929 for the storage side only.
//!
//! Data layout follows [`crate::transient_storage::TransientStorage`]: a current-
//! state `BTreeSet`, a flat journal of insertions, and a stack of journal-index
//! checkpoints marking frame boundaries.

use alloc::{collections::BTreeSet, vec::Vec};
use sp_core::H160;

/// One entry per `(contract address, storage slot)` accessed in the current tx.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub struct AccessEntry {
	/// Contract whose child trie is being touched.
	pub address: H160,
	/// Raw 32-byte EVM slot key (substrate hashes it internally for the child
	/// trie lookup).
	pub slot: [u8; 32],
}

/// Per-transaction access list with per-frame rollback support.
///
/// Two variants per design §8.2: `Enabled` does full tracking; `Disabled` is
/// a zero-state no-op selected when `T::ColdWarmPricingEnabled = false`.
pub enum AccessList {
	/// Full tracking — used when cold/warm pricing is enabled.
	Enabled {
		/// All currently-warm entries.
		accessed: BTreeSet<AccessEntry>,
		/// Flat journal of insertions (in order). Each entry was added by exactly
		/// one frame; `checkpoints` marks the frame boundaries inside this `Vec`.
		journal: Vec<AccessEntry>,
		/// Stack of journal indices. `checkpoints.last()` is the index at which
		/// the current frame started inserting; rolling back means draining
		/// `journal` from that index and removing those entries from `accessed`.
		checkpoints: Vec<usize>,
	},
	/// No-op variant — used when cold/warm pricing is disabled at runtime.
	///
	/// Every [`touch`](Self::touch) call returns `true` (cold) but performs no
	/// work. Frame hooks are no-ops. Weight dispatch in `RuntimeCosts::weight`
	/// routes to the legacy `seal_*` weights — bit-identical to pre-change.
	Disabled,
}

impl Default for AccessList {
	/// Match the runtime default: `T::ColdWarmPricingEnabled = false` ⇒
	/// `Disabled`. A `Default::default()` caller that didn't read the flag
	/// silently inherits the zero-state path instead of accidentally
	/// opting in.
	fn default() -> Self {
		Self::Disabled
	}
}

impl AccessList {
	/// Initialize for a new transaction with cold/warm tracking enabled.
	///
	/// No pre-warmed entries — first-touch on any entry is always cold. No
	/// initial checkpoint; the outermost `Stack::run()` does NOT call
	/// `enter_frame`, so touches at the outermost call land directly in the
	/// bare journal and persist for the whole transaction.
	pub fn new_enabled() -> Self {
		Self::Enabled {
			accessed: BTreeSet::new(),
			journal: Vec::new(),
			checkpoints: Vec::new(),
		}
	}

	/// Initialize the no-op variant. Used when cold/warm pricing is disabled.
	pub fn new_disabled() -> Self {
		Self::Disabled
	}

	/// Open a new frame (called on nested CALL/CREATE, not on the outermost
	/// run). O(1) when enabled, no-op when disabled.
	pub fn enter_frame(&mut self) {
		if let Self::Enabled { journal, checkpoints, .. } = self {
			checkpoints.push(journal.len());
		}
	}

	/// Commit the top frame: its journal entries stay (they may still be rolled
	/// back if a parent frame later reverts). O(1) when enabled, no-op when
	/// disabled.
	pub fn commit_frame(&mut self) {
		if let Self::Enabled { checkpoints, .. } = self {
			checkpoints.pop().expect("frame open; qed");
		}
	}

	/// Rollback the top frame: drain its journal entries and remove them from
	/// `accessed`. O(n) in the number of entries the frame inserted. No-op when
	/// disabled.
	pub fn rollback_frame(&mut self) {
		if let Self::Enabled { accessed, journal, checkpoints } = self {
			let checkpoint = checkpoints.pop().expect("frame open; qed");
			for entry in journal.drain(checkpoint..) {
				accessed.remove(&entry);
			}
		}
	}

	/// Register the entry and return `true` if this access is cold (newly
	/// inserted), `false` if it was already warm. The `Disabled` variant always
	/// returns `true` without recording state.
	///
	/// Warm path takes no clone — only a `contains` lookup. Cold path pays one
	/// clone (unavoidable: the entry must live in both `accessed` and `journal`).
	pub fn touch(&mut self, entry: AccessEntry) -> bool {
		match self {
			Self::Disabled => true,
			Self::Enabled { accessed, journal, .. } => {
				if accessed.contains(&entry) {
					return false;
				}
				accessed.insert(entry.clone());
				journal.push(entry);
				true
			},
		}
	}

	/// Check warmth without registering (testing / introspection). Always
	/// returns `false` on the `Disabled` variant.
	#[cfg(test)]
	pub fn is_warm(&self, entry: &AccessEntry) -> bool {
		match self {
			Self::Disabled => false,
			Self::Enabled { accessed, .. } => accessed.contains(entry),
		}
	}

	/// Returns the current number of warm entries (testing / metrics).
	#[cfg(test)]
	pub fn len(&self) -> usize {
		match self {
			Self::Disabled => 0,
			Self::Enabled { accessed, .. } => accessed.len(),
		}
	}

	/// Returns the current frame depth (number of open checkpoints).
	#[cfg(test)]
	pub fn frame_depth(&self) -> usize {
		match self {
			Self::Disabled => 0,
			Self::Enabled { checkpoints, .. } => checkpoints.len(),
		}
	}
}

/// Cost struct returned by `Ext::get_storage` recording the cold/warm state of
/// the slot access. `None` indicates the read did not happen on this call
/// path (zero charge for the read part).
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetStorageReadCosts {
	pub is_cold: Option<bool>,
}

/// Same as [`GetStorageReadCosts`] but for `Ext::set_storage` — the implicit
/// read-of-old-value that the SSTORE charge model accounts for.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetStorageReadCosts {
	pub is_cold: Option<bool>,
}

// ===========================================================================
// Cold/warm weight helpers.
//
// `cost_read::<T>` and the per-opcode `*_weight<T>` helpers compose into a
// new pricing model that the `Token<T>::weight` dispatcher picks when
// `T::ColdWarmPricingEnabled` is true.
//
// The three leaf functions derive approximations from existing `WeightInfo`
// benchmarks. Dedicated benchmarks (`access_list_touch`, `storage_read_cold`,
// `storage_read_warm`) are a follow-up task and will replace these
// derivations one-for-one without touching call sites.
// ===========================================================================

use frame_support::weights::Weight;

use crate::{Config, weights::WeightInfo};

/// Per-touch access-list bookkeeping cost (one `BTreeSet` insert + one `Vec`
/// push). Approximation: 1/8 of `seal_caller` (a known-cheap host fn) since
/// the operation is in-memory only.
fn access_list_touch_weight<T: Config>() -> Weight {
	T::WeightInfo::seal_caller().saturating_div(8)
}

/// Cost of a substrate-key cold read of `value_size` bytes. Approximation:
/// the full `seal_get_storage(value_size)` benchmark.
fn storage_read_cold_weight<T: Config>(value_size: u32) -> Weight {
	T::WeightInfo::seal_get_storage(value_size)
}

/// Cost of a substrate-key warm read of `value_size` bytes. Warm reads dedup
/// at the storage proof recorder, so no substrate I/O actually happens —
/// only the in-memory access-set check. Approximation: 1/20 of
/// `seal_get_storage(0)` (~5%, a rough EIP-2929 cold/warm ratio).
fn storage_read_warm_weight<T: Config>(value_size: u32) -> Weight {
	let _ = value_size;
	T::WeightInfo::seal_get_storage(0).saturating_div(20)
}

/// Weight charged for one observed substrate read at this opcode:
/// - `None` → the read didn't happen; charge nothing.
/// - `Some(true)` → cold: touch bookkeeping + full read.
/// - `Some(false)` → warm: touch bookkeeping + in-memory dedup.
pub fn cost_read<T: Config>(is_cold: Option<bool>, value_size: u32) -> Weight {
	match is_cold {
		None => Weight::zero(),
		Some(true) =>
			access_list_touch_weight::<T>().saturating_add(storage_read_cold_weight::<T>(value_size)),
		Some(false) =>
			access_list_touch_weight::<T>().saturating_add(storage_read_warm_weight::<T>(value_size)),
	}
}

pub fn get_storage_weight<T: Config>(len: u32, costs: &GetStorageReadCosts) -> Weight {
	cost_read::<T>(costs.is_cold, len)
}

pub fn set_storage_weight<T: Config>(
	old_bytes: u32,
	_new_bytes: u32,
	costs: &SetStorageReadCosts,
) -> Weight {
	// SSTORE's read-of-old-value is what cold/warm applies to; the write
	// surcharge is independent of cold/warm and stays in the legacy mapping.
	cost_read::<T>(costs.is_cold, old_bytes)
}

// ===========================================================================
// Unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
	use super::*;

	fn entry(slot_byte: u8) -> AccessEntry {
		AccessEntry { address: H160::zero(), slot: [slot_byte; 32] }
	}

	#[test]
	fn first_touch_is_cold() {
		let mut al = AccessList::new_enabled();
		assert!(al.touch(entry(1)));
		assert_eq!(al.len(), 1);
	}

	#[test]
	fn second_touch_is_warm() {
		let mut al = AccessList::new_enabled();
		assert!(al.touch(entry(1)));
		assert!(!al.touch(entry(1)));
		assert_eq!(al.len(), 1);
	}

	#[test]
	fn distinct_entries_are_independent() {
		let mut al = AccessList::new_enabled();
		assert!(al.touch(entry(1)));
		assert!(al.touch(entry(2)));
		assert_eq!(al.len(), 2);
	}

	#[test]
	fn rollback_removes_frame_entries() {
		let mut al = AccessList::new_enabled();
		al.touch(entry(1));
		al.enter_frame();
		al.touch(entry(2));
		al.touch(entry(3));
		assert_eq!(al.len(), 3);
		al.rollback_frame();
		assert_eq!(al.len(), 1);
		assert!(al.is_warm(&entry(1)));
		assert!(!al.is_warm(&entry(2)));
		assert!(!al.is_warm(&entry(3)));
	}

	#[test]
	fn commit_keeps_frame_entries() {
		let mut al = AccessList::new_enabled();
		al.touch(entry(1));
		al.enter_frame();
		al.touch(entry(2));
		al.commit_frame();
		assert_eq!(al.len(), 2);
		assert!(al.is_warm(&entry(1)));
		assert!(al.is_warm(&entry(2)));
	}

	#[test]
	fn rollback_keeps_entries_warmed_by_parent() {
		// Parent warms entry A. Child also touches A (warm hit, not journaled
		// in child frame). Child rolls back. A must remain warm.
		let mut al = AccessList::new_enabled();
		al.touch(entry(1));
		al.enter_frame();
		let child_cold = al.touch(entry(1));
		assert!(!child_cold, "child sees A as warm");
		al.rollback_frame();
		assert!(al.is_warm(&entry(1)), "parent's warming of A must survive child revert");
	}

	#[test]
	fn nested_commit_then_rollback() {
		let mut al = AccessList::new_enabled();
		al.enter_frame();
		al.touch(entry(1));
		al.enter_frame();
		al.touch(entry(2));
		al.commit_frame(); // commit inner; entry(2) still in journal of outer
		assert_eq!(al.frame_depth(), 1);
		al.rollback_frame(); // rollback outer; both gone
		assert_eq!(al.len(), 0);
	}

	#[test]
	fn disabled_touch_always_returns_cold() {
		let mut al = AccessList::new_disabled();
		assert!(al.touch(entry(1)));
		assert!(al.touch(entry(1)));
		assert_eq!(al.len(), 0);
		assert!(!al.is_warm(&entry(1)));
	}

	#[test]
	fn disabled_frame_hooks_are_noops() {
		let mut al = AccessList::new_disabled();
		al.enter_frame();
		al.commit_frame();
		al.rollback_frame();
		assert_eq!(al.frame_depth(), 0);
	}

	#[test]
	#[should_panic(expected = "frame open; qed")]
	fn rollback_without_frame_panics() {
		let mut al = AccessList::new_enabled();
		al.rollback_frame();
	}

	#[test]
	#[should_panic(expected = "frame open; qed")]
	fn commit_without_frame_panics() {
		let mut al = AccessList::new_enabled();
		al.commit_frame();
	}
}

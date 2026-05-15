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
//! TODO: the per-frame rollback machinery here (flat journal + checkpoint
//! stack, with `enter_frame` / `commit_frame` / `rollback_frame` wired into
//! `Stack::run`) duplicates [`crate::transient_storage::TransientStorage`].
//! Factor the shared layout into a generic helper (e.g. `Journaled<T>`) and
//! have both `TransientStorage` and `AccessList` depend on it.

use alloc::{collections::BTreeSet, vec::Vec};
use frame_support::weights::Weight;
use sp_core::H160;

use crate::{Config, weights::WeightInfo};

/// One entry per `(contract address, storage slot)` accessed in the current tx.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub struct AccessEntry {
	/// Contract whose child trie is being touched.
	pub address: H160,
	/// 32-byte slot identifier, projected from a `Key` via [`crate::exec::Key::to_slot`].
	pub slot: [u8; 32],
}

/// Per-transaction access list with per-frame rollback support. `Disabled`
/// is a zero-state no-op when `T::ColdWarmPricingEnabled = false`.
pub enum AccessList {
	/// Full tracking — used when cold/warm pricing is enabled. Layout
	/// follows [`crate::transient_storage::TransientStorage`]: a current-state
	/// set, a flat journal of insertions, and journal-index checkpoints.
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
	Disabled,
}

/// On the `Disabled` variant, `enter_frame` / `commit_frame` / `rollback_frame`
/// are no-ops and `touch` always returns `true` (cold) without recording state.
/// All methods below describe the behavior on the `Enabled` variant.
impl AccessList {
	/// Initialize for a new transaction with cold/warm tracking enabled.
	///
	/// First-touch on any entry is always cold. No initial checkpoint is
	/// opened — root-scope touches survive the whole transaction.
	pub fn new_enabled() -> Self {
		Self::Enabled { accessed: BTreeSet::new(), journal: Vec::new(), checkpoints: Vec::new() }
	}

	/// Initialize the no-op variant. Used when cold/warm pricing is disabled.
	pub fn new_disabled() -> Self {
		Self::Disabled
	}

	/// Open a new nested frame.
	///
	/// This allows to either commit or roll back all touches that are made
	/// after this call. For every `enter_frame` there must be a matching call
	/// to either `commit_frame` or `rollback_frame`.
	pub fn enter_frame(&mut self) {
		if let Self::Enabled { journal, checkpoints, .. } = self {
			checkpoints.push(journal.len());
		}
	}

	/// Commit the top frame.
	///
	/// Touches made during that frame stay, but may still be rolled back if a
	/// parent frame later reverts.
	///
	/// # Panics
	///
	/// Will panic if there is no open frame.
	pub fn commit_frame(&mut self) {
		if let Self::Enabled { checkpoints, .. } = self {
			checkpoints.pop().expect("frame open; qed");
		}
	}

	/// Rollback the top frame.
	///
	/// Touches made during that frame are removed from the access list.
	///
	/// # Panics
	///
	/// Will panic if there is no open frame.
	pub fn rollback_frame(&mut self) {
		if let Self::Enabled { accessed, journal, checkpoints } = self {
			let checkpoint = checkpoints.pop().expect("frame open; qed");
			for entry in journal.drain(checkpoint..) {
				accessed.remove(&entry);
			}
		}
	}

	/// Register the entry and return `true` if this access is cold (newly
	/// inserted), `false` if it was already warm.
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

/// Cold/warm state of a substrate read seen by an `Ext::*storage` call.
/// `None` = the read didn't happen on this call path.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageAccessCost {
	pub is_cold: Option<bool>,
}

impl StorageAccessCost {
	/// Worst-case marker used for pre-charging before an `Ext::*storage` call:
	/// assume the access is cold, then `adjust_weight` refunds the difference
	/// once the actual cold/warm status is known.
	pub const fn cold() -> Self {
		Self { is_cold: Some(true) }
	}
}

// Cold/warm weight helpers.
//
// TODO: replace these approximations with dedicated benchmarks
// (`access_list_touch`, `storage_read_cold`, `storage_read_warm`).

/// Cold touch bookkeeping: BTreeSet `contains` + `insert` (with clone) + Vec
/// `push`. Approximated as 1/8 of `seal_caller`.
fn access_list_touch_cold_weight<T: Config>() -> Weight {
	T::WeightInfo::seal_caller().saturating_div(8)
}

/// Warm touch bookkeeping: BTreeSet `contains` only. ~3× cheaper than the cold
/// path; approximated as 1/24 of `seal_caller`.
fn access_list_touch_warm_weight<T: Config>() -> Weight {
	T::WeightInfo::seal_caller().saturating_div(24)
}

/// Cold substrate read of `value_size` bytes. Approximated as full `seal_get_storage(value_size)`.
fn storage_read_cold_weight<T: Config>(value_size: u32) -> Weight {
	T::WeightInfo::seal_get_storage(value_size)
}

/// Warm substrate read. Approximated as 1/5 of `seal_get_storage(value_size)`.
fn storage_read_warm_weight<T: Config>(value_size: u32) -> Weight {
	T::WeightInfo::seal_get_storage(value_size).saturating_div(5)
}

/// Weight charged for one observed substrate read at this opcode:
/// - `None` → the read didn't happen; charge nothing.
/// - `Some(true)` → cold: cold touch bookkeeping + full read.
/// - `Some(false)` → warm: warm touch bookkeeping + in-memory dedup.
pub fn cost_read<T: Config>(is_cold: Option<bool>, value_size: u32) -> Weight {
	match is_cold {
		None => Weight::zero(),
		Some(true) => access_list_touch_cold_weight::<T>()
			.saturating_add(storage_read_cold_weight::<T>(value_size)),
		Some(false) => access_list_touch_warm_weight::<T>()
			.saturating_add(storage_read_warm_weight::<T>(value_size)),
	}
}

// ===========================================================================
// Unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
	use super::*;

	/// Full lifecycle: root scope + two nested frames, one commits, one reverts.
	#[test]
	fn lifecycle() {
		let mut al = AccessList::new_enabled();
		let (a, b, c, d) = (
			AccessEntry { address: H160::zero(), slot: [0xA; 32] },
			AccessEntry { address: H160::zero(), slot: [0xB; 32] },
			AccessEntry { address: H160::zero(), slot: [0xC; 32] },
			AccessEntry { address: H160::zero(), slot: [0xD; 32] },
		);

		assert!(al.touch(a.clone()), "A: first touch cold");
		assert!(!al.touch(a.clone()), "A: second touch warm");

		al.enter_frame();
		assert_eq!(al.frame_depth(), 1);

		assert!(al.touch(b.clone()), "B in F1: cold");
		assert!(!al.touch(a.clone()), "A in F1: warm via parent");

		al.enter_frame();
		assert!(al.touch(c.clone()), "C in F2: cold");

		al.commit_frame();
		assert_eq!(al.frame_depth(), 1);
		assert!(al.is_warm(&c), "C: survives F2 commit");

		assert!(al.touch(d.clone()), "D in F1: cold");
		assert_eq!(al.len(), 4);

		al.rollback_frame();
		assert_eq!(al.frame_depth(), 0);
		assert_eq!(al.len(), 1);
		assert!(al.is_warm(&a), "A: root scope, survives F1 revert");
		assert!(!al.is_warm(&b), "B: inserted by F1, rolled back");
		assert!(!al.is_warm(&c), "C: F2-committed-into-F1, gone when F1 reverts");
		assert!(!al.is_warm(&d), "D: inserted by F1, rolled back");
	}
}

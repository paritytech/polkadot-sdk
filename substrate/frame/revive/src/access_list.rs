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

//! Per-transaction cold/hot access list.
//!
//! TODO: the per-frame rollback machinery here (flat journal + checkpoint
//! stack, with `enter_frame` / `commit_frame` / `rollback_frame` wired into
//! `Stack::run`) duplicates [`crate::transient_storage::TransientStorage`].
//! Factor the shared layout into a generic helper (e.g. `Journaled<T>`) and
//! have both `TransientStorage` and `AccessList` depend on it.

use alloc::{collections::BTreeSet, vec::Vec};
use frame_support::BoundedVec;
use sp_core::{ConstU32, H160};

use crate::limits;

/// Inline-storage cap for `Slot::VarInline`. Sized to fit SCALE-encoded
/// `Address`, `H256`, `AccountId32`, etc., with or without a 4-byte storage
/// prefix; longer keys fall through to `Slot::VarLong`.
pub const MAX_INLINE_KEY_LEN: usize = 36;

/// Storage slot identifier for an access-list entry.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub enum Slot {
	/// Fixed 32-byte storage key.
	Fix([u8; 32]),
	/// Variable-length key up to [`MAX_INLINE_KEY_LEN`].
	VarInline { bytes: [u8; MAX_INLINE_KEY_LEN], len: u8 },
	/// Variable-length key longer than [`MAX_INLINE_KEY_LEN`], up to
	/// `limits::STORAGE_KEY_BYTES`.
	VarLong(BoundedVec<u8, ConstU32<{ limits::STORAGE_KEY_BYTES }>>),
}

/// Classification of a storage access for pricing.
#[derive(Clone, Copy, Debug)]
pub enum StorageAccessKind {
	/// Persistent storage, first access in this transaction.
	PersistentCold,
	/// Persistent storage, slot already in the access list.
	PersistentHot,
	/// Transient storage, not tracked by the access list.
	Transient,
}

impl StorageAccessKind {
	/// Classify a storage access for pricing.
	pub fn for_access(transient: bool, cold_check: impl FnOnce() -> bool) -> Self {
		if transient {
			Self::Transient
		} else if cold_check() {
			Self::PersistentCold
		} else {
			Self::PersistentHot
		}
	}
}

/// Snapshot of per-transaction access-list counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessListMetrics {
	/// Currently-hot entries (across all open frames).
	pub size: usize,
	/// Total cold touches across the transaction, including ones later rolled back.
	pub cold: u32,
	/// Total hot touches across the transaction, including ones later rolled back.
	pub hot: u32,
}

/// One entry per `(contract address, storage slot)` accessed in the current tx.
///
/// Field order is `slot, address` so the derived `Ord` decides on `slot`
/// first, the most-discriminating field in the typical access pattern (one
/// contract touching many slots within a transaction).
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub struct AccessEntry {
	/// Slot identifier.
	pub slot: Slot,
	/// Contract whose child trie is being touched.
	pub address: H160,
}

/// Per-transaction access list with per-frame rollback support. Layout
/// follows [`crate::transient_storage::TransientStorage`]: a current-state
/// set, a flat journal of insertions, and journal-index checkpoints.
pub struct AccessList {
	/// All currently-hot entries.
	accessed: BTreeSet<AccessEntry>,
	/// Flat journal of insertions (in order). Each entry was added by exactly
	/// one frame; `checkpoints` marks the frame boundaries inside this `Vec`.
	journal: Vec<AccessEntry>,
	/// Stack of journal indices. `checkpoints.last()` is the index at which
	/// the current frame started inserting; rolling back means draining
	/// `journal` from that index and removing those entries from `accessed`.
	checkpoints: Vec<usize>,
	/// Total cold touches across the transaction. Includes touches in
	/// frames that later rolled back.
	cold_count: u32,
	/// Total hot touches across the transaction. Includes touches in
	/// frames that later rolled back.
	hot_count: u32,
}

impl AccessList {
	/// Initialize for a new transaction.
	///
	/// The first touch on any entry is cold. No initial checkpoint is
	/// opened; first-frame touches survive the whole transaction.
	pub fn new() -> Self {
		Self {
			accessed: BTreeSet::new(),
			journal: Vec::new(),
			checkpoints: Vec::new(),
			cold_count: 0,
			hot_count: 0,
		}
	}

	/// Open a new nested frame.
	///
	/// This allows to either commit or roll back all touches that are made
	/// after this call. For every `enter_frame` there must be a matching call
	/// to either `commit_frame` or `rollback_frame`.
	pub fn enter_frame(&mut self) {
		self.checkpoints.push(self.journal.len());
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
		self.checkpoints.pop().expect("frame open; qed");
	}

	/// Rollback the top frame.
	///
	/// Touches made during that frame are removed from the access list.
	///
	/// # Panics
	///
	/// Will panic if there is no open frame.
	pub fn rollback_frame(&mut self) {
		let checkpoint = self.checkpoints.pop().expect("frame open; qed");
		for entry in self.journal.drain(checkpoint..) {
			self.accessed.remove(&entry);
		}
	}

	/// Non-mutating sibling of `touch`. Returns `true` if `entry` is cold.
	pub fn peek(&self, entry: &AccessEntry) -> bool {
		!self.accessed.contains(entry)
	}

	/// Register the entry and return `true` if this access is cold (newly
	/// inserted), `false` if it was already hot.
	pub fn touch(&mut self, entry: AccessEntry) -> bool {
		if self.accessed.insert(entry.clone()) {
			self.journal.push(entry);
			self.cold_count = self.cold_count.saturating_add(1);
			true
		} else {
			self.hot_count = self.hot_count.saturating_add(1);
			false
		}
	}

	/// Per-transaction metrics snapshot.
	pub fn metrics(&self) -> AccessListMetrics {
		AccessListMetrics { size: self.accessed.len(), cold: self.cold_count, hot: self.hot_count }
	}

	/// Check hot state without registering (testing / introspection).
	#[cfg(test)]
	pub fn is_hot(&self, entry: &AccessEntry) -> bool {
		self.accessed.contains(entry)
	}

	/// Returns the current number of hot entries (testing / metrics).
	#[cfg(test)]
	pub fn len(&self) -> usize {
		self.accessed.len()
	}

	/// Returns the current frame depth (number of open checkpoints).
	#[cfg(test)]
	pub fn frame_depth(&self) -> usize {
		self.checkpoints.len()
	}
}

// ===========================================================================
// Unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
	use super::*;

	/// Full lifecycle: first frame + two nested frames, one commits, one reverts.
	#[test]
	fn lifecycle() {
		let mut al = AccessList::new();
		let (a, b, c, d) = (
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xA; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xB; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xC; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xD; 32]) },
		);

		assert!(al.touch(a.clone()), "A: first touch cold");
		assert!(!al.touch(a.clone()), "A: second touch hot");

		al.enter_frame();
		assert_eq!(al.frame_depth(), 1);

		assert!(al.touch(b.clone()), "B in F1: cold");
		assert!(!al.touch(a.clone()), "A in F1: hot via parent");

		al.enter_frame();
		assert!(al.touch(c.clone()), "C in F2: cold");

		al.commit_frame();
		assert_eq!(al.frame_depth(), 1);
		assert!(al.is_hot(&c), "C: survives F2 commit");

		assert!(al.touch(d.clone()), "D in F1: cold");
		assert_eq!(al.len(), 4);

		al.rollback_frame();
		assert_eq!(al.frame_depth(), 0);
		assert_eq!(al.len(), 1);
		assert!(al.is_hot(&a), "A: first frame, survives F1 revert");
		assert!(!al.is_hot(&b), "B: inserted by F1, rolled back");
		assert!(!al.is_hot(&c), "C: F2-committed-into-F1, gone when F1 reverts");
		assert!(!al.is_hot(&d), "D: inserted by F1, rolled back");

		// Counters never decrement, even for entries that later roll back:
		// A (cold) + B,C,D (cold) -> 4 cold; A,A (hot) -> 2 hot. Only A still hot,
		// so `size` is 1.
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 4, hot: 2 },
			"counters must include rolled-back touches",
		);
	}

	/// `peek` reports cold/hot without mutating the access list or its counters.
	#[test]
	fn peek_does_not_mutate() {
		let mut al = AccessList::new();
		let entry = AccessEntry { address: H160::zero(), slot: Slot::Fix([1; 32]) };

		assert!(al.peek(&entry), "peek on untouched entry: cold");
		assert!(al.peek(&entry), "repeated peek: still cold");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 0, cold: 0, hot: 0 },
			"peek must not bump counters",
		);

		al.touch(entry.clone());

		assert!(!al.peek(&entry), "peek after touch: hot");
		assert!(!al.peek(&entry), "repeated peek: still hot");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 1, hot: 0 },
			"peek must not bump the hot counter",
		);
	}
}

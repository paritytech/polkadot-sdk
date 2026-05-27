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
use sp_core::H160;

/// Tags an [`AccessEntry`] with the `Key` variant it came from. Prevents
/// `Fix(blake2_256(v))` and `Var(v)` from aliasing on the projected `slot`.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy)]
pub enum KeyKind {
	Fix,
	Var,
}

/// One entry per `(contract address, storage slot)` accessed in the current tx.
///
/// Field order is `slot, address, key_kind` so the derived `Ord` short-circuits
/// on the slot first — the most-discriminating field in the typical access
/// pattern (one contract touching many slots within a transaction).
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub struct AccessEntry {
	/// 32-byte slot identifier, projected from a `Key` via [`crate::exec::Key::to_slot`].
	pub slot: [u8; 32],
	/// Contract whose child trie is being touched.
	pub address: H160,
	/// Whether the originating `Key` was `Fix` or `Var`.
	pub key_kind: KeyKind,
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
	/// First-touch on any entry is always cold. No initial checkpoint is
	/// opened — first-frame touches survive the whole transaction.
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

	/// Register the entry and return `true` if this access is cold (newly
	/// inserted), `false` if it was already hot.
	pub fn touch(&mut self, entry: AccessEntry) -> bool {
		if self.accessed.contains(&entry) {
			self.hot_count = self.hot_count.saturating_add(1);
			return false;
		}
		self.accessed.insert(entry.clone());
		self.journal.push(entry);
		self.cold_count = self.cold_count.saturating_add(1);
		true
	}

	/// Per-transaction metrics: (currently-hot entries, total cold touches,
	/// total hot touches).
	pub fn metrics(&self) -> (usize, u32, u32) {
		(self.accessed.len(), self.cold_count, self.hot_count)
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
			AccessEntry { address: H160::zero(), key_kind: KeyKind::Fix, slot: [0xA; 32] },
			AccessEntry { address: H160::zero(), key_kind: KeyKind::Fix, slot: [0xB; 32] },
			AccessEntry { address: H160::zero(), key_kind: KeyKind::Fix, slot: [0xC; 32] },
			AccessEntry { address: H160::zero(), key_kind: KeyKind::Fix, slot: [0xD; 32] },
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
	}
}

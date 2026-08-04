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
//! The per-frame rollback machinery here (flat journal + checkpoint stack, with
//! `enter_frame` / `commit_frame` / `rollback_frame` wired into `Stack::run`)
//! mirrors [`crate::transient_storage::TransientStorage`].

use alloc::vec::Vec;
use frame_support::{BoundedBTreeMap, BoundedVec};
use sp_core::{ConstU32, H160};

use crate::{exec::Key, limits};

/// Inline-storage cap for `Slot::VarInline`. Covers word-sized keys (`H160`,
/// `H256`, `AccountId32`). `Slot` stays 40 bytes for any cap up to ~38, at no
/// memory cost.
pub const MAX_INLINE_KEY_LEN: usize = 36;

/// Maximum number of distinct `(address, slot)` entries tracked in the
/// access list within a single transaction.
///
/// Bounds the working memory `AccessList` can allocate per transaction.
/// EIP-2929 does not specify a structural cap; Ethereum relies on gas to
/// implicitly bound growth.
///
/// Past this cap, new touches bill cold without being tracked;
/// slots already tracked continue to bill hot.
///
/// Memory grows discontinuously due to the runtime allocator (sc-allocator)
/// rounding allocations up to power-of-2 size classes.
///
/// All figures below are approximate order-of-magnitude estimates. The
/// Ethereum-gas column shows the EIP-2929 cost of filling the map to that
/// size via cold SLOADs (2 100 gas each).
///
/// | Entries | Fix/Inline (Best) | VarLong (Worst) |     Gas (Ethereum) |
/// |---------|-------------------|-----------------|--------------------|
/// |       1 |      ~1.3 KB      |     ~1.4 KB     |          2.1 k gas |
/// |       2 |      ~1.3 KB      |     ~1.6 KB     |          4.2 k gas |
/// |       8 |      ~1.5 KB      |     ~2.6 KB     |         16.8 k gas |
/// |      32 |       ~7 KB       |      ~11 KB     |         67.2 k gas |
/// |     128 |      ~45 KB       |      ~65 KB     |          269 k gas |
/// |   2 048 |      ~730 KB      |       ~1 MB     |          4.3 M gas |
///
/// Set ~2× above the current PoV-reachable ceiling as a backstop: each
/// cold access charges ~10 KB `proof_size`, capping a transaction
/// (~7.5 MiB PoV) at ~770 cold touches.
pub const MAX_ACCESS_LIST_ENTRIES: usize = 2_048;

/// Worst-case per-entry memory in the `BoundedBTreeMap` + journal, measured against
/// sc-allocator (8-byte headers, power-of-2 buckets). `Slot::Fix` and
/// `Slot::VarInline` measure ~366 B; `Slot::VarLong` ~502 B. Rounded up to 512
/// for headroom. The 1-byte [`Paid`] value fits in the tree nodes' spare
/// space and adds nothing to these figures.
const MAX_ACCESS_LIST_ENTRY_BYTES: usize = 512;

/// Worst-case total memory the access list can hold per transaction.
pub const MAX_ACCESS_LIST_BYTES: u32 =
	MAX_ACCESS_LIST_ENTRIES.saturating_mul(MAX_ACCESS_LIST_ENTRY_BYTES) as u32;

/// Storage slot identifier for an access-list entry.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub enum Slot {
	/// Fixed 32-byte storage key.
	Fix([u8; 32]),
	/// Variable-length key up to [`MAX_INLINE_KEY_LEN`], stored inline to
	/// avoid the per-entry heap allocation `VarLong` requires, while keeping
	/// `Slot` size bounded.
	VarInline { bytes: [u8; MAX_INLINE_KEY_LEN], len: u8 },
	/// Variable-length key longer than [`MAX_INLINE_KEY_LEN`], up to
	/// `limits::STORAGE_KEY_BYTES`.
	VarLong(BoundedVec<u8, ConstU32<{ limits::STORAGE_KEY_BYTES }>>),
}

impl From<&Key> for Slot {
	fn from(key: &Key) -> Self {
		match key {
			Key::Fix(v) => Slot::Fix(*v),
			Key::Var(v) => {
				let raw: &[u8] = v.as_ref();
				if raw.len() <= MAX_INLINE_KEY_LEN {
					let mut bytes = [0u8; MAX_INLINE_KEY_LEN];
					bytes[..raw.len()].copy_from_slice(raw);
					Slot::VarInline { bytes, len: raw.len() as u8 }
				} else {
					Slot::VarLong(v.clone())
				}
			},
		}
	}
}

/// How a storage access is priced. `Persistent` carries its access-list warmth;
/// `Transient` has no warmth, so every access costs the same.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub enum ContractStorageKind {
	/// Persistent storage, priced by its access-list warmth.
	Persistent(Warmth),
	/// Transient storage, not tracked by the access list.
	Transient,
}

/// What a transaction has already paid for a slot.
///
/// A write costs more than a read, and the difference is owed once per slot:
/// the first write pays it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Paid {
	/// Only the read cost is paid.
	Read,
	/// The full write cost is paid.
	Write,
}

/// The same two variants named for the operation being performed.
pub type StorageOp = Paid;

/// Warmth of an access-list entry, as it stood **before** the access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Warmth {
	/// Entry is in the access list. `first_write` is true when this access is
	/// the first write to the entry.
	Hot { first_write: bool },
	/// Entry is not in the access list; when `revertible` is true, the touch is
	/// tied to the current frame, so a `rollback_frame` drops it and the entry
	/// becomes cold again.
	Cold { revertible: bool },
}

impl Warmth {
	/// Whether this was the first access to the slot this transaction.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	pub(crate) fn is_cold(&self) -> bool {
		matches!(self, Self::Cold { .. })
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

/// One entry per `(storage slot, contract address)` accessed in the current tx.
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
/// map, a flat journal of insertions, and journal-index checkpoints.
///
/// # Safety invariant
///
/// Callers touch the `AccessList` before charging gas, so reverts must roll back the touches
/// they made. Without that, an out-of-gas at the cold charge after the touch would leave the slot
/// hot without the cold charge being paid, and a later access would then be billed hot.
///
/// The [`Paid`] cost is the exception: a `Read` to `Write` upgrade survives the
/// revert. A reverted frame keeps its weight charges, so dropping the upgrade
/// refunds nothing and only charges again if the slot is written once more.

#[derive(Default)]
pub struct AccessList {
	/// All currently-hot entries with the cost each has paid.
	accessed: BoundedBTreeMap<AccessEntry, Paid, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Flat journal of insertions (in order); Each entry was added by exactly one frame;
	/// `checkpoints` marks the frame boundaries inside this journal.
	journal: BoundedVec<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
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
	/// Create an empty access list for a new transaction.
	pub fn new() -> Self {
		Self::default()
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

	/// Non-mutating sibling of [`touch`](Self::touch): the warmth a touch with
	/// `op` would return. A peek never journals, so a cold result is always
	/// non-revertible.
	pub fn peek(&self, entry: &AccessEntry, op: StorageOp) -> Warmth {
		match self.accessed.get(entry) {
			Some(paid) => {
				Warmth::Hot { first_write: op == StorageOp::Write && *paid == Paid::Read }
			},
			None => Warmth::Cold { revertible: false },
		}
	}

	/// Whether the map is at the entry cap.
	fn is_full(&self) -> bool {
		self.accessed.len() >= MAX_ACCESS_LIST_ENTRIES
	}

	/// Whether a nested-frame checkpoint is open.
	fn in_nested_frame(&self) -> bool {
		!self.checkpoints.is_empty()
	}

	/// Register the entry, returning its warmth. `op` is the operation being
	/// performed on the slot.
	///
	/// Past [`MAX_ACCESS_LIST_ENTRIES`], new entries are billed cold without
	/// being journaled; previously-hot slots continue to bill hot.
	pub fn touch(&mut self, entry: AccessEntry, op: StorageOp) -> Warmth {
		// Tracked slots come first so a write can still upgrade them once the
		// map is full.
		if let Some(tracked) = self.accessed.get_mut(&entry) {
			self.hot_count = self.hot_count.saturating_add(1);
			let first_write = op == StorageOp::Write && *tracked == Paid::Read;
			if first_write {
				*tracked = Paid::Write;
			}
			return Warmth::Hot { first_write };
		}

		self.cold_count = self.cold_count.saturating_add(1);

		if self.is_full() {
			return Warmth::Cold { revertible: false };
		}

		self.accessed
			.try_insert(entry.clone(), op)
			.expect("under cap; checked is_full above; qed");
		self.journal
			.try_push(entry)
			.expect("journal grows in lockstep with accessed and shares its bound; qed");
		Warmth::Cold { revertible: self.in_nested_frame() }
	}

	/// Per-transaction metrics snapshot.
	pub fn metrics(&self) -> AccessListMetrics {
		AccessListMetrics { size: self.accessed.len(), cold: self.cold_count, hot: self.hot_count }
	}

	/// Returns the number of open checkpoints.
	#[cfg(test)]
	fn frame_depth(&self) -> usize {
		self.checkpoints.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn nested_commit_then_parent_rollback_drops_all() {
		let mut al = AccessList::new();
		let (a, b, c, d) = (
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xA; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xB; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xC; 32]) },
			AccessEntry { address: H160::zero(), slot: Slot::Fix([0xD; 32]) },
		);

		// Root frame: cold, but no checkpoint covers it, so it is not revertible.
		assert_eq!(
			al.touch(a.clone(), StorageOp::Read),
			Warmth::Cold { revertible: false },
			"A: first touch cold"
		);
		assert!(!al.touch(a.clone(), StorageOp::Read).is_cold(), "A: second touch hot");

		al.enter_frame();
		assert_eq!(al.frame_depth(), 1);

		// Inside F1: journaled under the open checkpoint, so it is revertible.
		assert_eq!(
			al.touch(b.clone(), StorageOp::Read),
			Warmth::Cold { revertible: true },
			"B in F1: cold"
		);
		assert!(!al.touch(a.clone(), StorageOp::Read).is_cold(), "A in F1: hot via parent");

		al.enter_frame();
		assert!(al.touch(c.clone(), StorageOp::Read).is_cold(), "C in F2: cold");

		al.commit_frame();
		assert_eq!(al.frame_depth(), 1);
		assert!(!al.peek(&c, StorageOp::Read).is_cold(), "C: survives F2 commit");

		assert!(al.touch(d.clone(), StorageOp::Read).is_cold(), "D in F1: cold");
		assert_eq!(al.metrics().size, 4);

		al.rollback_frame();
		assert_eq!(al.frame_depth(), 0);
		assert!(!al.peek(&a, StorageOp::Read).is_cold(), "A: first frame, survives F1 revert");
		assert!(al.peek(&b, StorageOp::Read).is_cold(), "B: inserted by F1, rolled back");
		assert!(
			al.peek(&c, StorageOp::Read).is_cold(),
			"C: F2-committed-into-F1, gone when F1 reverts"
		);
		assert!(al.peek(&d, StorageOp::Read).is_cold(), "D: inserted by F1, rolled back");

		// Counters never decrement, even for entries that later roll back:
		// A (cold) + B,C,D (cold) -> 4 cold; A,A (hot) -> 2 hot. Only A still hot,
		// so `size` is 1.
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 4, hot: 2 },
			"counters must include rolled-back touches",
		);
	}

	#[test]
	fn touch_caps_at_max_entries() {
		let mut al = AccessList::new();
		// Fill to the cap with distinct addresses.
		for i in 0..MAX_ACCESS_LIST_ENTRIES {
			let address = H160::from_low_u64_be(i as u64);
			assert!(
				al.touch(AccessEntry { address, slot: Slot::Fix([0; 32]) }, StorageOp::Read)
					.is_cold()
			);
		}
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES);

		let new_entry = AccessEntry {
			address: H160::from_low_u64_be(MAX_ACCESS_LIST_ENTRIES as u64),
			slot: Slot::Fix([0; 32]),
		};
		// Past the cap a new entry is billed cold but never journaled, so even
		// inside an open frame it can't be rolled back.
		al.enter_frame();
		assert_eq!(
			al.touch(new_entry.clone(), StorageOp::Read),
			Warmth::Cold { revertible: false },
			"past cap: bills cold, not revertible",
		);
		al.commit_frame();
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES, "map size stays at cap");
		assert!(al.peek(&new_entry, StorageOp::Read).is_cold(), "past-cap entry is not tracked");

		assert!(
			al.touch(new_entry, StorageOp::Read).is_cold(),
			"past cap re-touch: still cold (not tracked)"
		);

		let existing = AccessEntry { address: H160::zero(), slot: Slot::Fix([0; 32]) };
		assert!(
			!al.touch(existing.clone(), StorageOp::Read).is_cold(),
			"existing entry still hot at cap"
		);

		// A write can still upgrade a tracked slot once the map is full.
		assert_eq!(
			al.touch(existing.clone(), StorageOp::Write),
			Warmth::Hot { first_write: true },
			"first write at cap",
		);
		assert_eq!(
			al.touch(existing, StorageOp::Write),
			Warmth::Hot { first_write: false },
			"write at cap: upgraded, repeat is not first",
		);
	}

	#[test]
	fn peek_does_not_mutate() {
		let mut al = AccessList::new();
		let entry = AccessEntry { address: H160::zero(), slot: Slot::Fix([1; 32]) };

		assert!(al.peek(&entry, StorageOp::Write).is_cold(), "untouched entry: cold");
		assert!(al.peek(&entry, StorageOp::Write).is_cold(), "repeated query: still cold");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 0, cold: 0, hot: 0 },
			"peek must not bump counters",
		);

		al.touch(entry.clone(), StorageOp::Read);

		let would_be_first = Warmth::Hot { first_write: true };
		assert_eq!(al.peek(&entry, StorageOp::Write), would_be_first, "peeked write is first");
		assert_eq!(al.peek(&entry, StorageOp::Write), would_be_first, "peek must not upgrade");
		assert_eq!(
			al.peek(&entry, StorageOp::Read),
			Warmth::Hot { first_write: false },
			"peeked read is never a first write",
		);
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 1, hot: 0 },
			"peek must not bump the hot counter",
		);
	}

	#[test]
	fn a_write_upgrades_the_op_and_nothing_downgrades_it() {
		let mut al = AccessList::new();
		let entry = AccessEntry { address: H160::zero(), slot: Slot::Fix([2; 32]) };

		let first = Warmth::Hot { first_write: true };
		let repeat = Warmth::Hot { first_write: false };

		assert!(al.touch(entry.clone(), StorageOp::Read).is_cold(), "first read: cold");
		assert_eq!(al.touch(entry.clone(), StorageOp::Read), repeat, "read after read");
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), first, "first write");
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), repeat, "write after write");
		assert_eq!(al.touch(entry.clone(), StorageOp::Read), repeat, "read after write");
		assert_eq!(al.touch(entry, StorageOp::Write), repeat, "a read never downgrades the op");

		// A first-access write starts at `Write` directly.
		let written = AccessEntry { address: H160::zero(), slot: Slot::Fix([3; 32]) };
		assert!(al.touch(written.clone(), StorageOp::Write).is_cold(), "first write: cold");
		assert_eq!(al.touch(written, StorageOp::Write), repeat, "cold write starts at Write");
	}

	#[test]
	fn paid_upgrade_survives_the_reverting_frame() {
		let mut al = AccessList::new();
		let outer = AccessEntry { address: H160::zero(), slot: Slot::Fix([4; 32]) };
		let inner = AccessEntry { address: H160::zero(), slot: Slot::Fix([5; 32]) };

		al.touch(outer.clone(), StorageOp::Read);

		al.enter_frame();
		assert_eq!(
			al.touch(outer.clone(), StorageOp::Write),
			Warmth::Hot { first_write: true },
			"upgrade in frame"
		);
		assert!(al.touch(inner.clone(), StorageOp::Write).is_cold(), "inserted in frame");
		al.rollback_frame();

		assert_eq!(
			al.peek(&outer, StorageOp::Write),
			Warmth::Hot { first_write: false },
			"upgrade survives the revert",
		);
		assert!(al.peek(&inner, StorageOp::Read).is_cold(), "own insertion is rolled back whole");
	}
}

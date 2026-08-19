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
//! The per-frame rollback machinery here (flat journals + checkpoint stack, with
//! `enter_frame` / `commit_frame` / `rollback_frame` wired into `Stack::run`)
//! mirrors [`crate::transient_storage::TransientStorage`].

use alloc::vec::Vec;
use frame_support::{BoundedBTreeMap, BoundedVec};
use sp_core::{ConstU32, H160, H256};

use crate::{exec::Key, limits};

/// Inline-storage cap for `Slot::VarInline`. Covers word-sized keys (`H160`,
/// `H256`, `AccountId32`). `Slot` stays 40 bytes for any cap up to ~38, at no
/// memory cost.
pub const MAX_INLINE_KEY_LEN: usize = 36;

/// Maximum number of distinct entries tracked in the access list within a
/// single transaction.
///
/// Bounds the working memory `AccessList` can allocate per transaction.
/// EIP-2929 does not specify a structural cap; Ethereum relies on gas to
/// implicitly bound growth.
///
/// Past this cap, new touches bill cold without being tracked;
/// entries already tracked continue to bill hot.
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

/// Worst-case per-entry memory in the `BoundedBTreeMap` + journals, measured
/// against sc-allocator (8-byte headers, power-of-2 buckets). `Slot::Fix` and
/// `Slot::VarInline` measure ~366 B; `Slot::VarLong` ~502 B. An entry in the
/// `upgrades` journal adds up to ~200 B on top. Rounded up to 768 for
/// headroom.
const MAX_ACCESS_LIST_ENTRY_BYTES: usize = 768;

/// Worst-case total memory the access list can hold per transaction.
pub const MAX_ACCESS_LIST_BYTES: u32 =
	MAX_ACCESS_LIST_ENTRIES.saturating_mul(MAX_ACCESS_LIST_ENTRY_BYTES) as u32;

/// The storage key of an [`AccessEntry::Storage`] entry.
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

/// What a transaction has already paid for an entry. Ordered: `Write` has
/// paid for everything `Read` has.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Paid {
	/// The read cost is paid.
	Read,
	/// The write cost is paid.
	Write,
}

impl Paid {
	/// Whether this level has already paid for an access performing `op`.
	pub fn covers(self, op: StorageOp) -> bool {
		self >= op
	}
}

/// The same two variants named for the operation being performed.
pub type StorageOp = Paid;

/// Warmth of an access-list entry, as it stood **before** the access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Warmth {
	/// Entry is in the access list; carries the level the entry had paid.
	Hot(Paid),
	/// Entry is not in the access list; when `revertible` is true, the touch
	/// rolls back with the current frame.
	Cold { revertible: bool },
}

impl Warmth {
	pub fn cold_non_revertible() -> Self {
		Self::Cold { revertible: false }
	}

	pub fn cold_revertible() -> Self {
		Self::Cold { revertible: true }
	}

	pub fn is_hot(&self) -> bool {
		matches!(self, Self::Hot(_))
	}

	/// Whether the access billed cold: the entry was not tracked.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	pub(crate) fn is_cold(&self) -> bool {
		matches!(self, Self::Cold { .. })
	}

	/// Converts a cold touch to non-revertible.
	pub fn non_revertible(self) -> Self {
		match self {
			Self::Hot(paid) => Self::Hot(paid),
			Self::Cold { .. } => Self::Cold { revertible: false },
		}
	}
}

/// How a storage access is priced. `Persistent` carries its access-list warmth;
/// `Transient` has no warmth, so every access costs the same.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub enum StorageAccessKind {
	/// Persistent storage, priced by its access-list warmth.
	Persistent(Warmth),
	/// Transient storage, every access costs the same.
	Transient,
}

impl StorageAccessKind {
	/// See [`Warmth::non_revertible`].
	pub fn non_revertible(self) -> Self {
		match self {
			Self::Persistent(warmth) => Self::Persistent(warmth.non_revertible()),
			Self::Transient => Self::Transient,
		}
	}
}

/// Warmth of the two state items a code load reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodeLoadWarmth {
	/// The `CodeInfoOf` entry.
	pub info: Warmth,
	/// The `PristineCode` entry.
	pub blob: Warmth,
}

impl CodeLoadWarmth {
	pub fn cold_non_revertible() -> Self {
		Self { info: Warmth::cold_non_revertible(), blob: Warmth::cold_non_revertible() }
	}
}

/// A state-accessing operation, one variant per opcode.
#[derive(Clone, Copy, Debug)]
pub enum StateAccess {
	Call { target: H160 },
	DelegateCall { target: H160 },
}

impl StateAccess {
	/// Builds the call variant matching the `delegate` flag.
	pub fn call(target: H160, delegate: bool) -> Self {
		if delegate { Self::DelegateCall { target } } else { Self::Call { target } }
	}

	/// Maps `warmth_of` over each state item this operation reads and
	/// collects the results into a [`StateWarmth`].
	fn expand(self, mut warmth_of: impl FnMut(AccessEntry) -> Warmth) -> StateWarmth {
		match self {
			Self::Call { target } => StateWarmth::Call {
				account: warmth_of(AccessEntry::Account { address: target }),
				account_info: warmth_of(AccessEntry::AccountInfo { address: target }),
			},
			Self::DelegateCall { target } => StateWarmth::DelegateCall {
				account_info: warmth_of(AccessEntry::AccountInfo { address: target }),
			},
		}
	}
}

/// Warmth of the state items a [`StateAccess`] reads, one variant per opcode.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub enum StateWarmth {
	/// A normal call reads the target's account state and contract metadata.
	Call { account: Warmth, account_info: Warmth },
	/// A delegate call reads only the target's contract metadata.
	DelegateCall { account_info: Warmth },
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

/// One entry per distinct state item accessed in the current transaction.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub enum AccessEntry {
	/// Account at `address`: both `System::Account` and `OriginalAccount`.
	Account { address: H160 },
	/// A contract storage slot. Field order is `slot, address` so comparison
	/// decides on `slot` first, the most-discriminating field in the typical
	/// access pattern (one contract touching many slots within a transaction).
	Storage { slot: Slot, address: H160 },
	/// Account metadata (`AccountInfoOf`) of `address`.
	AccountInfo { address: H160 },
	/// Code metadata. Keyed by code hash: code is
	/// deduplicated, so contracts sharing a blob share its metadata warmth.
	CodeInfo { hash: H256 },
	/// Code blob. Keyed by code hash for the same reason.
	CodeBlob { hash: H256 },
}

impl AccessEntry {
	// Number of state reads per entry.
	pub(crate) const ACCOUNT_READS: u64 = 2; // `OriginalAccount` + `System::Account`
	pub(crate) const ACCOUNT_INFO_READS: u64 = 1;
	pub(crate) const STORAGE_READS: u64 = 1;
	pub(crate) const CODE_INFO_READS: u64 = 1;
	pub(crate) const CODE_BLOB_READS: u64 = 1;
}

// Compile-time check that every `AccessEntry` variant has a read count.
const _: () = {
	fn _reads(entry: &AccessEntry) -> u64 {
		match entry {
			AccessEntry::Account { .. } => AccessEntry::ACCOUNT_READS,
			AccessEntry::Storage { .. } => AccessEntry::STORAGE_READS,
			AccessEntry::AccountInfo { .. } => AccessEntry::ACCOUNT_INFO_READS,
			AccessEntry::CodeInfo { .. } => AccessEntry::CODE_INFO_READS,
			AccessEntry::CodeBlob { .. } => AccessEntry::CODE_BLOB_READS,
		}
	}
};

/// Per-transaction access list with per-frame rollback support. Layout
/// follows [`crate::transient_storage::TransientStorage`]: a current-state
/// map, flat journals of insertions and of upgrades, and checkpoints holding
/// both journals' lengths at frame entry. Two journals instead of one with
/// tagged entries: an upgrade needs its own entry either way, and untagged
/// entries use less memory.
///
/// A reverting frame rolls back every entry it touched, regardless of whether
/// the touch happened before or after its charge:
///
/// - Storage opcodes touch before charging: an out-of-gas at the charge must not leave the entry
///   warm without its cold cost ever being paid, nor a `Read` to `Write` upgrade unpaid, so the
///   rollback removes the frame's insertions and downgrades its upgrades.
/// - Call and code entries charge before touching: a reverting frame simply discards the warmth it
///   added (EIP-2929 revert semantics).

#[derive(Default)]
pub struct AccessList {
	/// All currently-hot entries with the cost each has paid.
	accessed: BoundedBTreeMap<AccessEntry, Paid, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Flat journal of insertions (in order); each entry was added by exactly
	/// one frame, and `checkpoints` marks the frame boundaries inside this journal.
	journal: BoundedVec<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Flat journal of `Read` to `Write` upgrades (in order).
	upgrades: BoundedVec<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Stack of `(journal, upgrades)` lengths at frame entry. Rolling back
	/// drains both journals down to `checkpoints.last()`: drained insertions
	/// are removed from `accessed`, drained upgrades downgraded to `Read`.
	checkpoints: Vec<(usize, usize)>,
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
		self.checkpoints.push((self.journal.len(), self.upgrades.len()));
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
	/// Entries inserted during that frame are removed from the access list;
	/// its `Read` to `Write` upgrades are downgraded.
	///
	/// # Panics
	///
	/// Will panic if there is no open frame.
	pub fn rollback_frame(&mut self) {
		let (journal_checkpoint, upgrades_checkpoint) =
			self.checkpoints.pop().expect("frame open; qed");
		for entry in self.journal.drain(journal_checkpoint..) {
			self.accessed.remove(&entry);
		}
		for entry in self.upgrades.drain(upgrades_checkpoint..) {
			// Removed already if the same frame also inserted the entry.
			if let Some(paid) = self.accessed.get_mut(&entry) {
				*paid = Paid::Read;
			}
		}
	}

	/// Non-mutating sibling of [`touch`](Self::touch).
	pub fn peek(&self, entry: &AccessEntry) -> Warmth {
		match self.accessed.get(entry) {
			Some(paid) => Warmth::Hot(*paid),
			None if self.is_full() => Warmth::Cold { revertible: false },
			None => Warmth::Cold { revertible: self.in_nested_frame() },
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
		if let Some(paid) = self.accessed.get_mut(&entry) {
			self.hot_count = self.hot_count.saturating_add(1);
			let previous = *paid;
			if !previous.covers(op) {
				// Defensive: one upgrade per tracked slot, so the journal
				// cannot fill. If it does, later writes just pay the surcharge again.
				let journaled = self.upgrades.try_push(entry);
				debug_assert!(journaled.is_ok(), "at most one live upgrade per tracked slot");
				if journaled.is_ok() {
					*paid = Paid::Write;
				}
			}
			return Warmth::Hot(previous);
		}
		self.cold_count = self.cold_count.saturating_add(1);
		if self.is_full() {
			return Warmth::Cold { revertible: false };
		}
		self.accessed
			.try_insert(entry.clone(), op)
			.expect("under cap; is_full checked above; qed");
		self.journal
			.try_push(entry)
			.expect("journal grows in lockstep with accessed and shares its bound; qed");
		Warmth::Cold { revertible: self.in_nested_frame() }
	}

	/// Warms every state item the operation reads, returning its warmth.
	pub fn warm_operation(&mut self, state_access: StateAccess) -> StateWarmth {
		state_access.expand(|entry| self.touch(entry, StorageOp::Read))
	}

	/// Non-mutating sibling of [`warm_operation`](Self::warm_operation).
	pub fn operation_warmth(&self, state_access: StateAccess) -> StateWarmth {
		state_access.expand(|entry| self.peek(&entry))
	}

	/// Warms the two entries a code load reads, returning their warmth.
	pub fn warm_code(&mut self, hash: H256) -> CodeLoadWarmth {
		CodeLoadWarmth {
			info: self.touch(AccessEntry::CodeInfo { hash }, StorageOp::Read),
			blob: self.touch(AccessEntry::CodeBlob { hash }, StorageOp::Read),
		}
	}

	/// Non-mutating sibling of [`warm_code`](Self::warm_code).
	pub fn code_warmth(&self, hash: H256) -> CodeLoadWarmth {
		CodeLoadWarmth {
			info: self.peek(&AccessEntry::CodeInfo { hash }),
			blob: self.peek(&AccessEntry::CodeBlob { hash }),
		}
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
			AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([0xA; 32]) },
			AccessEntry::Account { address: H160::zero() },
			AccessEntry::AccountInfo { address: H160::zero() },
			AccessEntry::CodeBlob { hash: H256::repeat_byte(0xD) },
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
		assert!(al.peek(&c).is_hot(), "C: survives frame 2 commit");

		assert!(al.touch(d.clone(), StorageOp::Read).is_cold(), "D in F1: cold");
		assert_eq!(al.metrics().size, 4);

		al.rollback_frame();
		assert_eq!(al.frame_depth(), 0);
		assert!(al.peek(&a).is_hot(), "A: first frame, survives frame 1 revert");
		assert!(!al.peek(&b).is_hot(), "B: inserted by frame 1, rolled back");
		assert!(
			!al.peek(&c).is_hot(),
			"C: frame-2-committed-into-frame-1, gone when frame 1 reverts"
		);
		assert!(!al.peek(&d).is_hot(), "D: inserted by frame 1, rolled back");

		// Counters never decrement, even for entries that later roll back:
		// A (cold) + B,C,D (cold) -> 4 cold; A,A (hot) -> 2 hot. Only A still hot,
		// so `size` is 1.
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 4, hot: 2 },
			"counters must include rolled-back touches",
		);
	}

	/// Touch read-paid entries with distinct addresses until the map is full.
	fn fill_to_cap(al: &mut AccessList) {
		for i in 0..MAX_ACCESS_LIST_ENTRIES {
			let address = H160::from_low_u64_be(i as u64);
			let entry = AccessEntry::Storage { address, slot: Slot::Fix([0; 32]) };
			assert!(al.touch(entry, StorageOp::Read).is_cold(), "fill entries must be new");
		}
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES, "map filled to the cap");
	}

	#[test]
	fn touch_caps_at_max_entries() {
		let mut al = AccessList::new();
		fill_to_cap(&mut al);

		let new_entry = AccessEntry::Storage {
			address: H160::from_low_u64_be(MAX_ACCESS_LIST_ENTRIES as u64),
			slot: Slot::Fix([0; 32]),
		};
		al.enter_frame();
		assert_eq!(
			al.touch(new_entry.clone(), StorageOp::Read),
			Warmth::Cold { revertible: false },
			"past cap: bills cold, not revertible",
		);
		al.commit_frame();
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES, "map size stays at cap");
		assert!(al.peek(&new_entry).is_cold(), "past-cap entry is not tracked");

		assert!(
			al.touch(new_entry, StorageOp::Read).is_cold(),
			"past cap re-touch: still cold (not tracked)"
		);

		let existing = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([0; 32]) };
		assert!(
			!al.touch(existing.clone(), StorageOp::Read).is_cold(),
			"existing entry still hot at cap"
		);

		// A write can still upgrade a tracked slot once the map is full.
		assert_eq!(
			al.touch(existing.clone(), StorageOp::Write),
			Warmth::Hot(Paid::Read),
			"first write at cap: was read-paid",
		);
		assert_eq!(
			al.touch(existing, StorageOp::Write),
			Warmth::Hot(Paid::Write),
			"write at cap: upgraded",
		);
	}

	#[test]
	fn call_peek_and_touch_diverge_at_cap_boundary() {
		let mut al = AccessList::new();
		// Fill to one below the cap.
		for i in 0..(MAX_ACCESS_LIST_ENTRIES - 1) {
			al.touch(
				AccessEntry::Storage {
					address: H160::from_low_u64_be(i as u64),
					slot: Slot::Fix([0; 32]),
				},
				StorageOp::Read,
			);
		}
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES - 1);

		let target = H160::from_low_u64_be(0xdead_beef);
		// Nested frame, so a journaled cold touch would be revertible.
		al.enter_frame();

		// The set is below the cap, so peek prices both call entries revertible
		// cold: it cannot see that touching the first entry fills the cap.
		assert_eq!(
			al.operation_warmth(StateAccess::Call { target }),
			StateWarmth::Call {
				account: Warmth::cold_revertible(),
				account_info: Warmth::cold_revertible(),
			},
			"peek sees the not-full set for both entries",
		);

		// The first touch fills the cap, so ContractInfo lands past it: non-revertible.
		assert_eq!(
			al.warm_operation(StateAccess::Call { target }),
			StateWarmth::Call {
				account: Warmth::cold_revertible(),
				account_info: Warmth::cold_non_revertible(),
			},
			"touch journals only the first entry before the cap fills",
		);

		al.commit_frame();
	}

	#[test]
	fn peek_does_not_mutate() {
		let mut al = AccessList::new();
		let entry = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([1; 32]) };

		assert!(!al.peek(&entry).is_hot(), "untouched entry: cold");
		assert!(!al.peek(&entry).is_hot(), "repeated query: still cold");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 0, cold: 0, hot: 0 },
			"peek must not bump counters",
		);

		al.touch(entry.clone(), StorageOp::Read);

		let read_paid = Warmth::Hot(Paid::Read);
		assert_eq!(al.peek(&entry), read_paid, "peek reports the paid level");
		assert_eq!(al.peek(&entry), read_paid, "peek must not upgrade");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 1, hot: 0 },
			"peek must not bump the hot counter",
		);
	}

	#[test]
	fn touches_never_downgrade_the_paid_level() {
		let mut al = AccessList::new();
		let entry = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([2; 32]) };

		let read_paid = Warmth::Hot(Paid::Read);
		let write_paid = Warmth::Hot(Paid::Write);

		assert!(al.touch(entry.clone(), StorageOp::Read).is_cold(), "first read: cold");
		assert_eq!(al.touch(entry.clone(), StorageOp::Read), read_paid, "read after read");
		assert_eq!(
			al.touch(entry.clone(), StorageOp::Write),
			read_paid,
			"first write: was read-paid"
		);
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), write_paid, "write after write");
		assert_eq!(al.touch(entry.clone(), StorageOp::Read), write_paid, "read after write");
		assert_eq!(
			al.touch(entry, StorageOp::Write),
			write_paid,
			"a read never downgrades the level"
		);

		let written = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([3; 32]) };
		assert!(al.touch(written.clone(), StorageOp::Write).is_cold(), "first write: cold");
		assert_eq!(al.touch(written, StorageOp::Write), write_paid, "cold write starts at Write");
	}

	#[test]
	fn peek_agrees_with_touch() {
		fn agree(al: &mut AccessList, entry: AccessEntry, op: StorageOp, expected: Warmth) {
			assert_eq!(al.peek(&entry), expected, "peek must report like touch");
			assert_eq!(al.touch(entry, op), expected, "touch must report like peek");
		}

		let entry =
			|i: u8| AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([i; 32]) };
		let mut al = AccessList::new();

		agree(&mut al, entry(1), StorageOp::Read, Warmth::Cold { revertible: false });
		agree(&mut al, entry(1), StorageOp::Read, Warmth::Hot(Paid::Read));
		agree(&mut al, entry(1), StorageOp::Write, Warmth::Hot(Paid::Read));
		agree(&mut al, entry(1), StorageOp::Write, Warmth::Hot(Paid::Write));
		agree(&mut al, entry(1), StorageOp::Read, Warmth::Hot(Paid::Write));

		al.enter_frame();
		agree(&mut al, entry(2), StorageOp::Write, Warmth::Cold { revertible: true });
		al.rollback_frame();

		fill_to_cap(&mut al);

		al.enter_frame();
		// Peek's own past-cap arm must agree with touch too.
		agree(&mut al, entry(3), StorageOp::Write, Warmth::Cold { revertible: false });
		// A tracked read-paid slot still upgrades at the cap.
		let filled =
			AccessEntry::Storage { address: H160::from_low_u64_be(0), slot: Slot::Fix([0; 32]) };
		agree(&mut al, filled.clone(), StorageOp::Write, Warmth::Hot(Paid::Read));
		al.rollback_frame();
		assert_eq!(
			al.peek(&filled),
			Warmth::Hot(Paid::Read),
			"an at-cap upgrade rolls back with its frame"
		);
	}

	#[test]
	fn upgrade_rolls_back_with_the_reverting_frame() {
		let mut al = AccessList::new();
		let entry = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([9; 32]) };
		al.touch(entry.clone(), StorageOp::Read);

		al.enter_frame();
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), Warmth::Hot(Paid::Read));
		al.rollback_frame();
		assert_eq!(
			al.peek(&entry),
			Warmth::Hot(Paid::Read),
			"the reverted frame's write was undone, so the next write pays again"
		);

		al.enter_frame();
		al.enter_frame();
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), Warmth::Hot(Paid::Read));
		al.commit_frame();
		assert_eq!(
			al.peek(&entry),
			Warmth::Hot(Paid::Write),
			"a committed upgrade belongs to the parent frame"
		);
		al.rollback_frame();
		assert_eq!(
			al.peek(&entry),
			Warmth::Hot(Paid::Read),
			"the parent's revert drops the committed upgrade"
		);
	}

	#[test]
	fn upgrade_survives_a_nested_frames_rollback() {
		let mut al = AccessList::new();
		let upgraded = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([7; 32]) };
		al.touch(upgraded.clone(), StorageOp::Read);
		al.touch(upgraded.clone(), StorageOp::Write);

		al.enter_frame();
		al.touch(
			AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([6; 32]) },
			StorageOp::Write,
		);
		al.rollback_frame();

		assert_eq!(
			al.peek(&upgraded),
			Warmth::Hot(Paid::Write),
			"a rollback must only drop its own frame's upgrades"
		);
	}

	#[test]
	fn same_frame_insert_and_upgrade_roll_back_together() {
		let mut al = AccessList::new();
		let entry = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([8; 32]) };
		al.enter_frame();
		al.touch(entry.clone(), StorageOp::Read);
		al.touch(entry.clone(), StorageOp::Write);
		al.rollback_frame();
		assert!(al.peek(&entry).is_cold(), "the entry and its upgrade are both gone");
	}
}

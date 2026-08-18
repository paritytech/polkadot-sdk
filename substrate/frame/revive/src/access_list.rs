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
use frame_support::{BoundedBTreeSet, BoundedVec};
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
/// Past this cap, new touches bill cold without being added to the set;
/// entries already tracked continue to bill hot.
///
/// Memory grows discontinuously due to the runtime allocator (sc-allocator)
/// rounding allocations up to power-of-2 size classes.
///
/// All figures below are approximate order-of-magnitude estimates. The
/// Ethereum-gas column shows the EIP-2929 cost of filling the set to that
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

/// Worst-case per-entry memory in the `BoundedBTreeSet` + journal, measured against
/// sc-allocator (8-byte headers, power-of-2 buckets). `Slot::Fix` and
/// `Slot::VarInline` measure ~366 B; `Slot::VarLong` ~502 B. Rounded up to 512
/// for headroom.
const MAX_ACCESS_LIST_ENTRY_BYTES: usize = 512;

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

/// Warmth of an access-list entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Warmth {
	/// Entry is in the access list.
	Hot,
	/// Entry is not in the access list; when `revertible` is true, the touch is
	/// tied to the current frame, so a `rollback_frame` drops it and the entry
	/// becomes cold again.
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
		matches!(self, Self::Hot)
	}

	/// Converts a cold touch to non-revertible.
	pub fn non_revertible(self) -> Self {
		match self {
			Self::Hot => Self::Hot,
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
	/// Transient storage, not tracked by the access list.
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
	/// A delegate call runs in the caller's context and reads only the
	/// target's contract metadata.
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
/// set, a flat journal of insertions, and journal-index checkpoints.
///
/// A reverting frame rolls back every entry it touched, regardless of whether
/// the touch happened before or after its charge:
///
/// - Touch before charge: an out-of-gas at the charge must not leave the entry warm without its
///   cold cost ever being paid.
/// - Charge before touch: a reverting frame discards the warmth it added (EIP-2929 revert
///   semantics).

#[derive(Default)]
pub struct AccessList {
	/// All currently-hot entries.
	accessed: BoundedBTreeSet<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Flat journal of insertions (in order). Each entry was added by exactly
	/// one frame; `checkpoints` marks the frame boundaries inside this journal.
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

	/// Whether the set is at the entry cap.
	fn is_full(&self) -> bool {
		self.accessed.len() >= MAX_ACCESS_LIST_ENTRIES
	}

	/// Whether a nested-frame checkpoint is open.
	fn in_nested_frame(&self) -> bool {
		!self.checkpoints.is_empty()
	}

	/// Non-mutating sibling of [`touch`](Self::touch): the `Warmth` a touch
	/// of this entry would return.
	pub fn peek(&self, entry: &AccessEntry) -> Warmth {
		if self.accessed.contains(entry) {
			Warmth::Hot
		} else if self.is_full() {
			Warmth::cold_non_revertible()
		} else {
			Warmth::Cold { revertible: self.in_nested_frame() }
		}
	}

	/// Register the entry, returning its warmth **before** this touch.
	///
	/// Past [`MAX_ACCESS_LIST_ENTRIES`], new entries are billed cold without
	/// being journaled; previously-hot entries continue to bill hot.
	pub fn touch(&mut self, entry: AccessEntry) -> Warmth {
		let kind = if self.is_full() {
			// Past the cap: bill by membership, but never journal.
			self.peek(&entry)
		} else if self
			.accessed
			.try_insert(entry.clone())
			.expect("under cap; checked is_full above; qed")
		{
			// Newly inserted: journal it so the owning frame's rollback can drop it.
			self.journal
				.try_push(entry)
				.expect("journal grows in lockstep with accessed and shares its bound; qed");
			Warmth::Cold { revertible: self.in_nested_frame() }
		} else {
			Warmth::Hot
		};

		match kind {
			Warmth::Cold { .. } => self.cold_count = self.cold_count.saturating_add(1),
			Warmth::Hot => self.hot_count = self.hot_count.saturating_add(1),
		}
		kind
	}

	/// Warms every state item the operation reads, returning its warmth.
	pub fn warm_operation(&mut self, state_access: StateAccess) -> StateWarmth {
		state_access.expand(|entry| self.touch(entry))
	}

	/// Non-mutating sibling of [`warm_operation`](Self::warm_operation).
	pub fn operation_warmth(&self, state_access: StateAccess) -> StateWarmth {
		state_access.expand(|entry| self.peek(&entry))
	}

	/// Warms the two entries a code load reads, returning their warmth.
	pub fn warm_code(&mut self, hash: H256) -> CodeLoadWarmth {
		CodeLoadWarmth {
			info: self.touch(AccessEntry::CodeInfo { hash }),
			blob: self.touch(AccessEntry::CodeBlob { hash }),
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
		assert_eq!(al.touch(a.clone()), Warmth::Cold { revertible: false }, "A: first touch cold");
		assert!(al.touch(a.clone()).is_hot(), "A: second touch hot");

		al.enter_frame();
		assert_eq!(al.frame_depth(), 1);

		assert_eq!(al.touch(b.clone()), Warmth::cold_revertible(), "B in frame 1: cold");
		assert!(al.touch(a.clone()).is_hot(), "A in frame 1: hot via parent");

		al.enter_frame();
		assert!(!al.touch(c.clone()).is_hot(), "C in frame 2: cold");

		al.commit_frame();
		assert_eq!(al.frame_depth(), 1);
		assert!(al.peek(&c).is_hot(), "C: survives frame 2 commit");

		assert!(!al.touch(d.clone()).is_hot(), "D in frame 1: cold");
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

	#[test]
	fn touch_caps_at_max_entries() {
		let mut al = AccessList::new();
		// Fill to the cap with distinct addresses.
		for i in 0..MAX_ACCESS_LIST_ENTRIES {
			let address = H160::from_low_u64_be(i as u64);
			assert!(!al.touch(AccessEntry::Storage { address, slot: Slot::Fix([0; 32]) }).is_hot());
		}
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES);

		let new_entry = AccessEntry::Storage {
			address: H160::from_low_u64_be(MAX_ACCESS_LIST_ENTRIES as u64),
			slot: Slot::Fix([0; 32]),
		};
		// Past the cap a new entry is billed cold but never journaled, so even
		// inside an open frame it can't be rolled back.
		al.enter_frame();
		assert_eq!(
			al.touch(new_entry.clone()),
			Warmth::Cold { revertible: false },
			"past cap: bills cold, not revertible",
		);
		al.commit_frame();
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES, "set size stays at cap");
		assert!(!al.peek(&new_entry).is_hot(), "past-cap entry is not tracked");

		assert!(!al.touch(new_entry).is_hot(), "past cap re-touch: still cold (not tracked)");

		let existing = AccessEntry::Storage { address: H160::zero(), slot: Slot::Fix([0; 32]) };
		assert!(al.touch(existing).is_hot(), "existing entry still hot at cap");
	}

	#[test]
	fn call_peek_and_touch_diverge_at_cap_boundary() {
		let mut al = AccessList::new();
		// Fill to one below the cap.
		for i in 0..(MAX_ACCESS_LIST_ENTRIES - 1) {
			al.touch(AccessEntry::Storage {
				address: H160::from_low_u64_be(i as u64),
				slot: Slot::Fix([0; 32]),
			});
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

		al.touch(entry.clone());

		assert!(al.peek(&entry).is_hot(), "after touch: hot");
		assert!(al.peek(&entry).is_hot(), "repeated query: still hot");
		assert_eq!(
			al.metrics(),
			AccessListMetrics { size: 1, cold: 1, hot: 0 },
			"peek must not bump the hot counter",
		);
	}
}

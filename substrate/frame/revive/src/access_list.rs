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

#[cfg(any(test, feature = "runtime-benchmarks"))]
use alloc::vec;

use core::cmp::Ordering;

use alloc::{
	collections::btree_map::{BTreeMap, Entry},
	vec::Vec,
};

use frame_support::{BoundedVec, defensive_assert};
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
/// Memory grows discontinuously due to the runtime allocator (sc-allocator)
/// rounding allocations up to power-of-2 size classes.
///
/// All figures below are approximate order-of-magnitude estimates; every slot
/// includes an upgrade. The Ethereum-gas column shows the EIP-2929 cost of
/// filling the map to that size via cold SLOADs (2 100 gas each).
///
/// | Entries | Fix/Inline (Best) | VarLong (Worst) |     Gas (Ethereum) |
/// |---------|-------------------|-----------------|--------------------|
/// |       1 |      ~1.5 KB      |     ~1.8 KB     |          2.1 k gas |
/// |       2 |      ~1.5 KB      |     ~2.2 KB     |          4.2 k gas |
/// |       8 |      ~2.3 KB      |     ~5.3 KB     |         16.8 k gas |
/// |      32 |      ~11 KB       |      ~23 KB     |         67.2 k gas |
/// |     128 |      ~45 KB       |      ~96 KB     |          269 k gas |
/// |   2 048 |      ~730 KB      |     ~1.5 MB     |          4.3 M gas |
///
/// Set ~2× above the current PoV-reachable ceiling as a backstop: each
/// cold access charges ~10 KB `proof_size`, capping a transaction
/// (~7.5 MiB PoV) at ~770 cold touches.
pub const MAX_ACCESS_LIST_ENTRIES: usize = 2_048;

/// Worst-case per-entry memory in the `BTreeMap` + journals, measured
/// against sc-allocator (8-byte headers, power-of-2 buckets). `Slot::Fix` and
/// `Slot::VarInline` measure ~366 B; `Slot::VarLong` ~502 B. An entry in the
/// `upgrades` journal adds up to ~200 B on top. Rounded up to 768 for
/// headroom.
const MAX_ACCESS_LIST_ENTRY_BYTES: usize = 768;

/// Worst-case total memory the access list can hold per transaction.
pub const MAX_ACCESS_LIST_BYTES: u32 =
	MAX_ACCESS_LIST_ENTRIES.saturating_mul(MAX_ACCESS_LIST_ENTRY_BYTES) as u32;

/// A contract's storage key, in the form the access list keeps it.
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

/// A slot inside one contract's storage.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ContractSlot {
	pub slot: Slot,
	pub address: H160,
}

impl Ord for ContractSlot {
	/// Compares on `slot` first, the most-discriminating field in the typical access pattern
	/// (one contract touching many slots within a transaction).
	fn cmp(&self, other: &Self) -> Ordering {
		self.slot.cmp(&other.slot).then_with(|| self.address.cmp(&other.address))
	}
}

impl PartialOrd for ContractSlot {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

/// The kind of key an entry carries. Slots are much longer than addresses, so each family has
/// its own benchmarks.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum KeyFamily {
	/// A storage slot.
	Slot,
	/// An address or a code hash.
	Address,
}

/// The operation an access performs on a state item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOp {
	Read,
	Write,
}

impl StorageOp {
	/// Returns whether charging `self` also pays for `op`.
	pub fn covers(self, op: StorageOp) -> bool {
		match self {
			StorageOp::Write => true,
			StorageOp::Read => matches!(op, StorageOp::Read),
		}
	}
}

/// Warmth of an access-list entry, as it stood **before** the access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Warmth {
	/// Entry is in the access list; `charged` is the operation it has paid for.
	Hot { charged: StorageOp },
	/// Entry is not in the access list; when `revertible` is true, the touch
	/// rolls back with the current frame.
	Cold { revertible: bool },
}

impl Warmth {
	/// Returns whether the entry is already in the access list.
	pub fn is_hot(&self) -> bool {
		matches!(self, Self::Hot { .. })
	}

	/// Returns whether a write on this entry costs more than what it already paid.
	fn owes_upgrade(self, op: StorageOp) -> bool {
		matches!(self, Self::Hot { charged } if !charged.covers(op))
	}

	/// Returns whether the entry is removed from the access list if the frame reverts.
	pub fn is_revertible(&self) -> bool {
		matches!(self, Self::Cold { revertible: true })
	}

	/// Returns a cold warmth whose touch stays in the list if the frame reverts.
	pub fn cold_non_revertible() -> Self {
		Self::Cold { revertible: false }
	}

	/// Returns a cold warmth whose touch is dropped if the frame reverts.
	pub fn cold_revertible() -> Self {
		Self::Cold { revertible: true }
	}

	/// Returns this warmth with a cold touch made non-revertible.
	pub fn to_non_revertible(self) -> Self {
		match self {
			Self::Hot { charged } => Self::Hot { charged },
			Self::Cold { .. } => Self::Cold { revertible: false },
		}
	}

	/// Returns a hot warmth that has paid for a read.
	#[cfg(test)]
	pub fn read_paid() -> Self {
		Self::Hot { charged: StorageOp::Read }
	}

	/// Returns a hot warmth that has paid for a write.
	#[cfg(test)]
	pub fn write_paid() -> Self {
		Self::Hot { charged: StorageOp::Write }
	}
}

/// An access's entries counted by warmth.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct WarmthSummary {
	/// Entries this access pays for.
	pub total: u32,
	/// How many are cold.
	pub cold: u32,
	/// How many of the cold ones roll back with the frame.
	pub cold_revertible: u32,
	/// Hot entries being written that had paid only for a read.
	pub upgrades: u32,
}

impl WarmthSummary {
	/// Counts an entry this access pays for, at the operation it performs on it.
	pub fn count(mut self, warmth: Warmth, op: StorageOp) -> Self {
		self.total = self.total.saturating_add(1);
		match warmth {
			Warmth::Cold { revertible } => {
				self.cold = self.cold.saturating_add(1);
				self.cold_revertible = self.cold_revertible.saturating_add(u32::from(revertible));
			},
			Warmth::Hot { .. } => {
				self.upgrades = self.upgrades.saturating_add(u32::from(warmth.owes_upgrade(op)))
			},
		}
		self
	}

	/// Returns whether every entry was already in the access list.
	pub fn all_hot(&self) -> bool {
		self.cold == 0
	}

	/// Returns how many entries were already in the access list.
	pub fn hot(&self) -> u32 {
		self.total.saturating_sub(self.cold)
	}
}

/// A state item the access list tracks, with the address, code hash or slot that identifies it.
#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
pub enum AccessEntry {
	/// Account state (`System::Account`) of `address`.
	Account { address: H160 },
	/// Address mapping (`OriginalAccount`) of `address`.
	OriginalAccount { address: H160 },
	/// Contract info (`AccountInfoOf`) of `address`.
	AccountInfo { address: H160 },
	/// Code info (`CodeInfoOf`), keyed by code hash: contracts with the same code share one entry.
	CodeInfo { hash: H256 },
	/// Code blob (`PristineCode`), keyed by code hash for the same reason.
	CodeBlob { hash: H256 },
	/// A contract storage slot, the only entry a contract can hold many of.
	Storage(ContractSlot),
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
impl AccessEntry {
	/// Builds the `i`-th entry of `key`'s family. Only the trailing bytes carry `i`, so a
	/// comparison runs the whole shared prefix before it can decide.
	pub fn with_index(i: usize, key: KeyFamily) -> Self {
		match key {
			KeyFamily::Slot => {
				let slot = Key::try_from_var(vec![0xFFu8; limits::STORAGE_KEY_BYTES as usize])
					.expect("key fits STORAGE_KEY_BYTES bound; qed");
				Self::Storage(ContractSlot {
					slot: Slot::from(&slot),
					address: H160::from_low_u64_be(i as u64),
				})
			},
			KeyFamily::Address => Self::CodeInfo { hash: H256::from_low_u64_be(i as u64) },
		}
	}
}

/// A group of state items that warm and price together.
pub trait Access {
	type Warmth;

	/// The family every entry this access touches belongs to.
	const KEY_FAMILY: KeyFamily;

	/// Calls `visit` for each state item this access touches, with the operation it performs on it.
	fn expand(self, visit: impl FnMut(AccessEntry, StorageOp) -> Warmth) -> Self::Warmth;

	/// Returns how many entries this access touches.
	#[cfg(test)]
	fn entry_count(self) -> u32
	where
		Self: Sized,
	{
		let mut entries = 0;
		self.expand(|_entry, _op| {
			entries += 1;
			Warmth::cold_non_revertible()
		});
		entries
	}
}

/// Warmth of the entries [`CallItems`] covers, one variant per call kind.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub enum CallWarmth {
	/// A normal call touches the target's address mapping and account info; `transfer` carries the
	/// value transfer's warmth when the call moves value.
	Plain { original_account: Warmth, account_info: Warmth, transfer: Option<TransferWarmth> },
	/// A delegate call reads only the target's account info.
	Delegate { account_info: Warmth },
}

impl CallWarmth {
	/// Summarizes the entries the call itself pays for. A dust transfer pays the write on the
	/// callee's account info, so this counts it as a read.
	pub(crate) fn summary(&self) -> WarmthSummary {
		match self {
			Self::Plain { original_account, account_info, .. } => WarmthSummary::default()
				.count(*original_account, StorageOp::Read)
				.count(*account_info, StorageOp::Read),
			Self::Delegate { account_info } => {
				WarmthSummary::default().count(*account_info, StorageOp::Read)
			},
		}
	}

	/// Returns the value transfer's warmth, `None` when the call moves no value.
	pub fn transfer_warmth(self) -> Option<TransferWarmth> {
		match self {
			Self::Plain { transfer, .. } => transfer,
			Self::Delegate { .. } => None,
		}
	}
}

/// A call opcode's access, one variant per call kind.
#[derive(Clone, Copy, Debug)]
pub enum CallItems {
	Plain { target: H160, transfer: Option<TransferItems> },
	Delegate { target: H160 },
}

impl CallItems {
	/// Builds the call variant matching the `delegate` flag.
	pub fn new(target: H160, delegate: bool, transfer: Option<TransferItems>) -> Self {
		if delegate { Self::Delegate { target } } else { Self::Plain { target, transfer } }
	}
}

#[cfg(test)]
impl CallItems {
	/// Returns the entries a plain call to a contract touches: its own, plus the callee's code.
	pub(crate) fn plain_entries() -> u32 {
		Self::Plain { target: H160::zero(), transfer: None }.entry_count() +
			CodeLoadItems { hash: H256::zero() }.entry_count()
	}

	/// Returns the entries a plain call touches when it moves value: everything a zero-value call
	/// touches, plus the sender's and the receiver's account state and the sender's account info.
	pub(crate) fn value_call_entries() -> u32 {
		let transfer = TransferItems { from: H160::repeat_byte(1), dust: false };
		Self::Plain { target: H160::zero(), transfer: Some(transfer) }.entry_count() +
			CodeLoadItems { hash: H256::zero() }.entry_count()
	}

	/// Returns the entries a delegate call touches: the target's account info, plus its code.
	pub(crate) fn delegate_entries() -> u32 {
		Self::Delegate { target: H160::zero() }.entry_count() +
			CodeLoadItems { hash: H256::zero() }.entry_count()
	}
}

impl Access for CallItems {
	type Warmth = CallWarmth;
	const KEY_FAMILY: KeyFamily = KeyFamily::Address;

	fn expand(self, mut visit: impl FnMut(AccessEntry, StorageOp) -> Warmth) -> CallWarmth {
		match self {
			Self::Plain { target, transfer } => {
				let dust = transfer.is_some_and(|transfer| transfer.dust);
				let account_info_op = TransferItems::account_info_op(dust);
				let original_account =
					visit(AccessEntry::OriginalAccount { address: target }, StorageOp::Read);
				let account_info =
					visit(AccessEntry::AccountInfo { address: target }, account_info_op);
				let transfer = transfer.map(|transfer| TransferWarmth {
					account: visit(AccessEntry::Account { address: target }, StorageOp::Write),
					sender_account: visit(
						AccessEntry::Account { address: transfer.from },
						StorageOp::Write,
					),
					account_info,
					sender_account_info: visit(
						AccessEntry::AccountInfo { address: transfer.from },
						account_info_op,
					),
				});
				CallWarmth::Plain { original_account, account_info, transfer }
			},
			Self::Delegate { target } => CallWarmth::Delegate {
				account_info: visit(AccessEntry::AccountInfo { address: target }, StorageOp::Read),
			},
		}
	}
}

/// Warmth of the entries a call's value transfer touches.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub struct TransferWarmth {
	pub account: Warmth,
	pub sender_account: Warmth,
	/// The target's account info: the call reads it either way, a dust transfer also writes it.
	pub account_info: Warmth,
	pub sender_account_info: Warmth,
}

/// The value transfer a call performs.
#[derive(Clone, Copy, Debug)]
pub struct TransferItems {
	pub from: H160,
	pub dust: bool,
}

impl TransferWarmth {
	/// Summarizes the transfer's entries: the three it pays for, plus the write it owes on the
	/// callee's account info, which the call pays only to read.
	pub(crate) fn summary(&self, dust: bool) -> WarmthSummary {
		let account_info_op = TransferItems::account_info_op(dust);
		let mut summary = WarmthSummary::default()
			.count(self.account, StorageOp::Write)
			.count(self.sender_account, StorageOp::Write)
			.count(self.sender_account_info, account_info_op);
		summary.upgrades += u32::from(self.account_info.owes_upgrade(account_info_op));
		summary
	}
}

impl TransferItems {
	/// Returns `Write` when the transfer carries dust, else `Read`.
	pub(crate) fn account_info_op(dust: bool) -> StorageOp {
		if dust { StorageOp::Write } else { StorageOp::Read }
	}
}

/// Warmth of the entries a code load reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodeLoadWarmth {
	/// The `CodeInfoOf` entry.
	pub info: Warmth,
	/// The `PristineCode` entry.
	pub blob: Warmth,
}

impl CodeLoadWarmth {
	/// Summarizes both entries a code load pays for.
	pub(crate) fn summary(&self) -> WarmthSummary {
		WarmthSummary::default()
			.count(self.info, StorageOp::Read)
			.count(self.blob, StorageOp::Read)
	}

	pub fn cold_non_revertible() -> Self {
		Self { info: Warmth::cold_non_revertible(), blob: Warmth::cold_non_revertible() }
	}
}

/// A code load reads the info and the blob at `hash`.
#[derive(Clone, Copy, Debug)]
pub struct CodeLoadItems {
	pub hash: H256,
}

impl Access for CodeLoadItems {
	type Warmth = CodeLoadWarmth;
	const KEY_FAMILY: KeyFamily = KeyFamily::Address;

	fn expand(self, mut visit: impl FnMut(AccessEntry, StorageOp) -> Warmth) -> CodeLoadWarmth {
		CodeLoadWarmth {
			info: visit(AccessEntry::CodeInfo { hash: self.hash }, StorageOp::Read),
			blob: visit(AccessEntry::CodeBlob { hash: self.hash }, StorageOp::Read),
		}
	}
}

/// A read or write of one contract slot.
#[derive(Clone, Debug)]
pub struct StorageItems {
	pub key: ContractSlot,
	pub op: StorageOp,
}

impl StorageItems {
	/// Builds the items `op` touches on `address`'s storage slot `key`.
	pub fn new(address: H160, key: &Key, op: StorageOp) -> Self {
		Self { key: ContractSlot { slot: key.into(), address }, op }
	}
}

impl Access for StorageItems {
	type Warmth = Warmth;
	const KEY_FAMILY: KeyFamily = KeyFamily::Slot;

	fn expand(self, mut resolve: impl FnMut(AccessEntry, StorageOp) -> Warmth) -> Warmth {
		resolve(AccessEntry::Storage(self.key), self.op)
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
/// - Storage opcodes and calls touch before charging: their charge fails only on out-of-gas, which
///   reverts the whole frame, so the rollback removes the frame's insertions (and downgrades any
///   `Read` to `Write` upgrades) and never leaves an entry warm with its cold cost unpaid.
/// - Code loads touch after charging: the entry is already paid, so a revert just drops it.

#[derive(Default)]
pub struct AccessList {
	/// All currently-hot entries with the cost each has paid.
	///
	/// Not a `BoundedBTreeMap` because it has no `entry` API, which would make a
	/// cold touch search the map twice.
	accessed: BTreeMap<AccessEntry, StorageOp>,
	/// Flat journal of insertions (in order); each entry was added by exactly
	/// one frame, and `checkpoints` marks the frame boundaries inside this journal.
	journal: BoundedVec<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Flat journal of `Read` to `Write` upgrades (in order).
	upgrades: BoundedVec<AccessEntry, ConstU32<{ MAX_ACCESS_LIST_ENTRIES as u32 }>>,
	/// Stack of `(journal, upgrades)` lengths at frame entry.
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
		self.checkpoints.pop().expect(
			"A call to commit_frame must be preceded by a corresponding call to enter_frame;
			Stack::run closes every checkpoint it opens; qed",
		);
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
		let (journal_checkpoint, upgrades_checkpoint) = self.checkpoints.pop().expect(
			"A call to rollback_frame must be preceded by a corresponding call to enter_frame;
			Stack::run closes every checkpoint it opens; qed",
		);
		for entry in self.journal.drain(journal_checkpoint..) {
			self.accessed.remove(&entry);
		}
		for entry in self.upgrades.drain(upgrades_checkpoint..) {
			// Removed already if the same frame also inserted the entry.
			if let Some(charged) = self.accessed.get_mut(&entry) {
				*charged = StorageOp::Read;
			}
		}
	}

	/// Returns whether the map is at the entry cap.
	fn is_full(&self) -> bool {
		self.accessed.len() >= MAX_ACCESS_LIST_ENTRIES
	}

	/// Returns whether a nested-frame checkpoint is open.
	fn in_nested_frame(&self) -> bool {
		!self.checkpoints.is_empty()
	}

	/// Reports the warmth [`Self::touch`] would return, without recording it.
	fn peek(&self, entry: &AccessEntry) -> Warmth {
		match self.accessed.get(entry) {
			Some(charged) => Warmth::Hot { charged: *charged },
			None if self.is_full() => Warmth::Cold { revertible: false },
			None => Warmth::Cold { revertible: self.in_nested_frame() },
		}
	}

	/// Registers the entry, returning the warmth it had **before** this call.
	/// `op` is the operation being performed on the slot.
	///
	/// Past [`MAX_ACCESS_LIST_ENTRIES`], new entries are billed cold without
	/// being journaled; previously-hot slots continue to bill hot.
	pub fn touch(&mut self, access_entry: AccessEntry, op: StorageOp) -> Warmth {
		let at_cap = self.is_full();
		match self.accessed.entry(access_entry) {
			Entry::Occupied(mut tree_entry) => {
				self.hot_count = self.hot_count.saturating_add(1);
				let prev_charged = *tree_entry.get();
				if !prev_charged.covers(op) {
					// Defensive: one upgrade per tracked entry, so the journal
					// cannot fill. If it does, later writes just pay the surcharge again.
					let journaled = self.upgrades.try_push(tree_entry.key().clone());
					defensive_assert!(journaled.is_ok(), "at most one upgrade per tracked entry");
					if journaled.is_ok() {
						*tree_entry.get_mut() = StorageOp::Write;
					}
				}
				Warmth::Hot { charged: prev_charged }
			},
			Entry::Vacant(tree_entry) => {
				self.cold_count = self.cold_count.saturating_add(1);
				if at_cap {
					return Warmth::Cold { revertible: false };
				}
				self.journal
					.try_push(tree_entry.key().clone())
					.expect("journal grows in lockstep with accessed and shares its bound; qed");
				tree_entry.insert(op);
				Warmth::Cold { revertible: self.in_nested_frame() }
			},
		}
	}

	/// Warms every entry the access touches, returning the warmth each had
	/// **before** this call.
	pub fn warm<A: Access>(&mut self, access: A) -> A::Warmth {
		access.expand(|entry, op| self.touch(entry, op))
	}

	/// Reports what [`Self::warm`] would return, without recording anything.
	pub fn warmth_of<A: Access>(&self, access: A) -> A::Warmth {
		let mut free_slots = MAX_ACCESS_LIST_ENTRIES.saturating_sub(self.accessed.len());
		access.expand(|entry, _op| match self.peek(&entry) {
			Warmth::Cold { .. } => {
				let revertible = self.in_nested_frame() && free_slots > 0;
				free_slots = free_slots.saturating_sub(1);
				Warmth::Cold { revertible }
			},
			hot => hot,
		})
	}

	/// Returns the number of open checkpoints.
	pub fn frame_depth(&self) -> usize {
		self.checkpoints.len()
	}

	/// Returns a snapshot of the per-transaction metrics.
	pub fn metrics(&self) -> AccessListMetrics {
		AccessListMetrics { size: self.accessed.len(), cold: self.cold_count, hot: self.hot_count }
	}

	/// Builds an access list holding entries `0..entries` of the given `key` family.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn with_entries(entries: usize, key: KeyFamily) -> Self {
		let mut list = Self::new();
		list.fill_to(entries, key);
		list
	}

	/// Adds fresh entries of `key`'s family until the list holds `target_size` of them.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	pub fn fill_to(&mut self, target_size: usize, key: KeyFamily) {
		let already = self.metrics().size;
		assert!(already <= target_size, "the map is already past the target");
		for i in 0..target_size - already {
			let entry = AccessEntry::with_index(i, key);
			assert!(!self.touch(entry, StorageOp::Read).is_hot(), "fill entries must be new");
		}
		assert_eq!(self.metrics().size, target_size, "the map reached the requested size");
	}

	/// Returns the first entry in key order.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn first(&self) -> AccessEntry {
		self.accessed
			.keys()
			.next()
			.expect("fixtures only ask a non-empty list; qed")
			.clone()
	}

	/// Returns the last entry in key order.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn last(&self) -> AccessEntry {
		self.accessed
			.keys()
			.next_back()
			.expect("fixtures only ask a non-empty list; qed")
			.clone()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn nested_commit_then_parent_rollback_drops_all() {
		let mut al = AccessList::new();
		let (a, b, c, d) = (
			AccessEntry::Storage(ContractSlot {
				slot: Slot::Fix([0xA; 32]),
				address: H160::zero(),
			}),
			AccessEntry::Account { address: H160::zero() },
			AccessEntry::AccountInfo { address: H160::zero() },
			AccessEntry::CodeBlob { hash: H256::repeat_byte(0xD) },
		);

		// Root frame: cold, but no checkpoint covers it, so it is not revertible.
		assert_eq!(
			al.touch(a.clone(), StorageOp::Read),
			Warmth::cold_non_revertible(),
			"A: first touch cold"
		);
		assert!(al.touch(a.clone(), StorageOp::Read).is_hot(), "A: second touch hot");

		al.enter_frame();
		assert_eq!(al.frame_depth(), 1);

		// Inside F1: journaled under the open checkpoint, so it is revertible.
		assert_eq!(
			al.touch(b.clone(), StorageOp::Read),
			Warmth::cold_revertible(),
			"B in F1: cold"
		);
		assert!(al.touch(a.clone(), StorageOp::Read).is_hot(), "A in F1: hot via parent");

		al.enter_frame();
		assert!(!al.touch(c.clone(), StorageOp::Read).is_hot(), "C in F2: cold");

		al.commit_frame();
		assert_eq!(al.frame_depth(), 1);
		assert!(al.peek(&c).is_hot(), "C: survives frame 2 commit");

		assert!(!al.touch(d.clone(), StorageOp::Read).is_hot(), "D in F1: cold");
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
		al.fill_to(MAX_ACCESS_LIST_ENTRIES, KeyFamily::Slot);

		let new_entry = AccessEntry::with_index(MAX_ACCESS_LIST_ENTRIES, KeyFamily::Slot);
		al.enter_frame();
		assert_eq!(
			al.touch(new_entry.clone(), StorageOp::Read),
			Warmth::cold_non_revertible(),
			"past cap: bills cold, not revertible",
		);
		al.commit_frame();
		assert_eq!(al.metrics().size, MAX_ACCESS_LIST_ENTRIES, "map size stays at cap");
		assert!(!al.peek(&new_entry).is_hot(), "past-cap entry is not tracked");

		assert!(
			!al.touch(new_entry, StorageOp::Read).is_hot(),
			"past cap re-touch: still cold (not tracked)"
		);

		let existing = AccessEntry::with_index(0, KeyFamily::Slot);
		assert!(
			al.touch(existing.clone(), StorageOp::Read).is_hot(),
			"existing entry still hot at cap"
		);

		// A write can still upgrade a tracked slot once the map is full.
		assert_eq!(
			al.touch(existing.clone(), StorageOp::Write),
			Warmth::read_paid(),
			"first write at cap: was read-paid",
		);
		assert_eq!(
			al.touch(existing, StorageOp::Write),
			Warmth::write_paid(),
			"write at cap: upgraded",
		);

		assert_eq!(
			al.metrics().size,
			MAX_ACCESS_LIST_ENTRIES,
			"the cap holds across past-cap touches and upgrades",
		);
	}

	#[test]
	fn touches_never_downgrade_the_paid_level() {
		let mut al = AccessList::new();
		let entry =
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([2; 32]), address: H160::zero() });

		let read_paid = Warmth::read_paid();
		let write_paid = Warmth::write_paid();

		assert!(!al.touch(entry.clone(), StorageOp::Read).is_hot(), "first read: cold");
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

		let written =
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([3; 32]), address: H160::zero() });
		assert!(!al.touch(written.clone(), StorageOp::Write).is_hot(), "first write: cold");
		assert_eq!(al.touch(written, StorageOp::Write), write_paid, "cold write starts at Write");
	}

	#[test]
	fn peek_agrees_with_touch() {
		fn agree(al: &mut AccessList, entry: AccessEntry, op: StorageOp, expected: Warmth) {
			let before = al.metrics();
			assert_eq!(al.peek(&entry), expected, "peek must report like touch");
			assert_eq!(al.metrics(), before, "peek must not mutate the list");
			assert_eq!(al.touch(entry, op), expected, "touch must report like peek");
		}

		// Indices past the fill range, so the fill never hands out these entries itself.
		let upgraded = AccessEntry::with_index(MAX_ACCESS_LIST_ENTRIES, KeyFamily::Slot);
		let reverted = AccessEntry::with_index(MAX_ACCESS_LIST_ENTRIES + 1, KeyFamily::Slot);
		let upgraded_at_cap = AccessEntry::with_index(MAX_ACCESS_LIST_ENTRIES + 2, KeyFamily::Slot);

		let mut al = AccessList::new();

		agree(&mut al, upgraded.clone(), StorageOp::Read, Warmth::cold_non_revertible());
		agree(&mut al, upgraded.clone(), StorageOp::Read, Warmth::read_paid());
		agree(&mut al, upgraded.clone(), StorageOp::Write, Warmth::read_paid());
		agree(&mut al, upgraded.clone(), StorageOp::Write, Warmth::write_paid());
		agree(&mut al, upgraded, StorageOp::Read, Warmth::write_paid());

		al.enter_frame();
		agree(&mut al, reverted.clone(), StorageOp::Write, Warmth::cold_revertible());
		al.rollback_frame();

		// Touched before the fill, so it is already in the list when the list fills up.
		agree(&mut al, upgraded_at_cap.clone(), StorageOp::Read, Warmth::cold_non_revertible());
		al.fill_to(MAX_ACCESS_LIST_ENTRIES, KeyFamily::Slot);

		al.enter_frame();
		// The rolled-back entry is gone, so at the cap it reads like any untracked one.
		agree(&mut al, reverted, StorageOp::Write, Warmth::cold_non_revertible());
		// A tracked read-paid slot still upgrades at the cap.
		agree(&mut al, upgraded_at_cap, StorageOp::Write, Warmth::read_paid());
	}

	#[test]
	fn a_committed_upgrade_rolls_back_with_the_parent_frame() {
		let mut al = AccessList::new();
		let entry =
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([9; 32]), address: H160::zero() });
		al.touch(entry.clone(), StorageOp::Read);

		al.enter_frame();
		al.enter_frame();
		assert_eq!(al.touch(entry.clone(), StorageOp::Write), Warmth::read_paid());
		al.commit_frame();
		assert_eq!(
			al.peek(&entry),
			Warmth::write_paid(),
			"a committed upgrade belongs to the parent frame"
		);
		al.rollback_frame();
		assert_eq!(
			al.peek(&entry),
			Warmth::read_paid(),
			"the parent's revert drops the committed upgrade"
		);
	}

	#[test]
	fn upgrade_survives_a_nested_frames_rollback() {
		let mut al = AccessList::new();
		let upgraded =
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([7; 32]), address: H160::zero() });
		al.touch(upgraded.clone(), StorageOp::Read);
		al.touch(upgraded.clone(), StorageOp::Write);

		al.enter_frame();
		al.touch(
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([6; 32]), address: H160::zero() }),
			StorageOp::Write,
		);
		al.rollback_frame();

		assert_eq!(
			al.peek(&upgraded),
			Warmth::write_paid(),
			"a rollback must only drop its own frame's upgrades"
		);
	}

	#[test]
	fn same_frame_insert_and_upgrade_roll_back_together() {
		let mut al = AccessList::new();
		let entry =
			AccessEntry::Storage(ContractSlot { slot: Slot::Fix([8; 32]), address: H160::zero() });
		al.enter_frame();
		al.touch(entry.clone(), StorageOp::Read);
		al.touch(entry.clone(), StorageOp::Write);
		al.rollback_frame();
		assert!(!al.peek(&entry).is_hot(), "the entry and its upgrade are both gone");
	}

	#[test]
	fn the_senders_transfer_entries_stay_hot_for_the_next_call() {
		let mut al = AccessList::new();
		let sender = H160::from_low_u64_be(0xcafe);
		let transfer = Some(TransferItems { from: sender, dust: false });

		// A first value call, that puts the sender's two entries in the list.
		al.warm(CallItems::Plain { target: H160::from_low_u64_be(1), transfer });

		let expected = CallWarmth::Plain {
			original_account: Warmth::cold_non_revertible(),
			account_info: Warmth::cold_non_revertible(),
			transfer: Some(TransferWarmth {
				account: Warmth::cold_non_revertible(),
				sender_account: Warmth::write_paid(), // a transfer always writes the balance
				account_info: Warmth::cold_non_revertible(),
				sender_account_info: Warmth::read_paid(), // no dust, so it was only read
			}),
		};
		assert_eq!(
			al.warmth_of(CallItems::Plain { target: H160::from_low_u64_be(2), transfer }),
			expected
		);
	}

	#[test]
	fn only_the_call_entries_that_fit_the_cap_are_revertible() {
		let mut al = AccessList::new();
		al.fill_to(MAX_ACCESS_LIST_ENTRIES - 1, KeyFamily::Slot);

		let target = H160::from_low_u64_be(0xdead_beef);

		al.enter_frame();

		let sender = H160::from_low_u64_be(0xcafe);
		let access = CallItems::Plain {
			target,
			transfer: Some(TransferItems { from: sender, dust: false }),
		};

		let expected = CallWarmth::Plain {
			original_account: Warmth::cold_revertible(), // the only entry that fits
			account_info: Warmth::cold_non_revertible(),
			transfer: Some(TransferWarmth {
				account: Warmth::cold_non_revertible(),
				sender_account: Warmth::cold_non_revertible(),
				account_info: Warmth::cold_non_revertible(),
				sender_account_info: Warmth::cold_non_revertible(),
			}),
		};
		assert_eq!(al.warmth_of(access), expected);
		assert_eq!(al.warm(access), expected);
	}

	#[test]
	fn a_storage_key_orders_by_slot_before_address() {
		let key = |slot: u8, address: u64| ContractSlot {
			slot: Slot::Fix([slot; 32]),
			address: H160::from_low_u64_be(address),
		};

		assert!(
			key(1, 9) < key(2, 0),
			"a smaller slot sorts first even when its address is larger"
		);
		assert!(key(1, 0) < key(1, 1), "the address only breaks ties inside the same slot");
	}

	#[test]
	fn an_access_touches_the_expected_entries() {
		// A second access reports the level the first one recorded.
		fn recorded<A: Access + Clone>(access: A) -> A::Warmth {
			let mut al = AccessList::new();
			al.warm(access.clone());
			al.warm(access)
		}

		let sender = H160::repeat_byte(1);
		let target = H160::repeat_byte(2);
		let transfer = |dust| Some(TransferItems { from: sender, dust });

		assert_eq!(
			recorded(CallItems::Plain { target, transfer: None }),
			CallWarmth::Plain {
				original_account: Warmth::read_paid(),
				account_info: Warmth::read_paid(),
				transfer: None,
			},
		);

		assert_eq!(
			recorded(CallItems::Delegate { target }),
			CallWarmth::Delegate { account_info: Warmth::read_paid() },
		);

		assert_eq!(
			recorded(CodeLoadItems { hash: H256::zero() }),
			CodeLoadWarmth { info: Warmth::read_paid(), blob: Warmth::read_paid() },
		);

		let slot_access = |op| StorageItems::new(target, &Key::Fix([3; 32]), op);
		assert_eq!(recorded(slot_access(StorageOp::Read)), Warmth::read_paid());
		assert_eq!(recorded(slot_access(StorageOp::Write)), Warmth::write_paid());

		assert_eq!(
			recorded(CallItems::Plain { target, transfer: transfer(false) }),
			CallWarmth::Plain {
				original_account: Warmth::read_paid(),
				account_info: Warmth::read_paid(),
				transfer: Some(TransferWarmth {
					account: Warmth::write_paid(),
					sender_account: Warmth::write_paid(),
					account_info: Warmth::read_paid(),
					sender_account_info: Warmth::read_paid(),
				}),
			},
		);

		assert_eq!(
			recorded(CallItems::Plain { target, transfer: transfer(true) }),
			CallWarmth::Plain {
				original_account: Warmth::read_paid(),
				account_info: Warmth::write_paid(),
				transfer: Some(TransferWarmth {
					account: Warmth::write_paid(),
					sender_account: Warmth::write_paid(),
					account_info: Warmth::write_paid(),
					sender_account_info: Warmth::write_paid(),
				}),
			},
		);
	}
}

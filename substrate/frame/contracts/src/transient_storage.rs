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

//! This module contains routines for accessing and altering a contract transient storage.

use crate::{
	exec::{AccountIdOf, Key},
	storage::WriteOutcome,
	Config, Error,
};
use alloc::{collections::BTreeMap, vec::Vec};
use codec::Encode;
use core::{marker::PhantomData, mem};
use frame_support::DefaultNoBound;
use sp_runtime::{DispatchError, DispatchResult, Saturating};

/// Meter entry tracks transaction allocations.
#[derive(Default, Debug)]
pub struct MeterEntry {
	/// Allocations made in the current transaction.
	pub amount: u32,
	/// Allocations limit in the current transaction.
	pub limit: u32,
}

impl MeterEntry {
	/// Create a new entry.
	fn new(limit: u32) -> Self {
		Self { limit, amount: Default::default() }
	}

	/// Check if the allocated amount exceeds the limit.
	fn exceeds_limit(&self, amount: u32) -> bool {
		self.amount.saturating_add(amount) > self.limit
	}

	/// Absorb the allocation amount of the nested entry into the current entry.
	fn absorb(&mut self, rhs: Self) {
		self.amount.saturating_accrue(rhs.amount)
	}
}

// The storage meter enforces a limit for each transaction,
// which is calculated as free_storage * (1 - 1/16) for each subsequent frame.
#[derive(DefaultNoBound)]
pub struct StorageMeter<T: Config> {
	nested_meters: Vec<MeterEntry>,
	root_meter: MeterEntry,
	_phantom: PhantomData<T>,
}

impl<T: Config> StorageMeter<T> {
	const STORAGE_FRACTION_DENOMINATOR: u32 = 16;
	/// Create a new storage allocation meter.
	fn new(memory_limit: u32) -> Self {
		Self { root_meter: MeterEntry::new(memory_limit), ..Default::default() }
	}

	/// Charge the allocated amount of transaction storage from the meter.
	fn charge(&mut self, amount: u32) -> DispatchResult {
		let meter = self.current_mut();
		if meter.exceeds_limit(amount) {
			return Err(Error::<T>::OutOfTransientStorage.into());
		}
		meter.amount.saturating_accrue(amount);
		Ok(())
	}

	/// Revert a transaction meter.
	fn revert(&mut self) {
		self.nested_meters.pop().expect(
			"A call to revert a meter must be preceded by a corresponding call to start a meter;
			the code within this crate makes sure that this is always the case; qed",
		);
	}

	/// Start a transaction meter.
	fn start(&mut self) {
		let meter = self.current();
		let mut transaction_limit = meter.limit.saturating_sub(meter.amount);
		if !self.nested_meters.is_empty() {
			// Allow use of (1 - 1/STORAGE_FRACTION_DENOMINATOR) of free storage for subsequent
			// calls.
			transaction_limit.saturating_reduce(
				transaction_limit.saturating_div(Self::STORAGE_FRACTION_DENOMINATOR),
			);
		}

		self.nested_meters.push(MeterEntry::new(transaction_limit));
	}

	/// Commit a transaction meter.
	fn commit(&mut self) {
		let transaction_meter = self.nested_meters.pop().expect(
			"A call to commit a meter must be preceded by a corresponding call to start a meter;
			the code within this crate makes sure that this is always the case; qed",
		);
		self.current_mut().absorb(transaction_meter)
	}

	/// The total allocated amount of memory.
	#[cfg(test)]
	fn total_amount(&self) -> u32 {
		self.nested_meters
			.iter()
			.fold(self.root_meter.amount, |acc, e| acc.saturating_add(e.amount))
	}

	/// A mutable reference to the current meter entry.
	pub fn current_mut(&mut self) -> &mut MeterEntry {
		self.nested_meters.last_mut().unwrap_or(&mut self.root_meter)
	}

	/// A reference to the current meter entry.
	pub fn current(&self) -> &MeterEntry {
		self.nested_meters.last().unwrap_or(&self.root_meter)
	}
}

/// An entry representing a journal change.
struct JournalEntry {
	key: Vec<u8>,
	prev_value: Option<Vec<u8>>,
}

impl JournalEntry {
	/// Create a new change.
	fn new(key: Vec<u8>, prev_value: Option<Vec<u8>>) -> Self {
		Self { key, prev_value }
	}

	/// Revert the change.
	fn revert(self, storage: &mut Storage) {
		storage.write(&self.key, self.prev_value);
	}
}

/// A journal containing transient storage modifications.
struct Journal(Vec<JournalEntry>);

impl Journal {
	/// Create a new journal.
	fn new() -> Self {
		Self(Default::default())
	}

	/// Add a change to the journal.
	fn push(&mut self, entry: JournalEntry) {
		self.0.push(entry);
	}

	/// Length of the journal.
	fn len(&self) -> usize {
		self.0.len()
	}

	/// Roll back all journal changes until the chackpoint
	fn rollback(&mut self, storage: &mut Storage, checkpoint: usize) {
		self.0.drain(checkpoint..).rev().for_each(|entry| entry.revert(storage));
	}
}

/// Storage for maintaining the current transaction state.
#[derive(Default)]
struct Storage(BTreeMap<Vec<u8>, Vec<u8>>);

impl Storage {
	/// Read the storage entry.
	fn read(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
		self.0.get(key).cloned()
	}

	/// Write the storage entry.
	fn write(&mut self, key: &Vec<u8>, value: Option<Vec<u8>>) -> Option<Vec<u8>> {
		if let Some(value) = value {
			// Insert storage entry.
			self.0.insert(key.clone(), value)
		} else {
			// Remove storage entry.
			self.0.remove(key)
		}
	}
}

/// Transient storage behaves almost identically to regular storage but is discarded after each
/// transaction. It consists of a `BTreeMap` for the current state, a journal of all changes, and a
/// list of checkpoints. On entry to the `start_transaction` function, a marker (checkpoint) is
/// added to the list. New values are written to the current state, and the previous value is
/// recorded in the journal (`write`). When the `commit_transaction` function is called, the marker
/// to the journal index (checkpoint) of when that call was entered is discarded.
/// On `rollback_transaction`, all entries are reverted up to the last checkpoint.
pub struct TransientStorage<T: Config> {
	// The storage and journal size is limited by the storage meter.
	storage: Storage,
	journal: Journal,
	// The size of the StorageMeter is limited by the stack depth.
	meter: StorageMeter<T>,
	// The size of the checkpoints is limited by the stack depth.
	checkpoints: Vec<usize>,
}

impl<T: Config> TransientStorage<T> {
	/// Create new transient storage with the supplied memory limit.
	pub fn new(memory_limit: u32) -> Self {
		TransientStorage {
			storage: Default::default(),
			journal: Journal::new(),
			checkpoints: Default::default(),
			meter: StorageMeter::new(memory_limit),
		}
	}

	/// Read the storage value. If the entry does not exist, `None` is returned.
	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Vec<u8>> {
		self.storage.read(&Self::storage_key(&account.encode(), &key.hash()))
	}

	/// Write a value to storage.
	///
	/// If the `value` is `None`, then the entry is removed. If `take` is true,
	/// a [`WriteOutcome::Taken`] is returned instead of a [`WriteOutcome::Overwritten`].
	/// If the entry did not exist, [`WriteOutcome::New`] is returned.
	pub fn write(
		&mut self,
		account: &AccountIdOf<T>,
		key: &Key<T>,
		value: Option<Vec<u8>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let key = Self::storage_key(&account.encode(), &key.hash());
		let prev_value = self.storage.read(&key);
		// Skip if the same value is being set.
		if prev_value != value {
			// Calculate the allocation size.
			if let Some(value) = &value {
				// Charge the key, value and journal entry.
				// If a new value is written, a new journal entry is created. The previous value is
				// moved to the journal along with its key, and the new value is written to
				// storage.
				let key_len = key.capacity();
				let mut amount = value
					.capacity()
					.saturating_add(key_len)
					.saturating_add(mem::size_of::<JournalEntry>());
				if prev_value.is_none() {
					// Charge a new storage entry.
					// If there was no previous value, a new entry is added to storage (BTreeMap)
					// containing a Vec for the key and a Vec for the value. The value was already
					// included in the amount.
					amount.saturating_accrue(key_len.saturating_add(mem::size_of::<Vec<u8>>()));
				}
				self.meter.charge(amount as _)?;
			}
			self.storage.write(&key, value);
			// Update the journal.
			self.journal.push(JournalEntry::new(key, prev_value.clone()));
		}

		Ok(match (take, prev_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(prev_value)) => WriteOutcome::Overwritten(prev_value.len() as _),
			(true, Some(prev_value)) => WriteOutcome::Taken(prev_value),
		})
	}

	/// Start a new nested transaction.
	///
	/// This allows to either commit or roll back all changes that are made after this call.
	/// For every transaction there must be a matching call to either `rollback_transaction`
	/// or `commit_transaction`.
	pub fn start_transaction(&mut self) {
		self.meter.start();
		self.checkpoints.push(self.journal.len());
	}

	/// Rollback the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are discarded.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	pub fn rollback_transaction(&mut self) {
		let checkpoint = self
			.checkpoints
			.pop()
			.expect(
				"A call to rollback_transaction must be preceded by a corresponding call to start_transaction;
				the code within this crate makes sure that this is always the case; qed"
			);
		self.meter.revert();
		self.journal.rollback(&mut self.storage, checkpoint);
	}

	/// Commit the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are committed.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	pub fn commit_transaction(&mut self) {
		self.checkpoints
			.pop()
			.expect(
				"A call to commit_transaction must be preceded by a corresponding call to start_transaction;
				the code within this crate makes sure that this is always the case; qed"
			);
		self.meter.commit();
	}

	/// The storage allocation meter used for transaction metering.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	pub fn meter(&mut self) -> &mut StorageMeter<T> {
		return &mut self.meter
	}

	fn storage_key(account: &[u8], key: &[u8]) -> Vec<u8> {
		let mut storage_key = Vec::with_capacity(account.len() + key.len());
		storage_key.extend_from_slice(&account);
		storage_key.extend_from_slice(&key);
		storage_key
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{Test, ALICE, BOB, CHARLIE},
		Error,
	};
	use core::u32::MAX;

	// Calculate the allocation size for the given entry.
	fn allocation_size(
		account: &AccountIdOf<Test>,
		key: &Key<Test>,
		value: Option<Vec<u8>>,
	) -> u32 {
		let mut storage: TransientStorage<Test> = TransientStorage::<Test>::new(MAX);
		storage
			.write(account, key, value, false)
			.expect("Could not write to transient storage.");
		storage.meter().current().amount
	}

	#[test]
	fn read_write_works() {
		let mut storage: TransientStorage<Test> = TransientStorage::<Test>::new(2048);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![2]), true),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&BOB, &Key::Fix([3; 32]), Some(vec![3]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
		assert_eq!(storage.read(&ALICE, &Key::Fix([2; 32])), Some(vec![2]));
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), Some(vec![3]));
		// Overwrite values.
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![4, 5]), false),
			Ok(WriteOutcome::Overwritten(1))
		);
		assert_eq!(
			storage.write(&BOB, &Key::Fix([3; 32]), Some(vec![6, 7]), true),
			Ok(WriteOutcome::Taken(vec![3]))
		);
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
		assert_eq!(storage.read(&ALICE, &Key::Fix([2; 32])), Some(vec![4, 5]));
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), Some(vec![6, 7]));

		// Check for an empty value.
		assert_eq!(
			storage.write(&BOB, &Key::Fix([3; 32]), Some(vec![]), true),
			Ok(WriteOutcome::Taken(vec![6, 7]))
		);
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), Some(vec![]));

		assert_eq!(
			storage.write(&BOB, &Key::Fix([3; 32]), None, true),
			Ok(WriteOutcome::Taken(vec![]))
		);
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), None);
	}

	#[test]
	fn read_write_with_var_sized_keys_works() {
		let mut storage = TransientStorage::<Test>::new(2048);
		assert_eq!(
			storage.write(
				&ALICE,
				&Key::<Test>::try_from_var([1; 64].to_vec()).unwrap(),
				Some(vec![1]),
				false
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(
				&BOB,
				&Key::<Test>::try_from_var([2; 64].to_vec()).unwrap(),
				Some(vec![2, 3]),
				false
			),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.read(&ALICE, &Key::<Test>::try_from_var([1; 64].to_vec()).unwrap()),
			Some(vec![1])
		);
		assert_eq!(
			storage.read(&BOB, &Key::<Test>::try_from_var([2; 64].to_vec()).unwrap()),
			Some(vec![2, 3])
		);
		// Overwrite values.
		assert_eq!(
			storage.write(
				&ALICE,
				&Key::<Test>::try_from_var([1; 64].to_vec()).unwrap(),
				Some(vec![4, 5]),
				false
			),
			Ok(WriteOutcome::Overwritten(1))
		);
		assert_eq!(
			storage.read(&ALICE, &Key::<Test>::try_from_var([1; 64].to_vec()).unwrap()),
			Some(vec![4, 5])
		);
	}

	#[test]
	fn rollback_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.rollback_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None)
	}

	#[test]
	fn commit_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]))
	}

	#[test]
	fn overwrite_and_commmit_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1, 2]), false),
			Ok(WriteOutcome::Overwritten(1))
		);
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1, 2]))
	}

	#[test]
	fn rollback_in_nested_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&BOB, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.rollback_transaction();
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
		assert_eq!(storage.read(&BOB, &Key::Fix([1; 32])), None)
	}

	#[test]
	fn commit_in_nested_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&BOB, &Key::Fix([1; 32]), Some(vec![2]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&CHARLIE, &Key::Fix([1; 32]), Some(vec![3]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		storage.commit_transaction();
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
		assert_eq!(storage.read(&BOB, &Key::Fix([1; 32])), Some(vec![2]));
		assert_eq!(storage.read(&CHARLIE, &Key::Fix([1; 32])), Some(vec![3]));
	}

	#[test]
	fn rollback_all_transactions_works() {
		let mut storage = TransientStorage::<Test>::new(1024);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&BOB, &Key::Fix([1; 32]), Some(vec![2]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&CHARLIE, &Key::Fix([1; 32]), Some(vec![3]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		storage.commit_transaction();
		storage.rollback_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
		assert_eq!(storage.read(&BOB, &Key::Fix([1; 32])), None);
		assert_eq!(storage.read(&CHARLIE, &Key::Fix([1; 32])), None);
	}

	#[test]
	fn metering_transactions_works() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size * 2);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		let limit = storage.meter().current().limit;
		storage.commit_transaction();

		storage.start_transaction();
		assert_eq!(storage.meter().current().limit, limit - size);
		assert_eq!(storage.meter().current().limit - storage.meter().current().amount, size);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(storage.meter().current().amount, size);
		storage.commit_transaction();
		assert_eq!(storage.meter().total_amount(), size * 2);
	}

	#[test]
	fn metering_nested_transactions_works() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size * 3);

		storage.start_transaction();
		let limit = storage.meter().current().limit;
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(storage.meter().total_amount(), size);
		assert!(storage.meter().current().limit < limit - size);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		assert_eq!(storage.meter().current().limit, limit);
		assert_eq!(storage.meter().total_amount(), storage.meter().current().amount);
		storage.commit_transaction();
	}

	#[test]
	fn metering_transaction_fails() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size - 1);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Err(Error::<Test>::OutOfTransientStorage.into())
		);
		assert_eq!(storage.meter.current().amount, 0);
		storage.commit_transaction();
		assert_eq!(storage.meter.total_amount(), 0);
	}

	#[test]
	fn metering_nested_transactions_fails() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size * 2);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1u8; 4096]), false),
			Err(Error::<Test>::OutOfTransientStorage.into())
		);
		storage.commit_transaction();
		storage.commit_transaction();
		assert_eq!(storage.meter.total_amount(), size);
	}

	#[test]
	fn metering_nested_transaction_with_rollback_works() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size * 2);

		storage.start_transaction();
		let limit = storage.meter.current().limit;
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.rollback_transaction();

		assert_eq!(storage.meter.total_amount(), 0);
		assert_eq!(storage.meter.current().limit, limit);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		let amount = storage.meter().current().amount;
		assert_eq!(storage.meter().total_amount(), amount);
		storage.commit_transaction();
	}

	#[test]
	fn metering_with_rollback_works() {
		let size = allocation_size(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]));
		let mut storage = TransientStorage::<Test>::new(size * 5);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		let amount = storage.meter.total_amount();
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&BOB, &Key::Fix([1; 32]), Some(vec![1u8; 4096]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		storage.rollback_transaction();
		assert_eq!(storage.meter.total_amount(), amount);
		storage.commit_transaction();
	}
}

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

use core::marker::PhantomData;

use crate::{
	exec::{AccountIdOf, Key},
	storage::WriteOutcome,
	Config, Error,
};
use codec::Encode;
use frame_support::DefaultNoBound;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::{
	collections::btree_map::BTreeMap,
	mem,
	ops::Bound::{Included, Unbounded},
	vec::Vec,
};
type MeterEntries = Vec<MeterEntry>;
type Checkpoints = Vec<usize>;

/// Meter entry tracks transaction allocations.
#[derive(Default, Debug)]
struct MeterEntry {
	/// Allocation commited from subsequent transactions.
	pub nested: u32,
	/// Allocation made in the current transaction.
	pub current: u32,
}

impl MeterEntry {
	pub fn sum(&self) -> u32 {
		self.current.saturating_add(self.nested)
	}

	pub fn absorb(&mut self, rhs: Self) {
		self.nested = self.nested.saturating_add(rhs.sum())
	}
}

/// The storage meter enforces a limit for each nested transaction and the total allocation limit.
#[derive(DefaultNoBound)]
struct StorageMeter<T: Config> {
	total_limit: u32,
	transaction_limit: u32,
	nested: MeterEntries,
	root: MeterEntry,
	_phantom: PhantomData<T>,
}

impl<T: Config> StorageMeter<T> {
	pub fn new(total_limit: u32, transaction_limit: u32) -> Self {
		Self { total_limit, transaction_limit, ..Default::default() }
	}

	/// Charge the allocated amount of transaction storage from the meter.
	pub fn charge(&mut self, amount: u32) -> DispatchResult {
		let current_amount = self.current_amount().saturating_add(amount);
		if current_amount > self.transaction_limit ||
			amount.saturating_add(self.total_amount()) > self.total_limit
		{
			return Err(Error::<T>::OutOfStorage.into());
		}
		self.top_meter_mut().current = current_amount;
		Ok(())
	}

	/// The allocated amount of memory inside the current transaction.
	pub fn current_amount(&self) -> u32 {
		self.top_meter().current
	}

	/// The total allocated amount of memory.
	pub fn total_amount(&self) -> u32 {
		self.nested
			.iter()
			.map(|e: &MeterEntry| e.sum())
			.fold(self.root.sum(), |acc, e| acc.saturating_add(e))
	}

	/// Revert a transaction meter.
	pub fn revert(&mut self) {
		self.nested.pop().expect("There is no nested meter that can be reverted.");
	}

	/// Start a nested transaction meter.
	pub fn start(&mut self) {
		self.nested.push(Default::default());
	}

	/// Commit a transaction meter.
	pub fn commit(&mut self) {
		let nested_meter =
			self.nested.pop().expect("There is no nested meter that can be committed.");
		self.top_meter_mut().absorb(nested_meter);
	}

	fn top_meter_mut(&mut self) -> &mut MeterEntry {
		self.nested.last_mut().unwrap_or(&mut self.root)
	}

	fn top_meter(&self) -> &MeterEntry {
		self.nested.last().unwrap_or(&self.root)
	}
}

/// An entry representing a journal change.
struct JournalEntry {
	account: Vec<u8>,
	key: Vec<u8>,
	prev_value: Option<Vec<u8>>,
}

impl JournalEntry {
	/// Create a new change.
	pub fn new(account: &[u8], key: &[u8], prev_value: Option<Vec<u8>>) -> Self {
		Self { account: account.to_vec(), key: key.to_vec(), prev_value }
	}

	/// Revert the change.
	pub fn revert(self, storage: &mut Storage) {
		storage.write(&self.account, &self.key, self.prev_value);
	}
}

/// A journal containing transient storage modifications.
struct Journal(Vec<JournalEntry>);

impl Journal {
	/// Create a new journal.
	pub fn new() -> Self {
		Self(Default::default())
	}

	/// Add a chenge to the journal.
	pub fn push(&mut self, entry: JournalEntry) {
		self.0.push(entry);
	}

	/// Length of the journal.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Roll back all journal changes until the chackpoint
	pub fn rollback(&mut self, storage: &mut Storage, checkpoint: usize) {
		self.0.drain(checkpoint..).rev().for_each(|entry| entry.revert(storage));
	}
}

/// Storage for maintaining the current transaction state.
#[derive(Default)]
struct Storage(BTreeMap<Vec<u8>, Vec<u8>>);

impl Storage {
	/// Read the storage entry.
	pub fn read(&self, account: &[u8], key: &[u8]) -> Option<Vec<u8>> {
		self.0.get(&Self::storage_key(account, key)).cloned()
	}

	/// Write the storage entry.
	pub fn write(&mut self, account: &[u8], key: &[u8], value: Option<Vec<u8>>) -> Option<Vec<u8>> {
		if let Some(value) = value {
			// Insert storage entry.
			self.0.insert(Self::storage_key(account, key), value)
		} else {
			// Remove storage entry.
			self.0.remove(&Self::storage_key(account, key))
		}
	}

	/// Get the storage keys for the account.
	pub fn keys<'a>(&'a self, account: &'a [u8]) -> impl Iterator<Item = Vec<u8>> + 'a {
		self.0
			.range((Included(account.to_vec()), Unbounded))
			.take_while(|(key, _)| key.starts_with(account))
			.map(|(key, _)| key[account.len()..].to_vec())
	}

	fn storage_key(account: &[u8], key: &[u8]) -> Vec<u8> {
		let mut storage_key = Vec::with_capacity(account.len() + key.len());
		storage_key.extend_from_slice(&account);
		storage_key.extend_from_slice(&key);
		storage_key
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
	current: Storage,
	journal: Journal,
	// The size of the StorageMeter is limited by the stack depth.
	meter: StorageMeter<T>,
	// The size of the checkpoints is limited by the stack depth.
	checkpoints: Checkpoints,
}

impl<T: Config> TransientStorage<T> {
	pub fn new(total_limit: u32, transaction_limit: u32) -> Self {
		TransientStorage {
			current: Default::default(),
			journal: Journal::new(),
			checkpoints: Default::default(),
			meter: StorageMeter::new(total_limit, transaction_limit),
		}
	}

	/// Read the storage entry.
	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Vec<u8>> {
		self.current.read(&account.encode(), &key.hash())
	}

	/// Write a value to storage.
	pub fn write(
		&mut self,
		account: &AccountIdOf<T>,
		key: &Key<T>,
		value: Option<Vec<u8>>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let prev_value = self.read(account, key);
		// Skip if the same value is being set.
		if prev_value != value {
			let key = key.hash();
			let account = account.encode();

			// Calculate the allocation size.
			if let Some(value) = &value {
				// Charge the keys, value and journal entry.
				// If a new value is written, a new journal entry is created. The previous value is
				// moved to the journal along with its keys, and the new value is written to
				// storage.
				let keys_len = account.len().saturating_add(key.len());
				let mut amount = value
					.len()
					.saturating_add(keys_len)
					.saturating_add(mem::size_of::<JournalEntry>());
				if prev_value.is_none() {
					// Charge a new storage entry.
					// If there was no previous value, a new entry is added to storage (BTreeMap)
					// containing a Vec for the key and a Vec for the value. The value was already
					// included in the amount.
					amount = amount
						.saturating_add(keys_len)
						.saturating_add(mem::size_of::<Vec<u8>>().saturating_mul(2));
				}
				self.meter.charge(amount as _)?;
			}
			self.current.write(&account, &key, value);
			// Update the journal.
			self.journal.push(JournalEntry::new(&account, &key, prev_value.clone()));
		}

		Ok(match (take, prev_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(prev_value)) => WriteOutcome::Overwritten(prev_value.len() as _),
			(true, Some(prev_value)) => WriteOutcome::Taken(prev_value),
		})
	}

	/// Remove a contract, clearing all its storage entries.
	pub fn remove(&mut self, account: &AccountIdOf<T>) {
		let account = account.encode();
		// Remove all account entries.
		let keys = self.current.keys(&account).collect::<Vec<_>>();
		for key in keys {
			let prev_value = self.current.write(&account, &key, None);
			// Update the journal.
			self.journal.push(JournalEntry::new(&account, &key, prev_value));
		}
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
			.expect("No open transient storage transaction that can be rolled back.");
		self.meter.revert();
		self.journal.rollback(&mut self.current, checkpoint);
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
			.expect("No open transient storage transaction that can be committed.");
		self.meter.commit();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{Test, ALICE, BOB, CHARLIE},
		Error,
	};

	#[test]
	fn read_write_works() {
		let mut storage = TransientStorage::<Test>::new(2048, 2048);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![2]), false),
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
	}

	#[test]
	fn remove_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 1024);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![2]), false),
			Ok(WriteOutcome::New)
		);
		assert_eq!(
			storage.write(&BOB, &Key::Fix([1; 32]), Some(vec![3]), false),
			Ok(WriteOutcome::New)
		);
		storage.remove(&ALICE);
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
		assert_eq!(storage.read(&ALICE, &Key::Fix([2; 32])), None);
		assert_eq!(storage.read(&BOB, &Key::Fix([1; 32])), Some(vec![3]));
	}

	#[test]
	fn commit_remove_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 256);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.remove(&ALICE);
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);

		storage.start_transaction();
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
		storage.remove(&ALICE);
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
	}

	#[test]
	fn rollback_remove_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 256);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		storage.remove(&ALICE);
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
		storage.rollback_transaction();
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
	}

	#[test]
	fn rollback_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 256);

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
		let mut storage = TransientStorage::<Test>::new(1024, 256);

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
		let mut storage = TransientStorage::<Test>::new(1024, 512);
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
		let mut storage = TransientStorage::<Test>::new(1024, 256);
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
		let mut storage = TransientStorage::<Test>::new(1024, 256);
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
	fn commit_rollback_in_nested_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 256);
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
		storage.rollback_transaction();
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), Some(vec![1]));
		assert_eq!(storage.read(&BOB, &Key::Fix([1; 32])), None);
		assert_eq!(storage.read(&CHARLIE, &Key::Fix([1; 32])), None);
	}

	#[test]
	fn metering_nested_limit_works() {
		let mut storage = TransientStorage::<Test>::new(1024, 128);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Err(Error::<Test>::OutOfStorage.into())
		);
		storage.commit_transaction();
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
	}

	#[test]
	fn metering_total_limit_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);

		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1]), false),
			Err(Error::<Test>::OutOfStorage.into())
		);
		storage.commit_transaction();
		storage.commit_transaction();
	}

	#[test]
	fn metering_total_limit_with_rollback_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);

		storage.start_transaction();
		storage.start_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([2; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.rollback_transaction();
		assert_eq!(
			storage.write(&ALICE, &Key::Fix([1; 32]), Some(vec![1]), false),
			Ok(WriteOutcome::New)
		);
		storage.commit_transaction();
	}
}

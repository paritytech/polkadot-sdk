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
use frame_support::DefaultNoBound;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::{collections::btree_map::{BTreeMap, Keys}, vec::Vec};

type MeterEntries = Vec<MeterEntry>;
type Value = Vec<u8>;
type StorageKey = Vec<u8>;
type Checkpoints = Vec<usize>;
type ContractStorage = BTreeMap<StorageKey, Value>;

/// Meter entry tracks transaction allocations.
#[derive(Default, Debug)]
struct MeterEntry {
	/// Allocation commited from subsequent transactions.
	pub nested: u32,
	/// Allocation made in current transaction.
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

/// Storage meter enforces the limit for each nested transaction and total limit for transaction.
#[derive(DefaultNoBound)]
struct StorageMeter<T: Config> {
	limit: u32,
	frame_limit: u32,
	nested: MeterEntries,
	root: MeterEntry,
	_phantom: PhantomData<T>,
}

impl<T: Config> StorageMeter<T> {
	pub fn new(limit: u32, frame_limit: u32) -> Self {
		Self { limit, frame_limit, ..Default::default() }
	}

	/// Charge the allocated amount of transaction storage from the meter.
	pub fn charge(&mut self, amount: u32) -> DispatchResult {
		let current = self.top_meter().current.saturating_add(amount);
		if amount.saturating_add(self.current_amount()) > self.frame_limit || amount.saturating_add(self.total_amount()) > self.limit {
			return Err(Error::<T>::OutOfStorage.into());
		}
		self.top_meter_mut().current = current;
		Ok(())
	}

	pub fn current_amount(&self) -> u32 {
		self.top_meter().current
	}

	pub fn total_amount(&self) -> u32{
		self
			.nested
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

	fn top_meter(&mut self) -> &MeterEntry {
		self.nested.last().unwrap_or(&self.root)
	}
}


/// Journal change entry.
struct JournalEntry<T: Config> {
	account: AccountIdOf<T>,
	key: StorageKey,
	prev_value: Option<Value>,
}

impl<T: Config> JournalEntry<T> {
	pub fn new(account: AccountIdOf<T>, key: StorageKey, prev_value: Option<Value>) -> Self {
		Self { account, key, prev_value }
	}

	/// Revert the storage to previous state.
	pub fn revert(self, storage: &mut Storage<T>) {
		storage.write(&self.account, &self.key, self.prev_value);
	}
}

/// Journal of transient storage modifications.
struct Journal<T: Config>(Vec<JournalEntry<T>>);

impl<T: Config> Journal<T> {
	pub fn new() -> Self {
		Self(Default::default())
	}

	pub fn push(&mut self, entry: JournalEntry<T>) {
		self.0.push(entry);
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn rollback(&mut self, storage: &mut Storage<T>, checkpoint: usize) {
		self.0.drain(checkpoint..).rev().for_each(|entry| entry.revert(storage));
	}
}

#[derive(DefaultNoBound)]
struct Storage<T: Config>(BTreeMap<AccountIdOf<T>, ContractStorage>);

impl <T: Config> Storage<T> {
	pub fn read(&self, account: &AccountIdOf<T>, key: &StorageKey) -> Option<Value> {
		self.0.get(account)
		.and_then(|contract| contract.get(key))
		.cloned()
	}

	pub fn write(&mut self, account: &AccountIdOf<T>, key: &StorageKey,
		value: Option<Value>,
	) -> Option<Value> {
		let mut old_value = None;
		if let Some(value) = value {
			// Insert storage entry.
			old_value = self.0.entry(account.clone()).or_default().insert(key.clone(), value);
		} else {
			// Remove storage entry.
			let mut remove_account = false;
			self.0.entry(account.clone()).and_modify(|contract| {
				{
					old_value = contract.remove(key);
					if contract.is_empty() {
						// If the contract is empty, remove the account entry from the current state
						remove_account = true;
					}
				};
			});
			// All entries for the account have been removed, so remove the account
			if remove_account {
				self.0.remove(account);
			}
		}

		old_value
	}

	pub fn keys(&self, account: &AccountIdOf<T>) -> Option<Keys<StorageKey, Value>> {
		self.0.get(account).map(|c| c.keys())
	}
}


/// Transient storage behaves almost identically to storage but is discarded after every transaction.
pub struct TransientStorage<T: Config> {
	current: Storage<T>,
	journal: Journal<T>,
	meter: StorageMeter<T>,
	// The size of the checkpoints is limited by the stack depth.
	checkpoints: Checkpoints,
}

impl<T: Config> TransientStorage<T> {
	pub fn new(limit: u32, frame_limit: u32) -> Self {
		TransientStorage {
			current: Default::default(),
			journal: Journal::new(),
			checkpoints: Default::default(),
			meter: StorageMeter::new(limit, frame_limit),
		}
	}

	/// Read the storage entry.
	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Value> {
		self.current.read(account, &key.hash())
	}

	/// Write a value to storage.
	pub fn write(
		&mut self,
		account: &AccountIdOf<T>,
		key: &Key<T>,
		value: Option<Value>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let old_value = self.read(account, key);
		// Skip if the same value is being set.
		if old_value != value {
			let key: Vec<u8> = key.hash();
			let len = value.clone().map(|v| v.len()).unwrap_or_default(); 
			if len > 0 {
				self.meter.charge(len as _)?;
			}
			self.current.write(account, &key, value);
			// Update the journal.
			self.journal.push(JournalEntry::new(account.clone(), key, old_value.clone()));
		}

		Ok(match (take, old_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(old_value)) => WriteOutcome::Overwritten(old_value.len() as _),
			(true, Some(old_value)) => WriteOutcome::Taken(old_value),
		})
	}

	/// Remove a contract, clearing all its storage entries.
	pub fn remove(&mut self, account: &AccountIdOf<T>) {
		// Remove all account entries.
		if let Some(keys) = self.current.keys(account) {
			let keys: Vec<_> = keys.cloned().collect();
			for key in keys {
				let old_value = self.current.write(account, &key, None);
				// Update the journal.
				self.journal.push(JournalEntry::new(account.clone(), key, old_value));
			}
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
	use crate::tests::{Test, ALICE, BOB, CHARLIE};

	#[test]
	fn read_write_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		assert_eq!(storage.read(&ALICE, &Key::Fix([2; 32])), Some(vec![4,5]));
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), Some(vec![6, 7]));
	}

	#[test]
	fn remove_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		storage.remove(&ALICE);
		assert_eq!(storage.read(&ALICE, &Key::Fix([1; 32])), None);
		assert_eq!(storage.read(&ALICE, &Key::Fix([2; 32])), None);
		assert_eq!(storage.read(&BOB, &Key::Fix([3; 32])), Some(vec![3]));
	}

	#[test]
	fn commit_remove_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		let mut storage = TransientStorage::<Test>::new(256, 256);

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
		let mut storage = TransientStorage::<Test>::new(256, 256);

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
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
		let mut storage = TransientStorage::<Test>::new(256, 256);
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
}
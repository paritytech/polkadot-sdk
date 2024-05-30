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
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

type MeterEntries = Vec<MeterEntry>;
type Value = Vec<u8>;
type StorageKey = Vec<u8>;
type State<T> = BTreeMap<AccountIdOf<T>, Storage>;
type Checkpoints = Vec<usize>;
type Storage = BTreeMap<StorageKey, Value>;

#[derive(Default, Debug)]
struct MeterEntry {
	// Allocation commited from subsequent frames.
	pub nested: u32,
	// Allocation made in current frame.
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

/// Storage meter enforces the limit for each transaction and total limit for transaction.
/// It tracks journal and storage allocation.
struct StorageMeter<T: Config> {
	limit: u32,
	frame_limit: u32,
	nested: MeterEntries,
	_phantom: PhantomData<T>,
}

impl<T: Config> StorageMeter<T> {
	pub fn new(limit: u32, frame_limit: u32) -> Self {
		Self { limit, frame_limit, nested: Default::default(), _phantom: Default::default() }
	}

	pub fn charge(&mut self, amount: u32) -> DispatchResult {
		let total: u32 = self
			.nested
			.iter()
			.map(|e: &MeterEntry| e.sum())
			.fold(0, |acc, e| acc.saturating_add(e));
		let nested_meter = self.nested.last_mut().expect("Metering is not started.");
		let current = nested_meter.current.saturating_add(amount);
		if current > self.frame_limit || total.saturating_add(amount) > self.limit {
			return Err(Error::<T>::OutOfStorage.into());
		}
		nested_meter.current = current;

		Ok(())
	}

	fn revert(&mut self) {
		self.nested.pop();
	}

	pub fn start(&mut self) {
		self.nested.push(Default::default());
	}

	pub fn commit(&mut self) {
		let last_meter = self.nested.pop().expect("There is no meter that can be committed.");
		if let Some(prev_meter) = self.nested.last_mut() {
			prev_meter.absorb(last_meter);
		}
	}
}

struct JournalEntry<T: Config> {
	account: AccountIdOf<T>,
	key: StorageKey,
	prev_value: Option<Value>,
}

impl<T: Config> JournalEntry<T> {
	pub fn new(account: AccountIdOf<T>, key: StorageKey, prev_value: Option<Value>) -> Self {
		Self { account, key, prev_value }
	}

	pub fn revert(self, storage: &mut State<T>) {
		if let Some(contract) = storage.get_mut(&self.account) {
			if let Some(prev_value) = self.prev_value {
				contract.insert(self.key, prev_value);
			} else {
				contract.remove(&self.key);
			}
		}
	}
}

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

	pub fn rollback(&mut self, storage: &mut State<T>, checkpoint: usize) {
		self.0.drain(checkpoint..).rev().for_each(|entry| entry.revert(storage));
	}
}

pub struct TransientStorage<T: Config> {
	current: State<T>,
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
			checkpoints: vec![],
			meter: StorageMeter::new(limit, frame_limit),
		}
	}

	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Value> {
		self.current
			.get(account)
			.and_then(|contract| contract.get(&key.hash()))
			.cloned()
	}

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
			self.write_internal(account, key, value)?;
		}

		Ok(match (take, old_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(old_value)) => WriteOutcome::Overwritten(old_value.len() as _),
			(true, Some(old_value)) => WriteOutcome::Taken(old_value),
		})
	}

	pub fn terminate(&mut self, account: &AccountIdOf<T>)-> DispatchResult {
		// Remove all account entries.
		if let Some(contract) = self.current.get(account) {
			let keys: Vec<_> = contract.keys().cloned().collect();
			// Delete each key using the write function
			for key in keys {
				self.write_internal(account, key, None)?;
			}
		}
		Ok(())
	}

	pub fn commit_transaction(&mut self) {
		self.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be committed.");
		self.meter.commit();
	}

	pub fn start_transaction(&mut self) {
		self.meter.start();
		self.checkpoints.push(self.journal.len());
	}

	pub fn rollback_transaction(&mut self) {
		let checkpoint = self
			.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be rolled back.");
		self.meter.revert();
		self.journal.rollback(&mut self.current, checkpoint);
	}

	fn write_internal(
		&mut self,
		account: &AccountIdOf<T>,
		key: StorageKey,
		value: Option<Value>,
	) -> DispatchResult {
		// Update the current state.
		let mut old_value = None;
		if let Some(value) = value {
			// Insert storage entry.
			self.meter.charge(value.len() as _)?;
			old_value = self.current
				.entry(account.clone())
				.or_default()
				.insert(key.clone(), value);
		} else {
			// Remove storage entry.
			let mut remove_account = false;
			self.current.entry(account.clone()).and_modify(|contract| {
				{
					old_value = contract.remove(&key);
					if contract.is_empty() {
						// If the contract is empty, remove the account entry from the current state
						remove_account = true;
					}
				};
			});
			// All entries for the account have been removed, so remove the account
			if remove_account {
				self.current.remove(&account);
			}
		}

		// Update the journal.
		self.journal.push(JournalEntry::new(account.clone(), key, old_value.clone()));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{Test, ALICE, BOB, CHARLIE};

	#[test]
	fn rollback_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(256, 256);
		storage.start_transaction();
		storage.rollback_transaction();

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
		storage.commit_transaction();

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
		storage.commit_transaction();

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
}

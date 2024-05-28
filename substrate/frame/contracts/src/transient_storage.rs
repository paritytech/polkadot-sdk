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

type Value = Vec<u8>;
type StorageKey = Vec<u8>;
type State<T> = BTreeMap<AccountIdOf<T>, Storage<T>>;
type MeterEntries = Vec<MeterEntry>;

#[derive(Default)]
struct MeterEntry {
	// Allocation commited by subsequent frames.
	nested: Diff,
	// Current allocation made by frame.
	current: Diff,
}

// impl MeterEntry {
// 	fn saturating_add(&self, rhs: &Self) -> Self {
// 		Self {
// 			bytes_added: self.bytes_added.saturating_add(rhs.bytes_added),
// 			bytes_removed: self.bytes_removed.saturating_add(rhs.bytes_removed),
// 		}
// 	}
// }


/// This type is used to describe a storage change when charging from the meter.
#[derive(Default)]
pub struct Diff {
	/// How many bytes were added to storage.
	pub bytes_added: u32,
	/// How many bytes were removed from storage.
	pub bytes_removed: u32,
}

impl Diff {
	fn saturating_add(&self, rhs: &Self) -> Self {
		Self {
			bytes_added: self.bytes_added.saturating_add(rhs.bytes_added),
			bytes_removed: self.bytes_removed.saturating_add(rhs.bytes_removed),
		}
	}
}

/// Storage meter enforces the limit for each frame and total limit for transaction.
/// It tracks journal and storage allocation.
struct StorageMeter<T: Config> {
	limit: u32,
	frame_limit: u32,
	nested: MeterEntries,
	_phantom: PhantomData<T>
}

impl <T: Config> StorageMeter<T> {

	pub fn new(limit: u32) -> Self {
		Self {
			limit,
			frame_limit: 256,
			nested: vec![Default::default()],
			_phantom: Default::default()
		}
	}

	fn charge(&mut self, amount: Diff) -> DispatchResult {
		let total: usize = self.nested.iter().sum();
		if let Some(nested_meter) = self.nested.last_mut() {
			let current = nested_meter.saturating_add(amount);
			if current > self.frame_limit as _ || total > self.limit as _ {
				return Err(Error::<T>::OutOfStorage.into());
			}
			*nested_meter = current;
		}
		Ok(())
	}

	fn revert(&mut self) {
		self.nested.pop();
	}

	fn start(&mut self) {
		self.nested.push(Default::default());
	}

	fn commit(&mut self) {
		if let Some(amount) = self.nested.pop()
		{
			if let Some(nested_meter) = self.nested.last_mut() {

			let total: usize = self.nested.iter().sum();
			}
		}
	}
}

struct JournalEntry<T: Config> {
	account: AccountIdOf<T>,
	key: StorageKey,
	prev_value: Option<Value>,
}

impl <T: Config> JournalEntry<T> {
	pub fn new(account: AccountIdOf<T>, key: StorageKey, prev_value: Option<Value>) -> Self {
		Self{
			account,
			key,
			prev_value,
		}
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

struct Journal<T: Config> (Vec<JournalEntry<T>>);

impl <T: Config> Journal<T> {
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
		self.0
        .drain(checkpoint..)
        .rev()
        .for_each(|entry| entry.revert(storage));
	}
}

type Checkpoints = Vec<usize>;
struct Storage<T: Config> {
	data: BTreeMap<StorageKey, Value>,
	_phantom: PhantomData<T>,
}

impl<T: Config> Storage<T> {
	pub fn new(max_size: usize) -> Self {
		Self { data: Default::default(), _phantom: Default::default() }
	}

	pub fn get(&self, key: &StorageKey) -> Option<&Value> {
		self.data.get(key)
	}

	pub fn try_insert(
		&mut self,
		key: StorageKey,
		value: Value,
		meter: &mut StorageMeter<T>,
	) -> DispatchResult {
		let new_value_size = value.len();
		meter.charge(new_value_size)?;
		self.data.insert(key, value);
		Ok(())
	}

	pub fn insert(
		&mut self,
		key: StorageKey,
		value: Value,
	) {
		self.data.insert(key, value);
	}

	pub fn remove(&mut self, key: &StorageKey) {
		self.data.remove(key);
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
	pub fn new(max_size: u32) -> Self {
		TransientStorage {
			current: Default::default(),
			journal: Journal::new(),
			checkpoints: vec![],
			meter: StorageMeter::new(max_size)
		}
	}

	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Value> {
		self.current
			.get(account)
			.and_then(|contract| contract.data.get(&key.hash()))
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
			// Update the current state.
			if let Some(value) = value {
				// Insert storage value.
				self.current
					.entry(account.clone())
					.or_insert_with(|| Storage::new(256))
					.try_insert(key.clone(), value, &mut self.meter)?;
			} else {
				// Remove storage entry.
				self.current.entry(account.clone()).and_modify(|contract| {
					{
						contract.remove(&key);
					};
				});
			}

			// Update the journal.
			self.journal.push(JournalEntry::new(
				account.clone(),
				key,
				old_value.clone()
			));
		}

		Ok(match (take, old_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(old_value)) => WriteOutcome::Overwritten(old_value.len() as _),
			(true, Some(old_value)) => WriteOutcome::Taken(old_value),
		})
	}

	pub fn commit_transaction(&mut self) {
		self.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be committed.");
		self.meter.commit();
	}

	pub fn start_transaction(&mut self) {
		self.checkpoints.push(self.journal.len());
		self.meter.start()
	}

	pub fn rollback_transaction(&mut self) {
		let checkpoint = self
			.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be rolled back.");
		self.meter.revert();
		self.journal.rollback(&mut self.current, checkpoint);

	}

	pub fn terminate(&mut self, account: &AccountIdOf<T>) {

	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{Test, ALICE, BOB, CHARLIE};

	#[test]
	fn rollback_transaction_works() {
		let mut storage = TransientStorage::<Test>::new(256);
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
		let mut storage = TransientStorage::<Test>::new(256);
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
		let mut storage = TransientStorage::<Test>::new(256);
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
		let mut storage = TransientStorage::<Test>::new(256);
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
		let mut storage = TransientStorage::<Test>::new(2);
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

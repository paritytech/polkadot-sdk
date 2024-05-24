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
use sp_runtime::DispatchError;
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

type Value = Vec<u8>;
type StorageKey = Vec<u8>;

#[derive(Clone)]
struct JournalEntry<T: Config> {
	account: AccountIdOf<T>,
	key: StorageKey,
	prev_value: Option<Value>,
}

type Journal<T> = Vec<JournalEntry<T>>;
type Checkpoints = Vec<usize>;
type Storage = (BTreeMap<StorageKey, Value>, usize);

pub struct TransientStorage<T: Config> {
	current: BTreeMap<AccountIdOf<T>, Storage>,
	journal: Journal<T>,
	checkpoints: Checkpoints,
	// Maximum size in bytes per contract.
	max_size: usize,
}

impl<T: Config> TransientStorage<T> {
	pub fn new(max_size: u32) -> Self {
		TransientStorage {
			current: Default::default(),
			journal: vec![],
			checkpoints: vec![],
			max_size: max_size as _,
		}
	}

	pub fn read(&self, account: &AccountIdOf<T>, key: &Key<T>) -> Option<Value> {
		self.current
			.get(account)
			.and_then(|contract| contract.0.get(&key.hash()))
			.cloned()
	}

	fn current_size(&self, account: &AccountIdOf<T>) -> usize {
		self.current.get(account).map(|contract| contract.1).unwrap_or_default()
	}

	pub fn write(
		&mut self,
		account: &AccountIdOf<T>,
		key: &Key<T>,
		value: Option<Value>,
		take: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let old_value = self.read(&account, &key);
		let key = key.hash();

		// Calculate new size and check if it exceeds max_size.
		let old_value_size = old_value.as_ref().map(|e| e.len()).unwrap_or_default();
		let new_value_size = value.as_ref().map(|e| e.len()).unwrap_or_default();

		let current_size = self.current_size(account);
		let size = current_size.saturating_sub(old_value_size).saturating_add(new_value_size);
		if size > self.max_size {
			return Err(Error::<T>::OutOfStorage.into());
		}

		// Update the journal.
		self.journal.push(JournalEntry {
			account: account.clone(),
			key: key.clone(),
			prev_value: old_value.clone(),
		});

		// Update the current state.
		if let Some(value) = value {
			self.current
				.entry(account.clone())
				.or_insert_with(|| (BTreeMap::new(), size))
				.0
				.insert(key, value);
		} else {
			self.current.entry(account.clone()).and_modify(|e| {
				{
					e.0.remove(&key);
					e.1 = size;
				};
			});
		}

		Ok(match (take, old_value) {
			(_, None) => WriteOutcome::New,
			(false, Some(_)) => WriteOutcome::Overwritten(old_value_size as _),
			(true, Some(old_value)) => WriteOutcome::Taken(old_value),
		})
	}

	pub fn commit_transaction(&mut self) {
		self.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be committed.");
	}

	pub fn start_transaction(&mut self) {
		self.checkpoints.push(self.journal.len());
	}

	pub fn rollback_transaction(&mut self) {
		let last_checkpoint = self
			.checkpoints
			.pop()
			.expect("No open transient storage transaction that can be rolled back.");

		for i in (last_checkpoint..self.journal.len()).rev() {
			let JournalEntry { account, key, prev_value } = &self.journal[i];

			if let Some(contract) = self.current.get_mut(account) {
				// Calculate new size.
				let current_value_size = contract.0.len();
				let prev_value_size = prev_value.as_ref().map(|e| e.len()).unwrap_or_default();

				contract.1 =
					contract.1.saturating_sub(current_value_size).saturating_add(prev_value_size);

				if let Some(prev_value) = prev_value {
					contract.0.insert(key.clone(), prev_value.clone());
				} else {
					contract.0.remove(key);
				}
			}
		}

		self.journal.truncate(last_checkpoint);
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
}

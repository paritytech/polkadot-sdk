// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! RocksDB-based offchain workers local storage.

use std::{
	collections::HashMap,
	sync::Arc,
};

use crate::columns;
use kvdb::KeyValueDB;
use parking_lot::Mutex;

/// Offchain local storage
#[derive(Clone)]
pub struct LocalStorage {
	db: Arc<KeyValueDB>,
	locks: Arc<Mutex<HashMap<Vec<u8>, Arc<Mutex<()>>>>>,
}

impl std::fmt::Debug for LocalStorage {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.debug_struct("LocalStorage")
			.finish()
	}
}

impl LocalStorage {
	/// Create new offchain storage for tests (backed by memorydb)
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_test() -> Self {
		let db = Arc::new(::kvdb_memorydb::create(crate::utils::NUM_COLUMNS));
		Self::new(db as _)
	}

	/// Create offchain local storage with given `KeyValueDB` backend.
	pub fn new(db: Arc<KeyValueDB>) -> Self {
		Self {
			db,
			locks: Default::default(),
		}
	}
}

impl client::backend::OffchainStorage for LocalStorage {
	fn set(&mut self, prefix: &[u8], key: &[u8], value: &[u8]) {
		let key: Vec<u8> = prefix.iter().chain(key).cloned().collect();
		let mut tx = self.db.transaction();
		tx.put(columns::OFFCHAIN, &key, value);

		if let Err(e) = self.db.write(tx) {
			log::warn!("Error writing to the offchain DB: {:?}", e);
		}
	}

	fn get(&self, prefix: &[u8], key: &[u8]) -> Option<Vec<u8>> {
		let key: Vec<u8> = prefix.iter().chain(key).cloned().collect();
		self.db.get(columns::OFFCHAIN, &key)
			.ok()
			.and_then(|x| x)
			.map(|v| v.to_vec())
	}

	fn compare_and_set(
		&mut self,
		prefix: &[u8],
		item_key: &[u8],
		old_value: &[u8],
		new_value: &[u8],
	) -> bool {
		let key: Vec<u8> = prefix.iter().chain(item_key).cloned().collect();
		let key_lock = {
			let mut locks = self.locks.lock();
			locks.entry(key.clone()).or_default().clone()
		};

		let is_set;
		{
			let _key_guard = key_lock.lock();
			is_set = self.db.get(columns::OFFCHAIN, &key)
				.ok()
				.and_then(|x| x)
				.map(|v| &*v == old_value)
				.unwrap_or(true);

			if is_set {
				self.set(prefix, item_key, new_value)
			}
		}

		// clean the lock map if we're the only entry
		let mut locks = self.locks.lock();
		{
			drop(key_lock);
			let key_lock = locks.get_mut(&key);
			if let Some(_) = key_lock.and_then(Arc::get_mut) {
				locks.remove(&key);
			}
		}
		is_set
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use client::backend::OffchainStorage;

	#[test]
	fn should_compare_and_set_and_clear_the_locks_map() {
		let mut storage = LocalStorage::new_test();
		let prefix = b"prefix";
		let key = b"key";
		let value = b"value";

		storage.set(prefix, key, value);
		assert_eq!(storage.get(prefix, key), Some(value.to_vec()));

		assert_eq!(storage.compare_and_set(prefix, key, value, b"asd"), true);
		assert_eq!(storage.get(prefix, key), Some(b"asd".to_vec()));
		assert!(storage.locks.lock().is_empty(), "Locks map should be empty!");
	}
}

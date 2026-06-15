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

/// A wrapper around `kvdb::Database` that implements `sp_database::Database` trait
use ::kvdb::{DBTransaction, KeyValueDB};
use std::collections::HashMap;

use crate::{error, Change, ColumnId, Database, Transaction};

struct DbAdapter<D: KeyValueDB + 'static>(D);

fn handle_err<T>(result: std::io::Result<T>) -> T {
	match result {
		Ok(r) => r,
		Err(e) => {
			panic!("Critical database error: {:?}", e);
		},
	}
}

/// Read the reference counter for a key, taking into account any pending updates
/// that earlier operations in the same transaction have made but not yet committed.
fn read_counter(
	db: &dyn KeyValueDB,
	pending: &HashMap<(ColumnId, Vec<u8>), Option<u32>>,
	col: ColumnId,
	key: &[u8],
) -> error::Result<(Vec<u8>, Option<u32>)> {
	let mut counter_key = key.to_vec();
	counter_key.push(0);
	if let Some(counter) = pending.get(&(col, counter_key.clone())) {
		return Ok((counter_key, *counter));
	}
	Ok(match db.get(col, &counter_key).map_err(|e| error::DatabaseError(Box::new(e)))? {
		Some(data) => {
			let mut counter_data = [0; 4];
			if data.len() != 4 {
				return Err(error::DatabaseError(Box::new(std::io::Error::new(
					std::io::ErrorKind::Other,
					format!("Unexpected counter len {}", data.len()),
				))));
			}
			counter_data.copy_from_slice(&data);
			let counter = u32::from_le_bytes(counter_data);
			(counter_key, Some(counter))
		},
		None => (counter_key, None),
	})
}

/// Commit a transaction to a KeyValueDB.
fn commit_impl<H: Clone + AsRef<[u8]>>(
	db: &dyn KeyValueDB,
	transaction: Transaction<H>,
) -> error::Result<()> {
	let mut tx = DBTransaction::new();
	// Tracks the reference counter value left by `Store`/`Reference`/`Release` operations
	// already processed in this transaction, so later operations on the same key observe
	// those pending updates instead of the last committed value.
	let mut pending_counters: HashMap<(ColumnId, Vec<u8>), Option<u32>> = HashMap::new();
	for change in transaction.0.into_iter() {
		match change {
			Change::Set(col, key, value) => tx.put_vec(col, &key, value),
			Change::Remove(col, key) => tx.delete(col, &key),
			Change::Store(col, key, value) => {
				match read_counter(db, &pending_counters, col, key.as_ref())? {
					(counter_key, Some(mut counter)) => {
						counter += 1;
						tx.put(col, &counter_key, &counter.to_le_bytes());
						pending_counters.insert((col, counter_key), Some(counter));
					},
					(counter_key, None) => {
						let counter = 1u32;
						tx.put(col, &counter_key, &counter.to_le_bytes());
						tx.put_vec(col, key.as_ref(), value);
						pending_counters.insert((col, counter_key), Some(counter));
					},
				}
			},
			Change::Reference(col, key) => {
				let (counter_key, counter) =
					read_counter(db, &pending_counters, col, key.as_ref())?;
				if let Some(mut counter) = counter {
					counter += 1;
					tx.put(col, &counter_key, &counter.to_le_bytes());
					pending_counters.insert((col, counter_key), Some(counter));
				}
			},
			Change::Release(col, key) => {
				let (counter_key, counter) =
					read_counter(db, &pending_counters, col, key.as_ref())?;
				if let Some(mut counter) = counter {
					counter -= 1;
					if counter == 0 {
						tx.delete(col, &counter_key);
						tx.delete(col, key.as_ref());
						pending_counters.insert((col, counter_key), None);
					} else {
						tx.put(col, &counter_key, &counter.to_le_bytes());
						pending_counters.insert((col, counter_key), Some(counter));
					}
				}
			},
		}
	}
	db.write(tx).map_err(|e| error::DatabaseError(Box::new(e)))
}

/// Wrap generic kvdb-based database into a trait object that implements [`Database`].
pub fn as_database<D, H>(db: D) -> std::sync::Arc<dyn Database<H>>
where
	D: KeyValueDB + 'static,
	H: Clone + AsRef<[u8]>,
{
	std::sync::Arc::new(DbAdapter(db))
}

impl<D: KeyValueDB, H: Clone + AsRef<[u8]>> Database<H> for DbAdapter<D> {
	fn commit(&self, transaction: Transaction<H>) -> error::Result<()> {
		commit_impl(&self.0, transaction)
	}

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		handle_err(self.0.get(col, key))
	}

	fn contains(&self, col: ColumnId, key: &[u8]) -> bool {
		handle_err(self.0.has_key(col, key))
	}
}

/// RocksDB-specific adapter that implements `optimize_db` via `force_compact`.
#[cfg(feature = "rocksdb")]
pub struct RocksDbAdapter(kvdb_rocksdb::Database);

#[cfg(feature = "rocksdb")]
impl<H: Clone + AsRef<[u8]>> Database<H> for RocksDbAdapter {
	fn commit(&self, transaction: Transaction<H>) -> error::Result<()> {
		commit_impl(&self.0, transaction)
	}

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		handle_err(self.0.get(col, key))
	}

	fn contains(&self, col: ColumnId, key: &[u8]) -> bool {
		handle_err(self.0.has_key(col, key))
	}

	fn optimize_db_col(&self, col: ColumnId) -> error::Result<()> {
		self.0.force_compact(col).map_err(|e| error::DatabaseError(Box::new(e)))
	}
}

/// Wrap RocksDB database into a trait object with `optimize_db` support.
#[cfg(feature = "rocksdb")]
pub fn as_rocksdb_database<H>(db: kvdb_rocksdb::Database) -> std::sync::Arc<dyn Database<H>>
where
	H: Clone + AsRef<[u8]>,
{
	std::sync::Arc::new(RocksDbAdapter(db))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Transaction;

	const COL: ColumnId = 0;

	fn new_db() -> std::sync::Arc<dyn Database<Vec<u8>>> {
		as_database(kvdb_memorydb::create(1))
	}

	#[test]
	fn store_then_release_in_same_transaction_evicts_entry() {
		let db = new_db();
		let key = vec![1, 2, 3];
		let value = vec![4, 5, 6];

		let mut tx = Transaction::new();
		tx.store(COL, key.clone(), value.clone());
		tx.release(COL, key.clone());
		db.commit(tx).unwrap();

		assert_eq!(db.get(COL, &key), None);
	}

	#[test]
	fn store_reference_then_release_in_same_transaction_keeps_entry() {
		let db = new_db();
		let key = vec![1, 2, 3];
		let value = vec![4, 5, 6];

		let mut tx = Transaction::new();
		tx.store(COL, key.clone(), value.clone());
		tx.reference(COL, key.clone());
		tx.release(COL, key.clone());
		db.commit(tx).unwrap();

		assert_eq!(db.get(COL, &key), Some(value));
	}

	#[test]
	fn release_across_separate_transactions_evicts_entry() {
		let db = new_db();
		let key = vec![1, 2, 3];
		let value = vec![4, 5, 6];

		let mut tx = Transaction::new();
		tx.store(COL, key.clone(), value.clone());
		db.commit(tx).unwrap();
		assert_eq!(db.get(COL, &key), Some(value));

		let mut tx = Transaction::new();
		tx.release(COL, key.clone());
		db.commit(tx).unwrap();

		assert_eq!(db.get(COL, &key), None);
	}
}

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

/// Read the reference counter for a key.
fn read_counter(
	db: &dyn KeyValueDB,
	col: ColumnId,
	key: &[u8],
) -> error::Result<(Vec<u8>, Option<u32>)> {
	let mut counter_key = key.to_vec();
	counter_key.push(0);
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

#[derive(Default)]
struct RefCountedKeyState {
	delta: i64,
	stored_value: Option<Vec<u8>>,
	had_store: bool,
}

/// Commit a transaction to a KeyValueDB.
///
/// Ref-counted ops on the same `(col, key)` are aggregated before being applied: without
/// this, multiple `Store`/`Reference`/`Release` in one tx would each read the stale on-disk
/// counter and write back to the same counter key — the underlying batch keeps only the
/// last `put`, collapsing N ops into one.
///
/// `Set`/`Remove` are emitted in submission order; ref-counted ops are emitted afterwards.
/// Mixing the two styles on the same `(col, key)` in one tx is undefined.
fn commit_impl<H: Clone + AsRef<[u8]>>(
	db: &dyn KeyValueDB,
	transaction: Transaction<H>,
) -> error::Result<()> {
	let mut tx = DBTransaction::new();
	let mut ref_counted: HashMap<(ColumnId, Vec<u8>), RefCountedKeyState> = HashMap::new();

	for change in transaction.0.into_iter() {
		match change {
			Change::Set(col, key, value) => tx.put_vec(col, &key, value),
			Change::Remove(col, key) => tx.delete(col, &key),
			Change::Store(col, key, value) => {
				let entry = ref_counted.entry((col, key.as_ref().to_vec())).or_default();
				entry.delta += 1;
				if !entry.had_store {
					entry.had_store = true;
					entry.stored_value = Some(value);
				}
			},
			Change::Reference(col, key) => {
				ref_counted.entry((col, key.as_ref().to_vec())).or_default().delta += 1;
			},
			Change::Release(col, key) => {
				ref_counted.entry((col, key.as_ref().to_vec())).or_default().delta -= 1;
			},
		}
	}

	for ((col, key), state) in ref_counted {
		let (counter_key, on_disk_counter) = read_counter(db, col, &key)?;
		// Counter and value are co-resident under the kvdb refcount scheme (written and
		// deleted as a pair per commit), so counter-presence is a faithful proxy for
		// value-presence here.
		let value_in_db = on_disk_counter.is_some();
		// Reference/Release on a missing key without a Store stays a no-op.
		let writes_new_value = state.had_store && !value_in_db;
		if !value_in_db && !writes_new_value {
			continue;
		}
		let new_counter = on_disk_counter.unwrap_or(0) as i64 + state.delta;
		if new_counter <= 0 {
			tx.delete(col, &counter_key);
			tx.delete(col, &key);
		} else {
			let new_counter_u32: u32 = new_counter.try_into().map_err(|_| {
				error::DatabaseError(Box::new(std::io::Error::other(format!(
					"Refcount overflow for key {key:02x?} in column {col}",
				))))
			})?;
			tx.put(col, &counter_key, &new_counter_u32.to_le_bytes());
			if writes_new_value {
				tx.put_vec(
					col,
					&key,
					state
						.stored_value
						.expect("had_store=true implies stored_value is Some per Store branch"),
				);
			}
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

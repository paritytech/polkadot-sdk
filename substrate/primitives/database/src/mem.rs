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

//! In-memory implementation of `Database`

use crate::{
	error, Change, ColumnId, Database, DatabaseWithSeekableIterator, SeekableIterator, Transaction,
};
use parking_lot::RwLock;
use std::collections::{btree_map::Entry, BTreeMap, HashMap};

type ColumnSpace = BTreeMap<Vec<u8>, (u32, Vec<u8>)>;

#[derive(Default)]
/// This implements `Database` as an in-memory hash map. `commit` is not atomic.
pub struct MemDb(RwLock<HashMap<ColumnId, ColumnSpace>>);

impl<H> Database<H> for MemDb
where
	H: Clone + AsRef<[u8]>,
{
	fn commit(&self, transaction: Transaction<H>) -> error::Result<()> {
		let mut s = self.0.write();
		for change in transaction.0.into_iter() {
			match change {
				Change::Set(col, key, value) => {
					s.entry(col).or_default().insert(key, (1, value));
				},
				Change::Remove(col, key) => {
					s.entry(col).or_default().remove(&key);
				},
				Change::Store(col, hash, value) => {
					s.entry(col)
						.or_default()
						.entry(hash.as_ref().to_vec())
						.and_modify(|(c, _)| *c += 1)
						.or_insert_with(|| (1, value));
				},
				Change::Reference(col, hash) => {
					if let Entry::Occupied(mut entry) =
						s.entry(col).or_default().entry(hash.as_ref().to_vec())
					{
						entry.get_mut().0 += 1;
					}
				},
				Change::Release(col, hash) => {
					if let Entry::Occupied(mut entry) =
						s.entry(col).or_default().entry(hash.as_ref().to_vec())
					{
						entry.get_mut().0 -= 1;
						if entry.get().0 == 0 {
							entry.remove();
						}
					}
				},
			}
		}

		Ok(())
	}

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		let s = self.0.read();
		s.get(&col).and_then(|c| c.get(key).map(|(_, v)| v.clone()))
	}
}

enum IterState {
	Valid { current_key: Vec<u8> },
	Invalid,
}

struct MemDbSeekableIter<'db> {
	db: &'db MemDb,
	column: ColumnId,
	state: IterState,
}

impl<'db> MemDbSeekableIter<'db> {
	fn lock_col_space<T>(&self, callback: impl FnOnce(&ColumnSpace) -> T) -> T {
		let lock = self.db.0.read();
		let column_space = lock
			.get(&self.column)
			.expect("Iterator must always point to an existing column");
		callback(column_space)
	}
}

impl<'db> SeekableIterator for MemDbSeekableIter<'db> {
	fn seek(&mut self, key: &[u8]) {
		let next_kv = self.lock_col_space(|col_space| {
			let mut range = col_space
				.range::<[u8], _>((std::ops::Bound::Included(key), std::ops::Bound::Unbounded));
			range.next().map(|(k, _)| k.to_owned())
		});
		self.state = match next_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn seek_prev(&mut self, key: &[u8]) {
		let prev_kv = self.lock_col_space(|col_space| {
			let mut range = col_space
				.range::<[u8], _>((std::ops::Bound::Unbounded, std::ops::Bound::Included(key)));
			range.next_back().map(|(k, _)| k.to_owned())
		});
		self.state = match prev_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn prev(&mut self) {
		let prev_kv = match self.state {
			IterState::Valid { ref current_key } => self.lock_col_space(|col_space| {
				let mut range = col_space.range::<Vec<u8>, _>((
					std::ops::Bound::Unbounded,
					std::ops::Bound::Excluded(current_key),
				));
				range.next_back().map(|(k, _)| k.to_owned())
			}),
			IterState::Invalid => None,
		};
		self.state = match prev_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn next(&mut self) {
		let next_kv = match &self.state {
			IterState::Valid { current_key } => self.lock_col_space(|col_space| {
				let mut range = col_space.range::<Vec<u8>, _>((
					std::ops::Bound::Excluded(current_key),
					std::ops::Bound::Unbounded,
				));
				range.next().map(|(k, _)| k.to_owned())
			}),
			IterState::Invalid => None,
		};
		self.state = match next_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn get(&self) -> Option<(&[u8], Vec<u8>)> {
		match self.state {
			IterState::Valid { ref current_key } => Some((
				&current_key,
				self.lock_col_space(|col_space| {
					col_space
						.get(current_key)
						.expect("Iterator in valid state must always point to an existing key")
						.1
						.clone()
				}),
			)),
			IterState::Invalid => None,
		}
	}
}

impl<H> DatabaseWithSeekableIterator<H> for MemDb
where
	H: Clone + AsRef<[u8]>,
{
	fn seekable_iter<'a>(&'a self, column: u32) -> Option<Box<dyn crate::SeekableIterator + 'a>> {
		if self.0.read().contains_key(&column) {
			Some(Box::new(MemDbSeekableIter { db: self, column, state: IterState::Invalid }))
		} else {
			None
		}
	}
}

impl MemDb {
	/// Create a new instance
	pub fn new() -> Self {
		MemDb::default()
	}

	/// Count number of values in a column
	pub fn count(&self, col: ColumnId) -> usize {
		let s = self.0.read();
		s.get(&col).map(|c| c.len()).unwrap_or(0)
	}
}

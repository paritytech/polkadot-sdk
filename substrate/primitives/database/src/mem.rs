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

type ColumnSpace<Key> = BTreeMap<Key, (u32, Vec<u8>)>;

pub trait GenericKey: Ord + Clone + From<Vec<u8>> + Send + Sync + AsRef<[u8]> {
	type Key<'a>: Ord + Clone + From<Vec<u8>> + Send + Sync + AsRef<[u8]> + From<&'a [u8]>;
}

impl GenericKey for Vec<u8> {
	type Key<'a> = Vec<u8>;
}

/// This implements `Database` as an in-memory hash map. `commit` is not atomic.
pub struct MemDb<K: GenericKey = Vec<u8>>(RwLock<HashMap<ColumnId, ColumnSpace<K::Key<'static>>>>);

impl<K: GenericKey> Default for MemDb<K> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<H, K: GenericKey> Database<H> for MemDb<K>
where
	H: Clone + AsRef<[u8]>,
{
	fn commit(&self, transaction: Transaction<H>) -> error::Result<()> {
		let mut s = self.0.write();
		for change in transaction.0.into_iter() {
			match change {
				Change::Set(col, key, value) => {
					s.entry(col).or_default().insert(key.into(), (1, value));
				},
				Change::Remove(col, key) => {
					s.entry(col).or_default().remove(&key.into());
				},
				Change::Store(col, hash, value) => {
					s.entry(col)
						.or_default()
						.entry(hash.as_ref().to_vec().into())
						.and_modify(|(c, _)| *c += 1)
						.or_insert_with(|| (1, value));
				},
				Change::Reference(col, hash) => {
					if let Entry::Occupied(mut entry) =
						s.entry(col).or_default().entry(hash.as_ref().to_vec().into())
					{
						entry.get_mut().0 += 1;
					}
				},
				Change::Release(col, hash) => {
					if let Entry::Occupied(mut entry) =
						s.entry(col).or_default().entry(hash.as_ref().to_vec().into())
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

	fn get<'a>(&self, col: ColumnId, key: &'a [u8]) -> Option<Vec<u8>> {
		let key: <K as GenericKey>::Key<'_> = K::Key::from(key);
		// since Key in BTreeMap in self is Key<'static>, things breaks when we pass
		// Key<'a>, although no memory safety violations are happening here.
		let key: K::Key<'static> = unsafe { std::mem::transmute(key) };
		let s = self.0.read();
		let col = s.get(&col)?;
		let (_, val) = col.get(&key)?;
		Some(val.clone())
	}
}

enum IterState<Key> {
	Valid { current_key: Key },
	Invalid,
}

struct MemDbSeekableIter<'db, K: GenericKey> {
	db: &'db MemDb<K>,
	column: ColumnId,
	state: IterState<K::Key<'static>>,
}

impl<'db, K: GenericKey> MemDbSeekableIter<'db, K> {
	fn lock_col_space<T>(&self, callback: impl FnOnce(&ColumnSpace<K::Key<'static>>) -> T) -> T {
		let lock = self.db.0.read();
		let column_space = lock
			.get(&self.column)
			.expect("Iterator must always point to an existing column");
		callback(column_space)
	}
}

impl<'db, K: GenericKey> SeekableIterator for MemDbSeekableIter<'db, K> {
	fn seek(&mut self, key: &[u8]) {
		let key: <K as GenericKey>::Key<'_> = K::Key::from(key);
		// since Key in BTreeMap in self is Key<'static>, things breaks when we pass
		// Key<'a>, although no memory safety violations are happening here.
		let key: K::Key<'static> = unsafe { std::mem::transmute(key) };

		let next_kv = self.lock_col_space(|col_space| {
			let mut range = col_space.range::<K::Key<'static>, _>((
				std::ops::Bound::Included(K::Key::from(key)),
				std::ops::Bound::Unbounded,
			));
			range.next().map(|(k, _)| k.to_owned())
		});
		self.state = match next_kv {
			Some(key) => IterState::Valid { current_key: key.clone() },
			None => IterState::Invalid,
		};
	}

	fn seek_prev(&mut self, key: &[u8]) {
		let key: <K as GenericKey>::Key<'_> = K::Key::from(key);
		// since Key in BTreeMap in self is Key<'static>, things breaks when we pass
		// Key<'a>, although no memory safety violations are happening here.
		let key: K::Key<'static> = unsafe { std::mem::transmute(key) };

		let prev_kv = self.lock_col_space(|col_space| {
			let mut range = col_space.range::<K::Key<'static>, _>((
				std::ops::Bound::Unbounded,
				std::ops::Bound::Included(K::Key::from(key)),
			));
			range.next_back().map(|(k, _)| k.to_owned())
		});
		self.state = match prev_kv {
			Some(key) => IterState::Valid { current_key: key.clone() },
			None => IterState::Invalid,
		};
	}

	fn prev(&mut self) {
		let prev_kv = match self.state {
			IterState::Valid { ref current_key } => self.lock_col_space(|col_space| {
				let mut range = col_space.range::<K::Key<'static>, _>((
					std::ops::Bound::Unbounded,
					std::ops::Bound::Excluded(current_key),
				));
				range.next_back().map(|(k, _)| k.to_owned())
			}),
			IterState::Invalid => None,
		};
		self.state = match prev_kv {
			Some(key) => IterState::Valid { current_key: key.clone() },
			None => IterState::Invalid,
		};
	}

	fn next(&mut self) {
		let next_kv = match &self.state {
			IterState::Valid { current_key } => self.lock_col_space(|col_space| {
				let mut range = col_space.range::<K::Key<'static>, _>((
					std::ops::Bound::Excluded(current_key),
					std::ops::Bound::Unbounded,
				));
				range.next().map(|(k, _)| k.to_owned())
			}),
			IterState::Invalid => None,
		};
		self.state = match next_kv {
			Some(key) => IterState::Valid { current_key: key.clone() },
			None => IterState::Invalid,
		};
	}

	fn get(&self) -> Option<(&[u8], Vec<u8>)> {
		match self.state {
			IterState::Valid { ref current_key } => Some((
				current_key.as_ref(),
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

impl<H, Key: GenericKey> DatabaseWithSeekableIterator<H> for MemDb<Key>
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

impl<K: GenericKey> MemDb<K> {
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

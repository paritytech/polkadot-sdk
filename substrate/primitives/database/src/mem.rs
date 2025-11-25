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

/// Custom comparison function for key ordering
pub trait Comparator {
	fn cmp(k1: &[u8], k2: &[u8]) -> std::cmp::Ordering;
}

/// Wrapper to allow comparing external key slices with stored keys using a custom comparator
enum CustomOrdKey<'a, F> {
	Owned { key: Vec<u8>, _phantom: std::marker::PhantomData<F> },
	Ref { key: &'a [u8], _phantom: std::marker::PhantomData<F> },
}

impl<'a, F> CustomOrdKey<'a, F> {
	fn new_owned(key: Vec<u8>) -> Self {
		Self::Owned { key, _phantom: Default::default() }
	}
}

impl<'a, F> Clone for CustomOrdKey<'a, F> {
	fn clone(&self) -> Self {
		match self {
			Self::Owned { key, _phantom } => key.clone().into(),
			Self::Ref { key, _phantom } => (*key).into(),
		}
	}
}

impl<'a, F> From<Vec<u8>> for CustomOrdKey<'a, F> {
	fn from(key: Vec<u8>) -> Self {
		Self::Owned { key, _phantom: Default::default() }
	}
}

impl<'a, F> From<&'a [u8]> for CustomOrdKey<'a, F> {
	fn from(key: &'a [u8]) -> Self {
		Self::Ref { key, _phantom: Default::default() }
	}
}

impl<'a, F> CustomOrdKey<'a, F> {
	fn key_slice(&'a self) -> &'a [u8] {
		match self {
			CustomOrdKey::Owned { key, .. } => key.as_slice(),
			CustomOrdKey::Ref { key, .. } => key,
		}
	}
}

impl<'a, F> PartialEq for CustomOrdKey<'a, F>
where
	F: Comparator,
{
	fn eq(&self, other: &Self) -> bool {
		F::cmp(self.key_slice(), other.key_slice()) == std::cmp::Ordering::Equal
	}
}

impl<'a, F> Eq for CustomOrdKey<'a, F> where F: Comparator {}

impl<'a, F> PartialOrd for CustomOrdKey<'a, F>
where
	F: Comparator,
{
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(F::cmp(self.key_slice(), other.key_slice()))
	}
}

impl<'a, F> Ord for CustomOrdKey<'a, F>
where
	F: Comparator,
{
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		F::cmp(self.key_slice(), other.key_slice())
	}
}

/// Default comparator using standard Vec cmp
pub struct VecComparator {}

impl Comparator for VecComparator {
	fn cmp(k1: &[u8], k2: &[u8]) -> std::cmp::Ordering {
		k1.cmp(k2)
	}
}

/// This implements `Database` as an in-memory hash map. `commit` is not atomic.
pub struct MemDb<Cmp: Comparator = VecComparator>(
	RwLock<HashMap<ColumnId, ColumnSpace<CustomOrdKey<'static, Cmp>>>>,
);

impl<Cmp: Comparator> Default for MemDb<Cmp> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<H, Cmp: Comparator + Send + Sync> Database<H> for MemDb<Cmp>
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

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		let key = key.into();

		let s = self.0.read();
		let col = s.get(&col)?;
		let (_, val) = col.get(&key)?;
		Some(val.clone())
	}
}

enum IterState<Cmp: Comparator> {
	Valid { current_key: CustomOrdKey<'static, Cmp> },
	Invalid,
}

struct MemDbSeekableIter<'db, Cmp: Comparator> {
	db: &'db MemDb<Cmp>,
	column: ColumnId,
	state: IterState<Cmp>,
}

impl<'db, Cmp: Comparator> MemDbSeekableIter<'db, Cmp> {
	fn lock_col_space<T>(
		&self,
		callback: impl FnOnce(&ColumnSpace<CustomOrdKey<'static, Cmp>>) -> T,
	) -> T {
		let lock = self.db.0.read();
		let column_space = lock
			.get(&self.column)
			.expect("Iterator must always point to an existing column");
		callback(column_space)
	}
}

impl<'db, Cmp: Comparator> SeekableIterator for MemDbSeekableIter<'db, Cmp> {
	fn seek<'a>(&mut self, key: &'a [u8]) {
		let key: CustomOrdKey<'a, Cmp> = CustomOrdKey::Ref { key, _phantom: Default::default() };
		let next_kv = self.lock_col_space(|col_space| {
			let mut range =
				col_space.range((std::ops::Bound::Included(&key), std::ops::Bound::Unbounded));
			range.next().map(|(key, _)| CustomOrdKey::new_owned(key.key_slice().to_vec()))
		});
		self.state = match next_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn seek_prev(&mut self, key: &[u8]) {
		let key: CustomOrdKey<'_, Cmp> = key.into();

		let prev_kv = self.lock_col_space(|col_space| {
			let mut range =
				col_space.range((std::ops::Bound::Unbounded, std::ops::Bound::Included(key)));
			range
				.next_back()
				.map(|(key, _)| CustomOrdKey::new_owned(key.key_slice().to_vec()))
		});
		self.state = match prev_kv {
			Some(key) => IterState::Valid { current_key: key },
			None => IterState::Invalid,
		};
	}

	fn prev(&mut self) {
		let prev_kv = match self.state {
			IterState::Valid { ref current_key } => self.lock_col_space(|col_space| {
				let mut range = col_space
					.range((std::ops::Bound::Unbounded, std::ops::Bound::Excluded(current_key)));
				range.next_back().map(|(k, _)| k.clone())
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
				let mut range = col_space
					.range((std::ops::Bound::Excluded(current_key), std::ops::Bound::Unbounded));
				range.next().map(|(k, _)| k.clone())
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
				current_key.key_slice(),
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

impl<H, Cmp: Comparator + Send + Sync> DatabaseWithSeekableIterator<H> for MemDb<Cmp>
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

impl<Cmp: Comparator> MemDb<Cmp> {
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

// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Provides structures for storing and managing transactions in a transaction memory pool.
//!
//! The module includes `SizeTrackedStore`, a map designed for concurrent access, and
//! `IndexedStorage`, which manages transaction entries by key and priority. Transactions are stored
//! efficiently with operations to insert items based on priority and manage space utilization. This
//! module provides core functionality for maintaining the `TxMemPool` state.

use std::{
	collections::{BTreeMap, HashMap},
	sync::{
		atomic::{AtomicIsize, Ordering as AtomicOrdering},
		Arc,
	},
};

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Something that can report its size.
pub(super) trait Size {
	fn size(&self) -> usize;
}

/// Trait for items with a priority and timestamp ordering.
///
/// `PriorityAndTimestamp` defines methods to access the priority and timestamp,
/// facilitating sorting in structures like `SizeTrackedStore`.
pub(super) trait PriorityAndTimestamp {
	type Priority: Ord;
	type Timestamp: Ord;

	fn priority(&self) -> Self::Priority;
	fn timestamp(&self) -> Self::Timestamp;
}

/// A dual-key struct for ordering by priority and timestamp.
///
/// `PriorityKey<U, V>` allows sorting where the primary criteria is `Priority`
/// and ties are broken using `Timestamp`.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct PriorityKey<U, V>(U, V);

/// Composite key for sorting collections by priority keys (embedded in `PriorityKey`) and item key
/// (typically hash).
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
struct SortKey<P, K>(P, K);

impl<K, A, B> SortKey<PriorityKey<A, B>, K>
where
	K: Ord + Copy,
	A: Ord,
	B: Ord,
{
	/// Creates a new `SortKey` for a given item and its key, based on the item's priority and
	/// timestamp.
	fn new<V: PriorityAndTimestamp<Priority = A, Timestamp = B>>(key: &K, item: &V) -> Self {
		Self(PriorityKey(item.priority(), item.timestamp()), *key)
	}
}

/// Internal storage for managing TxMemPool entries.
///
/// `IndexedStorage` uses `HashMap` for fast access by key and `BTreeMap` for
/// efficient priority ordering.
#[derive(Debug)]
struct IndexedStorage<K, S, V>
where
	K: Ord,
	S: Ord,
{
	/// HashMap storing transactions by unique key for quick access.
	items_by_hashes: HashMap<K, V>,
	/// BTreeMap ordering transactions for prioritized access based on sort key.
	items_by_priority: BTreeMap<SortKey<S, K>, V>,
}

/// Core structure for storing and managing transactions in TxMemPool.
///
/// `SizeTrackedStore` is a thread-safe map designed to track the size and priority
/// of transactions, optimized for use in a transaction memory pool. The map
/// preserves sorting order based on priority and timestamp.
///
/// Size reported might be slightly off and only approximately true.
#[derive(Debug)]
pub struct SizeTrackedStore<K, S, V>
where
	K: Ord,
	S: Ord,
{
	/// Internal storage maintaining transaction entries.
	index: Arc<RwLock<IndexedStorage<K, S, V>>>,
	/// Atomic counter tracking the total size, in bytes, of all transactions.
	bytes: AtomicIsize,
	/// Atomic counter maintaining the count of transactions.
	length: AtomicIsize,
}

impl<K, S, V> Default for IndexedStorage<K, S, V>
where
	K: Ord,
	S: Ord,
{
	fn default() -> Self {
		Self { items_by_hashes: Default::default(), items_by_priority: Default::default() }
	}
}

impl<K, S, V> IndexedStorage<K, S, V>
where
	K: Ord + std::hash::Hash,
	S: Ord,
{
	/// Retrieves a reference to the value corresponding to the key, if present.
	pub fn get(&self, key: &K) -> Option<&V> {
		self.items_by_hashes.get(key)
	}

	/// Checks if the map contains the specified key.
	pub fn contains_key(&self, key: &K) -> bool {
		self.items_by_hashes.contains_key(key)
	}

	/// Returns an iterator over the values in the map.
	pub fn values(&self) -> std::collections::hash_map::Values<K, V> {
		self.items_by_hashes.values()
	}

	/// Returns the number of elements in the map.
	pub fn len(&self) -> usize {
		debug_assert_eq!(self.items_by_hashes.len(), self.items_by_priority.len());
		self.items_by_hashes.len()
	}

	/// Removes and returns the first entry in the priority map, if it exists (testing only).
	#[cfg(test)]
	pub fn pop_first(&mut self) -> Option<V> {
		self.items_by_priority.pop_first().map(|(_, v)| v)
	}

	pub fn with_items<F, R>(&self, f: F) -> R
	where
		F: Fn(std::collections::hash_map::Iter<K, V>) -> R,
	{
		f(self.items_by_hashes.iter())
	}
}

impl<K, A, B, V> IndexedStorage<K, PriorityKey<A, B>, V>
where
	K: Ord + std::hash::Hash + Copy,
	A: Ord,
	B: Ord,
	V: Clone + PriorityAndTimestamp<Priority = A, Timestamp = B>,
	V: std::cmp::PartialEq + std::fmt::Debug,
{
	/// Inserts a key-value pair into the map, ordering by priority.
	pub fn insert(&mut self, key: K, val: V) -> Option<V> {
		let r = self.items_by_hashes.insert(key, val.clone());

		if let Some(ref removed) = r {
			let a = self.items_by_priority.remove(&SortKey::new(&key, removed));
			debug_assert_eq!(r, a);
		}
		let a = self.items_by_priority.insert(SortKey::new(&key, &val), val);
		debug_assert!(a.is_none());
		r
	}

	/// Removes a key-value pair from the map based on the key.
	pub fn remove(&mut self, key: &K) -> Option<V> {
		let r = self.items_by_hashes.remove(key);
		let _ = r.as_ref().map(|r| {
			let k = SortKey::new(key, r);
			let a = self.items_by_priority.remove(&k);
			debug_assert_eq!(r.clone(), a.expect("item should be in both maps. qed."));
		});
		r
	}

	/// Allows to mutate item for given key with a closure, if key exists.
	///
	/// Intended to mutate priority and timestamp. Changing size is not possible.
	pub fn update_item<F>(&mut self, key: &K, f: F) -> Option<()>
	where
		F: FnOnce(&mut V),
	{
		let item = self.items_by_hashes.get_mut(key)?;

		let old_key = SortKey::new(key, item);
		f(item);
		let new_key = SortKey::new(key, item);

		if old_key != new_key {
			self.items_by_priority.remove(&old_key);
			self.items_by_priority.insert(new_key, item.clone());
		}

		Some(())
	}
}

impl<K, A, B, V> IndexedStorage<K, PriorityKey<A, B>, V>
where
	K: Ord + std::hash::Hash + Copy + std::fmt::Debug,
	A: Ord + std::fmt::Debug,
	B: Ord + std::fmt::Debug,
	V: Clone + PriorityAndTimestamp<Priority = A, Timestamp = B> + Size,
	V: std::cmp::PartialEq + std::fmt::Debug,
{
	/// Attempts to insert an item with replacement based on free space and priority.
	/// Returns the total size in bytes of removed items, and their keys.
	///
	/// Insertion always results with other item's removal, the len bound is kept elsewhere
	///
	/// If nothing was inserted `(None,0)` is returned.
	pub fn try_insert_with_replacement(
		&mut self,
		free_bytes: usize,
		key: K,
		item: V,
	) -> (Option<Vec<K>>, usize) {
		let mut total_size_removed = 0usize;
		let mut to_be_removed = vec![];

		if item.size() == 0 {
			return (None, 0);
		}

		if self.contains_key(&key) {
			return (None, 0);
		}

		for (SortKey(PriorityKey(worst_priority, worst_timestamp), worst_key), worst_item) in
			&self.items_by_priority
		{
			if *worst_priority > item.priority() {
				return (None, 0);
			}
			if *worst_priority == item.priority() && *worst_timestamp < item.timestamp() {
				return (None, 0);
			}

			total_size_removed += worst_item.size();
			to_be_removed.push(*worst_key);

			if free_bytes + total_size_removed >= item.size() {
				break;
			}
		}

		if item.size() > free_bytes + total_size_removed {
			return (None, 0);
		}

		self.insert(key, item);

		for worst_key in &to_be_removed {
			self.remove(worst_key);
		}

		(Some(to_be_removed), total_size_removed)
	}
}

impl<K, S, V> Default for SizeTrackedStore<K, S, V>
where
	K: Ord,
	S: Ord,
{
	fn default() -> Self {
		Self {
			index: Arc::new(IndexedStorage::default().into()),
			bytes: 0.into(),
			length: 0.into(),
		}
	}
}
//
impl<K, S, V> SizeTrackedStore<K, S, V>
where
	K: Ord,
	S: Ord,
{
	/// Current tracked length of the content.
	pub fn len(&self) -> usize {
		std::cmp::max(self.length.load(AtomicOrdering::Relaxed), 0) as usize
	}

	/// Current sum of content length.
	pub fn bytes(&self) -> usize {
		std::cmp::max(self.bytes.load(AtomicOrdering::Relaxed), 0) as usize
	}

	/// Lock map for read.
	pub async fn read(&self) -> SizeTrackedStoreReadAccess<K, S, V> {
		SizeTrackedStoreReadAccess { inner_guard: self.index.read().await }
	}

	/// Lock map for write.
	pub async fn write(&self) -> SizeTrackedStoreWriteAccess<K, S, V> {
		SizeTrackedStoreWriteAccess {
			inner_guard: self.index.write().await,
			bytes: &self.bytes,
			length: &self.length,
		}
	}
}

pub struct SizeTrackedStoreReadAccess<'a, K, S, V>
where
	K: Ord,
	S: Ord,
{
	inner_guard: RwLockReadGuard<'a, IndexedStorage<K, S, V>>,
}

impl<K, S, V> SizeTrackedStoreReadAccess<'_, K, S, V>
where
	K: Ord + std::hash::Hash,
	S: Ord,
{
	/// Returns true if the map contains given key.
	pub fn contains_key(&self, key: &K) -> bool {
		self.inner_guard.contains_key(key)
	}

	/// Returns the reference to the contained value by key, if exists.
	pub fn get(&self, key: &K) -> Option<&V> {
		self.inner_guard.get(key)
	}

	/// Returns an iterator over all values.
	pub fn values(&self) -> std::collections::hash_map::Values<K, V> {
		self.inner_guard.values()
	}

	/// Returns the number of elements in the map.
	pub fn len(&self) -> usize {
		self.inner_guard.len()
	}

	pub fn with_items<F, R>(&self, f: F) -> R
	where
		F: Fn(std::collections::hash_map::Iter<K, V>) -> R,
	{
		self.inner_guard.with_items(f)
	}
}

pub struct SizeTrackedStoreWriteAccess<'a, K, S, V>
where
	K: Ord,
	S: Ord,
{
	bytes: &'a AtomicIsize,
	length: &'a AtomicIsize,
	inner_guard: RwLockWriteGuard<'a, IndexedStorage<K, S, V>>,
}

impl<K, A, B, V> SizeTrackedStoreWriteAccess<'_, K, PriorityKey<A, B>, V>
where
	K: Ord + std::hash::Hash + Copy + std::fmt::Debug,
	A: Ord + std::fmt::Debug,
	B: Ord + std::fmt::Debug,
	V: Clone + PriorityAndTimestamp<Priority = A, Timestamp = B> + Size,
	V: std::cmp::PartialEq + std::fmt::Debug,
{
	/// Insert value and return previous (if any).
	pub fn insert(&mut self, key: K, val: V) -> Option<V> {
		let new_bytes = val.size();
		self.bytes.fetch_add(new_bytes as isize, AtomicOrdering::Relaxed);
		self.length.fetch_add(1, AtomicOrdering::Relaxed);
		self.inner_guard.insert(key, val).inspect(|old_val| {
			self.bytes.fetch_sub(old_val.size() as isize, AtomicOrdering::Relaxed);
			self.length.fetch_sub(1, AtomicOrdering::Relaxed);
		})
	}

	/// Remove value by key.
	pub fn remove(&mut self, key: &K) -> Option<V> {
		let val = self.inner_guard.remove(key);
		if let Some(size) = val.as_ref().map(Size::size) {
			self.bytes.fetch_sub(size as isize, AtomicOrdering::Relaxed);
			self.length.fetch_sub(1, AtomicOrdering::Relaxed);
		}
		val
	}

	/// Refer to [`IndexedStorage::try_insert_with_replacement`]
	pub fn try_insert_with_replacement(
		&mut self,
		max_total_bytes: usize,
		key: K,
		item: V,
	) -> Option<Vec<K>> {
		let item_size = item.size();
		let current_bytes = std::cmp::max(self.bytes.load(AtomicOrdering::Relaxed), 0) as usize;
		let free_bytes = max_total_bytes - current_bytes;
		let (removed_keys, removed_bytes) =
			self.inner_guard.try_insert_with_replacement(free_bytes, key, item);

		if let Some(ref removed_keys) = removed_keys {
			let delta = item_size as isize - removed_bytes as isize;
			self.bytes.fetch_add(delta, AtomicOrdering::Relaxed);
			self.length.fetch_sub(removed_keys.len() as isize, AtomicOrdering::Relaxed);
			self.length.fetch_add(1, AtomicOrdering::Relaxed);
		}
		removed_keys
	}

	/// Allows to mutate item for given key, if exists.
	///
	/// Intended to mutate priority and timestamp. Changing size is not possible.
	pub fn update_item<F>(&mut self, hash: &K, f: F) -> Option<()>
	where
		F: FnOnce(&mut V),
	{
		self.inner_guard.update_item(hash, f)
	}
}

impl<K, S, V> SizeTrackedStoreWriteAccess<'_, K, S, V>
where
	K: Ord + std::hash::Hash,
	S: Ord,
{
	/// Returns `true` if the inner map contains a value for the specified key.
	pub fn contains_key(&self, key: &K) -> bool {
		self.inner_guard.contains_key(key)
	}

	/// Returns the number of elements in the map.
	pub fn len(&mut self) -> usize {
		self.inner_guard.len()
	}

	#[cfg(test)]
	pub fn pop_first(&mut self) -> Option<V> {
		self.inner_guard.pop_first()
	}
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::*;

	#[derive(Clone, Debug, PartialEq)]
	struct TestItem {
		size: usize,
		prio: u32,
		ts: u32,
	}

	impl PriorityAndTimestamp for TestItem {
		type Priority = u32;
		type Timestamp = u32;

		fn priority(&self) -> Self::Priority {
			self.prio
		}
		fn timestamp(&self) -> Self::Timestamp {
			self.ts
		}
	}

	impl Size for TestItem {
		fn size(&self) -> usize {
			self.size
		}
	}

	impl TestItem {
		fn new(prio: u32, ts: u32, size: usize) -> Self {
			Self { prio, ts, size }
		}
	}

	#[tokio::test]
	async fn basic() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 10);
		let i1 = TestItem::new(2, 0, 11);
		let i2 = TestItem::new(3, 0, 20);

		map.write().await.insert(0xa, i0);
		map.write().await.insert(0xb, i1);

		assert_eq!(map.bytes(), 21);
		assert_eq!(map.len(), 2);

		map.write().await.insert(0xc, i2);

		assert_eq!(map.bytes(), 41);
		assert_eq!(map.len(), 3);

		map.write().await.remove(&0xa);
		assert_eq!(map.bytes(), 31);
		assert_eq!(map.len(), 2);
		assert_eq!(map.read().await.len(), 2);
	}

	#[tokio::test]
	async fn insert_same_hash() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 10);
		let i1 = TestItem::new(2, 0, 11);
		let i2 = TestItem::new(3, 0, 20);
		let i3 = TestItem::new(4, 0, 40);
		let i4 = TestItem::new(5, 0, 50);
		let i5 = TestItem::new(6, 0, 1);

		map.write().await.insert(0xa, i0);
		map.write().await.insert(0xb, i1.clone());
		map.write().await.insert(0xc, i2);
		assert_eq!(map.bytes(), 41);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		map.write().await.insert(0xc, i3.clone());
		assert_eq!(map.bytes(), 61);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		map.write().await.insert(0xa, i4);
		assert_eq!(map.bytes(), 101);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		map.write().await.insert(0xa, i5.clone());
		assert_eq!(map.bytes(), 52);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		let items = map.read().await.values().cloned().collect::<Vec<_>>();
		let expected = [i1, i3, i5];
		assert!(expected.iter().all(|e| items.contains(e)),);
		assert_eq!(items.len(), expected.len());
	}

	#[tokio::test]
	async fn remove_non_existent() {
		let map = SizeTrackedStore::<_, _, TestItem>::default();
		map.write().await.remove(&0xa);
		assert_eq!(map.bytes(), 0);
		assert_eq!(map.len(), 0);
		assert_eq!(map.read().await.len(), 0);
	}

	#[rstest]
	#[case(20, 30, 50, 50, 100, 3, 100, 2, 100)]
	#[case(2, 46, 50, 3, 100, 3, 98, 3, 99)]
	#[case(2, 46, 50, 2, 100, 3, 98, 3, 98)]
	#[case(2, 47, 50, 4, 100, 3, 99, 2, 54)]
	#[case(1, 47, 50, 99, 100, 3, 98, 1, 99)]
	#[case(1, 1, 1, 2, 100, 3, 3, 3, 4)] //always remove
	#[case(20, 30, 40, 150, 100, 3, 90, 3, 90)]
	#[case(10, 20, 30, 80, 100, 3, 60, 1, 80)]
	#[tokio::test]
	#[allow(clippy::too_many_arguments)]
	async fn try_insert_with_replacement_works_param2(
		#[case] i0_bytes: usize,
		#[case] i1_bytes: usize,
		#[case] i2_bytes: usize,
		#[case] i3_bytes: usize,
		#[case] max_bytes: usize,
		#[case] expected_len_before: usize,
		#[case] expected_bytes_before: usize,
		#[case] expected_len_after: usize,
		#[case] expected_bytes_after: usize,
	) {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, i0_bytes);
		let i1 = TestItem::new(2, 0, i1_bytes);
		let i2 = TestItem::new(3, 0, i2_bytes);

		map.write().await.insert(0xa, i0);
		map.write().await.insert(0xb, i1);
		map.write().await.insert(0xc, i2.clone());

		assert_eq!(map.bytes(), expected_bytes_before);
		assert_eq!(map.len(), expected_len_before);

		let i3 = TestItem::new(4, 0, i3_bytes);
		map.write().await.try_insert_with_replacement(max_bytes, 0xd, i3.clone());

		assert_eq!(map.len(), expected_len_after);
		assert_eq!(map.read().await.len(), expected_len_after);
		assert_eq!(map.bytes(), expected_bytes_after);
	}

	#[tokio::test]
	async fn try_insert_with_replacement_items() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 20);
		let i1 = TestItem::new(2, 0, 30);
		let i2 = TestItem::new(3, 0, 50);

		map.write().await.insert(0xa, i0);
		map.write().await.insert(0xb, i1);
		map.write().await.insert(0xc, i2.clone());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);

		let i3 = TestItem::new(4, 0, 50);
		let _ = map.write().await.try_insert_with_replacement(100, 0xd, i3.clone());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 2);
		assert_eq!(map.read().await.len(), 2);

		let items = map.read().await.values().cloned().collect::<Vec<_>>();
		let expected = [&i2, &i3];
		assert!(expected.iter().all(|e| items.contains(e)),);
		assert_eq!(items.len(), expected.len());

		assert_eq!(map.write().await.pop_first().unwrap(), i2);
		assert_eq!(map.write().await.pop_first().unwrap(), i3);
	}

	#[tokio::test]
	async fn try_insert_with_replacement_works_known_key_reject() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 20);
		let i1 = TestItem::new(2, 0, 30);
		let i2 = TestItem::new(3, 0, 50);

		map.write().await.insert(0xa, i0.clone());
		map.write().await.insert(0xb, i1.clone());
		map.write().await.insert(0xc, i2.clone());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);

		let i3 = TestItem::new(4, 0, 50);
		let r = map.write().await.try_insert_with_replacement(100, 0xb, i3);

		assert!(r.is_none());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		let items = map.read().await.values().cloned().collect::<Vec<_>>();
		let expected = [&i0, &i1, &i2];
		assert!(expected.iter().all(|e| items.contains(e)),);
		assert_eq!(items.len(), expected.len());
		assert_eq!(map.write().await.pop_first().unwrap(), i0);
		assert_eq!(map.write().await.pop_first().unwrap(), i1);
		assert_eq!(map.write().await.pop_first().unwrap(), i2);
	}

	#[tokio::test]
	async fn try_insert_with_replacement_zero_size_reject() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 20);
		let i1 = TestItem::new(2, 0, 30);
		let i2 = TestItem::new(3, 0, 50);

		map.write().await.insert(0xa, i0.clone());
		map.write().await.insert(0xb, i1.clone());
		map.write().await.insert(0xc, i2.clone());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);

		let i3 = TestItem::new(4, 0, 0);
		let r = map.write().await.try_insert_with_replacement(100, 0xb, i3);

		assert!(r.is_none());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		let items = map.read().await.values().cloned().collect::<Vec<_>>();
		let expected = [i0, i1, i2];
		assert!(expected.iter().all(|e| items.contains(e)),);
		assert_eq!(items.len(), expected.len());
	}

	#[tokio::test]
	async fn sorting_works() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 10);
		let i1 = TestItem::new(1, 1, 10);
		let i2 = TestItem::new(2, 0, 20);
		let i3 = TestItem::new(3, 0, 30);
		let i4 = TestItem::new(4, 0, 40);

		map.write().await.insert(0xc, i2.clone());
		map.write().await.insert(0xb, i1.clone());
		map.write().await.insert(0xa, i0.clone());
		map.write().await.insert(0xe, i4.clone());
		map.write().await.insert(0xd, i3.clone());

		assert_eq!(map.bytes(), 110);
		assert_eq!(map.len(), 5);
		assert_eq!(map.write().await.pop_first().unwrap(), i0);
		assert_eq!(map.write().await.pop_first().unwrap(), i1);
		assert_eq!(map.write().await.pop_first().unwrap(), i2);
		assert_eq!(map.write().await.pop_first().unwrap(), i3);
		assert_eq!(map.write().await.pop_first().unwrap(), i4);
	}

	#[tokio::test]
	async fn update_item() {
		let map = SizeTrackedStore::default();

		let i0 = TestItem::new(1, 0, 20);
		let i1 = TestItem::new(2, 0, 30);
		let iu = TestItem::new(0, 0, 30);
		let i2 = TestItem::new(3, 0, 50);

		map.write().await.insert(0xa, i0.clone());
		map.write().await.insert(0xb, i1.clone());
		map.write().await.insert(0xc, i2.clone());

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		map.write().await.update_item(&0xb, |item| item.prio = iu.prio).unwrap();

		assert_eq!(map.bytes(), 100);
		assert_eq!(map.len(), 3);
		assert_eq!(map.read().await.len(), 3);

		let items = map.read().await.values().cloned().collect::<Vec<_>>();
		let expected = [&i0, &iu, &i2];
		assert!(expected.iter().all(|e| items.contains(e)),);
		assert_eq!(items.len(), expected.len());

		assert_eq!(map.write().await.pop_first().unwrap(), iu);
		assert_eq!(map.write().await.pop_first().unwrap(), i0);
		assert_eq!(map.write().await.pop_first().unwrap(), i2);
	}
}

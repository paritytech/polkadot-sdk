// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Bounded, in-memory store of the package info this node knows about.

use crate::{handler::PackageInfoProvider, types::PackageInfo};
use parking_lot::Mutex;
use schnellru::{ByLength, LruMap};

/// Default capacity of [`PackageInfoStore`], in blocks.
pub const DEFAULT_CAPACITY: u32 = 256;

/// A bounded LRU map from block hash to [`PackageInfo`].
pub struct PackageInfoStore<Hash: Eq + core::hash::Hash + Clone>(
	Mutex<LruMap<Hash, PackageInfo, ByLength>>,
);

impl<Hash: Eq + core::hash::Hash + Clone> PackageInfoStore<Hash> {
	/// Create a store holding at most `capacity` entries.
	pub fn new(capacity: u32) -> Self {
		Self(Mutex::new(LruMap::new(ByLength::new(capacity))))
	}

	/// Insert `info` for `hash`, evicting the least recently used entry if the store is full.
	pub fn insert(&self, hash: Hash, info: PackageInfo) {
		self.0.lock().insert(hash, info);
	}

	/// The info stored for `hash`, if any. Marks the entry as recently used.
	pub fn get(&self, hash: &Hash) -> Option<PackageInfo> {
		self.0.lock().get(hash).cloned()
	}

	/// Whether the store holds an entry for `hash`. Does not change the eviction order.
	pub fn contains(&self, hash: &Hash) -> bool {
		self.0.lock().peek(hash).is_some()
	}

	/// Remove and return the entry for `hash`, if any.
	pub fn remove(&self, hash: &Hash) -> Option<PackageInfo> {
		self.0.lock().remove(hash)
	}
}

impl<Hash> PackageInfoProvider<Hash> for PackageInfoStore<Hash>
where
	Hash: Eq + core::hash::Hash + Clone + Send + Sync + 'static,
{
	fn package_info(&self, block: &Hash) -> Option<PackageInfo> {
		self.get(block)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn info(seed: u8) -> PackageInfo {
		PackageInfo {
			authorization: vec![seed],
			prerequisites: vec![],
			pov: crate::types::PovSpec { hash: [seed; 32], len: seed as u32 },
		}
	}

	#[test]
	fn store_insert_get_contains_remove() {
		let store = PackageInfoStore::<[u8; 32]>::new(4);

		assert!(!store.contains(&[1u8; 32]));
		store.insert([1u8; 32], info(1));
		assert!(store.contains(&[1u8; 32]));
		assert_eq!(store.get(&[1u8; 32]), Some(info(1)));

		assert_eq!(store.remove(&[1u8; 32]), Some(info(1)));
		assert!(!store.contains(&[1u8; 32]));
		assert_eq!(store.get(&[1u8; 32]), None);
		assert_eq!(store.remove(&[1u8; 32]), None);
	}

	#[test]
	fn store_evicts_past_capacity() {
		let store = PackageInfoStore::<[u8; 32]>::new(2);

		store.insert([1u8; 32], info(1));
		store.insert([2u8; 32], info(2));
		store.insert([3u8; 32], info(3));

		assert!(!store.contains(&[1u8; 32]), "the least recently used entry is evicted");
		assert!(store.contains(&[2u8; 32]));
		assert!(store.contains(&[3u8; 32]));
	}

	#[test]
	fn store_implements_package_info_provider() {
		let store = PackageInfoStore::<[u8; 32]>::new(2);
		store.insert([1u8; 32], info(1));

		assert_eq!(PackageInfoProvider::package_info(&store, &[1u8; 32]), Some(info(1)));
	}
}

// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

//! State machine backends. These manage the code and storage of contracts.

use std::{error, fmt, cmp::Ord, collections::{HashMap, BTreeMap}, marker::PhantomData, ops};
use log::warn;
use hash_db::Hasher;
use crate::trie_backend::TrieBackend;
use crate::trie_backend_essence::TrieBackendStorage;
use sp_trie::{
	TrieMut, MemoryDB, child_trie_root, default_child_trie_root, TrieConfiguration,
	trie_types::{TrieDBMut, Layout},
};
use codec::{Encode, Codec};
use sp_core::storage::{ChildInfo, OwnedChildInfo, Storage};

/// A state backend is used to read state data and can have changes committed
/// to it.
///
/// The clone operation (if implemented) should be cheap.
pub trait Backend<H: Hasher>: std::fmt::Debug {
	/// An error type when fetching data is not possible.
	type Error: super::Error;

	/// Storage changes to be applied if committing
	type Transaction: Consolidate + Default;

	/// Type of trie backend storage.
	type TrieBackendStorage: TrieBackendStorage<H>;

	/// Get keyed storage or None if there is nothing associated.
	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;

	/// Get keyed storage value hash or None if there is nothing associated.
	fn storage_hash(&self, key: &[u8]) -> Result<Option<H::Out>, Self::Error> {
		self.storage(key).map(|v| v.map(|v| H::hash(&v)))
	}

	/// Get keyed child storage or None if there is nothing associated.
	fn child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error>;

	/// Get child keyed storage value hash or None if there is nothing associated.
	fn child_storage_hash(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<H::Out>, Self::Error> {
		self.child_storage(storage_key, child_info, key).map(|v| v.map(|v| H::hash(&v)))
	}

	/// true if a key exists in storage.
	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		Ok(self.storage(key)?.is_some())
	}

	/// true if a key exists in child storage.
	fn exists_child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		Ok(self.child_storage(storage_key, child_info, key)?.is_some())
	}

	/// Return the next key in storage in lexicographic order or `None` if there is no value.
	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;

	/// Return the next key in child storage in lexicographic order or `None` if there is no value.
	fn next_child_storage_key(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8]
	) -> Result<Option<Vec<u8>>, Self::Error>;

	/// Retrieve all entries keys of child storage and call `f` for each of those keys.
	fn for_keys_in_child_storage<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		f: F,
	);

	/// Retrieve all entries keys which start with the given prefix and
	/// call `f` for each of those keys.
	fn for_keys_with_prefix<F: FnMut(&[u8])>(&self, prefix: &[u8], mut f: F) {
		self.for_key_values_with_prefix(prefix, |k, _v| f(k))
	}

	/// Retrieve all entries keys and values of which start with the given prefix and
	/// call `f` for each of those keys.
	fn for_key_values_with_prefix<F: FnMut(&[u8], &[u8])>(&self, prefix: &[u8], f: F);


	/// Retrieve all child entries keys which start with the given prefix and
	/// call `f` for each of those keys.
	fn for_child_keys_with_prefix<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
		f: F,
	);

	/// Calculate the storage root, with given delta over what is already stored in
	/// the backend, and produce a "transaction" that can be used to commit.
	/// Does not include child storage updates.
	fn storage_root<I>(&self, delta: I) -> (H::Out, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		H::Out: Ord;

	/// Calculate the child storage root, with given delta over what is already stored in
	/// the backend, and produce a "transaction" that can be used to commit. The second argument
	/// is true if child storage root equals default storage root.
	fn child_storage_root<I>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		delta: I,
	) -> (H::Out, bool, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		H::Out: Ord;

	/// Get all key/value pairs into a Vec.
	fn pairs(&self) -> Vec<(Vec<u8>, Vec<u8>)>;

	/// Get all keys with given prefix
	fn keys(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
		let mut all = Vec::new();
		self.for_keys_with_prefix(prefix, |k| all.push(k.to_vec()));
		all
	}

	/// Get all keys of child storage with given prefix
	fn child_keys(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
	) -> Vec<Vec<u8>> {
		let mut all = Vec::new();
		self.for_child_keys_with_prefix(storage_key, child_info, prefix, |k| all.push(k.to_vec()));
		all
	}

	/// Try convert into trie backend.
	fn as_trie_backend(&mut self) -> Option<&TrieBackend<Self::TrieBackendStorage, H>> {
		None
	}

	/// Calculate the storage root, with given delta over what is already stored
	/// in the backend, and produce a "transaction" that can be used to commit.
	/// Does include child storage updates.
	fn full_storage_root<I1, I2i, I2>(
		&self,
		delta: I1,
		child_deltas: I2)
	-> (H::Out, Self::Transaction)
	where
		I1: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		I2i: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		I2: IntoIterator<Item=(Vec<u8>, I2i, OwnedChildInfo)>,
		H::Out: Ord + Encode,
	{
		let mut txs: Self::Transaction = Default::default();
		let mut child_roots: Vec<_> = Default::default();
		// child first
		for (storage_key, child_delta, child_info) in child_deltas {
			let (child_root, empty, child_txs) =
				self.child_storage_root(&storage_key[..], child_info.as_ref(), child_delta);
			txs.consolidate(child_txs);
			if empty {
				child_roots.push((storage_key, None));
			} else {
				child_roots.push((storage_key, Some(child_root.encode())));
			}
		}
		let (root, parent_txs) = self.storage_root(
			delta.into_iter().chain(child_roots.into_iter())
		);
		txs.consolidate(parent_txs);
		(root, txs)
	}
}

impl<'a, T: Backend<H>, H: Hasher> Backend<H> for &'a T {
	type Error = T::Error;
	type Transaction = T::Transaction;
	type TrieBackendStorage = T::TrieBackendStorage;

	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		(*self).storage(key)
	}

	fn child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		(*self).child_storage(storage_key, child_info, key)
	}

	fn for_keys_in_child_storage<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		f: F,
	) {
		(*self).for_keys_in_child_storage(storage_key, child_info, f)
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		(*self).next_storage_key(key)
	}

	fn next_child_storage_key(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		(*self).next_child_storage_key(storage_key, child_info, key)
	}

	fn for_keys_with_prefix<F: FnMut(&[u8])>(&self, prefix: &[u8], f: F) {
		(*self).for_keys_with_prefix(prefix, f)
	}

	fn for_child_keys_with_prefix<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
		f: F,
	) {
		(*self).for_child_keys_with_prefix(storage_key, child_info, prefix, f)
	}

	fn storage_root<I>(&self, delta: I) -> (H::Out, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		H::Out: Ord,
	{
		(*self).storage_root(delta)
	}

	fn child_storage_root<I>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		delta: I,
	) -> (H::Out, bool, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		H::Out: Ord,
	{
		(*self).child_storage_root(storage_key, child_info, delta)
	}

	fn pairs(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
		(*self).pairs()
	}

	fn for_key_values_with_prefix<F: FnMut(&[u8], &[u8])>(&self, prefix: &[u8], f: F) {
		(*self).for_key_values_with_prefix(prefix, f);
	}
}

/// Trait that allows consolidate two transactions together.
pub trait Consolidate {
	/// Consolidate two transactions into one.
	fn consolidate(&mut self, other: Self);
}

impl Consolidate for () {
	fn consolidate(&mut self, _: Self) {
		()
	}
}

impl Consolidate for Vec<(
		Option<(Vec<u8>, OwnedChildInfo)>,
		Vec<(Vec<u8>, Option<Vec<u8>>)>,
	)> {
	fn consolidate(&mut self, mut other: Self) {
		self.append(&mut other);
	}
}

impl<H: Hasher, KF: sp_trie::KeyFunction<H>> Consolidate for sp_trie::GenericMemoryDB<H, KF> {
	fn consolidate(&mut self, other: Self) {
		sp_trie::GenericMemoryDB::consolidate(self, other)
	}
}

/// Error impossible.
// FIXME: use `!` type when stabilized. https://github.com/rust-lang/rust/issues/35121
#[derive(Debug)]
pub enum Void {}

impl fmt::Display for Void {
	fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
		match *self {}
	}
}

impl error::Error for Void {
	fn description(&self) -> &str { "unreachable error" }
}

/// In-memory backend. Fully recomputes tries each time `as_trie_backend` is called but useful for
/// tests and proof checking.
pub struct InMemory<H: Hasher> {
	inner: HashMap<Option<(Vec<u8>, OwnedChildInfo)>, BTreeMap<Vec<u8>, Vec<u8>>>,
	// This field is only needed for returning reference in `as_trie_backend`.
	trie: Option<TrieBackend<MemoryDB<H>, H>>,
	_hasher: PhantomData<H>,
}

impl<H: Hasher> std::fmt::Debug for InMemory<H> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "InMemory ({} values)", self.inner.len())
	}
}

impl<H: Hasher> Default for InMemory<H> {
	fn default() -> Self {
		InMemory {
			inner: Default::default(),
			trie: None,
			_hasher: PhantomData,
		}
	}
}

impl<H: Hasher> Clone for InMemory<H> {
	fn clone(&self) -> Self {
		InMemory {
			inner: self.inner.clone(),
			trie: None,
			_hasher: PhantomData,
		}
	}
}

impl<H: Hasher> PartialEq for InMemory<H> {
	fn eq(&self, other: &Self) -> bool {
		self.inner.eq(&other.inner)
	}
}

impl<H: Hasher> InMemory<H> where H::Out: Codec {
	/// Copy the state, with applied updates
	pub fn update(&self, changes: <Self as Backend<H>>::Transaction) -> Self {
		let mut inner = self.inner.clone();
		for (child_info, key_values) in changes {
			let entry = inner.entry(child_info).or_default();
			for (key, val) in key_values {
				match val {
					Some(v) => { entry.insert(key, v); },
					None => { entry.remove(&key); },
				}
			}
		}
		inner.into()
	}
}

impl<H: Hasher> From<HashMap<Option<(Vec<u8>, OwnedChildInfo)>, BTreeMap<Vec<u8>, Vec<u8>>>> for InMemory<H> {
	fn from(inner: HashMap<Option<(Vec<u8>, OwnedChildInfo)>, BTreeMap<Vec<u8>, Vec<u8>>>) -> Self {
		InMemory {
			inner: inner,
			trie: None,
			_hasher: PhantomData,
		}
	}
}

impl<H: Hasher> From<Storage> for InMemory<H> {
	fn from(inners: Storage) -> Self {
		let mut inner: HashMap<Option<(Vec<u8>, OwnedChildInfo)>, BTreeMap<Vec<u8>, Vec<u8>>>
			= inners.children.into_iter().map(|(k, c)| (Some((k, c.child_info)), c.data)).collect();
		inner.insert(None, inners.top);
		InMemory {
			inner: inner,
			trie: None,
			_hasher: PhantomData,
		}
	}
}

impl<H: Hasher> From<BTreeMap<Vec<u8>, Vec<u8>>> for InMemory<H> {
	fn from(inner: BTreeMap<Vec<u8>, Vec<u8>>) -> Self {
		let mut expanded = HashMap::new();
		expanded.insert(None, inner);
		InMemory {
			inner: expanded,
			trie: None,
			_hasher: PhantomData,
		}
	}
}

impl<H: Hasher> From<Vec<(Option<(Vec<u8>, OwnedChildInfo)>, Vec<(Vec<u8>, Option<Vec<u8>>)>)>>
	for InMemory<H> {
	fn from(
		inner: Vec<(Option<(Vec<u8>, OwnedChildInfo)>, Vec<(Vec<u8>, Option<Vec<u8>>)>)>,
	) -> Self {
		let mut expanded: HashMap<Option<(Vec<u8>, OwnedChildInfo)>, BTreeMap<Vec<u8>, Vec<u8>>>
			= HashMap::new();
		for (child_info, key_values) in inner {
			let entry = expanded.entry(child_info).or_default();
			for (key, value) in key_values {
				if let Some(value) = value {
					entry.insert(key, value);
				}
			}
		}
		expanded.into()
	}
}

impl<H: Hasher> InMemory<H> {
	/// child storage key iterator
	pub fn child_storage_keys(&self) -> impl Iterator<Item=(&[u8], ChildInfo)> {
		self.inner.iter().filter_map(|item|
			item.0.as_ref().map(|v|(&v.0[..], v.1.as_ref()))
		)
	}
}

impl<H: Hasher> Backend<H> for InMemory<H> where H::Out: Codec {
	type Error = Void;
	type Transaction = Vec<(
		Option<(Vec<u8>, OwnedChildInfo)>,
		Vec<(Vec<u8>, Option<Vec<u8>>)>,
	)>;
	type TrieBackendStorage = MemoryDB<H>;

	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		Ok(self.inner.get(&None).and_then(|map| map.get(key).map(Clone::clone)))
	}

	fn child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		Ok(self.inner.get(&Some((storage_key.to_vec(), child_info.to_owned())))
			.and_then(|map| map.get(key).map(Clone::clone)))
	}

	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		Ok(self.inner.get(&None).map(|map| map.get(key).is_some()).unwrap_or(false))
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		let range = (ops::Bound::Excluded(key), ops::Bound::Unbounded);
		let next_key = self.inner.get(&None)
			.and_then(|map| map.range::<[u8], _>(range).next().map(|(k, _)| k).cloned());

		Ok(next_key)
	}

	fn next_child_storage_key(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		let range = (ops::Bound::Excluded(key), ops::Bound::Unbounded);
		let next_key = self.inner.get(&Some((storage_key.to_vec(), child_info.to_owned())))
			.and_then(|map| map.range::<[u8], _>(range).next().map(|(k, _)| k).cloned());

		Ok(next_key)
	}

	fn for_keys_with_prefix<F: FnMut(&[u8])>(&self, prefix: &[u8], f: F) {
		self.inner.get(&None).map(|map| map.keys().filter(|key| key.starts_with(prefix)).map(|k| &**k).for_each(f));
	}

	fn for_key_values_with_prefix<F: FnMut(&[u8], &[u8])>(&self, prefix: &[u8], mut f: F) {
		self.inner.get(&None).map(|map| map.iter().filter(|(key, _val)| key.starts_with(prefix))
			.for_each(|(k, v)| f(k, v)));
	}

	fn for_keys_in_child_storage<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		mut f: F,
	) {
		self.inner.get(&Some((storage_key.to_vec(), child_info.to_owned())))
			.map(|map| map.keys().for_each(|k| f(&k)));
	}

	fn for_child_keys_with_prefix<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
		f: F,
	) {
		self.inner.get(&Some((storage_key.to_vec(), child_info.to_owned())))
			.map(|map| map.keys().filter(|key| key.starts_with(prefix)).map(|k| &**k).for_each(f));
	}

	fn storage_root<I>(&self, delta: I) -> (H::Out, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		<H as Hasher>::Out: Ord,
	{
		let existing_pairs = self.inner.get(&None)
			.into_iter()
			.flat_map(|map| map.iter().map(|(k, v)| (k.clone(), Some(v.clone()))));

		let transaction: Vec<_> = delta.into_iter().collect();
		let root = Layout::<H>::trie_root(existing_pairs.chain(transaction.iter().cloned())
			.collect::<HashMap<_, _>>()
			.into_iter()
			.filter_map(|(k, maybe_val)| maybe_val.map(|val| (k, val)))
		);

		let full_transaction = transaction.into_iter().collect();

		(root, vec![(None, full_transaction)])
	}

	fn child_storage_root<I>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		delta: I,
	) -> (H::Out, bool, Self::Transaction)
	where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
		H::Out: Ord
	{
		let storage_key = storage_key.to_vec();
		let child_info = Some((storage_key.clone(), child_info.to_owned()));


		let existing_pairs = self.inner.get(&child_info)
			.into_iter()
			.flat_map(|map| map.iter().map(|(k, v)| (k.clone(), Some(v.clone()))));

		let transaction: Vec<_> = delta.into_iter().collect();
		let root = child_trie_root::<Layout<H>, _, _, _>(
			&storage_key,
			existing_pairs.chain(transaction.iter().cloned())
				.collect::<HashMap<_, _>>()
				.into_iter()
				.filter_map(|(k, maybe_val)| maybe_val.map(|val| (k, val)))
		);

		let full_transaction = transaction.into_iter().collect();

		let is_default = root == default_child_trie_root::<Layout<H>>(&storage_key);

		(root, is_default, vec![(child_info, full_transaction)])
	}

	fn pairs(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
		self.inner.get(&None)
			.into_iter()
			.flat_map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())))
			.collect()
	}

	fn keys(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
		self.inner.get(&None)
			.into_iter()
			.flat_map(|map| map.keys().filter(|k| k.starts_with(prefix)).cloned())
			.collect()
	}

	fn child_keys(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
	) -> Vec<Vec<u8>> {
		self.inner.get(&Some((storage_key.to_vec(), child_info.to_owned())))
			.into_iter()
			.flat_map(|map| map.keys().filter(|k| k.starts_with(prefix)).cloned())
			.collect()
	}

	fn as_trie_backend(&mut self)-> Option<&TrieBackend<Self::TrieBackendStorage, H>> {
		let mut mdb = MemoryDB::default();
		let mut new_child_roots = Vec::new();
		let mut root_map = None;
		for (child_info, map) in &self.inner {
			if let Some((storage_key, _child_info)) = child_info.as_ref() {
				// no need to use child_info at this point because we use a MemoryDB for
				// proof (with PrefixedMemoryDB it would be needed).
				let ch = insert_into_memory_db::<H, _>(&mut mdb, map.clone().into_iter())?;
				new_child_roots.push((storage_key.clone(), ch.as_ref().into()));
			} else {
				root_map = Some(map);
			}
		}
		let root = match root_map {
			Some(map) => insert_into_memory_db::<H, _>(
				&mut mdb,
				map.clone().into_iter().chain(new_child_roots.into_iter()),
			)?,
			None => insert_into_memory_db::<H, _>(
				&mut mdb,
				new_child_roots.into_iter(),
			)?,
		};
		self.trie = Some(TrieBackend::new(mdb, root));
		self.trie.as_ref()
	}
}

/// Insert input pairs into memory db.
pub(crate) fn insert_into_memory_db<H, I>(mdb: &mut MemoryDB<H>, input: I) -> Option<H::Out>
	where
		H: Hasher,
		I: IntoIterator<Item=(Vec<u8>, Vec<u8>)>,
{
	let mut root = <H as Hasher>::Out::default();
	{
		let mut trie = TrieDBMut::<H>::new(mdb, &mut root);
		for (key, value) in input {
			if let Err(e) = trie.insert(&key, &value) {
				warn!(target: "trie", "Failed to write to trie: {}", e);
				return None;
			}
		}
	}

	Some(root)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Assert in memory backend with only child trie keys works as trie backend.
	#[test]
	fn in_memory_with_child_trie_only() {
		let storage = InMemory::<sp_core::Blake2Hasher>::default();
		let child_info = OwnedChildInfo::new_default(b"unique_id_1".to_vec());
		let mut storage = storage.update(
			vec![(
				Some((b"1".to_vec(), child_info.clone())),
				vec![(b"2".to_vec(), Some(b"3".to_vec()))]
			)]
		);
		let trie_backend = storage.as_trie_backend().unwrap();
		assert_eq!(trie_backend.child_storage(b"1", child_info.as_ref(), b"2").unwrap(),
			Some(b"3".to_vec()));
		assert!(trie_backend.storage(b"1").unwrap().is_some());
	}
}

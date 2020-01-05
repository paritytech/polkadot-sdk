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

//! Concrete externalities implementation.

use crate::{
	backend::Backend, OverlayedChanges,
	changes_trie::{
		Storage as ChangesTrieStorage, CacheAction as ChangesTrieCacheAction, build_changes_trie,
	},
};

use hash_db::Hasher;
use sp_core::{
	storage::{ChildStorageKey, well_known_keys::is_child_storage_key, ChildInfo},
	traits::Externalities, hexdisplay::HexDisplay, hash::H256,
};
use sp_trie::{trie_types::Layout, MemoryDB, default_child_trie_root};
use sp_externalities::Extensions;
use codec::{Decode, Encode};

use std::{error, fmt, any::{Any, TypeId}};
use log::{warn, trace};

const EXT_NOT_ALLOWED_TO_FAIL: &str = "Externalities not allowed to fail within runtime";

/// Errors that can occur when interacting with the externalities.
#[derive(Debug, Copy, Clone)]
pub enum Error<B, E> {
	/// Failure to load state data from the backend.
	#[allow(unused)]
	Backend(B),
	/// Failure to execute a function.
	#[allow(unused)]
	Executor(E),
}

impl<B: fmt::Display, E: fmt::Display> fmt::Display for Error<B, E> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Error::Backend(ref e) => write!(f, "Storage backend error: {}", e),
			Error::Executor(ref e) => write!(f, "Sub-call execution error: {}", e),
		}
	}
}

impl<B: error::Error, E: error::Error> error::Error for Error<B, E> {
	fn description(&self) -> &str {
		match *self {
			Error::Backend(..) => "backend error",
			Error::Executor(..) => "executor error",
		}
	}
}

/// Wraps a read-only backend, call executor, and current overlayed changes.
pub struct Ext<'a, H, N, B, T> where H: Hasher<Out=H256>, B: 'a + Backend<H> {
	/// The overlayed changes to write to.
	overlay: &'a mut OverlayedChanges,
	/// The storage backend to read from.
	backend: &'a B,
	/// The storage transaction necessary to commit to the backend. Is cached when
	/// `storage_root` is called and the cache is cleared on every subsequent change.
	storage_transaction: Option<(B::Transaction, H::Out)>,
	/// Changes trie storage to read from.
	changes_trie_storage: Option<&'a T>,
	/// The changes trie transaction necessary to commit to the changes trie backend.
	/// Set to Some when `storage_changes_root` is called. Could be replaced later
	/// by calling `storage_changes_root` again => never used as cache.
	/// This differs from `storage_transaction` behavior, because the moment when
	/// `storage_changes_root` is called matters + we need to remember additional
	/// data at this moment (block number).
	changes_trie_transaction: Option<(MemoryDB<H>, H::Out, ChangesTrieCacheAction<H::Out, N>)>,
	/// Pseudo-unique id used for tracing.
	pub id: u16,
	/// Dummy usage of N arg.
	_phantom: std::marker::PhantomData<N>,
	/// Extensions registered with this instance.
	extensions: Option<&'a mut Extensions>,
}

impl<'a, H, N, B, T> Ext<'a, H, N, B, T>
where
	H: Hasher<Out=H256>,
	B: 'a + Backend<H>,
	T: 'a + ChangesTrieStorage<H, N>,
	N: crate::changes_trie::BlockNumber,
{

	/// Create a new `Ext` from overlayed changes and read-only backend
	pub fn new(
		overlay: &'a mut OverlayedChanges,
		backend: &'a B,
		changes_trie_storage: Option<&'a T>,
		extensions: Option<&'a mut Extensions>,
	) -> Self {
		Ext {
			overlay,
			backend,
			storage_transaction: None,
			changes_trie_storage,
			changes_trie_transaction: None,
			id: rand::random(),
			_phantom: Default::default(),
			extensions,
		}
	}

	/// Get the transaction necessary to update the backend.
	pub fn transaction(&mut self) -> (
		(B::Transaction, H256),
		Option<crate::ChangesTrieTransaction<H, N>>,
	) {
		let _ = self.storage_root();

		let (storage_transaction, changes_trie_transaction) = (
			self.storage_transaction
				.take()
				.expect("storage_transaction always set after calling storage root; qed"),
			self.changes_trie_transaction
				.take()
				.map(|(tx, _, cache)| (tx, cache)),
		);

		(
			storage_transaction,
			changes_trie_transaction,
		)
	}

	/// Invalidates the currently cached storage root and the db transaction.
	///
	/// Called when there are changes that likely will invalidate the storage root.
	fn mark_dirty(&mut self) {
		self.storage_transaction = None;
	}
}

#[cfg(test)]
impl<'a, H, N, B, T> Ext<'a, H, N, B, T>
where
	H: Hasher<Out=H256>,
	B: 'a + Backend<H>,
	T: 'a + ChangesTrieStorage<H, N>,
	N: crate::changes_trie::BlockNumber,
{
	pub fn storage_pairs(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
		use std::collections::HashMap;

		self.backend.pairs().iter()
			.map(|&(ref k, ref v)| (k.to_vec(), Some(v.to_vec())))
			.chain(self.overlay.committed.top.clone().into_iter().map(|(k, v)| (k, v.value)))
			.chain(self.overlay.prospective.top.clone().into_iter().map(|(k, v)| (k, v.value)))
			.collect::<HashMap<_, _>>()
			.into_iter()
			.filter_map(|(k, maybe_val)| maybe_val.map(|val| (k, val)))
			.collect()
	}
}

impl<'a, H, B, T, N> Externalities for Ext<'a, H, N, B, T>
where
	H: Hasher<Out=H256>,
	B: 'a + Backend<H>,
	T: 'a + ChangesTrieStorage<H, N>,
	N: crate::changes_trie::BlockNumber,
{
	fn storage(&self, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.overlay.storage(key).map(|x| x.map(|x| x.to_vec())).unwrap_or_else(||
			self.backend.storage(key).expect(EXT_NOT_ALLOWED_TO_FAIL));
		trace!(target: "state-trace", "{:04x}: Get {}={:?}",
			self.id,
			HexDisplay::from(&key),
			result.as_ref().map(HexDisplay::from)
		);
		result
	}

	fn storage_hash(&self, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.overlay
			.storage(key)
			.map(|x| x.map(|x| H::hash(x)))
			.unwrap_or_else(|| self.backend.storage_hash(key).expect(EXT_NOT_ALLOWED_TO_FAIL));

		trace!(target: "state-trace", "{:04x}: Hash {}={:?}",
			self.id,
			HexDisplay::from(&key),
			result,
		);
		result.map(|r| r.encode())
	}

	fn original_storage(&self, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.backend.storage(key).expect(EXT_NOT_ALLOWED_TO_FAIL);

		trace!(target: "state-trace", "{:04x}: GetOriginal {}={:?}",
			self.id,
			HexDisplay::from(&key),
			result.as_ref().map(HexDisplay::from)
		);
		result
	}

	fn original_storage_hash(&self, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.backend.storage_hash(key).expect(EXT_NOT_ALLOWED_TO_FAIL);

		trace!(target: "state-trace", "{:04x}: GetOriginalHash {}={:?}",
			self.id,
			HexDisplay::from(&key),
			result,
		);
		result.map(|r| r.encode())
	}

	fn child_storage(
		&self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.overlay
			.child_storage(storage_key.as_ref(), key)
			.map(|x| x.map(|x| x.to_vec()))
			.unwrap_or_else(||
				self.backend.child_storage(storage_key.as_ref(), child_info, key)
					.expect(EXT_NOT_ALLOWED_TO_FAIL)
			);

		trace!(target: "state-trace", "{:04x}: GetChild({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			result.as_ref().map(HexDisplay::from)
		);

		result
	}

	fn child_storage_hash(
		&self,
		storage_key: ChildStorageKey,
		_child_info: ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.overlay
			.child_storage(storage_key.as_ref(), key)
			.map(|x| x.map(|x| H::hash(x)))
			.unwrap_or_else(||
				self.backend.storage_hash(key).expect(EXT_NOT_ALLOWED_TO_FAIL)
			);

		trace!(target: "state-trace", "{:04x}: ChildHash({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			result,
		);

		result.map(|r| r.encode())
	}

	fn original_child_storage(
		&self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.backend
			.child_storage(storage_key.as_ref(), child_info, key)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);

		trace!(target: "state-trace", "{:04x}: ChildOriginal({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			result.as_ref().map(HexDisplay::from),
		);
		result
	}

	fn original_child_storage_hash(
		&self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = self.backend
			.child_storage_hash(storage_key.as_ref(), child_info, key)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);

		trace!(target: "state-trace", "{}: ChildHashOriginal({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			result,
		);
		result.map(|r| r.encode())
	}

	fn exists_storage(&self, key: &[u8]) -> bool {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		let result = match self.overlay.storage(key) {
			Some(x) => x.is_some(),
			_ => self.backend.exists_storage(key).expect(EXT_NOT_ALLOWED_TO_FAIL),
		};

		trace!(target: "state-trace", "{:04x}: Exists {}={:?}",
			self.id,
			HexDisplay::from(&key),
			result,
		);
		result

	}

	fn exists_child_storage(
		&self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: &[u8],
	) -> bool {
		let _guard = sp_panic_handler::AbortGuard::force_abort();

		let result = match self.overlay.child_storage(storage_key.as_ref(), key) {
			Some(x) => x.is_some(),
			_ => self.backend
				.exists_child_storage(storage_key.as_ref(), child_info, key)
				.expect(EXT_NOT_ALLOWED_TO_FAIL),
		};

		trace!(target: "state-trace", "{:04x}: ChildExists({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			result,
		);
		result
	}

	fn next_storage_key(&self, key: &[u8]) -> Option<Vec<u8>> {
		let next_backend_key = self.backend.next_storage_key(key).expect(EXT_NOT_ALLOWED_TO_FAIL);
		let next_overlay_key_change = self.overlay.next_storage_key_change(key);

		match (next_backend_key, next_overlay_key_change) {
			(Some(backend_key), Some(overlay_key)) if &backend_key[..] < overlay_key.0 => Some(backend_key),
			(backend_key, None) => backend_key,
			(_, Some(overlay_key)) => if overlay_key.1.value.is_some() {
				Some(overlay_key.0.to_vec())
			} else {
				self.next_storage_key(&overlay_key.0[..])
			},
		}
	}

	fn next_child_storage_key(
		&self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: &[u8],
	) -> Option<Vec<u8>> {
		let next_backend_key = self.backend
			.next_child_storage_key(storage_key.as_ref(), child_info, key)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);
		let next_overlay_key_change = self.overlay.next_child_storage_key_change(
			storage_key.as_ref(),
			key
		);

		match (next_backend_key, next_overlay_key_change) {
			(Some(backend_key), Some(overlay_key)) if &backend_key[..] < overlay_key.0 => Some(backend_key),
			(backend_key, None) => backend_key,
			(_, Some(overlay_key)) => if overlay_key.1.value.is_some() {
				Some(overlay_key.0.to_vec())
			} else {
				self.next_child_storage_key(
					storage_key,
					child_info,
					&overlay_key.0[..],
				)
			},
		}
	}

	fn place_storage(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
		trace!(target: "state-trace", "{:04x}: Put {}={:?}",
			self.id,
			HexDisplay::from(&key),
			value.as_ref().map(HexDisplay::from)
		);
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		if is_child_storage_key(&key) {
			warn!(target: "trie", "Refuse to directly set child storage key");
			return;
		}

		self.mark_dirty();
		self.overlay.set_storage(key, value);
	}

	fn place_child_storage(
		&mut self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		key: Vec<u8>,
		value: Option<Vec<u8>>,
	) {
		trace!(target: "state-trace", "{:04x}: PutChild({}) {}={:?}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&key),
			value.as_ref().map(HexDisplay::from)
		);
		let _guard = sp_panic_handler::AbortGuard::force_abort();

		self.mark_dirty();
		self.overlay.set_child_storage(storage_key.into_owned(), child_info, key, value);
	}

	fn kill_child_storage(
		&mut self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
	) {
		trace!(target: "state-trace", "{:04x}: KillChild({})",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
		);
		let _guard = sp_panic_handler::AbortGuard::force_abort();

		self.mark_dirty();
		self.overlay.clear_child_storage(storage_key.as_ref(), child_info);
		self.backend.for_keys_in_child_storage(storage_key.as_ref(), child_info, |key| {
			self.overlay.set_child_storage(storage_key.as_ref().to_vec(), child_info, key.to_vec(), None);
		});
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		trace!(target: "state-trace", "{:04x}: ClearPrefix {}",
			self.id,
			HexDisplay::from(&prefix),
		);
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		if is_child_storage_key(prefix) {
			warn!(target: "trie", "Refuse to directly clear prefix that is part of child storage key");
			return;
		}

		self.mark_dirty();
		self.overlay.clear_prefix(prefix);
		self.backend.for_keys_with_prefix(prefix, |key| {
			self.overlay.set_storage(key.to_vec(), None);
		});
	}

	fn clear_child_prefix(
		&mut self,
		storage_key: ChildStorageKey,
		child_info: ChildInfo,
		prefix: &[u8],
	) {
		trace!(target: "state-trace", "{:04x}: ClearChildPrefix({}) {}",
			self.id,
			HexDisplay::from(&storage_key.as_ref()),
			HexDisplay::from(&prefix),
		);
		let _guard = sp_panic_handler::AbortGuard::force_abort();

		self.mark_dirty();
		self.overlay.clear_child_prefix(storage_key.as_ref(), child_info, prefix);
		self.backend.for_child_keys_with_prefix(storage_key.as_ref(), child_info, prefix, |key| {
			self.overlay.set_child_storage(storage_key.as_ref().to_vec(), child_info, key.to_vec(), None);
		});
	}

	fn chain_id(&self) -> u64 {
		42
	}

	fn storage_root(&mut self) -> Vec<u8> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		if let Some((_, ref root)) = self.storage_transaction {
			trace!(target: "state-trace", "{:04x}: Root (cached) {}",
				self.id,
				HexDisplay::from(&root.as_ref()),
			);
			return root.encode();
		}

		let child_storage_keys = self.overlay.prospective.children.keys()
				.chain(self.overlay.committed.children.keys());
		let child_delta_iter = child_storage_keys.map(|storage_key|
			(
				storage_key.clone(),
				self.overlay.committed.children.get(storage_key)
					.into_iter()
					.flat_map(|(map, _)| map.iter().map(|(k, v)| (k.clone(), v.value.clone())))
					.chain(
						self.overlay.prospective.children.get(storage_key)
							.into_iter()
							.flat_map(|(map, _)| map.iter().map(|(k, v)| (k.clone(), v.value.clone())))
					),
				self.overlay.child_info(storage_key).cloned()
					.expect("child info initialized in either committed or prospective"),
			)
		);


		// compute and memoize
		let delta = self.overlay.committed.top.iter().map(|(k, v)| (k.clone(), v.value.clone()))
			.chain(self.overlay.prospective.top.iter().map(|(k, v)| (k.clone(), v.value.clone())));

		let (root, transaction) = self.backend.full_storage_root(delta, child_delta_iter);
		self.storage_transaction = Some((transaction, root));
		trace!(target: "state-trace", "{:04x}: Root {}",
			self.id,
			HexDisplay::from(&root.as_ref()),
		);
		root.encode()
	}

	fn child_storage_root(
		&mut self,
		storage_key: ChildStorageKey,
	) -> Vec<u8> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();
		if self.storage_transaction.is_some() {
			let root = self
				.storage(storage_key.as_ref())
				.and_then(|k| Decode::decode(&mut &k[..]).ok())
				.unwrap_or(
					default_child_trie_root::<Layout<H>>(storage_key.as_ref())
				);
			trace!(target: "state-trace", "{:04x}: ChildRoot({}) (cached) {}",
				self.id,
				HexDisplay::from(&storage_key.as_ref()),
				HexDisplay::from(&root.as_ref()),
			);
			root.encode()
		} else {
			let storage_key = storage_key.as_ref();

			if let Some(child_info) = self.overlay.child_info(storage_key).cloned() {
				let (root, is_empty, _) = {
					let delta = self.overlay.committed.children.get(storage_key)
						.into_iter()
						.flat_map(|(map, _)| map.clone().into_iter().map(|(k, v)| (k, v.value)))
						.chain(
							self.overlay.prospective.children.get(storage_key)
								.into_iter()
								.flat_map(|(map, _)| map.clone().into_iter().map(|(k, v)| (k, v.value)))
						);

					self.backend.child_storage_root(storage_key, child_info.as_ref(), delta)
				};

				let root = root.encode();
				// We store update in the overlay in order to be able to use 'self.storage_transaction'
				// cache. This is brittle as it rely on Ext only querying the trie backend for
				// storage root.
				// A better design would be to manage 'child_storage_transaction' in a
				// similar way as 'storage_transaction' but for each child trie.
				if is_empty {
					self.overlay.set_storage(storage_key.into(), None);
				} else {
					self.overlay.set_storage(storage_key.into(), Some(root.clone()));
				}

				trace!(target: "state-trace", "{:04x}: ChildRoot({}) {}",
					self.id,
					HexDisplay::from(&storage_key.as_ref()),
					HexDisplay::from(&root.as_ref()),
				);
				root
			} else {
				// empty overlay
				let root = self
					.storage(storage_key.as_ref())
					.and_then(|k| Decode::decode(&mut &k[..]).ok())
					.unwrap_or(
						default_child_trie_root::<Layout<H>>(storage_key.as_ref())
					);
				trace!(target: "state-trace", "{:04x}: ChildRoot({}) (no change) {}",
					self.id,
					HexDisplay::from(&storage_key.as_ref()),
					HexDisplay::from(&root.as_ref()),
				);
				root.encode()
			}
		}
	}

	fn storage_changes_root(&mut self, parent_hash: &[u8]) -> Result<Option<Vec<u8>>, ()> {
		let _guard = sp_panic_handler::AbortGuard::force_abort();

		self.changes_trie_transaction = build_changes_trie::<_, T, H, N>(
			self.backend,
			self.changes_trie_storage.clone(),
			self.overlay,
			H256::decode(&mut &parent_hash[..]).map_err(|e|
				trace!(
					target: "state-trace",
					"Failed to decode changes root parent hash: {}",
					e,
				)
			)?,
		)?;
		let result = Ok(
			self.changes_trie_transaction.as_ref().map(|(_, root, _)| root.encode())
		);

		trace!(target: "state-trace", "{:04x}: ChangesRoot({}) {:?}",
			self.id,
			HexDisplay::from(&parent_hash.as_ref()),
			result,
		);
		result
	}
}

impl<'a, H, B, T, N> sp_externalities::ExtensionStore for Ext<'a, H, N, B, T>
where
	H: Hasher<Out=H256>,
	B: 'a + Backend<H>,
	T: 'a + ChangesTrieStorage<H, N>,
	N: crate::changes_trie::BlockNumber,
{
	fn extension_by_type_id(&mut self, type_id: TypeId) -> Option<&mut dyn Any> {
		self.extensions.as_mut().and_then(|exts| exts.get_mut(type_id))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;
	use codec::Encode;
	use sp_core::{Blake2Hasher, storage::well_known_keys::EXTRINSIC_INDEX, map};
	use crate::{
		changes_trie::{
			Configuration as ChangesTrieConfiguration,
			InMemoryStorage as InMemoryChangesTrieStorage,
		}, backend::InMemory, overlayed_changes::OverlayedValue,
	};
	use sp_core::storage::{Storage, StorageChild};

	type TestBackend = InMemory<Blake2Hasher>;
	type TestChangesTrieStorage = InMemoryChangesTrieStorage<Blake2Hasher, u64>;
	type TestExt<'a> = Ext<'a, Blake2Hasher, u64, TestBackend, TestChangesTrieStorage>;

	fn prepare_overlay_with_changes() -> OverlayedChanges {
		OverlayedChanges {
			prospective: vec![
				(EXTRINSIC_INDEX.to_vec(), OverlayedValue {
					value: Some(3u32.encode()),
					extrinsics: Some(vec![1].into_iter().collect())
				}),
				(vec![1], OverlayedValue {
					value: Some(vec![100].into_iter().collect()),
					extrinsics: Some(vec![1].into_iter().collect())
				}),
			].into_iter().collect(),
			committed: Default::default(),
			changes_trie_config: Some(ChangesTrieConfiguration {
				digest_interval: 0,
				digest_levels: 0,
			}),
		}
	}

	#[test]
	fn storage_changes_root_is_none_when_storage_is_not_provided() {
		let mut overlay = prepare_overlay_with_changes();
		let backend = TestBackend::default();
		let mut ext = TestExt::new(&mut overlay, &backend, None, None);
		assert_eq!(ext.storage_changes_root(&H256::default().encode()).unwrap(), None);
	}

	#[test]
	fn storage_changes_root_is_none_when_extrinsic_changes_are_none() {
		let mut overlay = prepare_overlay_with_changes();
		overlay.changes_trie_config = None;
		let storage = TestChangesTrieStorage::with_blocks(vec![(100, Default::default())]);
		let backend = TestBackend::default();
		let mut ext = TestExt::new(&mut overlay, &backend, Some(&storage), None);
		assert_eq!(ext.storage_changes_root(&H256::default().encode()).unwrap(), None);
	}

	#[test]
	fn storage_changes_root_is_some_when_extrinsic_changes_are_non_empty() {
		let mut overlay = prepare_overlay_with_changes();
		let storage = TestChangesTrieStorage::with_blocks(vec![(99, Default::default())]);
		let backend = TestBackend::default();
		let mut ext = TestExt::new(&mut overlay, &backend, Some(&storage), None);
		assert_eq!(
			ext.storage_changes_root(&H256::default().encode()).unwrap(),
			Some(hex!("bb0c2ef6e1d36d5490f9766cfcc7dfe2a6ca804504c3bb206053890d6dd02376").to_vec()),
		);
	}

	#[test]
	fn storage_changes_root_is_some_when_extrinsic_changes_are_empty() {
		let mut overlay = prepare_overlay_with_changes();
		overlay.prospective.top.get_mut(&vec![1]).unwrap().value = None;
		let storage = TestChangesTrieStorage::with_blocks(vec![(99, Default::default())]);
		let backend = TestBackend::default();
		let mut ext = TestExt::new(&mut overlay, &backend, Some(&storage), None);
		assert_eq!(
			ext.storage_changes_root(&H256::default().encode()).unwrap(),
			Some(hex!("96f5aae4690e7302737b6f9b7f8567d5bbb9eac1c315f80101235a92d9ec27f4").to_vec()),
		);
	}

	#[test]
	fn next_storage_key_works() {
		let mut overlay = OverlayedChanges::default();
		overlay.set_storage(vec![20], None);
		overlay.set_storage(vec![30], Some(vec![31]));
		let backend = Storage {
			top: map![
				vec![10] => vec![10],
				vec![20] => vec![20],
				vec![40] => vec![40]
			],
			children: map![]
		}.into();

		let ext = TestExt::new(&mut overlay, &backend, None, None);

		// next_backend < next_overlay
		assert_eq!(ext.next_storage_key(&[5]), Some(vec![10]));

		// next_backend == next_overlay but next_overlay is a delete
		assert_eq!(ext.next_storage_key(&[10]), Some(vec![30]));

		// next_overlay < next_backend
		assert_eq!(ext.next_storage_key(&[20]), Some(vec![30]));

		// next_backend exist but next_overlay doesn't exist
		assert_eq!(ext.next_storage_key(&[30]), Some(vec![40]));

		drop(ext);
		overlay.set_storage(vec![50], Some(vec![50]));
		let ext = TestExt::new(&mut overlay, &backend, None, None);

		// next_overlay exist but next_backend doesn't exist
		assert_eq!(ext.next_storage_key(&[40]), Some(vec![50]));
	}

	#[test]
	fn next_child_storage_key_works() {
		const CHILD_KEY_1: &[u8] = b":child_storage:default:Child1";

		const CHILD_UUID_1: &[u8] = b"unique_id_1";
		const CHILD_INFO_1: ChildInfo<'static> = ChildInfo::new_default(CHILD_UUID_1);


		let child = || ChildStorageKey::from_slice(CHILD_KEY_1).unwrap();
		let mut overlay = OverlayedChanges::default();
		overlay.set_child_storage(child().as_ref().to_vec(), CHILD_INFO_1, vec![20], None);
		overlay.set_child_storage(child().as_ref().to_vec(), CHILD_INFO_1, vec![30], Some(vec![31]));
		let backend = Storage {
			top: map![],
			children: map![
				child().as_ref().to_vec() => StorageChild {
					data: map![
						vec![10] => vec![10],
						vec![20] => vec![20],
						vec![40] => vec![40]
					],
					child_info: CHILD_INFO_1.to_owned(),
				}
			],
		}.into();


		let ext = TestExt::new(&mut overlay, &backend, None, None);

		// next_backend < next_overlay
		assert_eq!(ext.next_child_storage_key(child(), CHILD_INFO_1, &[5]), Some(vec![10]));

		// next_backend == next_overlay but next_overlay is a delete
		assert_eq!(ext.next_child_storage_key(child(), CHILD_INFO_1, &[10]), Some(vec![30]));

		// next_overlay < next_backend
		assert_eq!(ext.next_child_storage_key(child(), CHILD_INFO_1, &[20]), Some(vec![30]));

		// next_backend exist but next_overlay doesn't exist
		assert_eq!(ext.next_child_storage_key(child(), CHILD_INFO_1, &[30]), Some(vec![40]));

		drop(ext);
		overlay.set_child_storage(child().as_ref().to_vec(), CHILD_INFO_1, vec![50], Some(vec![50]));
		let ext = TestExt::new(&mut overlay, &backend, None, None);

		// next_overlay exist but next_backend doesn't exist
		assert_eq!(ext.next_child_storage_key(child(), CHILD_INFO_1, &[40]), Some(vec![50]));
	}
}

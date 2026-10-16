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

//! Concrete externalities implementation.

#[cfg(feature = "std")]
use crate::overlayed_changes::OverlayedExtensions;
use crate::{
	backend::Backend, IndexOperation, IterArgs, OverlayedChanges, StorageKey, StorageValue,
};
use codec::{Compact, CompactLen, Decode, Encode};
use hash_db::Hasher;
#[cfg(feature = "std")]
use sp_core::hexdisplay::HexDisplay;
use sp_core::storage::{
	well_known_keys::is_child_storage_key, ChildInfo, StateVersion, TrackedStorageKey,
};
#[cfg(feature = "std")]
use sp_externalities::TransactionType;
use sp_externalities::{Extension, ExtensionStore, Externalities, MultiRemovalResults};

use crate::{trace, warn};
use alloc::{boxed::Box, vec::Vec};
use core::{
	any::{Any, TypeId},
	cmp::Ordering,
};

const EXT_NOT_ALLOWED_TO_FAIL: &str = "Externalities not allowed to fail within runtime";
const BENCHMARKING_FN: &str = "\
	This is a special fn only for benchmarking where a database commit happens from the runtime.
	For that reason client started transactions before calling into runtime are not allowed.
	Without client transactions the loop condition guarantees the success of the tx close.";

#[cfg(feature = "std")]
fn guard() -> sp_panic_handler::AbortGuard {
	sp_panic_handler::AbortGuard::force_abort()
}

#[cfg(not(feature = "std"))]
fn guard() -> () {
	()
}

/// Wraps a read-only backend, call executor, and current overlayed changes.
pub struct Ext<'a, H, B>
where
	H: Hasher,
	B: 'a + Backend<H>,
{
	/// The overlayed changes to write to.
	overlay: &'a mut OverlayedChanges<H>,
	/// The storage backend to read from.
	backend: &'a B,
	/// Pseudo-unique id used for tracing.
	pub id: u16,
	/// Extensions registered with this instance.
	#[cfg(feature = "std")]
	extensions: Option<OverlayedExtensions<'a>>,
}

impl<'a, H, B> Ext<'a, H, B>
where
	H: Hasher,
	B: Backend<H>,
{
	/// Create a new `Ext`.
	#[cfg(not(feature = "std"))]
	pub fn new(overlay: &'a mut OverlayedChanges<H>, backend: &'a B) -> Self {
		Ext { overlay, backend, id: 0 }
	}

	/// Create a new `Ext` from overlayed changes and read-only backend
	#[cfg(feature = "std")]
	pub fn new(
		overlay: &'a mut OverlayedChanges<H>,
		backend: &'a B,
		extensions: Option<&'a mut sp_externalities::Extensions>,
	) -> Self {
		Self {
			overlay,
			backend,
			id: rand::random(),
			extensions: extensions.map(OverlayedExtensions::new),
		}
	}
}

#[cfg(test)]
impl<'a, H, B> Ext<'a, H, B>
where
	H: Hasher,
	H::Out: Ord + 'static,
	B: 'a + Backend<H>,
{
	/// Return all storage pairs from the backend and overlay combined.
	pub fn storage_pairs(&mut self) -> Vec<(StorageKey, StorageValue)> {
		use std::collections::HashMap;

		self.backend
			.pairs(Default::default())
			.expect("never fails in tests; qed.")
			.map(|key_value| key_value.expect("never fails in tests; qed."))
			.map(|(k, v)| (k, Some(v)))
			.chain(self.overlay.changes_mut().map(|(k, v)| (k.clone(), v.value().cloned())))
			.collect::<HashMap<_, _>>()
			.into_iter()
			.filter_map(|(k, maybe_val)| maybe_val.map(|val| (k, val)))
			.collect()
	}
}

impl<'a, H, B> Externalities for Ext<'a, H, B>
where
	H: Hasher,
	H::Out: Ord + 'static + codec::Codec,
	B: Backend<H>,
{
	fn set_offchain_storage(&mut self, key: &[u8], value: Option<&[u8]>) {
		self.overlay.set_offchain_storage(key, value)
	}

	fn storage(&mut self, key: &[u8]) -> Option<StorageValue> {
		let _guard = guard();
		let result = self
			.overlay
			.storage(key)
			.map(|x| x.map(|x| x.to_vec()))
			.unwrap_or_else(|| self.backend.storage(key).expect(EXT_NOT_ALLOWED_TO_FAIL));

		// NOTE: be careful about touching the key names – used outside substrate!
		trace!(
			target: "state",
			method = "Get",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			key = %HexDisplay::from(&key),
			result = ?result.as_ref().map(HexDisplay::from),
			result_encoded = %HexDisplay::from(
				&result
					.as_ref()
					.map(|v| EncodeOpaqueValue(v.clone()))
					.encode()
			),
		);

		result
	}

	fn storage_hash(&mut self, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = guard();
		let result = self
			.overlay
			.storage(key)
			.map(|x| x.map(|x| H::hash(x)))
			.unwrap_or_else(|| self.backend.storage_hash(key).expect(EXT_NOT_ALLOWED_TO_FAIL));

		trace!(
			target: "state",
			method = "Hash",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			key = %HexDisplay::from(&key),
			?result,
		);
		result.map(|r| r.encode())
	}

	fn child_storage(&mut self, child_info: &ChildInfo, key: &[u8]) -> Option<StorageValue> {
		let _guard = guard();
		let result = self
			.overlay
			.child_storage(child_info, key)
			.map(|x| x.map(|x| x.to_vec()))
			.unwrap_or_else(|| {
				self.backend.child_storage(child_info, key).expect(EXT_NOT_ALLOWED_TO_FAIL)
			});

		trace!(
			target: "state",
			method = "ChildGet",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			key = %HexDisplay::from(&key),
			result = ?result.as_ref().map(HexDisplay::from)
		);

		result
	}

	fn child_storage_hash(&mut self, child_info: &ChildInfo, key: &[u8]) -> Option<Vec<u8>> {
		let _guard = guard();
		let result = self
			.overlay
			.child_storage(child_info, key)
			.map(|x| x.map(|x| H::hash(x)))
			.unwrap_or_else(|| {
				self.backend.child_storage_hash(child_info, key).expect(EXT_NOT_ALLOWED_TO_FAIL)
			});

		trace!(
			target: "state",
			method = "ChildHash",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			key = %HexDisplay::from(&key),
			?result,
		);

		result.map(|r| r.encode())
	}

	fn exists_storage(&mut self, key: &[u8]) -> bool {
		let _guard = guard();
		let result = match self.overlay.storage(key) {
			Some(x) => x.is_some(),
			_ => self.backend.exists_storage(key).expect(EXT_NOT_ALLOWED_TO_FAIL),
		};

		trace!(
			target: "state",
			method = "Exists",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			key = %HexDisplay::from(&key),
			%result,
		);

		result
	}

	fn exists_child_storage(&mut self, child_info: &ChildInfo, key: &[u8]) -> bool {
		let _guard = guard();

		let result = match self.overlay.child_storage(child_info, key) {
			Some(x) => x.is_some(),
			_ => self
				.backend
				.exists_child_storage(child_info, key)
				.expect(EXT_NOT_ALLOWED_TO_FAIL),
		};

		trace!(
			target: "state",
			method = "ChildExists",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			key = %HexDisplay::from(&key),
			%result,
		);
		result
	}

	fn next_storage_key(&mut self, key: &[u8]) -> Option<StorageKey> {
		let mut next_backend_key =
			self.backend.next_storage_key(key).expect(EXT_NOT_ALLOWED_TO_FAIL);
		let mut overlay_changes = self.overlay.iter_after(key).peekable();

		match (&next_backend_key, overlay_changes.peek()) {
			(_, None) => next_backend_key,
			(Some(_), Some(_)) => {
				for overlay_key in overlay_changes {
					let cmp = next_backend_key.as_deref().map(|v| v.cmp(overlay_key.0));

					// If `backend_key` is less than the `overlay_key`, we found out next key.
					if cmp == Some(Ordering::Less) {
						return next_backend_key;
					} else if overlay_key.1.value().is_some() {
						// If there exists a value for the `overlay_key` in the overlay
						// (aka the key is still valid), it means we have found our next key.
						return Some(overlay_key.0.to_vec());
					} else if cmp == Some(Ordering::Equal) {
						// If the `backend_key` and `overlay_key` are equal, it means that we need
						// to search for the next backend key, because the overlay has overwritten
						// this key.
						next_backend_key = self
							.backend
							.next_storage_key(overlay_key.0)
							.expect(EXT_NOT_ALLOWED_TO_FAIL);
					}
				}

				next_backend_key
			},
			(None, Some(_)) => {
				// Find the next overlay key that has a value attached.
				overlay_changes.find_map(|k| k.1.value().as_ref().map(|_| k.0.to_vec()))
			},
		}
	}

	fn next_child_storage_key(&mut self, child_info: &ChildInfo, key: &[u8]) -> Option<StorageKey> {
		let mut next_backend_key = self
			.backend
			.next_child_storage_key(child_info, key)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);
		let mut overlay_changes =
			self.overlay.child_iter_after(child_info.storage_key(), key).peekable();

		match (&next_backend_key, overlay_changes.peek()) {
			(_, None) => next_backend_key,
			(Some(_), Some(_)) => {
				for overlay_key in overlay_changes {
					let cmp = next_backend_key.as_deref().map(|v| v.cmp(overlay_key.0));

					// If `backend_key` is less than the `overlay_key`, we found out next key.
					if cmp == Some(Ordering::Less) {
						return next_backend_key;
					} else if overlay_key.1.value().is_some() {
						// If there exists a value for the `overlay_key` in the overlay
						// (aka the key is still valid), it means we have found our next key.
						return Some(overlay_key.0.to_vec());
					} else if cmp == Some(Ordering::Equal) {
						// If the `backend_key` and `overlay_key` are equal, it means that we need
						// to search for the next backend key, because the overlay has overwritten
						// this key.
						next_backend_key = self
							.backend
							.next_child_storage_key(child_info, overlay_key.0)
							.expect(EXT_NOT_ALLOWED_TO_FAIL);
					}
				}

				next_backend_key
			},
			(None, Some(_)) => {
				// Find the next overlay key that has a value attached.
				overlay_changes.find_map(|k| k.1.value().as_ref().map(|_| k.0.to_vec()))
			},
		}
	}

	fn place_storage(&mut self, key: StorageKey, value: Option<StorageValue>) {
		let _guard = guard();
		if is_child_storage_key(&key) {
			warn!(target: "trie", "Refuse to directly set child storage key");
			return;
		}

		// NOTE: be careful about touching the key names – used outside substrate!
		trace!(
			target: "state",
			method = "Put",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			key = %HexDisplay::from(&key),
			value = ?value.as_ref().map(HexDisplay::from),
			value_encoded = %HexDisplay::from(
				&value
					.as_ref()
					.map(|v| EncodeOpaqueValue(v.clone()))
					.encode()
			),
		);

		self.overlay.set_storage(key, value);
	}

	fn place_child_storage(
		&mut self,
		child_info: &ChildInfo,
		key: StorageKey,
		value: Option<StorageValue>,
	) {
		trace!(
			target: "state",
			method = "ChildPut",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			key = %HexDisplay::from(&key),
			value = ?value.as_ref().map(HexDisplay::from),
		);
		let _guard = guard();

		self.overlay.set_child_storage(child_info, key, value);
	}

	fn kill_child_storage(
		&mut self,
		child_info: &ChildInfo,
		maybe_limit: Option<u32>,
		maybe_cursor: Option<&[u8]>,
	) -> MultiRemovalResults {
		trace!(
			target: "state",
			method = "ChildKill",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
		);
		let _guard = guard();
		let overlay = self.overlay.clear_child_storage(child_info);
		let (maybe_cursor, backend, loops) =
			self.limit_remove_from_backend(Some(child_info), None, maybe_limit, maybe_cursor);
		MultiRemovalResults { maybe_cursor, backend, unique: overlay + backend, loops }
	}

	fn clear_prefix(
		&mut self,
		prefix: &[u8],
		maybe_limit: Option<u32>,
		maybe_cursor: Option<&[u8]>,
	) -> MultiRemovalResults {
		trace!(
			target: "state",
			method = "ClearPrefix",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			prefix = %HexDisplay::from(&prefix),
		);
		let _guard = guard();

		if sp_core::storage::well_known_keys::starts_with_child_storage_key(prefix) {
			warn!(
				target: "trie",
				"Refuse to directly clear prefix that is part or contains of child storage key",
			);
			return MultiRemovalResults { maybe_cursor: None, backend: 0, unique: 0, loops: 0 };
		}

		let overlay = self.overlay.clear_prefix(prefix);
		let (maybe_cursor, backend, loops) =
			self.limit_remove_from_backend(None, Some(prefix), maybe_limit, maybe_cursor);
		MultiRemovalResults { maybe_cursor, backend, unique: overlay + backend, loops }
	}

	fn clear_child_prefix(
		&mut self,
		child_info: &ChildInfo,
		prefix: &[u8],
		maybe_limit: Option<u32>,
		maybe_cursor: Option<&[u8]>,
	) -> MultiRemovalResults {
		trace!(
			target: "state",
			method = "ChildClearPrefix",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			prefix = %HexDisplay::from(&prefix),
		);
		let _guard = guard();

		let overlay = self.overlay.clear_child_prefix(child_info, prefix);
		let (maybe_cursor, backend, loops) = self.limit_remove_from_backend(
			Some(child_info),
			Some(prefix),
			maybe_limit,
			maybe_cursor,
		);
		MultiRemovalResults { maybe_cursor, backend, unique: overlay + backend, loops }
	}

	fn storage_append(&mut self, key: Vec<u8>, value: Vec<u8>) {
		trace!(
			target: "state",
			method = "Append",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			key = %HexDisplay::from(&key),
			value = %HexDisplay::from(&value),
		);

		let _guard = guard();

		let backend = &mut self.backend;
		self.overlay.append_storage(key.clone(), value, || {
			backend.storage(&key).expect(EXT_NOT_ALLOWED_TO_FAIL).unwrap_or_default()
		});
	}

	fn storage_root(&mut self, state_version: StateVersion) -> Vec<u8> {
		let _guard = guard();

		let (root, _cached) = self.overlay.storage_root(self.backend, state_version);

		trace!(
			target: "state",
			method = "StorageRoot",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			storage_root = %HexDisplay::from(&root.as_ref()),
			cached = %_cached,
		);

		root.encode()
	}

	fn record_proof_for_dirty_keys(&mut self) {
		let _guard = guard();

		self.overlay.record_proof_for_dirty_keys(self.backend);

		trace!(
			target: "state",
			method = "RecordProofForDirtyKeys",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
		);
	}

	fn child_storage_root(
		&mut self,
		child_info: &ChildInfo,
		state_version: StateVersion,
	) -> Vec<u8> {
		let _guard = guard();

		let (root, _cached) = self
			.overlay
			.child_storage_root(child_info, self.backend, state_version)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);

		trace!(
			target: "state",
			method = "ChildStorageRoot",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			child_info = %HexDisplay::from(&child_info.storage_key()),
			storage_root = %HexDisplay::from(&root.as_ref()),
			cached = %_cached,
		);

		root.encode()
	}

	fn storage_index_transaction(&mut self, index: u32, hash: &[u8], size: u32) {
		trace!(
			target: "state",
			method = "IndexTransaction",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			%index,
			tx_hash = %HexDisplay::from(&hash),
			%size,
		);

		self.overlay.add_transaction_index(IndexOperation::Insert {
			extrinsic: index,
			hash: hash.to_vec(),
			size,
		});
	}

	/// Renew existing piece of data storage.
	fn storage_renew_transaction_index(&mut self, index: u32, hash: &[u8]) {
		trace!(
			target: "state",
			method = "RenewTransactionIndex",
			ext_id = %HexDisplay::from(&self.id.to_le_bytes()),
			%index,
			tx_hash = %HexDisplay::from(&hash),
		);

		self.overlay
			.add_transaction_index(IndexOperation::Renew { extrinsic: index, hash: hash.to_vec() });
	}

	fn storage_start_transaction(&mut self) {
		self.overlay.start_transaction();

		#[cfg(feature = "std")]
		if let Some(exts) = self.extensions.as_mut() {
			exts.start_transaction(TransactionType::Runtime);
		}
	}

	fn storage_rollback_transaction(&mut self) -> Result<(), ()> {
		self.overlay.rollback_transaction().map_err(|_| ())?;

		#[cfg(feature = "std")]
		if let Some(exts) = self.extensions.as_mut() {
			exts.rollback_transaction(TransactionType::Runtime);
		}

		Ok(())
	}

	fn storage_commit_transaction(&mut self) -> Result<(), ()> {
		self.overlay.commit_transaction().map_err(|_| ())?;

		#[cfg(feature = "std")]
		if let Some(exts) = self.extensions.as_mut() {
			exts.commit_transaction(TransactionType::Runtime);
		}

		Ok(())
	}

	fn wipe(&mut self) {
		for _ in 0..self.overlay.transaction_depth() {
			self.overlay.rollback_transaction().expect(BENCHMARKING_FN);
		}
		self.overlay
			.drain_storage_changes(self.backend, Default::default())
			.expect(EXT_NOT_ALLOWED_TO_FAIL);
		self.backend.wipe().expect(EXT_NOT_ALLOWED_TO_FAIL);
		self.overlay
			.enter_runtime()
			.expect("We have reset the overlay above, so we can not be in the runtime; qed");
	}

	fn commit(&mut self) {
		// Bench always use latest state.
		let state_version = StateVersion::default();
		for _ in 0..self.overlay.transaction_depth() {
			self.overlay.commit_transaction().expect(BENCHMARKING_FN);
		}
		let changes = self
			.overlay
			.drain_storage_changes(self.backend, state_version)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);
		self.backend
			.commit(
				changes.transaction_storage_root,
				changes.transaction,
				changes.main_storage_changes,
				changes.child_storage_changes,
			)
			.expect(EXT_NOT_ALLOWED_TO_FAIL);
		self.overlay
			.enter_runtime()
			.expect("We have reset the overlay above, so we can not be in the runtime; qed");
	}

	fn read_write_count(&self) -> (u32, u32, u32, u32) {
		self.backend.read_write_count()
	}

	fn reset_read_write_count(&mut self) {
		self.backend.reset_read_write_count()
	}

	fn get_whitelist(&self) -> Vec<TrackedStorageKey> {
		self.backend.get_whitelist()
	}

	fn set_whitelist(&mut self, new: Vec<TrackedStorageKey>) {
		self.backend.set_whitelist(new)
	}

	fn proof_size(&self) -> Option<u32> {
		self.backend.proof_size()
	}

	fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
		self.backend.get_read_and_written_keys()
	}
}

impl<'a, H, B> Ext<'a, H, B>
where
	H: Hasher,
	H::Out: Ord + 'static + codec::Codec,
	B: Backend<H>,
{
	fn limit_remove_from_backend(
		&mut self,
		child_info: Option<&ChildInfo>,
		prefix: Option<&[u8]>,
		maybe_limit: Option<u32>,
		start_at: Option<&[u8]>,
	) -> (Option<Vec<u8>>, u32, u32) {
		let iter = match self.backend.keys(IterArgs {
			child_info: child_info.cloned(),
			prefix,
			start_at,
			..IterArgs::default()
		}) {
			Ok(iter) => iter,
			Err(error) => {
				log::debug!(target: "trie", "Error while iterating the storage: {}", error);
				return (None, 0, 0);
			},
		};

		let mut delete_count: u32 = 0;
		let mut loop_count: u32 = 0;
		let mut maybe_next_key = None;
		for key in iter {
			let key = match key {
				Ok(key) => key,
				Err(error) => {
					log::debug!(target: "trie", "Error while iterating the storage: {}", error);
					break;
				},
			};

			if maybe_limit.map_or(false, |limit| loop_count == limit) {
				maybe_next_key = Some(key);
				break;
			}
			let overlay = match child_info {
				Some(child_info) => self.overlay.child_storage(child_info, &key),
				None => self.overlay.storage(&key),
			};
			if !matches!(overlay, Some(None)) {
				// not pending deletion from the backend - delete it.
				if let Some(child_info) = child_info {
					self.overlay.set_child_storage(child_info, key, None);
				} else {
					self.overlay.set_storage(key, None);
				}
				delete_count = delete_count.saturating_add(1);
			}
			loop_count = loop_count.saturating_add(1);
		}

		(maybe_next_key, delete_count, loop_count)
	}
}

/// Implement `Encode` by forwarding the stored raw vec.
#[allow(dead_code)]
struct EncodeOpaqueValue(Vec<u8>);

impl Encode for EncodeOpaqueValue {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		f(&self.0)
	}
}

/// Auxiliary structure for appending a value to a storage item.
pub(crate) struct StorageAppend<'a>(&'a mut Vec<u8>);

impl<'a> StorageAppend<'a> {
	/// Create a new instance using the given `storage` reference.
	pub fn new(storage: &'a mut Vec<u8>) -> Self {
		Self(storage)
	}

	/// Extract the length of the list like data structure.
	pub fn extract_length(&self) -> Option<u32> {
		Compact::<u32>::decode(&mut &self.0[..]).map(|c| c.0).ok()
	}

	/// Replace the length in the encoded data.
	///
	/// If `old_length` is `None`, the previous length will be assumed to be `0`.
	pub fn replace_length(&mut self, old_length: Option<u32>, new_length: u32) {
		let old_len_encoded_len = old_length.map(|l| Compact::<u32>::compact_len(&l)).unwrap_or(0);
		let new_len_encoded = Compact::<u32>(new_length).encode();
		self.0.splice(0..old_len_encoded_len, new_len_encoded);
	}

	/// Append the given `value` to the storage item.
	///
	/// If appending fails, `[value]` is stored in the storage item and we return false.
	#[cfg(any(test, feature = "fuzzing"))]
	pub fn append(&mut self, value: Vec<u8>) -> bool {
		use codec::EncodeAppend;
		let mut result = true;
		let value = vec![EncodeOpaqueValue(value)];

		let item = core::mem::take(self.0);

		*self.0 = match Vec::<EncodeOpaqueValue>::append_or_new(item, &value) {
			Ok(item) => item,
			Err(_) => {
				result = false;
				crate::log_error!(
					target: "runtime",
					"Failed to append value, resetting storage item to `[value]`.",
				);
				value.encode()
			},
		};
		result
	}

	/// Append to current buffer, do not touch the prefixed length.
	pub fn append_raw(&mut self, mut value: Vec<u8>) {
		self.0.append(&mut value)
	}
}

#[cfg(not(feature = "std"))]
impl<'a, H, B> ExtensionStore for Ext<'a, H, B>
where
	H: Hasher,
	H::Out: Ord + 'static + codec::Codec,
	B: Backend<H>,
{
	fn extension_by_type_id(&mut self, _type_id: TypeId) -> Option<&mut dyn Any> {
		None
	}

	fn register_extension_with_type_id(
		&mut self,
		_type_id: TypeId,
		_extension: Box<dyn Extension>,
	) -> Result<(), sp_externalities::Error> {
		Err(sp_externalities::Error::ExtensionsAreNotSupported)
	}

	fn deregister_extension_by_type_id(
		&mut self,
		_type_id: TypeId,
	) -> Result<(), sp_externalities::Error> {
		Err(sp_externalities::Error::ExtensionsAreNotSupported)
	}
}

#[cfg(feature = "std")]
impl<'a, H, B> ExtensionStore for Ext<'a, H, B>
where
	H: Hasher,
	B: 'a + Backend<H>,
{
	fn extension_by_type_id(&mut self, type_id: TypeId) -> Option<&mut dyn Any> {
		self.extensions.as_mut().and_then(|exts| exts.get_mut(type_id))
	}

	fn register_extension_with_type_id(
		&mut self,
		type_id: TypeId,
		extension: Box<dyn Extension>,
	) -> Result<(), sp_externalities::Error> {
		if let Some(ref mut extensions) = self.extensions {
			extensions.register(type_id, extension)
		} else {
			Err(sp_externalities::Error::ExtensionsAreNotSupported)
		}
	}

	fn deregister_extension_by_type_id(
		&mut self,
		type_id: TypeId,
	) -> Result<(), sp_externalities::Error> {
		if let Some(ref mut extensions) = self.extensions {
			if extensions.deregister(type_id) {
				Ok(())
			} else {
				Err(sp_externalities::Error::ExtensionIsNotRegistered(type_id))
			}
		} else {
			Err(sp_externalities::Error::ExtensionsAreNotSupported)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{InMemoryBackend, TrieBackendBuilder};
	use codec::{Decode, Encode};
	use sp_core::{
		map,
		storage::{Storage, StorageChild},
		Blake2Hasher,
	};
	use sp_trie::recorder::Recorder;

	type TestBackend = InMemoryBackend<Blake2Hasher>;
	type TestExt<'a> = Ext<'a, Blake2Hasher, TestBackend>;

	#[test]
	fn next_storage_key_works() {
		let mut overlay = OverlayedChanges::default();
		overlay.set_storage(vec![20], None);
		overlay.set_storage(vec![30], Some(vec![31]));
		let backend = (
			Storage {
				top: map![
					vec![10] => vec![10],
					vec![20] => vec![20],
					vec![40] => vec![40]
				],
				children_default: map![],
			},
			StateVersion::default(),
		)
			.into();

		let mut ext = TestExt::new(&mut overlay, &backend, None);

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
		let mut ext = TestExt::new(&mut overlay, &backend, None);

		// next_overlay exist but next_backend doesn't exist
		assert_eq!(ext.next_storage_key(&[40]), Some(vec![50]));
	}

	#[test]
	fn next_storage_key_works_with_a_lot_empty_values_in_overlay() {
		let mut overlay = OverlayedChanges::default();
		overlay.set_storage(vec![20], None);
		overlay.set_storage(vec![21], None);
		overlay.set_storage(vec![22], None);
		overlay.set_storage(vec![23], None);
		overlay.set_storage(vec![24], None);
		overlay.set_storage(vec![25], None);
		overlay.set_storage(vec![26], None);
		overlay.set_storage(vec![27], None);
		overlay.set_storage(vec![28], None);
		overlay.set_storage(vec![29], None);
		let backend = (
			Storage {
				top: map![
					vec![30] => vec![30]
				],
				children_default: map![],
			},
			StateVersion::default(),
		)
			.into();

		let mut ext = TestExt::new(&mut overlay, &backend, None);

		assert_eq!(ext.next_storage_key(&[5]), Some(vec![30]));

		drop(ext);
	}

	#[test]
	fn next_child_storage_key_works() {
		let child_info = ChildInfo::new_default(b"Child1");
		let child_info = &child_info;

		let mut overlay = OverlayedChanges::default();
		overlay.set_child_storage(child_info, vec![20], None);
		overlay.set_child_storage(child_info, vec![30], Some(vec![31]));
		let backend = (
			Storage {
				top: map![],
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: map![
							vec![10] => vec![10],
							vec![20] => vec![20],
							vec![40] => vec![40]
						],
						child_info: child_info.to_owned(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();

		let mut ext = TestExt::new(&mut overlay, &backend, None);

		// next_backend < next_overlay
		assert_eq!(ext.next_child_storage_key(child_info, &[5]), Some(vec![10]));

		// next_backend == next_overlay but next_overlay is a delete
		assert_eq!(ext.next_child_storage_key(child_info, &[10]), Some(vec![30]));

		// next_overlay < next_backend
		assert_eq!(ext.next_child_storage_key(child_info, &[20]), Some(vec![30]));

		// next_backend exist but next_overlay doesn't exist
		assert_eq!(ext.next_child_storage_key(child_info, &[30]), Some(vec![40]));

		drop(ext);
		overlay.set_child_storage(child_info, vec![50], Some(vec![50]));
		let mut ext = TestExt::new(&mut overlay, &backend, None);

		// next_overlay exist but next_backend doesn't exist
		assert_eq!(ext.next_child_storage_key(child_info, &[40]), Some(vec![50]));
	}

	#[test]
	fn child_storage_works() {
		let child_info = ChildInfo::new_default(b"Child1");
		let child_info = &child_info;
		let mut overlay = OverlayedChanges::default();
		overlay.set_child_storage(child_info, vec![20], None);
		overlay.set_child_storage(child_info, vec![30], Some(vec![31]));
		let backend = (
			Storage {
				top: map![],
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: map![
							vec![10] => vec![10],
							vec![20] => vec![20],
							vec![30] => vec![40]
						],
						child_info: child_info.to_owned(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();

		let mut ext = TestExt::new(&mut overlay, &backend, None);

		assert_eq!(ext.child_storage(child_info, &[10]), Some(vec![10]));
		assert_eq!(
			ext.child_storage_hash(child_info, &[10]),
			Some(Blake2Hasher::hash(&[10]).as_ref().to_vec()),
		);

		assert_eq!(ext.child_storage(child_info, &[20]), None);
		assert_eq!(ext.child_storage_hash(child_info, &[20]), None);

		assert_eq!(ext.child_storage(child_info, &[30]), Some(vec![31]));
		assert_eq!(
			ext.child_storage_hash(child_info, &[30]),
			Some(Blake2Hasher::hash(&[31]).as_ref().to_vec()),
		);
	}

	#[test]
	fn clear_prefix_cannot_delete_a_child_root() {
		let child_info = ChildInfo::new_default(b"Child1");
		let child_info = &child_info;
		let mut overlay = OverlayedChanges::default();
		let backend = (
			Storage {
				top: map![],
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: map![
							vec![30] => vec![40]
						],
						child_info: child_info.to_owned(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();

		let ext = TestExt::new(&mut overlay, &backend, None);

		use sp_core::storage::well_known_keys;
		let mut ext = ext;
		let mut not_under_prefix = well_known_keys::CHILD_STORAGE_KEY_PREFIX.to_vec();
		not_under_prefix[4] = 88;
		not_under_prefix.extend(b"path");
		ext.set_storage(not_under_prefix.clone(), vec![10]);

		let _ = ext.clear_prefix(&[], None, None);
		let _ = ext.clear_prefix(&well_known_keys::CHILD_STORAGE_KEY_PREFIX[..4], None, None);
		let mut under_prefix = well_known_keys::CHILD_STORAGE_KEY_PREFIX.to_vec();
		under_prefix.extend(b"path");
		let _ = ext.clear_prefix(&well_known_keys::CHILD_STORAGE_KEY_PREFIX[..4], None, None);
		assert_eq!(ext.child_storage(child_info, &[30]), Some(vec![40]));
		assert_eq!(ext.storage(not_under_prefix.as_slice()), Some(vec![10]));
		let _ = ext.clear_prefix(&not_under_prefix[..5], None, None);
		assert_eq!(ext.storage(not_under_prefix.as_slice()), None);
	}

	#[test]
	fn storage_append_works() {
		let mut data = Vec::new();
		let mut append = StorageAppend::new(&mut data);
		append.append(1u32.encode());
		append.append(2u32.encode());
		drop(append);

		assert_eq!(Vec::<u32>::decode(&mut &data[..]).unwrap(), vec![1, 2]);

		// Initialize with some invalid data
		let mut data = vec![1];
		let mut append = StorageAppend::new(&mut data);
		append.append(1u32.encode());
		append.append(2u32.encode());
		drop(append);

		assert_eq!(Vec::<u32>::decode(&mut &data[..]).unwrap(), vec![1, 2]);
	}

	fn large_values(count: u32) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + Clone {
		(0..count)
			.map(|i| (i.encode(), vec![i; 100].encode()))
			.chain((0..1000u32).map(|i| {
				let mut key = 1u32.encode();
				key.extend(i.encode());
				(key, vec![i; 100].encode())
			}))
	}

	fn backend_with_child(
		child_info: &ChildInfo,
		top: impl Iterator<Item = (Vec<u8>, Vec<u8>)>,
		child: impl Iterator<Item = (Vec<u8>, Vec<u8>)>,
	) -> TestBackend {
		(
			Storage {
				top: top.collect(),
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: child.collect(),
						child_info: child_info.clone(),
					}
				],
			},
			StateVersion::default(),
		)
			.into()
	}

	fn estimation_and_storage_root_proof_sizes(
		backend: TestBackend,
		apply_changes: impl FnOnce(&mut dyn Externalities),
	) -> (usize, usize) {
		estimation_and_storage_root_proof_sizes_with_recorder(backend, |ext, _| apply_changes(ext))
	}

	fn estimation_and_storage_root_proof_sizes_with_recorder(
		backend: TestBackend,
		apply_changes: impl FnOnce(&mut dyn Externalities, &Recorder<Blake2Hasher>),
	) -> (usize, usize) {
		let mut overlay = OverlayedChanges::default();

		let estimation_recorder = Recorder::<Blake2Hasher>::default();
		{
			let backend = TrieBackendBuilder::wrap(&backend)
				.with_recorder(estimation_recorder.clone())
				.build();
			let mut ext = Ext::new(&mut overlay, &backend, None);
			apply_changes(&mut ext, &estimation_recorder);
			ext.record_proof_for_dirty_keys();
		}

		let storage_root_recorder = Recorder::<Blake2Hasher>::default();
		{
			let backend = TrieBackendBuilder::wrap(&backend)
				.with_recorder(storage_root_recorder.clone())
				.build();
			let mut ext = Ext::new(&mut overlay, &backend, None);
			ext.storage_root(StateVersion::V1);
		}

		(estimation_recorder.estimate_encoded_size(), storage_root_recorder.estimate_encoded_size())
	}

	/// The estimation must record exactly the nodes the real storage root computation touches:
	/// too few would let a block overshoot its PoV budget, too many would waste it.
	fn assert_estimation_matches_storage_root(
		backend: TestBackend,
		apply_changes: impl FnOnce(&mut dyn Externalities),
	) {
		let (estimation, storage_root) =
			estimation_and_storage_root_proof_sizes(backend, apply_changes);
		assert_eq!(estimation, storage_root);
	}

	/// Deleting from a child trie may leave it empty, which removes its root from the main trie.
	/// The estimation cannot know whether that happens, so it assumes the removal and may record
	/// one extra sibling node. It must still never record less than the real computation.
	fn assert_estimation_covers_storage_root(
		backend: TestBackend,
		apply_changes: impl FnOnce(&mut dyn Externalities),
	) {
		let (estimation, storage_root) =
			estimation_and_storage_root_proof_sizes(backend, apply_changes);
		assert!(estimation >= storage_root, "{estimation} < {storage_root}");
	}

	/// Nibble paths of the block-start trie: root -5- A -0- B -0- L1 (K1), B -1- L2 (K2),
	/// A -1- L3 (K3). Deleting K1 and K2 collapses B into L2 and then A into L3, so L3 is loaded
	/// only when both deletes are seen by one trie.
	const K1: &[u8] = &[0x50, 0x00, 0xAA, 0xAA];
	const K2: &[u8] = &[0x50, 0x10, 0xBB, 0xBB];
	const K3: &[u8] = &[0x51, 0x00, 0xCC, 0xCC];
	const FILLER: &[u8] = &[0x00, 0x11, 0x22, 0x33];

	fn collapse_scenario_keys() -> Vec<(Vec<u8>, Vec<u8>)> {
		let fillers = [0x0u8, 0x1, 0x2, 0x3, 0xA, 0xB, 0xC, 0xD]
			.into_iter()
			.map(|nibble| (vec![nibble << 4, 0x11, 0x22, 0x33], vec![nibble; 64]));
		fillers
			.chain([
				(K1.to_vec(), vec![1u8; 64]),
				(K2.to_vec(), vec![2u8; 64]),
				(K3.to_vec(), vec![3u8; 64]),
			])
			.collect()
	}

	fn collapse_scenario_backend() -> TestBackend {
		(
			Storage {
				top: collapse_scenario_keys().into_iter().collect(),
				children_default: Default::default(),
			},
			StateVersion::V1,
		)
			.into()
	}

	/// Deleting in a child trie marks its root key as deleted in the top trie. The top keys share
	/// the first byte of that key, so its branch keeps enough siblings not to collapse and the
	/// top trie adds nothing beyond what the real root touches.
	fn collapse_scenario_child_backend(child_info: &ChildInfo) -> TestBackend {
		let child_root_key_first_byte = child_info.prefixed_storage_key().as_slice()[0];
		let top = (0..4u8).map(|nibble| {
			(vec![child_root_key_first_byte, nibble << 4, 0x22, 0x33], vec![nibble; 64])
		});
		backend_with_child(child_info, top, collapse_scenario_keys().into_iter())
	}

	/// A host transaction rolls back the recorder as well, as the runtime API glue does.
	fn start_host_transaction(ext: &mut dyn Externalities, recorder: &Recorder<Blake2Hasher>) {
		recorder.start_transaction();
		ext.storage_start_transaction();
	}

	fn rollback_host_transaction(ext: &mut dyn Externalities, recorder: &Recorder<Blake2Hasher>) {
		ext.storage_rollback_transaction().expect("transaction is open");
		recorder.rollback_transaction().expect("transaction is open");
	}

	/// Each flush must see the deletes of the earlier ones, otherwise a collapse that only
	/// happens jointly is never recorded and the block may overshoot its PoV budget.
	#[test]
	fn joint_collapse_across_flushes_is_estimated_exactly() {
		for (first, second) in [(K1, K2), (K2, K1)] {
			assert_estimation_matches_storage_root(collapse_scenario_backend(), |ext| {
				ext.clear_storage(first);
				ext.record_proof_for_dirty_keys();
				ext.clear_storage(second);
			});
		}
	}

	/// A host rollback drops the nodes recorded inside the transaction from the proof. The
	/// estimation trie must forget the deletes too, otherwise re-deleting the key is served from
	/// memory and records nothing.
	#[test]
	fn host_rollback_restores_the_estimation_trie() {
		let (estimation, storage_root) = estimation_and_storage_root_proof_sizes_with_recorder(
			collapse_scenario_backend(),
			|ext, recorder| {
				start_host_transaction(ext, recorder);
				ext.clear_storage(K1);
				ext.record_proof_for_dirty_keys();
				rollback_host_transaction(ext, recorder);

				ext.clear_storage(K1);
				ext.record_proof_for_dirty_keys();
				ext.clear_storage(K2);
			},
		);
		assert_eq!(estimation, storage_root);
	}

	/// A runtime rollback keeps the recorded nodes, but the estimation trie must still undo the
	/// collapse, otherwise the next delete collapses one level further than the real root does.
	#[test]
	fn runtime_rollback_restores_the_estimation_trie() {
		assert_estimation_matches_storage_root(collapse_scenario_backend(), |ext| {
			ext.storage_start_transaction();
			ext.clear_storage(K1);
			ext.record_proof_for_dirty_keys();
			ext.storage_rollback_transaction().expect("transaction is open");

			ext.clear_storage(K2);
		});
	}

	/// Committing an inner transaction hands its deletes to the outer one, so an outer rollback
	/// must undo them along with its own, back to the flush made before it started.
	#[test]
	fn outer_host_rollback_undoes_committed_inner_transaction() {
		let (estimation, storage_root) = estimation_and_storage_root_proof_sizes_with_recorder(
			collapse_scenario_backend(),
			|ext, recorder| {
				ext.clear_storage(FILLER);
				ext.record_proof_for_dirty_keys();

				start_host_transaction(ext, recorder);
				ext.clear_storage(K1);
				ext.record_proof_for_dirty_keys();
				ext.storage_start_transaction();
				ext.clear_storage(K2);
				ext.record_proof_for_dirty_keys();
				ext.storage_commit_transaction().expect("transaction is open");
				rollback_host_transaction(ext, recorder);

				ext.clear_storage(K1);
				ext.record_proof_for_dirty_keys();
				ext.clear_storage(K2);
			},
		);
		assert_eq!(estimation, storage_root);
	}

	/// Removing the last key turns the trie into the empty trie, which has no in-memory node to
	/// roll back to; the rollback must still restore the full trie.
	#[test]
	fn emptied_estimation_trie_is_restored_by_rollback() {
		let (estimation, storage_root) = estimation_and_storage_root_proof_sizes_with_recorder(
			collapse_scenario_backend(),
			|ext, recorder| {
				start_host_transaction(ext, recorder);
				for (key, _) in collapse_scenario_keys() {
					ext.clear_storage(&key);
				}
				ext.record_proof_for_dirty_keys();
				rollback_host_transaction(ext, recorder);

				ext.clear_storage(K1);
				ext.record_proof_for_dirty_keys();
				ext.clear_storage(K2);
			},
		);
		assert_eq!(estimation, storage_root);
	}

	#[test]
	fn joint_collapse_across_flushes_in_child_trie_is_estimated_exactly() {
		let child_info = ChildInfo::new_default(b"Child1");
		for (first, second) in [(K1, K2), (K2, K1)] {
			assert_estimation_matches_storage_root(
				collapse_scenario_child_backend(&child_info),
				|ext| {
					ext.clear_child_storage(&child_info, first);
					ext.record_proof_for_dirty_keys();
					ext.clear_child_storage(&child_info, second);
				},
			);
		}
	}

	#[test]
	fn host_rollback_restores_the_child_estimation_trie() {
		let child_info = ChildInfo::new_default(b"Child1");
		let (estimation, storage_root) = estimation_and_storage_root_proof_sizes_with_recorder(
			collapse_scenario_child_backend(&child_info),
			|ext, recorder| {
				start_host_transaction(ext, recorder);
				ext.clear_child_storage(&child_info, K1);
				ext.record_proof_for_dirty_keys();
				rollback_host_transaction(ext, recorder);

				ext.clear_child_storage(&child_info, K1);
				ext.record_proof_for_dirty_keys();
				ext.clear_child_storage(&child_info, K2);
			},
		);
		assert_eq!(estimation, storage_root);
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof_for_main_trie_changes() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend = backend_with_child(&child_info, large_values(100000), large_values(100000));

		assert_estimation_matches_storage_root(backend, |ext| {
			ext.place_storage(5000u32.encode(), Some(vec![30]));
			ext.place_storage(100000u32.encode(), Some(vec![40]));

			ext.place_storage(1u32.encode(), None);
			ext.place_storage(6000u32.encode(), None);
		});
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof_for_child_inserts() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend = backend_with_child(&child_info, large_values(100000), large_values(100000));

		assert_estimation_matches_storage_root(backend, |ext| {
			ext.place_child_storage(&child_info, 5000u32.encode(), Some(vec![30]));
			ext.place_child_storage(&child_info, 100000u32.encode(), Some(vec![40]));
		});
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend = backend_with_child(&child_info, large_values(100000), large_values(100000));

		assert_estimation_covers_storage_root(backend, |ext| {
			ext.place_storage(5000u32.encode(), Some(vec![30]));
			ext.place_storage(100000u32.encode(), Some(vec![40]));

			ext.place_storage(1u32.encode(), None);
			ext.place_storage(6000u32.encode(), None);

			ext.place_child_storage(&child_info, 5000u32.encode(), Some(vec![30]));
			ext.place_child_storage(&child_info, 100000u32.encode(), Some(vec![40]));

			ext.place_child_storage(&child_info, 1u32.encode(), None);
			ext.place_child_storage(&child_info, 6000u32.encode(), None);
		});
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof_2() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend = backend_with_child(&child_info, large_values(100000), large_values(100000));

		assert_estimation_covers_storage_root(backend, |ext| {
			let key1 = 5000u32;
			let key2 = 100000u32;
			ext.storage_start_transaction();
			ext.place_storage(key1.encode(), Some(vec![30]));
			ext.place_storage(key2.encode(), Some(vec![40]));

			ext.place_child_storage(&child_info, key1.encode(), Some(vec![30]));
			ext.place_child_storage(&child_info, key2.encode(), Some(vec![40]));
			let _ = ext.storage_commit_transaction();

			ext.record_proof_for_dirty_keys();

			ext.storage_start_transaction();
			ext.place_storage(key1.encode(), None);
			ext.place_storage(key2.encode(), None);

			ext.place_child_storage(&child_info, key1.encode(), None);
			ext.place_child_storage(&child_info, key2.encode(), None);
			let _ = ext.storage_commit_transaction();
		});
	}

	#[test]
	fn untouched_child_trie_is_not_re_estimated() {
		let keys = (0..10000u32).map(|i| (i.encode(), vec![i; 100].encode()));
		let child_info = ChildInfo::new_default(b"Child1");
		let child_info = &child_info;

		let backend: TestBackend = (
			Storage {
				top: keys.clone().collect(),
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: keys.collect(),
						child_info: child_info.to_owned(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();

		let estimate_top_only_change = |overlay: &mut OverlayedChanges<Blake2Hasher>| {
			let recorder = Recorder::<Blake2Hasher>::default();
			let backend =
				TrieBackendBuilder::wrap(&backend).with_recorder(recorder.clone()).build();
			let mut ext = Ext::new(overlay, &backend, None);
			ext.place_storage(5000u32.encode(), Some(vec![30]));
			ext.record_proof_for_dirty_keys();
			recorder.estimate_encoded_size()
		};

		let mut overlay_with_estimated_child = OverlayedChanges::default();
		{
			let backend =
				TrieBackendBuilder::wrap(&backend).with_recorder(Recorder::default()).build();
			let mut ext = Ext::new(&mut overlay_with_estimated_child, &backend, None);
			ext.place_child_storage(child_info, 5000u32.encode(), Some(vec![30]));
			ext.record_proof_for_dirty_keys();
		}

		// The child was already estimated and is untouched since, so it must not add any
		// trie nodes to the proof of a later estimation.
		assert_eq!(
			estimate_top_only_change(&mut overlay_with_estimated_child),
			estimate_top_only_change(&mut OverlayedChanges::default()),
		);
	}

	#[test]
	fn empty_child_delta_records_nothing() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend: TestBackend = (
			Storage {
				top: map![vec![1] => vec![1]],
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: map![vec![1] => vec![1]],
						child_info: child_info.clone(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();
		let recorder = Recorder::<Blake2Hasher>::default();
		let backend = TrieBackendBuilder::wrap(&backend).with_recorder(recorder.clone()).build();

		backend.record_proof_for_child_dirty_keys(
			&child_info,
			core::iter::empty::<(&[u8], crate::DeltaKeyOp)>(),
			&mut Default::default(),
		);

		assert_eq!(recorder.estimate_encoded_size(), 0);
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof_when_child_is_created() {
		let child_info = ChildInfo::new_default(b"Child1");
		let backend = (
			Storage {
				top: (0..10000u32).map(|i| (i.encode(), vec![i; 100].encode())).collect(),
				children_default: Default::default(),
			},
			StateVersion::default(),
		)
			.into();

		assert_estimation_matches_storage_root(backend, |ext| {
			ext.place_child_storage(&child_info, 1u32.encode(), Some(vec![30]));
		});
	}

	#[test]
	fn calculating_storage_root_should_not_change_storage_proof_when_child_is_emptied() {
		let child_info = ChildInfo::new_default(b"Child1");
		let child_keys = (0..3u32).map(|i| i.encode()).collect::<Vec<_>>();
		let backend = (
			Storage {
				top: (0..10000u32).map(|i| (i.encode(), vec![i; 100].encode())).collect(),
				children_default: map![
					child_info.storage_key().to_vec() => StorageChild {
						data: child_keys.iter().map(|k| (k.clone(), vec![30])).collect(),
						child_info: child_info.clone(),
					}
				],
			},
			StateVersion::default(),
		)
			.into();

		assert_estimation_matches_storage_root(backend, |ext| {
			for key in &child_keys {
				ext.place_child_storage(&child_info, key.clone(), None);
			}
		});
	}
}

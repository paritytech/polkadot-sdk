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

use alloc::{vec, vec::Vec};

#[cfg(not(substrate_runtime))]
use sp_core::storage::ChildInfo;

use sp_core::storage::StateVersion;

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnByCodec, AllocateAndReturnFatPointer, ConvertAndPassAs,
		ConvertAndReturnAs, PassAs, PassFatPointerAndDecode, PassFatPointerAndRead,
		PassFatPointerAndReadWrite, PassFatPointerAndWrite, PassOptionalFatPointerAndRead,
		PassPointerAndWrite,
	},
	runtime_interface,
};

#[cfg(not(substrate_runtime))]
use sp_externalities::Externalities;

pub use sp_externalities::MultiRemovalResults;

use crate::*;

/// Interface for accessing the storage from within the runtime.
#[runtime_interface]
pub trait Storage {
	/// Returns the data for `key` in the storage or `None` if the key can not be found.
	#[version(1, register_only)]
	fn get(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bytes::Bytes>> {
		self.storage(key).map(|s| bytes::Bytes::from(s.to_vec()))
	}

	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	fn read(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> AllocateAndReturnByCodec<Option<u32>> {
		self.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			data.len() as u32
		})
	}

	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	/// If `allow_partial` is non-zero, the function will copy as many bytes as possible into
	/// `value_out`, even if the value is longer than `value_out`.
	#[version(2)]
	#[raw_api]
	fn read(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndWrite<&mut [u8]>,
		value_offset: u32,
		allow_partial: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		self.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let out_len = core::cmp::min(data.len(), value_out.len());
			if value_out.len() >= data.len() || allow_partial != 0 {
				value_out[..out_len].copy_from_slice(&data[..out_len]);
			}
			data.len() as u32
		})
	}

	/// A convenience wrapper providing exact-read interface to the `read` host function.
	#[wrapper]
	fn read_exact(key: impl AsRef<[u8]>, value_out: &mut [u8], value_offset: u32) -> Option<u32> {
		read__raw(key.as_ref(), &mut value_out[..], value_offset, 0)
	}

	/// A convenience wrapper providing interface for partial storage reads (e.g. for `decode_len`).
	#[wrapper]
	fn read_partial(key: impl AsRef<[u8]>, value_out: &mut [u8], value_offset: u32) -> Option<u32> {
		read__raw(key.as_ref(), &mut value_out[..], value_offset, 1)
	}

	/// A convenience wrapper implementing the deprecated `get` host function
	/// functionality through the new interface.
	#[wrapper]
	fn get(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut value_out = vec![0u8; 256];
		let len = read_exact(key.as_ref(), &mut value_out[..], 0)?;
		if len as usize > value_out.len() {
			value_out.resize(len as usize, 0);
			read_exact(key.as_ref(), &mut value_out[..], 0)?;
		}
		value_out.truncate(len as usize);
		Some(value_out)
	}

	/// Set `key` to `value` in the storage.
	fn set(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<&[u8]>) {
		self.set_storage(key.to_vec(), value.to_vec());
	}

	/// Clear the storage of the given `key` and its value.
	fn clear(&mut self, key: PassFatPointerAndRead<&[u8]>) {
		self.clear_storage(key)
	}

	/// Check whether the given `key` exists in storage.
	fn exists(&mut self, key: PassFatPointerAndRead<&[u8]>) -> bool {
		self.exists_storage(key)
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	fn clear_prefix(&mut self, prefix: PassFatPointerAndRead<&[u8]>) {
		let _ = Externalities::clear_prefix(*self, prefix, None, None);
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// # Limit
	///
	/// Deletes all keys from the overlay and up to `limit` keys from the backend if
	/// it is set to `Some`. No limit is applied when `limit` is set to `None`.
	///
	/// The limit can be used to partially delete a prefix storage in case it is too large
	/// to delete in one go (block).
	///
	/// Returns [`KillStorageResult`] to inform about the result.
	///
	/// # Note
	///
	/// Please note that keys that are residing in the overlay for that prefix when
	/// issuing this call are all deleted without counting towards the `limit`. Only keys
	/// written during the current block are part of the overlay. Deleting with a `limit`
	/// mostly makes sense with an empty overlay for that prefix.
	///
	/// Calling this function multiple times per block for the same `prefix` does
	/// not make much sense because it is not cumulative when called inside the same block.
	/// The deletion would always start from `prefix` resulting in the same keys being deleted
	/// every time this function is called with the exact same arguments per block. This happens
	/// because the keys in the overlay are not taken into account when deleting keys in the
	/// backend.
	#[version(2)]
	fn clear_prefix(
		&mut self,
		prefix: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		Externalities::clear_prefix(*self, prefix, limit, None).into()
	}

	/// Partially clear the storage of each key-value pair where the key starts with the given
	/// prefix.
	///
	/// # Limit
	///
	/// A *limit* should always be provided through `maybe_limit`. This is one fewer than the
	/// maximum number of backend iterations which may be done by this operation and as such
	/// represents the maximum number of backend deletions which may happen. A *limit* of zero
	/// implies that no keys will be deleted, though there may be a single iteration done.
	///
	/// The limit can be used to partially delete a prefix storage in case it is too large or costly
	/// to delete in a single operation.
	///
	/// # Cursor
	///
	/// A *cursor* may be passed in to this operation with `maybe_cursor`. `None` should only be
	/// passed once (in the initial call) for any given `maybe_prefix` value. Subsequent calls
	/// operating on the same prefix should always pass `Some`, and this should be equal to the
	/// previous call result's `maybe_cursor` field.
	///
	/// Stores the output cursor and three counters (backend deletions, unique key deletions, number
	/// of iterations performed) into the provided output buffers. See
	/// [`MultiRemovalResults`](sp_io::MultiRemovalResults) for more details.
	///
	/// Returns the number of bytes in the output cursor. If the output buffer is not large enough,
	/// the cursor will be truncated to the length of the buffer, but the full length of the cursor
	/// is still returned.
	///
	/// NOTE: After the initial call for any given prefix, it is important that no further
	/// keys under the same prefix are inserted. If so, then they may or may not be deleted by
	/// subsequent calls.
	///
	/// NOTE: Please note that keys which are residing in the overlay for that prefix when
	/// issuing this call are deleted without counting towards the `limit`.
	#[version(3, register_only)]
	fn clear_prefix(
		&mut self,
		maybe_prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: PassFatPointerAndDecode<Option<u32>>,
		maybe_cursor: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<MultiRemovalResults> {
		Externalities::clear_prefix(
			*self,
			maybe_prefix,
			maybe_limit,
			maybe_cursor.as_ref().map(|x| &x[..]),
		)
		.into()
	}

	/// Same as version 3 but avoids host-side allocation.
	// ERRATA: The RFC specifies this as `ext_storage_clear_prefix_version_3`, but since
	// version 3 was already registered with a different signature prior to the RFC
	// implementation, this is registered as version 4 instead.
	#[version(4)]
	#[raw_api]
	fn clear_prefix(
		&mut self,
		maybe_prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassOptionalFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndWrite<&mut [u8]>,
		counters_out: PassPointerAndWrite<&mut StorageIterations, 12>,
	) -> u32 {
		let removal_results = Externalities::clear_prefix(
			*self,
			maybe_prefix,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		counters_out.backend = removal_results.backend;
		counters_out.unique = removal_results.unique;
		counters_out.loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `clear_prefix` host
	/// function.
	#[wrapper]
	fn clear_prefix(
		maybe_prefix: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor_in: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let mut counters = StorageIterations::default();
		let cursor_len = clear_prefix__raw(
			maybe_prefix.as_ref(),
			maybe_limit,
			maybe_cursor_in,
			&mut maybe_cursor_out,
			&mut counters,
		) as usize;
		result.backend = counters.backend;
		result.unique = counters.unique;
		result.loops = counters.loops;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				assert_eq!(
					cached_cursor_len.map(|len| len as usize),
					Some(cursor_len),
					"the cursor cached by the host must match the length it reported"
				);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}
		result
	}

	/// Append the encoded `value` to the storage item at `key`.
	///
	/// The storage item needs to implement [`EncodeAppend`](codec::EncodeAppend).
	///
	/// # Warning
	///
	/// If the storage item does not support [`EncodeAppend`](codec::EncodeAppend) or
	/// something else fails at appending, the storage item will be set to `[value]`.
	fn append(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<Vec<u8>>) {
		self.storage_append(key.to_vec(), value);
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	fn root(&mut self) -> AllocateAndReturnFatPointer<Vec<u8>> {
		self.storage_root()
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	// The `version` argument is ignored: the state version is a property of the execution
	// environment now. The host learns it from the `system_version` field of the runtime's
	// `RuntimeVersion` and sets it on the externalities before dispatching the runtime call
	// (see `Externalities::set_runtime_state_version`), so the value passed by the runtime
	// here is redundant.
	#[version(2)]
	fn root(&mut self, _version: PassAs<StateVersion, u8>) -> AllocateAndReturnFatPointer<Vec<u8>> {
		self.storage_root()
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash. Since the size of the resulting
	/// value is known to the caller, this function requires the provided buffer to be large enough
	/// to store the entire value; otherwise, it will panic.
	#[version(3)]
	#[raw_api]
	fn root(&mut self, out: PassFatPointerAndWrite<&mut [u8]>) {
		let root = self.storage_root();
		let encoded = codec::Encode::encode(&root);
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded_len].copy_from_slice(&encoded[..]);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `root` host
	/// function.
	#[wrapper]
	fn root() -> Vec<u8> {
		// By this point, all the information about the length of the hash representing the storage
		// root has been erased. We're using a generous buffer here. Making host functions generic
		// over the hasher type is a big refactoring and is not worth it.
		let mut root_out = vec![0u8; 256];
		root__raw(&mut root_out[..]);
		codec::Decode::decode(&mut &root_out[..])
			.expect("storage root is always a valid SCALE-encoded Vec<u8>; qed")
	}

	/// Always returns `None`. This function exists for compatibility reasons.
	#[version(1, register_only)]
	fn changes_root(
		&mut self,
		_parent_hash: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		None
	}

	/// Get the next key in storage after the given one in lexicographic order.
	fn next_key(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		self.next_storage_key(key)
	}

	/// Get the next key in storage after the given one in lexicographic order.
	#[raw_api]
	#[version(2)]
	fn next_key(
		&mut self,
		key_in: PassFatPointerAndRead<&[u8]>,
		key_out: PassFatPointerAndWrite<&mut [u8]>,
	) -> u32 {
		let next_key = self.next_storage_key(key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `next_key` host
	/// function.
	///
	/// On success, `key_out` is populated with the next storage key and `true` is returned.
	/// If there is no next key, `key_out` is cleared and `false` is returned. The caller can reuse
	/// the buffer across calls to avoid repeated allocations.
	#[wrapper]
	fn next_key(key_in: impl AsRef<[u8]>, key_out: &mut Vec<u8>) -> bool {
		let key_in = key_in.as_ref();
		let len = next_key__raw(key_in, key_out.as_mut_slice()) as usize;
		if len == 0 {
			key_out.clear();
			return false;
		}
		if len <= key_out.len() {
			key_out.truncate(len);
			return true;
		}
		key_out.resize(len, 0);
		next_key__raw(key_in, &mut key_out[..]);
		true
	}

	/// Start a new nested transaction.
	///
	/// This allows to either commit or roll back all changes that are made after this call.
	/// For every transaction there must be a matching call to either `rollback_transaction`
	/// or `commit_transaction`. This is also effective for all values manipulated using the
	/// `DefaultChildStorage` API.
	///
	/// # Warning
	///
	/// This is a low level API that is potentially dangerous as it can easily result
	/// in unbalanced transactions. For example, FRAME users should use high level storage
	/// abstractions.
	fn start_transaction(&mut self) {
		self.storage_start_transaction();
	}

	/// Rollback the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are discarded.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	fn rollback_transaction(&mut self) {
		self.storage_rollback_transaction()
			.expect("No open transaction that can be rolled back.");
	}

	/// Commit the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are committed.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	fn commit_transaction(&mut self) {
		self.storage_commit_transaction()
			.expect("No open transaction that can be committed.");
	}
}

/// Interface for accessing the child storage for default child trie,
/// from within the runtime.
#[runtime_interface]
pub trait DefaultChildStorage {
	/// Get a default child storage value for a given key.
	///
	/// Parameter `storage_key` is the unprefixed location of the root of the child trie in the
	/// parent trie. Result is `None` if the value for `key` in the child storage can not be found.
	#[version(1, register_only)]
	fn get(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key).map(|s| s.to_vec())
	}

	/// Allocation efficient variant of `get`.
	///
	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	fn read(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> AllocateAndReturnByCodec<Option<u32>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let out_len = core::cmp::min(data.len(), value_out.len());
			value_out[..out_len].copy_from_slice(&data[..out_len]);
			data.len() as u32
		})
	}

	/// Allocation efficient variant of `get`.
	///
	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	///
	/// If `allow_partial` is non-zero, the function will copy as many bytes as possible into
	/// `value_out`, even if the value is longer than `value_out`.
	#[version(2)]
	#[raw_api]
	fn read(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndWrite<&mut [u8]>,
		value_offset: u32,
		allow_partial: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key)
			.map(|value| {
				let value_offset = value_offset as usize;
				let data = &value[value_offset.min(value.len())..];
				let out_len = core::cmp::min(data.len(), value_out.len());
				if value_out.len() >= data.len() || allow_partial != 0 {
					value_out[..out_len].copy_from_slice(&data[..out_len]);
				}
				data.len() as u32
			})
			.into()
	}

	/// A convenience wrapper providing exact-read interface to the `read` host function.
	#[wrapper]
	fn read_exact(
		storage_key: impl AsRef<[u8]>,
		key: impl AsRef<[u8]>,
		value_out: &mut [u8],
		value_offset: u32,
	) -> Option<u32> {
		read__raw(storage_key.as_ref(), key.as_ref(), &mut value_out[..], value_offset, 0)
	}

	/// A convenience wrapper providing interface for partial storage reads (e.g. for `decode_len`).
	#[wrapper]
	fn read_partial(
		storage_key: impl AsRef<[u8]>,
		key: impl AsRef<[u8]>,
		value_out: &mut [u8],
		value_offset: u32,
	) -> Option<u32> {
		read__raw(storage_key.as_ref(), key.as_ref(), &mut value_out[..], value_offset, 1)
	}

	/// A convenience wrapper implementing the deprecated `get` host function
	/// functionality through the new interface.
	#[wrapper]
	fn get(storage_key: impl AsRef<[u8]>, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut value_out = vec![0u8; 256];
		let len = read_exact(storage_key.as_ref(), key.as_ref(), &mut value_out[..], 0)?;
		if len as usize > value_out.len() {
			value_out.resize(len as usize, 0);
			read_exact(storage_key.as_ref(), key.as_ref(), &mut value_out[..], 0)?;
		}
		value_out.truncate(len as usize);
		Some(value_out)
	}

	/// Set a child storage value.
	///
	/// Set `key` to `value` in the child storage denoted by `storage_key`.
	fn set(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		self.set_child_storage(&child_info, key.to_vec(), value.to_vec());
	}

	/// Clear a child storage key.
	///
	/// For the default child storage at `storage_key`, clear value at `key`.
	fn clear(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		self.clear_child_storage(&child_info, key);
	}

	/// Clear an entire child storage.
	///
	/// If it exists, the child storage for `storage_key`
	/// is removed.
	fn storage_kill(&mut self, storage_key: PassFatPointerAndRead<&[u8]>) {
		let child_info = ChildInfo::new_default(storage_key);
		let _ = self.kill_child_storage(&child_info, None, None);
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(2)]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> bool {
		let child_info = ChildInfo::new_default(storage_key);
		let r = self.kill_child_storage(&child_info, limit, None);
		r.maybe_cursor.is_none()
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(3)]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		let child_info = ChildInfo::new_default(storage_key);
		self.kill_child_storage(&child_info, limit, None).into()
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation.
	#[version(4, register_only)]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		maybe_limit: PassFatPointerAndDecode<Option<u32>>,
		maybe_cursor: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<MultiRemovalResults> {
		let child_info = ChildInfo::new_default(storage_key);
		self.kill_child_storage(&child_info, maybe_limit, maybe_cursor.as_ref().map(|x| &x[..]))
			.into()
	}

	/// Same as version 4 but avoids host-side allocation.
	// ERRATA: The RFC specifies this as `ext_default_child_storage_storage_kill_version_4`,
	// but since version 4 was already registered with a different signature prior to the RFC
	// implementation, this is registered as version 5 instead.
	#[version(5)]
	#[raw_api]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassOptionalFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndWrite<&mut [u8]>,
		counters_out: PassPointerAndWrite<&mut StorageIterations, 12>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = self.kill_child_storage(
			&child_info,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		counters_out.backend = removal_results.backend;
		counters_out.unique = removal_results.unique;
		counters_out.loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `storage_kill` host
	/// function.
	#[wrapper]
	fn storage_kill(
		storage_key: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let mut counters = StorageIterations::default();
		let cursor_len = storage_kill__raw(
			storage_key.as_ref(),
			maybe_limit,
			maybe_cursor,
			&mut maybe_cursor_out[..],
			&mut counters,
		) as usize;
		result.backend = counters.backend;
		result.unique = counters.unique;
		result.loops = counters.loops;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				assert_eq!(
					cached_cursor_len.map(|len| len as usize),
					Some(cursor_len),
					"the cursor cached by the host must match the length it reported"
				);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}

		result
	}

	/// Check a child storage key.
	///
	/// Check whether the given `key` exists in default child defined at `storage_key`.
	fn exists(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		let child_info = ChildInfo::new_default(storage_key);
		self.exists_child_storage(&child_info, key)
	}

	/// Clear child default key by prefix.
	///
	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		let _ = self.clear_child_prefix(&child_info, prefix, None, None);
	}

	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(2)]
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		let child_info = ChildInfo::new_default(storage_key);
		self.clear_child_prefix(&child_info, prefix, limit, None).into()
	}

	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// See `Storage` module `clear_prefix` documentation.
	#[version(3, register_only)]
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: PassFatPointerAndDecode<Option<u32>>,
		maybe_cursor: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<MultiRemovalResults> {
		let child_info = ChildInfo::new_default(storage_key);
		self.clear_child_prefix(
			&child_info,
			prefix,
			maybe_limit,
			maybe_cursor.as_ref().map(|x| &x[..]),
		)
		.into()
	}

	/// Same as version 3 but avoids host-side allocation.
	// ERRATA: The RFC specifies this as `ext_default_child_storage_clear_prefix_version_3`,
	// but since version 3 was already registered with a different signature prior to the RFC
	// implementation, this is registered as version 4 instead.
	#[version(4)]
	#[raw_api]
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassOptionalFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndWrite<&mut [u8]>,
		counters_out: PassPointerAndWrite<&mut StorageIterations, 12>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = self.clear_child_prefix(
			&child_info,
			prefix,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		counters_out.backend = removal_results.backend;
		counters_out.unique = removal_results.unique;
		counters_out.loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `clear_prefix` host
	/// function.
	#[wrapper]
	fn clear_prefix(
		storage_key: impl AsRef<[u8]>,
		maybe_prefix: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor_in: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let mut counters = StorageIterations::default();
		let cursor_len = clear_prefix__raw(
			storage_key.as_ref(),
			maybe_prefix.as_ref(),
			maybe_limit,
			maybe_cursor_in,
			&mut maybe_cursor_out,
			&mut counters,
		) as usize;
		result.backend = counters.backend;
		result.unique = counters.unique;
		result.loops = counters.loops;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				assert_eq!(
					cached_cursor_len.map(|len| len as usize),
					Some(cursor_len),
					"the cursor cached by the host must match the length it reported"
				);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}
		result
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnFatPointer<Vec<u8>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage_root(&child_info)
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	#[version(2)]
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		_version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnFatPointer<Vec<u8>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage_root(&child_info)
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash. Since the size of the resulting
	/// value is known to the caller, this function requires the provided buffer to be large enough
	/// to store the entire value; otherwise, it will panic.
	#[version(3)]
	#[raw_api]
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		let root = self.child_storage_root(&child_info);
		let encoded = codec::Encode::encode(&root);
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the child storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded.len()].copy_from_slice(&encoded[..]);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `root` host
	/// function.
	#[wrapper]
	fn root(storage_key: impl AsRef<[u8]>) -> Vec<u8> {
		// By this point, all the information about the length of the hash representing the storage
		// root has been erased. We're using a generous buffer here. Making host functions generic
		// over the hasher type is a big refactoring and is not worth it.
		let mut root_out = vec![0u8; 256];
		root__raw(storage_key.as_ref(), &mut root_out[..]);
		codec::Decode::decode(&mut &root_out[..])
			.expect("child storage root is always a valid SCALE-encoded Vec<u8>; qed")
	}

	/// Child storage key iteration.
	///
	/// Get the next key in storage after the given one in lexicographic order in child storage.
	fn next_key(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.next_child_storage_key(&child_info, key)
	}

	/// Child storage key iteration.
	///
	/// Get the next key in storage after the given one in lexicographic order in child storage.
	#[version(2)]
	#[raw_api]
	fn next_key(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key_in: PassFatPointerAndRead<&[u8]>,
		key_out: PassFatPointerAndWrite<&mut [u8]>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let next_key = self.next_child_storage_key(&child_info, key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `next_key` host
	/// function.
	///
	/// On success, `key_out` is populated with the next storage key and `true` is returned.
	/// If there is no next key, `key_out` is cleared and `false` is returned. The caller can reuse
	/// the buffer across calls to avoid repeated allocations.
	#[wrapper]
	fn next_key(
		storage_key: impl AsRef<[u8]>,
		key_in: impl AsRef<[u8]>,
		key_out: &mut Vec<u8>,
	) -> bool {
		let storage_key = storage_key.as_ref();
		let key_in = key_in.as_ref();
		let len = next_key__raw(storage_key, key_in, key_out.as_mut_slice()) as usize;
		if len == 0 {
			key_out.clear();
			return false;
		}
		if len <= key_out.len() {
			key_out.truncate(len);
			return true;
		}
		key_out.resize(len, 0);
		next_key__raw(storage_key, key_in, &mut key_out[..]);
		true
	}
}

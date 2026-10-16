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

//! The `storage` host calls: key-value storage access from within the runtime.
//!
//! Node logic lives in private `node_*` functions below, which acquire externalities. The
//! `host_calls!` bodies and std public wrappers both forward to those functions.

use alloc::vec::Vec;

use codec::Decode;
use sp_core::storage::StateVersion;

use crate::jam::KillStorageResult;

#[cfg(substrate_runtime)]
use super::option_u32_from_host;
#[cfg(not(substrate_runtime))]
use super::with_externalities;
#[cfg(not(substrate_runtime))]
use super::{option_u32_arg, option_u32_into_ffi};
#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;

/// Node logic of `ext_storage_read`.
#[cfg(not(substrate_runtime))]
fn node_read(
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	with_externalities(|ext| {
		ext.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let out_len = core::cmp::min(data.len(), value_out.len());
			if value_out.len() >= data.len() || allow_partial != 0 {
				value_out[..out_len].copy_from_slice(&data[..out_len]);
			}
			data.len() as u32
		})
	})
}

/// Node logic of `ext_storage_set`.
#[cfg(not(substrate_runtime))]
fn node_set(key: &[u8], value: &[u8]) {
	with_externalities(|ext| ext.set_storage(key.to_vec(), value.to_vec()))
}

/// Node logic of `ext_storage_clear`.
#[cfg(not(substrate_runtime))]
fn node_clear(key: &[u8]) {
	with_externalities(|ext| ext.clear_storage(key))
}

/// Node logic of `ext_storage_exists`.
#[cfg(not(substrate_runtime))]
fn node_exists(key: &[u8]) -> bool {
	with_externalities(|ext| ext.exists_storage(key))
}

/// Node logic of `ext_storage_clear_prefix`.
#[cfg(not(substrate_runtime))]
fn node_clear_prefix(prefix: &[u8], limit: Option<u32>) -> u64 {
	with_externalities(|ext| {
		let removal_results = ext.clear_prefix(prefix, limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

/// Node logic of `ext_storage_append`.
#[cfg(not(substrate_runtime))]
fn node_append(key: &[u8], value: Vec<u8>) {
	with_externalities(|ext| ext.storage_append(key.to_vec(), value))
}

/// Node logic of `ext_storage_root`.
#[cfg(not(substrate_runtime))]
fn node_root(out: &mut [u8]) {
	with_externalities(|ext| {
		let root = ext.storage_root();
		let encoded = codec::Encode::encode(&root);
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded_len].copy_from_slice(&encoded[..]);
	})
}

/// Node logic of `ext_storage_next_key`.
#[cfg(not(substrate_runtime))]
fn node_next_key(key_in: &[u8], key_out: &mut [u8]) -> u32 {
	with_externalities(|ext| {
		let next_key = ext.next_storage_key(key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	})
}

/// Node logic of `ext_storage_start_transaction`.
#[cfg(not(substrate_runtime))]
fn node_start_transaction() {
	with_externalities(|ext| ext.storage_start_transaction())
}

/// Node logic of `ext_storage_rollback_transaction`.
#[cfg(not(substrate_runtime))]
fn node_rollback_transaction() {
	with_externalities(|ext| {
		ext.storage_rollback_transaction()
			.expect("No open transaction that can be rolled back.");
	})
}

/// Node logic of `ext_storage_commit_transaction`.
#[cfg(not(substrate_runtime))]
fn node_commit_transaction() {
	with_externalities(|ext| {
		ext.storage_commit_transaction()
			.expect("No open transaction that can be committed.");
	})
}

sp_jam_runtime_interface::host_calls! {
	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	/// If `allow_partial` is non-zero, the function will copy as many bytes as possible into
	/// `value_out`, even if the value is longer than `value_out`.
	#[index = 301]
	fn ext_storage_read(
		ctx: &mut dyn FunctionContext,
		key: *const u8,
		key_len: u64,
		value_out: *mut u8,
		value_out_len: u64,
		value_offset: u32,
		allow_partial: u32,
	) -> i64 {
		let key = ctx.read_memory(key, key_len)?;
		ctx.with_buffer(value_out, value_out_len, |value_out| {
			node_read(&key, value_out, value_offset, allow_partial)
		})
		.map(option_u32_into_ffi)
	}

	/// Set `key` to `value` in the storage.
	#[index = 302]
	fn ext_storage_set(ctx: &mut dyn FunctionContext, key: *const u8, key_len: u64, value: *const u8, value_len: u64) {
		let key = ctx.read_memory(key, key_len)?;
		let value = ctx.read_memory(value, value_len)?;
		node_set(&key, &value);
		Ok(())
	}

	/// Clear the storage of the given `key` and its value.
	#[index = 303]
	fn ext_storage_clear(ctx: &mut dyn FunctionContext, key: *const u8, key_len: u64) {
		node_clear(&ctx.read_memory(key, key_len)?);
		Ok(())
	}

	/// Check whether the given `key` exists in storage.
	#[index = 304]
	fn ext_storage_exists(ctx: &mut dyn FunctionContext, key: *const u8, key_len: u64) -> u32 {
		Ok(node_exists(&ctx.read_memory(key, key_len)?) as u32)
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// Deletes all keys from the overlay and up to `limit` keys from the backend if it is set
	/// to `Some`. No limit is applied when `limit` is set to `None`. There is no continuation
	/// cursor: the returned `u64` encodes `(some_remaining as u64) << 32 | loops as u64`.
	#[index = 305]
	fn ext_storage_clear_prefix(ctx: &mut dyn FunctionContext, prefix: *const u8, prefix_len: u64, limit: i64) -> u64 {
		let prefix = ctx.read_memory(prefix, prefix_len)?;
		Ok(node_clear_prefix(&prefix, option_u32_arg(limit)?))
	}

	/// Append the encoded `value` to the storage item at `key`.
	///
	/// The storage item needs to implement [`EncodeAppend`](codec::EncodeAppend).
	///
	/// # Warning
	///
	/// If the storage item does not support [`EncodeAppend`](codec::EncodeAppend) or
	/// something else fails at appending, the storage item will be set to `[value]`.
	#[index = 306]
	fn ext_storage_append(ctx: &mut dyn FunctionContext, key: *const u8, key_len: u64, value: *const u8, value_len: u64) {
		let key = ctx.read_memory(key, key_len)?;
		let value = ctx.read_memory(value, value_len)?;
		node_append(&key, value);
		Ok(())
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash. Since the size of the
	/// resulting value is known to the caller, this function requires the provided buffer to
	/// be large enough to store the entire value; otherwise, it will panic.
	#[index = 307]
	fn ext_storage_root(ctx: &mut dyn FunctionContext, out: *mut u8, out_len: u64) {
		ctx.with_buffer(out, out_len, node_root)
	}

	/// Get the next key in storage after the given one in lexicographic order.
	///
	/// Writes the next key into `key_out` if it fits and returns the full length of the key;
	/// `0` means there is no next key.
	#[index = 308]
	fn ext_storage_next_key(
		ctx: &mut dyn FunctionContext,
		key_in: *const u8,
		key_in_len: u64,
		key_out: *mut u8,
		key_out_len: u64,
	) -> u32 {
		let key_in = ctx.read_memory(key_in, key_in_len)?;
		ctx.with_buffer(key_out, key_out_len, |key_out| node_next_key(&key_in, key_out))
	}

	/// Start a new nested transaction.
	///
	/// This allows to either commit or roll back all changes that are made after this call.
	/// For every transaction there must be a matching call to either `rollback_transaction`
	/// or `commit_transaction`. This is also effective for all values manipulated using the
	/// `DefaultChildStorage` API.
	#[index = 309]
	fn ext_storage_start_transaction(ctx: &mut dyn FunctionContext) {
		node_start_transaction();
		Ok(())
	}

	/// Rollback the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are discarded.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	#[index = 310]
	fn ext_storage_rollback_transaction(ctx: &mut dyn FunctionContext) {
		node_rollback_transaction();
		Ok(())
	}

	/// Commit the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are committed.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	#[index = 311]
	fn ext_storage_commit_transaction(ctx: &mut dyn FunctionContext) {
		node_commit_transaction();
		Ok(())
	}
}

/// Length of the buffer the first read of a storage value of unknown length goes into. Values
/// that don't fit are fetched with a second host call.
const INITIAL_VALUE_BUFFER_LEN: usize = 256;

/// Length of the buffer the first read of a storage key of unknown length goes into. Keys that
/// don't fit are fetched with a second host call.
const INITIAL_KEY_BUFFER_LEN: usize = 128;

/// Reads a value of unknown length through a `read`-style host function.
///
/// `read` gets the output buffer, the offset into the value and the `allow_partial` flag, and
/// returns the length of the value beyond the offset, or `None` if there is no value. The first
/// call reads a partial prefix into a buffer of [`INITIAL_VALUE_BUFFER_LEN`] bytes; a value that
/// doesn't fit is completed with a second, exact call for its remainder.
fn read_value(mut read: impl FnMut(&mut [u8], u32, u32) -> Option<u32>) -> Option<Vec<u8>> {
	let mut value = alloc::vec![0u8; INITIAL_VALUE_BUFFER_LEN];
	let len = read(&mut value[..], 0, 1)? as usize;
	if len > value.len() {
		let head = value.len();
		value.resize(len, 0);
		read(&mut value[head..], head as u32, 0)?;
	}
	value.truncate(len);
	Some(value)
}

/// Reads a key of unknown length through a `next_key`-style host function.
///
/// `next_key` gets the output buffer and returns the full length of the key (`0` if there is
/// none), writing the key only if it fits the buffer. A key that doesn't fit the
/// [`INITIAL_KEY_BUFFER_LEN`] bytes buffer is fetched with a second call.
fn read_next_key(mut next_key: impl FnMut(&mut [u8]) -> u32) -> Option<Vec<u8>> {
	let mut key = alloc::vec![0u8; INITIAL_KEY_BUFFER_LEN];
	let len = next_key(&mut key[..]) as usize;
	if len == 0 {
		return None;
	}
	if len > key.len() {
		key.resize(len, 0);
		next_key(&mut key[..]);
	}
	key.truncate(len);
	Some(key)
}

/// Decodes the `u64` removal encoding of `ext_storage_clear_prefix` into a
/// [`KillStorageResult`]: `(some_remaining as u64) << 32 | loops as u64`.
fn kill_storage_result(encoded: u64) -> KillStorageResult {
	let loops = encoded as u32;
	if encoded >> 32 != 0 {
		KillStorageResult::SomeRemaining(loops)
	} else {
		KillStorageResult::AllRemoved(loops)
	}
}

/// The implementation of [`read`] on top of the `ext_storage_read` host call (runtime) or the
/// node logic directly (std).
fn read_impl(
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return option_u32_from_host(unsafe {
		ext_storage_read(
			key.as_ptr(),
			key.len() as u64,
			value_out.as_mut_ptr(),
			value_out.len() as u64,
			value_offset,
			allow_partial,
		)
	});
	#[cfg(not(substrate_runtime))]
	return node_read(key, value_out, value_offset, allow_partial);
}

/// The implementation of [`next_key`] on top of the `ext_storage_next_key` host call
/// (runtime) or the node logic directly (std).
fn next_key_impl(key: &[u8], key_out: &mut [u8]) -> u32 {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return unsafe {
		ext_storage_next_key(
			key.as_ptr(),
			key.len() as u64,
			key_out.as_mut_ptr(),
			key_out.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	return node_next_key(key, key_out);
}

/// Returns the data for `key` in the storage or `None` if the key can not be found.
pub fn get(key: &[u8]) -> Option<bytes::Bytes> {
	read_value(|value_out, value_offset, allow_partial| {
		read_impl(key, value_out, value_offset, allow_partial)
	})
	.map(Into::into)
}

/// Get `key` from storage, placing the value into `value_out` and return the number of
/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
/// doesn't exist at all.
/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
/// are copied into `value_out`.
pub fn read(key: &[u8], value_out: &mut [u8], value_offset: u32) -> Option<u32> {
	read_impl(key, value_out, value_offset, 1)
}

/// Set `key` to `value` in the storage.
pub fn set(key: &[u8], value: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_storage_set(key.as_ptr(), key.len() as u64, value.as_ptr(), value.len() as u64)
	};
	#[cfg(not(substrate_runtime))]
	node_set(key, value);
}

/// Clear the storage of the given `key` and its value.
pub fn clear(key: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	unsafe {
		ext_storage_clear(key.as_ptr(), key.len() as u64)
	};
	#[cfg(not(substrate_runtime))]
	node_clear(key);
}

/// Check whether the given `key` exists in storage.
pub fn exists(key: &[u8]) -> bool {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return unsafe { ext_storage_exists(key.as_ptr(), key.len() as u64) } != 0;
	#[cfg(not(substrate_runtime))]
	node_exists(key)
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
pub fn clear_prefix(prefix: &[u8], limit: Option<u32>) -> KillStorageResult {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return kill_storage_result(unsafe {
		ext_storage_clear_prefix(
			prefix.as_ptr(),
			prefix.len() as u64,
			super::option_u32_into_ffi(limit),
		)
	});
	#[cfg(not(substrate_runtime))]
	kill_storage_result(node_clear_prefix(prefix, limit))
}

/// Append the encoded `value` to the storage item at `key`.
///
/// The storage item needs to implement [`EncodeAppend`](codec::EncodeAppend).
///
/// # Warning
///
/// If the storage item does not support [`EncodeAppend`](codec::EncodeAppend) or
/// something else fails at appending, the storage item will be set to `[value]`.
pub fn append(key: &[u8], value: Vec<u8>) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_storage_append(key.as_ptr(), key.len() as u64, value.as_ptr(), value.len() as u64)
	};
	#[cfg(not(substrate_runtime))]
	node_append(key, value);
}

/// "Commit" all existing operations and compute the resulting storage root.
///
/// The hashing algorithm is defined by the `Block`.
///
/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
///
/// The `version` argument is ignored: the state version is a property of the execution
/// environment and is set on the externalities before the runtime call is dispatched.
pub fn root(version: StateVersion) -> Vec<u8> {
	let _ = version;
	// By this point, all the information about the length of the hash representing the storage
	// root has been erased. We're using a generous buffer here. Making host functions generic
	// over the hasher type is a big refactoring and is not worth it.
	let mut root_out = alloc::vec![0u8; 256];
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	unsafe {
		ext_storage_root(root_out.as_mut_ptr(), root_out.len() as u64)
	};
	#[cfg(not(substrate_runtime))]
	node_root(&mut root_out[..]);
	Decode::decode(&mut &root_out[..])
		.expect("storage root is always a valid SCALE-encoded Vec<u8>; qed")
}

/// Always returns `None`. This function exists for compatibility reasons.
pub fn changes_root(parent_hash: &[u8]) -> Option<Vec<u8>> {
	let _ = parent_hash;
	None
}

/// Get the next key in storage after the given one in lexicographic order.
pub fn next_key(key: &[u8]) -> Option<Vec<u8>> {
	read_next_key(|key_out| next_key_impl(key, key_out))
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
pub fn start_transaction() {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no arguments.
	unsafe {
		ext_storage_start_transaction()
	};
	#[cfg(not(substrate_runtime))]
	node_start_transaction();
}

/// Rollback the last transaction started by `start_transaction`.
///
/// Any changes made during that transaction are discarded.
///
/// # Panics
///
/// Will panic if there is no open transaction.
pub fn rollback_transaction() {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no arguments.
	unsafe {
		ext_storage_rollback_transaction()
	};
	#[cfg(not(substrate_runtime))]
	node_rollback_transaction();
}

/// Commit the last transaction started by `start_transaction`.
///
/// Any changes made during that transaction are committed.
///
/// # Panics
///
/// Will panic if there is no open transaction.
pub fn commit_transaction() {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no arguments.
	unsafe {
		ext_storage_commit_transaction()
	};
	#[cfg(not(substrate_runtime))]
	node_commit_transaction();
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	use super::*;
	use crate::jam::TestExternalities;
	use codec::Encode;
	use sp_state_machine::Backend;

	fn test_ext() -> TestExternalities {
		TestExternalities::from((sp_core::storage::Storage::default(), StateVersion::V0))
	}

	#[test]
	fn set_get_read_exists_clear_work() {
		let mut ext = test_ext();
		ext.execute_with(|| {
			set(b"key", b"hello world");
			assert!(exists(b"key"));
			assert_eq!(&get(b"key").expect("value was just set; qed")[..], b"hello world");
			assert!(!exists(b"nonexistent"));
			assert!(get(b"nonexistent").is_none());

			// Read with an offset.
			let mut out = [0u8; 5];
			assert_eq!(read(b"key", &mut out, 6), Some(5));
			assert_eq!(&out, b"world");

			// Partial read into a smaller buffer returns the full length beyond the offset.
			let mut out = [0u8; 4];
			assert_eq!(read(b"key", &mut out, 0), Some(11));
			assert_eq!(&out, b"hell");

			// Offset beyond the value returns a length of 0.
			let mut out = [0u8; 4];
			assert_eq!(read(b"key", &mut out, 100), Some(0));

			// A value longer than the initial read buffer of `get`.
			let long_value = alloc::vec![42u8; INITIAL_VALUE_BUFFER_LEN + 100];
			set(b"long", &long_value);
			assert_eq!(&get(b"long").expect("value was just set; qed")[..], &long_value[..]);

			clear(b"key");
			assert!(!exists(b"key"));
			assert!(get(b"key").is_none());
		});
	}

	#[test]
	fn append_works() {
		let mut ext = test_ext();
		ext.execute_with(|| {
			append(b"key", 1u32.encode());
			append(b"key", 2u32.encode());
			let stored = get(b"key").expect("value was just appended; qed");
			let decoded = Vec::<u32>::decode(&mut &stored[..])
				.expect("appending two u32 items yields a valid Vec<u32>; qed");
			assert_eq!(decoded, alloc::vec![1u32, 2u32]);
		});
	}

	#[test]
	fn next_key_works() {
		let mut ext = test_ext();
		// A key longer than the initial key buffer of the `next_key` wrapper.
		let long_key = alloc::vec![200u8; INITIAL_KEY_BUFFER_LEN + 100];
		ext.execute_with(|| {
			set(b"aaa", b"1");
			set(b"aab", b"2");
			set(&long_key, b"3");
		});
		ext.execute_with(|| {
			// `TestExternalities` seeds a `:code` key, which sorts before `aaa`.
			assert_eq!(next_key(b"").expect("a key exists; qed"), b":code".to_vec());
			assert_eq!(next_key(b":code").expect("a next key exists; qed"), b"aaa".to_vec());
			assert_eq!(next_key(b"aaa").expect("a next key exists; qed"), b"aab".to_vec());
			assert_eq!(next_key(b"aab").expect("a next key exists; qed"), long_key);
			assert!(next_key(&long_key).is_none());
		});
	}

	#[test]
	fn clear_prefix_works() {
		let mut ext = test_ext();
		// Insert into the backend directly: keys residing in the overlay are deleted without
		// counting towards the limit.
		ext.insert(b"ab1".to_vec(), b"1".to_vec());
		ext.insert(b"ab2".to_vec(), b"2".to_vec());
		ext.insert(b"ab3".to_vec(), b"3".to_vec());
		ext.insert(b"zzz".to_vec(), b"4".to_vec());
		ext.execute_with(|| {
			match clear_prefix(b"ab", Some(1)) {
				KillStorageResult::SomeRemaining(loops) => assert_eq!(loops, 1),
				KillStorageResult::AllRemoved(loops) => {
					panic!("expected SomeRemaining, got AllRemoved({loops})")
				},
			}
			assert!(exists(b"ab1") || exists(b"ab2") || exists(b"ab3"));
			assert!(matches!(clear_prefix(b"ab", None), KillStorageResult::AllRemoved(_)));
			assert!(!exists(b"ab1"));
			assert!(!exists(b"ab2"));
			assert!(!exists(b"ab3"));
			assert!(exists(b"zzz"));
		});
	}

	#[test]
	fn transactions_work() {
		let mut ext = test_ext();
		ext.execute_with(|| {
			start_transaction();
			set(b"rolled_back", b"value");
			rollback_transaction();
			assert!(!exists(b"rolled_back"));

			start_transaction();
			set(b"committed", b"value");
			commit_transaction();
			assert_eq!(&get(b"committed").expect("value was committed; qed")[..], b"value");
		});
	}

	#[test]
	fn root_works() {
		let mut ext = test_ext();
		let root = ext.execute_with(|| {
			set(b"key1", b"value1");
			set(b"key2", b"value2");
			root(StateVersion::V0)
		});
		ext.commit_all().expect("no open transactions; qed");
		let (expected, _) = ext.backend.storage_root(core::iter::empty(), StateVersion::V0);
		assert_eq!(root, expected.encode());
	}

	#[test]
	fn changes_root_is_always_none() {
		let mut ext = test_ext();
		ext.execute_with(|| {
			set(b"key", b"value");
			assert!(changes_root(&[42u8; 32]).is_none());
		});
	}
}

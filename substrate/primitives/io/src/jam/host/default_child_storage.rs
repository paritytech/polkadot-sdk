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

//! The `default_child_storage` host calls: child storage access for the default child trie.
//!
//! Node logic lives in private `node_*` functions that the `host_calls!` bodies and std public
//! wrappers forward to. See `jam::storage`.

use alloc::vec::Vec;

use codec::Decode;
use sp_core::storage::StateVersion;

use crate::jam::KillStorageResult;

#[cfg(not(substrate_runtime))]
use super::with_externalities;
#[cfg(not(substrate_runtime))]
use super::{option_u32_arg, option_u32_into_ffi};
#[cfg(substrate_runtime)]
use super::{option_u32_from_host, option_u32_into_ffi};
#[cfg(not(substrate_runtime))]
use sp_core::storage::ChildInfo;
#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;

/// Node logic of `ext_default_child_storage_read`.
#[cfg(not(substrate_runtime))]
fn node_read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		ext.child_storage(&child_info, key).map(|value| {
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

/// Node logic of `ext_default_child_storage_set`.
#[cfg(not(substrate_runtime))]
fn node_set(storage_key: &[u8], key: &[u8], value: &[u8]) {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		ext.set_child_storage(&child_info, key.to_vec(), value.to_vec());
	})
}

/// Node logic of `ext_default_child_storage_clear`.
#[cfg(not(substrate_runtime))]
fn node_clear(storage_key: &[u8], key: &[u8]) {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		ext.clear_child_storage(&child_info, key);
	})
}

/// Node logic of `ext_default_child_storage_storage_kill`.
#[cfg(not(substrate_runtime))]
fn node_storage_kill(storage_key: &[u8], limit: Option<u32>) -> u64 {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = ext.kill_child_storage(&child_info, limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

/// Node logic of `ext_default_child_storage_exists`.
#[cfg(not(substrate_runtime))]
fn node_exists(storage_key: &[u8], key: &[u8]) -> bool {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		ext.exists_child_storage(&child_info, key)
	})
}

/// Node logic of `ext_default_child_storage_clear_prefix`.
#[cfg(not(substrate_runtime))]
fn node_clear_prefix(storage_key: &[u8], prefix: &[u8], limit: Option<u32>) -> u64 {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = ext.clear_child_prefix(&child_info, prefix, limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

/// Node logic of `ext_default_child_storage_root`.
#[cfg(not(substrate_runtime))]
fn node_root(storage_key: &[u8], out: &mut [u8]) {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		let root = ext.child_storage_root(&child_info);
		let encoded = codec::Encode::encode(&root);
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the child storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded_len].copy_from_slice(&encoded[..]);
	})
}

/// Node logic of `ext_default_child_storage_next_key`.
#[cfg(not(substrate_runtime))]
fn node_next_key(storage_key: &[u8], key_in: &[u8], key_out: &mut [u8]) -> u32 {
	with_externalities(|ext| {
		let child_info = ChildInfo::new_default(storage_key);
		let next_key = ext.next_child_storage_key(&child_info, key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	})
}

sp_jam_runtime_interface::host_calls! {
	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	/// If `allow_partial` is non-zero, the function will copy as many bytes as possible into
	/// `value_out`, even if the value is longer than `value_out`.
	#[index = 320]
	fn ext_default_child_storage_read(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		key: *const u8,
		key_len: u64,
		value_out: *mut u8,
		value_out_len: u64,
		value_offset: u32,
		allow_partial: u32,
	) -> i64 {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let key = ctx.read_memory(key, key_len)?;
		ctx.with_buffer(value_out, value_out_len, |value_out| {
			node_read(&storage_key, &key, value_out, value_offset, allow_partial)
		})
		.map(option_u32_into_ffi)
	}

	/// Set a child storage value.
	///
	/// Set `key` to `value` in the child storage denoted by `storage_key`.
	#[index = 321]
	fn ext_default_child_storage_set(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		key: *const u8,
		key_len: u64,
		value: *const u8,
		value_len: u64,
	) {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let key = ctx.read_memory(key, key_len)?;
		let value = ctx.read_memory(value, value_len)?;
		node_set(&storage_key, &key, &value);
		Ok(())
	}

	/// Clear a child storage key.
	///
	/// For the default child storage at `storage_key`, clear value at `key`.
	#[index = 322]
	fn ext_default_child_storage_clear(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		key: *const u8,
		key_len: u64,
	) {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let key = ctx.read_memory(key, key_len)?;
		node_clear(&storage_key, &key);
		Ok(())
	}

	/// Clear an entire child storage.
	///
	/// If it exists, the child storage for `storage_key` is removed, up to `limit` backend
	/// keys when `limit` is `Some`. There is no continuation cursor: the returned `u64`
	/// encodes `(some_remaining as u64) << 32 | loops as u64`.
	#[index = 323]
	fn ext_default_child_storage_storage_kill(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		limit: i64,
	) -> u64 {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		Ok(node_storage_kill(&storage_key, option_u32_arg(limit)?))
	}

	/// Check a child storage key.
	///
	/// Check whether the given `key` exists in default child defined at `storage_key`.
	#[index = 324]
	fn ext_default_child_storage_exists(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		key: *const u8,
		key_len: u64,
	) -> u32 {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let key = ctx.read_memory(key, key_len)?;
		Ok(node_exists(&storage_key, &key) as u32)
	}

	/// Clear the child storage of each key-value pair where the key starts with the given
	/// `prefix`.
	///
	/// Deletes all keys from the overlay and up to `limit` keys from the backend if it is set
	/// to `Some`. No limit is applied when `limit` is set to `None`. There is no continuation
	/// cursor: the returned `u64` encodes `(some_remaining as u64) << 32 | loops as u64`.
	#[index = 325]
	fn ext_default_child_storage_clear_prefix(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		prefix: *const u8,
		prefix_len: u64,
		limit: i64,
	) -> u64 {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let prefix = ctx.read_memory(prefix, prefix_len)?;
		Ok(node_clear_prefix(&storage_key, &prefix, option_u32_arg(limit)?))
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash. Since the size of the
	/// resulting value is known to the caller, this function requires the provided buffer to
	/// be large enough to store the entire value; otherwise, it will panic.
	#[index = 326]
	fn ext_default_child_storage_root(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		out: *mut u8,
		out_len: u64,
	) {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		ctx.with_buffer(out, out_len, |out| node_root(&storage_key, out))
	}

	/// Child storage key iteration.
	///
	/// Get the next key in storage after the given one in lexicographic order in child storage.
	/// Writes the next key into `key_out` if it fits and returns the full length of the key;
	/// `0` means there is no next key.
	#[index = 327]
	fn ext_default_child_storage_next_key(
		ctx: &mut dyn FunctionContext,
		storage_key: *const u8,
		storage_key_len: u64,
		key_in: *const u8,
		key_in_len: u64,
		key_out: *mut u8,
		key_out_len: u64,
	) -> u32 {
		let storage_key = ctx.read_memory(storage_key, storage_key_len)?;
		let key_in = ctx.read_memory(key_in, key_in_len)?;
		ctx.with_buffer(key_out, key_out_len, |key_out| {
			node_next_key(&storage_key, &key_in, key_out)
		})
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

/// Decodes the `u64` removal encoding shared by `ext_default_child_storage_storage_kill` and
/// `ext_default_child_storage_clear_prefix` into a [`KillStorageResult`]:
/// `(some_remaining as u64) << 32 | loops as u64`.
fn kill_storage_result(encoded: u64) -> KillStorageResult {
	let loops = encoded as u32;
	if encoded >> 32 != 0 {
		KillStorageResult::SomeRemaining(loops)
	} else {
		KillStorageResult::AllRemoved(loops)
	}
}

/// The implementation of [`read`] on top of the `ext_default_child_storage_read` host call
/// (runtime) or the node logic directly (std).
fn read_impl(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return option_u32_from_host(unsafe {
		ext_default_child_storage_read(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			key.as_ptr(),
			key.len() as u64,
			value_out.as_mut_ptr(),
			value_out.len() as u64,
			value_offset,
			allow_partial,
		)
	});
	#[cfg(not(substrate_runtime))]
	return node_read(storage_key, key, value_out, value_offset, allow_partial);
}

/// The implementation of [`next_key`] on top of the `ext_default_child_storage_next_key` host
/// call (runtime) or the node logic directly (std).
fn next_key_impl(storage_key: &[u8], key: &[u8], key_out: &mut [u8]) -> u32 {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return unsafe {
		ext_default_child_storage_next_key(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			key.as_ptr(),
			key.len() as u64,
			key_out.as_mut_ptr(),
			key_out.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	return node_next_key(storage_key, key, key_out);
}

/// Get a default child storage value for a given key.
///
/// Parameter `storage_key` is the unprefixed location of the root of the child trie in the
/// parent trie. Result is `None` if the value for `key` in the child storage can not be found.
pub fn get(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>> {
	read_value(|value_out, value_offset, allow_partial| {
		read_impl(storage_key, key, value_out, value_offset, allow_partial)
	})
}

/// Allocation efficient variant of `get`.
///
/// Get `key` from child storage, placing the value into `value_out` and return the number
/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
/// doesn't exist at all.
/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
/// are copied into `value_out`.
pub fn read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
) -> Option<u32> {
	read_impl(storage_key, key, value_out, value_offset, 1)
}

/// Set a child storage value.
///
/// Set `key` to `value` in the child storage denoted by `storage_key`.
pub fn set(storage_key: &[u8], key: &[u8], value: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_default_child_storage_set(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			key.as_ptr(),
			key.len() as u64,
			value.as_ptr(),
			value.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	node_set(storage_key, key, value);
}

/// Clear a child storage key.
///
/// For the default child storage at `storage_key`, clear value at `key`.
pub fn clear(storage_key: &[u8], key: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_default_child_storage_clear(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			key.as_ptr(),
			key.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	node_clear(storage_key, key);
}

/// Clear an entire child storage.
///
/// If it exists, the child storage for `storage_key` is removed.
///
/// See `storage::clear_prefix` documentation for `limit` usage.
pub fn storage_kill(storage_key: &[u8], limit: Option<u32>) -> KillStorageResult {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return kill_storage_result(unsafe {
		ext_default_child_storage_storage_kill(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			option_u32_into_ffi(limit),
		)
	});
	#[cfg(not(substrate_runtime))]
	kill_storage_result(node_storage_kill(storage_key, limit))
}

/// Check a child storage key.
///
/// Check whether the given `key` exists in default child defined at `storage_key`.
pub fn exists(storage_key: &[u8], key: &[u8]) -> bool {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return unsafe {
		ext_default_child_storage_exists(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			key.as_ptr(),
			key.len() as u64,
		)
	} != 0;
	#[cfg(not(substrate_runtime))]
	node_exists(storage_key, key)
}

/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
///
/// See `storage::clear_prefix` documentation for `limit` usage.
pub fn clear_prefix(storage_key: &[u8], prefix: &[u8], limit: Option<u32>) -> KillStorageResult {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return kill_storage_result(unsafe {
		ext_default_child_storage_clear_prefix(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			prefix.as_ptr(),
			prefix.len() as u64,
			option_u32_into_ffi(limit),
		)
	});
	#[cfg(not(substrate_runtime))]
	kill_storage_result(node_clear_prefix(storage_key, prefix, limit))
}

/// Default child root calculation.
///
/// "Commit" all existing operations and compute the resulting child storage root.
/// The hashing algorithm is defined by the `Block`.
///
/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
///
/// The `version` argument is ignored: the state version is a property of the execution
/// environment and is set on the externalities before the runtime call is dispatched.
pub fn root(storage_key: &[u8], version: StateVersion) -> Vec<u8> {
	let _ = version;
	// By this point, all the information about the length of the hash representing the child
	// storage root has been erased. We're using a generous buffer here. Making host functions
	// generic over the hasher type is a big refactoring and is not worth it.
	let mut root_out = alloc::vec![0u8; 256];
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_default_child_storage_root(
			storage_key.as_ptr(),
			storage_key.len() as u64,
			root_out.as_mut_ptr(),
			root_out.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	node_root(storage_key, &mut root_out[..]);
	Decode::decode(&mut &root_out[..])
		.expect("child storage root is always a valid SCALE-encoded Vec<u8>; qed")
}

/// Child storage key iteration.
///
/// Get the next key in storage after the given one in lexicographic order in child storage.
pub fn next_key(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>> {
	read_next_key(|key_out| next_key_impl(storage_key, key, key_out))
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	use super::*;
	use crate::jam::TestExternalities;
	use codec::Encode;
	use sp_state_machine::Backend;

	const CHILD: &[u8] = b"child";

	fn test_ext() -> TestExternalities {
		TestExternalities::from((sp_core::storage::Storage::default(), StateVersion::V0))
	}

	#[test]
	fn set_get_read_exists_clear_work() {
		let mut ext = test_ext();
		ext.execute_with(|| {
			set(CHILD, b"key", b"hello world");
			assert!(exists(CHILD, b"key"));
			assert_eq!(
				get(CHILD, b"key").expect("value was just set; qed"),
				b"hello world".to_vec()
			);
			assert!(!exists(CHILD, b"nonexistent"));
			assert!(get(CHILD, b"nonexistent").is_none());

			// Read with an offset.
			let mut out = [0u8; 5];
			assert_eq!(read(CHILD, b"key", &mut out, 6), Some(5));
			assert_eq!(&out, b"world");

			// Partial read into a smaller buffer returns the full length beyond the offset.
			let mut out = [0u8; 4];
			assert_eq!(read(CHILD, b"key", &mut out, 0), Some(11));
			assert_eq!(&out, b"hell");

			// A value longer than the initial read buffer of `get`.
			let long_value = alloc::vec![42u8; INITIAL_VALUE_BUFFER_LEN + 100];
			set(CHILD, b"long", &long_value);
			assert_eq!(get(CHILD, b"long").expect("value was just set; qed"), long_value);

			clear(CHILD, b"key");
			assert!(!exists(CHILD, b"key"));
			assert!(get(CHILD, b"key").is_none());
		});
	}

	#[test]
	fn next_key_works() {
		let mut ext = test_ext();
		// A key longer than the initial key buffer of the `next_key` wrapper.
		let long_key = alloc::vec![200u8; INITIAL_KEY_BUFFER_LEN + 100];
		ext.execute_with(|| {
			set(CHILD, b"aaa", b"1");
			set(CHILD, b"aab", b"2");
			set(CHILD, &long_key, b"3");
		});
		ext.execute_with(|| {
			assert_eq!(next_key(CHILD, b"").expect("a key exists; qed"), b"aaa".to_vec());
			assert_eq!(next_key(CHILD, b"aaa").expect("a next key exists; qed"), b"aab".to_vec());
			assert_eq!(next_key(CHILD, b"aab").expect("a next key exists; qed"), long_key);
			assert!(next_key(CHILD, &long_key).is_none());
		});
	}

	#[test]
	fn clear_prefix_works() {
		let mut ext = test_ext();
		let child_info = ChildInfo::new_default(CHILD);
		// Insert into the backend directly: keys residing in the overlay are deleted without
		// counting towards the limit.
		ext.insert_child(child_info.clone(), b"ab1".to_vec(), b"1".to_vec());
		ext.insert_child(child_info.clone(), b"ab2".to_vec(), b"2".to_vec());
		ext.insert_child(child_info.clone(), b"ab3".to_vec(), b"3".to_vec());
		ext.insert_child(child_info.clone(), b"zzz".to_vec(), b"4".to_vec());
		ext.execute_with(|| {
			match clear_prefix(CHILD, b"ab", Some(1)) {
				KillStorageResult::SomeRemaining(loops) => assert_eq!(loops, 1),
				KillStorageResult::AllRemoved(loops) => {
					panic!("expected SomeRemaining, got AllRemoved({loops})")
				},
			}
			assert!(exists(CHILD, b"ab1") || exists(CHILD, b"ab2") || exists(CHILD, b"ab3"));
			assert!(matches!(clear_prefix(CHILD, b"ab", None), KillStorageResult::AllRemoved(_)));
			assert!(!exists(CHILD, b"ab1"));
			assert!(!exists(CHILD, b"ab2"));
			assert!(!exists(CHILD, b"ab3"));
			assert!(exists(CHILD, b"zzz"));
		});
	}

	#[test]
	fn storage_kill_works() {
		let mut ext = test_ext();
		let child_info = ChildInfo::new_default(CHILD);
		ext.insert_child(child_info.clone(), b"a".to_vec(), b"1".to_vec());
		ext.insert_child(child_info.clone(), b"b".to_vec(), b"2".to_vec());
		ext.insert_child(child_info.clone(), b"c".to_vec(), b"3".to_vec());
		ext.execute_with(|| {
			// Limited kill leaves keys behind.
			assert!(matches!(storage_kill(CHILD, Some(1)), KillStorageResult::SomeRemaining(_)));
			// Killing the rest reports completion.
			assert!(matches!(storage_kill(CHILD, None), KillStorageResult::AllRemoved(_)));
			assert!(!exists(CHILD, b"a"));
			assert!(!exists(CHILD, b"b"));
			assert!(!exists(CHILD, b"c"));
			// Killing an already-empty child trie completes immediately.
			assert!(matches!(storage_kill(CHILD, None), KillStorageResult::AllRemoved(_)));
		});
	}

	#[test]
	fn root_works() {
		let mut ext = test_ext();
		let child_info = ChildInfo::new_default(CHILD);
		let root = ext.execute_with(|| {
			set(CHILD, b"key1", b"value1");
			set(CHILD, b"key2", b"value2");
			root(CHILD, StateVersion::V0)
		});
		ext.commit_all().expect("no open transactions; qed");
		let (expected, _, _) =
			ext.backend
				.child_storage_root(&child_info, core::iter::empty(), StateVersion::V0);
		assert_eq!(root, expected.encode());
	}
}

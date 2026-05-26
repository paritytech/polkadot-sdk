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

//! Operation on unhashed runtime storage.

use alloc::vec::Vec;
use codec::{Decode, Encode};

pub use sp_io::{MultiRemovalCounters, MultiRemovalCursor, MultiRemovalResults};

/// Return the value of the item in storage under `key`, or `None` if there is no explicit entry.
pub fn get<T: Decode + Sized>(key: &[u8]) -> Option<T> {
	sp_io::storage::get(key).and_then(|val| {
		Decode::decode(&mut &val[..]).map(Some).unwrap_or_else(|e| {
			// TODO #3700: error should be handleable.
			log::error!(
				target: "runtime::storage",
				"Corrupted state at `{}`: {:?}",
				array_bytes::bytes2hex("0x", key),
				e,
			);
			None
		})
	})
}

/// Return the value of the item in storage under `key`, or the type's default if there is no
/// explicit entry.
pub fn get_or_default<T: Decode + Sized + Default>(key: &[u8]) -> T {
	get(key).unwrap_or_default()
}

/// Return the value of the item in storage under `key`, or `default_value` if there is no
/// explicit entry.
pub fn get_or<T: Decode + Sized>(key: &[u8], default_value: T) -> T {
	get(key).unwrap_or(default_value)
}

/// Return the value of the item in storage under `key`, or `default_value()` if there is no
/// explicit entry.
pub fn get_or_else<T: Decode + Sized, F: FnOnce() -> T>(key: &[u8], default_value: F) -> T {
	get(key).unwrap_or_else(default_value)
}

/// Put `value` in storage under `key`.
pub fn put<T: Encode + ?Sized>(key: &[u8], value: &T) {
	value.using_encoded(|slice| sp_io::storage::set(key, slice));
}

/// Remove `key` from storage, returning its value if it had an explicit entry or `None` otherwise.
pub fn take<T: Decode + Sized>(key: &[u8]) -> Option<T> {
	let r = get(key);
	if r.is_some() {
		kill(key);
	}
	r
}

/// Remove `key` from storage, returning its value, or, if there was no explicit entry in storage,
/// the default for its type.
pub fn take_or_default<T: Decode + Sized + Default>(key: &[u8]) -> T {
	take(key).unwrap_or_default()
}

/// Return the value of the item in storage under `key`, or `default_value` if there is no
/// explicit entry. Ensure there is no explicit entry on return.
pub fn take_or<T: Decode + Sized>(key: &[u8], default_value: T) -> T {
	take(key).unwrap_or(default_value)
}

/// Return the value of the item in storage under `key`, or `default_value()` if there is no
/// explicit entry. Ensure there is no explicit entry on return.
pub fn take_or_else<T: Decode + Sized, F: FnOnce() -> T>(key: &[u8], default_value: F) -> T {
	take(key).unwrap_or_else(default_value)
}

/// Check to see if `key` has an explicit entry in storage.
pub fn exists(key: &[u8]) -> bool {
	sp_io::storage::exists(key)
}

/// Ensure `key` has no explicit entry in storage.
pub fn kill(key: &[u8]) {
	sp_io::storage::clear(key);
}

/// Partially clear the storage of all keys under a common `prefix`.
///
/// # Limit
///
/// A *limit* should always be provided through `maybe_limit`. This is one fewer than the
/// maximum number of backend iterations which may be done by this operation and as such
/// represents the maximum number of backend deletions which may happen. A *limit* of zero
/// implies that no keys will be deleted, though there may be a single iteration done.
///
/// The limit can be used to partially delete storage items in case it is too large or costly
/// to delete all in a single operation.
///
/// # Cursor
///
/// To continue the operation across multiple calls, the caller owns a
/// [`MultiRemovalCursor`](sp_io::MultiRemovalCursor) and passes `Some(&mut cursor)`: it should be
/// freshly [created](sp_io::MultiRemovalCursor::new) for the initial call and then passed back in
/// unchanged on subsequent calls until
/// [`MultiRemovalCounters::more`](sp_io::MultiRemovalCounters::more) is `false`. Reusing the same
/// cursor object across iterations avoids re-allocating the cursor buffer on every call.
///
/// Pass `None` when you don't need to continue: no cursor is materialized (no allocation), and
/// [`MultiRemovalCounters::more`](sp_io::MultiRemovalCounters::more) still reports whether keys
/// remain.
///
/// Returns [`MultiRemovalCounters`](sp_io::MultiRemovalCounters) to inform about the result. Once
/// `more` is `false`, no further items remain to be deleted.
///
/// NOTE: After the initial call for any given prefix, it is important that no further keys are
/// inserted. If so, then they may or may not be deleted by subsequent calls.
///
/// # Note
///
/// Please note that keys which are residing in the overlay for that prefix are deleted without
/// counting towards the `limit`.
pub fn clear_prefix(
	prefix: &[u8],
	maybe_limit: Option<u32>,
	cursor: Option<&mut sp_io::MultiRemovalCursor>,
) -> sp_io::MultiRemovalCounters {
	sp_io::storage::clear_prefix(prefix, maybe_limit, cursor)
}

/// Returns `true` if the storage contains any key, which starts with a certain prefix,
/// and is longer than said prefix.
/// This means that a key which equals the prefix will not be counted.
pub fn contains_prefixed_key(prefix: &[u8]) -> bool {
	let mut key = Vec::new();
	sp_io::storage::next_key(prefix, &mut key) && key.starts_with(prefix)
}

/// Get a Vec of bytes from storage.
pub fn get_raw(key: &[u8]) -> Option<Vec<u8>> {
	sp_io::storage::get(key).map(|value| value.to_vec())
}

/// Put a raw byte slice into storage.
pub fn put_raw(key: &[u8], value: &[u8]) {
	sp_io::storage::set(key, value)
}

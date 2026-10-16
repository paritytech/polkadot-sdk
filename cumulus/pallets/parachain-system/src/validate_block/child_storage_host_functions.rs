// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Host-function replacement implementations for default child storage operations.

#[cfg(jam)]
use codec::Encode;
use sp_core::storage::ChildInfo;

use super::host_functions::with_externalities;

// As in `host_functions`, each `#[cfg(jam)]` function whose shape differs on the relay-chain
// `#[runtime_interface]` has a `#[cfg(not(jam))]` twin of the same name for wasm runtimes.

#[cfg(not(jam))]
pub(super) fn host_default_child_storage_read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
) -> Option<u32> {
	let child_info = ChildInfo::new_default(storage_key);
	match with_externalities(|ext| ext.child_storage(&child_info, key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			Some(value.len() as u32)
		},
		None => None,
	}
}

#[cfg(not(jam))]
pub(super) fn host_default_child_storage_storage_kill(
	storage_key: &[u8],
	limit: Option<u32>,
) -> sp_io::KillStorageResult {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.kill_child_storage(&child_info, limit, None).into())
}

#[cfg(not(jam))]
pub(super) fn host_default_child_storage_clear_prefix(
	storage_key: &[u8],
	prefix: &[u8],
	limit: Option<u32>,
) -> sp_io::KillStorageResult {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.clear_child_prefix(&child_info, prefix, limit, None).into())
}

#[cfg(not(jam))]
pub(super) fn host_default_child_storage_root(
	storage_key: &[u8],
	version: sp_core::storage::StateVersion,
) -> alloc::vec::Vec<u8> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.child_storage_root_with_version(&child_info, version))
}

#[cfg(not(jam))]
pub(super) fn host_default_child_storage_next_key(
	storage_key: &[u8],
	key: &[u8],
) -> Option<alloc::vec::Vec<u8>> {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.next_child_storage_key(&child_info, key))
}

#[cfg(jam)]
pub(super) fn host_default_child_storage_read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	let child_info = ChildInfo::new_default(storage_key);
	match with_externalities(|ext| ext.child_storage(&child_info, key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let out_len = core::cmp::min(data.len(), value_out.len());
			if value_out.len() >= data.len() || allow_partial != 0 {
				value_out[..out_len].copy_from_slice(&data[..out_len]);
			}
			Some(data.len() as u32)
		},
		None => None,
	}
}

pub(super) fn host_default_child_storage_set(storage_key: &[u8], key: &[u8], value: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		ext.place_child_storage(&child_info, key.to_vec(), Some(value.to_vec()))
	})
}

pub(super) fn host_default_child_storage_clear(storage_key: &[u8], key: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.place_child_storage(&child_info, key.to_vec(), None))
}

/// Clear an entire child storage in the PoV-backed overlay.
///
/// Returns the `u64` encoding of [`sp_io::KillStorageResult`]: `(some_remaining as u64) << 32 |
/// loops as u64`. The JAM host-call set has no continuation cursor, so the cursor the backend
/// reports is consumed here rather than handed back to the runtime.
#[cfg(jam)]
pub(super) fn host_default_child_storage_storage_kill(
	storage_key: &[u8],
	maybe_limit: Option<u32>,
) -> u64 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let removal_results = ext.kill_child_storage(&child_info, maybe_limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

pub(super) fn host_default_child_storage_exists(storage_key: &[u8], key: &[u8]) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

/// Clear every key-value pair under `prefix` of a child storage in the PoV-backed overlay.
///
/// Returns the same `u64` encoding as [`host_default_child_storage_storage_kill`].
#[cfg(jam)]
pub(super) fn host_default_child_storage_clear_prefix(
	storage_key: &[u8],
	prefix: &[u8],
	maybe_limit: Option<u32>,
) -> u64 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let removal_results = ext.clear_child_prefix(&child_info, prefix, maybe_limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

#[cfg(jam)]
pub(super) fn host_default_child_storage_root(storage_key: &[u8], out: &mut [u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let root = ext.child_storage_root(&child_info);
		let encoded = root.encode();
		let write_len = encoded.len().min(out.len());
		out[..write_len].copy_from_slice(&encoded[..write_len]);
	})
}

#[cfg(jam)]
pub(super) fn host_default_child_storage_next_key(
	storage_key: &[u8],
	key_in: &[u8],
	key_out: &mut [u8],
) -> u32 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let next_key = ext.next_child_storage_key(&child_info, key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			let write_len = next_key.len().min(key_out.len());
			key_out[..write_len].copy_from_slice(&next_key[..write_len]);
		}
		next_key_len as u32
	})
}

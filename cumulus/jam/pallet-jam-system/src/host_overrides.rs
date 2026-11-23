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

//! The storage host functions whose shape is specific to the JAM flat host-call layer, plus the
//! JAM-only host functions served during validation.
//!
//! The validation core of `cumulus-pallet-parachain-system` replaces the storage host functions
//! whose shape every target shares; `jam_validate_block` installs these before it calls the core.

use alloc::vec::Vec;
use codec::Encode;
use sp_core::storage::ChildInfo;
use sp_externalities::Externalities;

/// Swap the JAM-shaped and the JAM-only host functions for the implementations in this module.
///
/// The returned guard restores the originals when dropped, so the caller must keep it alive for
/// the whole validation run.
#[must_use]
pub fn install_overrides() -> impl Sized {
	(
		sp_io::storage::host_read.replace_implementation(host_storage_read),
		sp_io::storage::host_get.replace_implementation(host_storage_get),
		sp_io::storage::host_root.replace_implementation(host_storage_root),
		sp_io::storage::host_clear_prefix.replace_implementation(host_storage_clear_prefix),
		sp_io::storage::host_next_key.replace_implementation(host_storage_next_key),
		sp_io::default_child_storage::host_read
			.replace_implementation(host_default_child_storage_read),
		sp_io::default_child_storage::host_storage_kill
			.replace_implementation(host_default_child_storage_storage_kill),
		sp_io::default_child_storage::host_clear_prefix
			.replace_implementation(host_default_child_storage_clear_prefix),
		sp_io::default_child_storage::host_root
			.replace_implementation(host_default_child_storage_root),
		sp_io::default_child_storage::host_next_key
			.replace_implementation(host_default_child_storage_next_key),
		// The riscv runtime reads JAM state via `jam_state_read`, served from the JAM state proof
		// carried in the PoV under `JAM_PROOF_KEY`, verified against the trusted anchor root.
		cumulus_jam_state_reader::jam_state::host_jam_state_read_into
			.replace_implementation(host_jam_state_read_into),
		// Upward messages go straight to the Parachain Service.
		cumulus_jam_state_reader::jam_upward::host_send_upward_message
			.replace_implementation(parachain_service_core::host::send_upward_message),
	)
}

/// The proof-backed JAM state reader threaded into the replaced `jam_state_read` host function
/// for the duration of block execution. Own module for the same reason as the validation core's
/// `host_functions::additional_data`: `environmental!` emits a scope-level `GLOBAL` static.
pub(crate) mod jam_data {
	use cumulus_jam_state_reader::JamStateReader;

	environmental::environmental!(env: trait JamStateReader);
	pub fn using<R, F: FnOnce() -> R>(t: &mut dyn JamStateReader, f: F) -> R {
		env::using(t, f)
	}
	pub fn with<R, F: for<'a> FnOnce(&'a mut (dyn JamStateReader + 'a)) -> R>(f: F) -> Option<R> {
		env::with(f)
	}
}

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

fn host_storage_read(
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	match with_externalities(|ext| ext.storage(key)) {
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

/// The value straight from the externalities, instead of copied into a scratch buffer by
/// [`host_storage_read`].
fn host_storage_get(key: &[u8]) -> Option<Vec<u8>> {
	with_externalities(|ext| ext.storage(key))
}

fn host_storage_root(out: &mut [u8]) {
	with_externalities(|ext| {
		let root = ext.storage_root();
		let encoded = root.encode();
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded_len].copy_from_slice(&encoded[..]);
	})
}

/// Clear every key-value pair under `prefix` in the PoV-backed overlay.
///
/// Returns the `u64` encoding of [`sp_io::KillStorageResult`]: `(some_remaining as u64) << 32 |
/// loops as u64`. The JAM host-call set has no continuation cursor, so the cursor the backend
/// reports is consumed here rather than handed back to the runtime.
fn host_storage_clear_prefix(prefix: &[u8], maybe_limit: Option<u32>) -> u64 {
	with_externalities(|ext| {
		let removal_results = ext.clear_prefix(prefix, maybe_limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

fn host_storage_next_key(key_in: &[u8], key_out: &mut [u8]) -> u32 {
	with_externalities(|ext| {
		let next_key = ext.next_storage_key(key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			let write_len = next_key.len().min(key_out.len());
			key_out[..write_len].copy_from_slice(&next_key[..write_len]);
		}
		next_key_len as u32
	})
}

fn host_default_child_storage_read(
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

/// Clear an entire child storage in the PoV-backed overlay.
///
/// Returns the `u64` encoding of [`sp_io::KillStorageResult`]: `(some_remaining as u64) << 32 |
/// loops as u64`. The JAM host-call set has no continuation cursor, so the cursor the backend
/// reports is consumed here rather than handed back to the runtime.
fn host_default_child_storage_storage_kill(storage_key: &[u8], maybe_limit: Option<u32>) -> u64 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let removal_results = ext.kill_child_storage(&child_info, maybe_limit, None);
		((removal_results.maybe_cursor.is_some() as u64) << 32) | removal_results.loops as u64
	})
}

/// Clear every key-value pair under `prefix` of a child storage in the PoV-backed overlay.
///
/// Returns the same `u64` encoding as [`host_default_child_storage_storage_kill`].
fn host_default_child_storage_clear_prefix(
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

fn host_default_child_storage_root(storage_key: &[u8], out: &mut [u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let root = ext.child_storage_root(&child_info);
		let encoded = root.encode();
		let write_len = encoded.len().min(out.len());
		out[..write_len].copy_from_slice(&encoded[..write_len]);
	})
}

fn host_default_child_storage_next_key(
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

fn host_jam_state_read_into(key: &[u8], value_out: &mut [u8]) -> i64 {
	// Served by the verifying proof-backed reader set up around block execution; if none is set (a
	// block with no JAM reads), reports the key as absent. A reader whose proof cannot
	// authenticate the key panics inside `read` (task 7's semantic), never reports absence.
	let key: &parachain_service_core::StateKey =
		key.try_into().expect("a JAM state key is 31 bytes; qed");
	match jam_data::with(|p| p.read(key)).flatten() {
		Some(v) => {
			let n = core::cmp::min(v.len(), value_out.len());
			value_out[..n].copy_from_slice(&v[..n]);
			v.len() as i64
		},
		None => -1,
	}
}

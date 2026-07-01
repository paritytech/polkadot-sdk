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

//! Raw-bytes storage for uploaded contract code, keyed by code hash.
//!
//! The value at each key is the bytecode blob exactly as uploaded — no SCALE
//! wrapper. The storage trie already delimits each value, so an in-band length
//! prefix would only duplicate information already carried by the trie node,
//! and would corrupt the leading `BLOB_MAGIC` bytes the JIT compiler expects at the start of
//! the value. Mirrors the convention used by Substrate's runtime wasm storage (`:code`).

use alloc::vec::Vec;
use frame_support::{storage::unhashed, traits::PalletInfo};
use sp_core::H256;

use crate::{Config, Pallet};

/// Trie key: `twox_128(pallet_name) ++ twox_128("PristineCode") ++ hash`.
///
/// `Identity`-style key composition (the code hash is appended verbatim);
/// equivalent to what `#[pallet::storage] StorageMap<_, Identity, H256, _>`
/// would have produced.
fn key<T: Config>(hash: &H256) -> [u8; 64] {
	let name = <T::PalletInfo as PalletInfo>::name::<Pallet<T>>()
		.expect("pallet revive is included in the runtime; qed");
	let prefix = frame_support::storage::storage_prefix(name.as_bytes(), b"PristineCode");
	let mut out = [0u8; 64];
	out[..32].copy_from_slice(&prefix);
	out[32..].copy_from_slice(hash.as_bytes());
	out
}

/// Read the raw bytecode blob for `hash`, or `None` if no entry exists.
pub fn get<T: Config>(hash: &H256) -> Option<Vec<u8>> {
	unhashed::get_raw(&key::<T>(hash))
}

/// Store `bytes` as the bytecode blob for `hash`, replacing any existing entry.
pub fn insert<T: Config>(hash: &H256, bytes: &[u8]) {
	unhashed::put_raw(&key::<T>(hash), bytes);
}

/// Remove the entry for `hash`, if any.
pub fn kill<T: Config>(hash: &H256) {
	unhashed::kill(&key::<T>(hash));
}

/// Whether an entry exists for `hash`.
///
/// Currently only consumed from `call_builder`'s containment helpers,
/// which are themselves gated to the bench / test cfgs.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub fn exists<T: Config>(hash: &H256) -> bool {
	unhashed::exists(&key::<T>(hash))
}

/// The trie key under which the blob for `hash` is stored.
///
/// Exposed so the JIT path can reuse it as the stable per-block cache identifier when compiling
/// the loaded bytes via `Module::from_bytes`, keeping the module cache keyed consistently.
#[cfg(any(revive_jit, feature = "runtime-benchmarks"))]
pub fn storage_key<T: Config>(hash: &H256) -> [u8; 64] {
	key::<T>(hash)
}

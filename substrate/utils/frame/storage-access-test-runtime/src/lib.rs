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

//! Test runtime to benchmark storage access on block validation

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use sp_core::storage::ChildInfo;
use sp_runtime::traits;
use sp_trie::StorageProof;

#[cfg(all(not(feature = "std"), feature = "runtime-benchmarks"))]
use {
	cumulus_pallet_parachain_system::validate_block::{
		trie_cache::CacheProvider, trie_recorder::SizeOnlyRecorderProvider,
	},
	sp_core::storage::StateVersion,
	sp_runtime::{generic, OpaqueExtrinsic},
	sp_state_machine::{Backend, TrieBackendBuilder},
};

// Include the WASM binary
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Parameters for benchmarking storage access on block validation.
///
/// On dry-run, the storage access is not performed to measure the cost of the runtime call.
#[derive(Decode, Clone)]
#[cfg_attr(feature = "std", derive(Encode))]
pub struct StorageAccessParams<B: traits::Block> {
	pub state_root: B::Hash,
	pub storage_proof: StorageProof,
	pub payload: StorageAccessPayload,
	/// On dry-run, we don't read/write to the storage.
	pub is_dry_run: bool,
}

/// Payload for benchmarking read and write operations on block validation.
#[derive(Debug, Clone, Decode, Encode)]
pub enum StorageAccessPayload {
	// Storage keys with optional child info.
	Read(Vec<(Vec<u8>, Option<ChildInfo>)>),
	// Storage key-value pairs with optional child info.
	Write((Vec<(Vec<u8>, Vec<u8>)>, Option<ChildInfo>)),
}

impl<B: traits::Block> StorageAccessParams<B> {
	/// Create a new params for reading from the storage.
	pub fn new_read(
		state_root: B::Hash,
		storage_proof: StorageProof,
		payload: Vec<(Vec<u8>, Option<ChildInfo>)>,
	) -> Self {
		Self {
			state_root,
			storage_proof,
			payload: StorageAccessPayload::Read(payload),
			is_dry_run: false,
		}
	}

	/// Create a new params for writing to the storage.
	pub fn new_write(
		state_root: B::Hash,
		storage_proof: StorageProof,
		payload: (Vec<(Vec<u8>, Vec<u8>)>, Option<ChildInfo>),
	) -> Self {
		Self {
			state_root,
			storage_proof,
			payload: StorageAccessPayload::Write(payload),
			is_dry_run: false,
		}
	}

	/// Create a dry-run version of the params.
	pub fn as_dry_run(&self) -> Self {
		Self {
			state_root: self.state_root,
			storage_proof: self.storage_proof.clone(),
			payload: self.payload.clone(),
			is_dry_run: true,
		}
	}
}

/// Imitates `cumulus_pallet_parachain_system::validate_block::implementation::validate_block`
///
/// Only performs the storage access, this is used to benchmark the storage access cost.
#[doc(hidden)]
#[cfg(all(not(feature = "std"), feature = "runtime-benchmarks"))]
pub fn proceed_storage_access<B: traits::Block>(mut params: &[u8]) {
	let StorageAccessParams { state_root, storage_proof, payload, is_dry_run } =
		StorageAccessParams::<B>::decode(&mut params)
			.expect("Invalid arguments to `validate_block`.");

	let db = storage_proof.into_memory_db();
	let recorder = SizeOnlyRecorderProvider::<traits::HashingFor<B>>::default();
	let cache_provider = CacheProvider::new();
	let backend = TrieBackendBuilder::new_with_cache(db, state_root, cache_provider)
		.with_recorder(recorder)
		.build();

	if is_dry_run {
		return;
	}

	match payload {
		StorageAccessPayload::Read(keys) =>
			for (key, maybe_child_info) in keys {
				match maybe_child_info {
					Some(child_info) => {
						let _ = backend
							.child_storage(&child_info, key.as_ref())
							.expect("Key not found")
							.ok_or("Value unexpectedly empty");
					},
					None => {
						let _ = backend
							.storage(key.as_ref())
							.expect("Key not found")
							.ok_or("Value unexpectedly empty");
					},
				}
			},
		StorageAccessPayload::Write((changes, maybe_child_info)) => {
			let delta = changes.iter().map(|(key, value)| (key.as_ref(), Some(value.as_ref())));
			match maybe_child_info {
				Some(child_info) => {
					backend.child_storage_root(&child_info, delta, StateVersion::V1);
				},
				None => {
					backend.storage_root(delta, StateVersion::V1);
				},
			}
		},
	}
}

/// Wasm binary unwrapped. If built with `SKIP_WASM_BUILD`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_unwrap() -> &'static [u8] {
	WASM_BINARY.expect(
		"Development wasm binary is not available. Unset SKIP_WASM_BUILD and compile the runtime again.",
	)
}

#[cfg(enable_alloc_error_handler)]
#[alloc_error_handler]
#[no_mangle]
pub fn oom(_: core::alloc::Layout) -> ! {
	core::intrinsics::abort();
}

#[cfg(all(not(feature = "std"), feature = "runtime-benchmarks"))]
#[no_mangle]
pub extern "C" fn validate_block(params: *const u8, len: usize) -> u64 {
	type Block = generic::Block<generic::Header<u32, traits::BlakeTwo256>, OpaqueExtrinsic>;
	let params = unsafe { alloc::slice::from_raw_parts(params, len) };
	proceed_storage_access::<Block>(params);
	1
}

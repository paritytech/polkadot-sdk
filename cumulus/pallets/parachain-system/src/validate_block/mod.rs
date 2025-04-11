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

//! A module that enables a runtime to work as parachain.

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub mod implementation;
#[cfg(test)]
mod tests;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
mod trie_cache;

#[cfg(any(test, not(feature = "std")))]
#[doc(hidden)]
mod trie_recorder;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use alloc::{boxed::Box, slice};
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use bytes;
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use codec::decode_from_bytes;
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use polkadot_parachain_primitives;
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use sp_runtime::traits::GetRuntimeBlockType;
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use sp_std;

/// Basically the same as
/// [`ValidationParams`](polkadot_parachain_primitives::primitives::ValidationParams), but a little
/// bit optimized for our use case here.
///
/// `block_data` and `head_data` are represented as [`bytes::Bytes`] to make them reuse
/// the memory of the input parameter of the exported `validate_blocks` function.
///
/// The layout of this type must match exactly the layout of
/// [`ValidationParams`](polkadot_parachain_primitives::primitives::ValidationParams) to have the
/// same SCALE encoding.
#[derive(codec::Decode)]
#[cfg_attr(feature = "std", derive(codec::Encode))]
#[doc(hidden)]
pub struct MemoryOptimizedValidationParams {
	pub parent_head: bytes::Bytes,
	pub block_data: bytes::Bytes,
	pub relay_parent_number: cumulus_primitives_core::relay_chain::BlockNumber,
	pub relay_parent_storage_root: cumulus_primitives_core::relay_chain::Hash,
}

#[derive(codec::Decode, Clone)]
#[cfg_attr(feature = "std", derive(codec::Encode))]
#[cfg(feature = "runtime-benchmarks")]
pub struct StorageAccessParams<B: sp_runtime::traits::Block> {
	pub state_root: B::Hash,
	pub storage_proof: sp_trie::CompactProof,
	pub payload: StorageAccessPayload,
	pub is_dry_run: bool,
}

#[derive(Debug, Clone, codec::Decode, codec::Encode)]
#[cfg(feature = "runtime-benchmarks")]
pub enum StorageAccessPayload {
	Read(crate::Vec<(crate::Vec<u8>, Option<sp_core::storage::ChildInfo>)>),
	Write((crate::Vec<(crate::Vec<u8>, crate::Vec<u8>)>, Option<sp_core::storage::ChildInfo>)),
}

#[cfg(feature = "runtime-benchmarks")]
impl<B: sp_runtime::traits::Block> StorageAccessParams<B> {
	pub fn new_read(
		state_root: B::Hash,
		storage_proof: sp_trie::CompactProof,
		payload: crate::Vec<(crate::Vec<u8>, Option<sp_core::storage::ChildInfo>)>,
	) -> Self {
		Self {
			state_root,
			storage_proof,
			payload: StorageAccessPayload::Read(payload),
			is_dry_run: false,
		}
	}

	pub fn new_write(
		state_root: B::Hash,
		storage_proof: sp_trie::CompactProof,
		payload: (
			crate::Vec<(crate::Vec<u8>, crate::Vec<u8>)>,
			Option<sp_core::storage::ChildInfo>,
		),
	) -> Self {
		Self {
			state_root,
			storage_proof,
			payload: StorageAccessPayload::Write(payload),
			is_dry_run: false,
		}
	}

	pub fn as_dry_run(&self) -> Self {
		Self {
			state_root: self.state_root,
			storage_proof: self.storage_proof.clone(),
			payload: self.payload.clone(),
			is_dry_run: true,
		}
	}
}

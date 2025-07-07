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
pub mod trie_cache;

#[cfg(any(test, not(feature = "std")))]
#[doc(hidden)]
pub mod trie_recorder;

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
#[cfg(any(test, not(feature = "std")))]
#[doc(hidden)]
use crate::{BlockT, RelayChainBlockNumber};
#[cfg(any(test, not(feature = "std")))]
#[doc(hidden)]
use cumulus_primitives_core::ParachainBlockData;
/// Build a seed from the head data of the parachain block, use both the relay parent storage root
/// and the hash of the blocks in the block data, to make sure the seed changes every block and that
/// the user cannot find about it ahead of time.
#[cfg(any(test, not(feature = "std")))]
#[doc(hidden)]
fn build_seed_from_head_data<B: BlockT>(
	block_data: &ParachainBlockData<B>,
	relay_parent_number: RelayChainBlockNumber,
	relay_parent_storage_root: crate::relay_chain::Hash,
) -> u64 {
	let relay_parent_seed: u64 = relay_parent_storage_root.as_fixed_bytes()[..size_of::<u64>()]
		.try_into()
		.map(|bytes| u64::from_be_bytes(bytes))
		.unwrap_or(relay_parent_number as u64);
	let hash_seed: u64 = block_data
		.blocks()
		.iter()
		.filter_map(|block| {
			block.hash().as_ref()[..size_of::<u64>()]
				.try_into()
				.map(|bytes| u64::from_be_bytes(bytes))
				.ok()
		})
		.fold(relay_parent_seed, |acc, hash| acc ^ hash);

	hash_seed
}

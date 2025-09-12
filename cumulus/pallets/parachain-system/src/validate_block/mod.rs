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

/// The validation params passed to `validate_block`.
///
/// Used by the pallet to validate the validation data passed by the collator.
pub struct ValidationParams {
	pub parent_head: bytes::Bytes,
	pub relay_parent_number: cumulus_primitives_core::relay_chain::BlockNumber,
	pub relay_parent_storage_root: cumulus_primitives_core::relay_chain::Hash,
}

// Stores the [`ValidationParams`] when running `execute_block` inside of `validate_block`.
//
// The pallet uses the params to verify the `ValidationData` coming from the collator.
environmental::environmental!(validation_params: ValidationParams);

/// Run `function` with the given `validation_params` available in its context.
#[cfg(not(feature = "std"))]
fn run_with_validation_params(validation_params: &mut ValidationParams, function: impl FnOnce()) {
	validation_params::using(validation_params, function)
}

/// Run `function` with access to [`ValidationParams`].
///
/// `function` will only be executed in the `validate_block` context, as otherwise the validation
/// parameters are not set.
pub(crate) fn with_validation_params(function: impl FnOnce(&mut ValidationParams)) {
	validation_params::with(function);
}

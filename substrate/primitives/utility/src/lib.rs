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

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

//! A crate which contains primitives that are useful for the utility pallet.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

/// Align the call size to 1KB. As we are currently compiling the runtime for native/wasm
/// the `size_of` of the `Call` can be different. To ensure that this doesn't lead to
/// mismatches between native/wasm or to different metadata for the same runtime, we
/// align the call size. The value is chosen big enough to hopefully never reach it.
pub const CALL_ALIGN: u32 = 1024;

/// The limit on the number of batched calls.
pub fn batched_calls_limit<Call>() -> u32 {
	let allocator_limit = sp_core::MAX_POSSIBLE_ALLOCATION;
	let call_size =
		((sp_std::mem::size_of::<Call>() as u32 + CALL_ALIGN - 1) / CALL_ALIGN) * CALL_ALIGN;
	// The margin to take into account vec doubling capacity.
	let margin_factor = 3;

	allocator_limit / margin_factor / call_size
}

/// Helper struct containing a batch of calls.
#[derive(Clone, Debug, Encode, Decode, PartialEq, TypeInfo)]
pub struct CallsBatch<Call>(pub Vec<Call>);

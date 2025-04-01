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

// Include the WASM binary
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

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

#[cfg(not(feature = "std"))]
#[no_mangle]
pub extern "C" fn validate_block(params: *const u8, len: usize) -> u64 {
	type Block = sp_runtime::generic::Block<
		sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
		sp_runtime::OpaqueExtrinsic,
	>;
	let params = unsafe {
		cumulus_pallet_parachain_system::validate_block::slice::from_raw_parts(params, len)
	};
	cumulus_pallet_parachain_system::validate_block::implementation::proceed_storage_access::<Block>(
		params,
	);
	1
}

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

//! This contract calls the Storage pre-compile _without a delegate call_.
//! This must result in a trap, it must not be possible to call this contract
//! succesfully!

#![no_std]
#![no_main]

include!("../panic_handler.rs");
include!("../sol_utils.rs");

use uapi::{ReturnErrorCode, HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	const KEY: [u8; 32] = [1u8; 32];
	const VALUE_1: [u8; 4] = [1u8; 4];

	api::set_storage(StorageFlags::empty(), &KEY, &VALUE_1);

	let mut buffer = [0u8; 512];
	let sel = solidity_selector("containsStorage(uint32,bool,bytes)");
	buffer[..4].copy_from_slice(&sel[..4]);

	let flags = encode_u32(StorageFlags::empty().bits());
	buffer[4..36].copy_from_slice(&flags[..32]);

	encode_bool(false, &mut buffer[36..68]); // `is_fixed_key`
	let n = encode_bytes(&KEY, &mut buffer[68..]);

	let mut output = [0u8; 64]; /* function returns (bool, uint) */
	match api::call(
		CallFlags::empty(),
		&STORAGE_PRECOMPILE_ADDR,
		u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
		u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&[0u8; 32],     // Value transferred to the contract.
		&buffer[..36 /* selector + `uint32` */ + 32 /* `bool` */ + n /* `bytes` */],
		Some(&mut &mut output[..]),
	) {
		Ok(_) => api::return_value(uapi::ReturnFlags::empty(), &output[..]),
		Err(ReturnErrorCode::CalleeReverted) => api::return_value(ReturnFlags::REVERT, &output[..]),
		Err(_) => panic!(),
	}
}

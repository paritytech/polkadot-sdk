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

//! This calls another contract as passed as its account id. It also creates some storage.
#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		buffer,
		input: [u8; 4],
		callee: &[u8; 20],
		deposit_limit: &[u8; 32],
	);

	// create 4 byte of storage before calling
	api::set_storage(StorageFlags::empty(), buffer, &[1u8; 4]);

	// Call the callee
	let ret = api::call(
		uapi::CallFlags::empty(),
		callee,
		u64::MAX, /* How much ref_time weight to devote for the execution. u64::MAX = use all
		           * resources. */
		u64::MAX, /* How much proof_size weight to devote for the execution. u64::MAX = use all
		           * resources. */
		deposit_limit,
		&[0u8; 32], // Value transferred to the contract.
		input,
		None,
	);
	if let Err(code) = ret {
		api::return_value(uapi::ReturnFlags::REVERT, &(code as u32).to_le_bytes());
	};

	// create 8 byte of storage after calling
	// item of 12 bytes because we override 4 bytes
	api::set_storage(StorageFlags::empty(), buffer, &[1u8; 12]);
}

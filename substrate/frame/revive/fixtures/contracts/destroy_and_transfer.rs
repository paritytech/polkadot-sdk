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

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, u256_bytes, HostFn, HostFnImpl as api, StorageFlags};

const ADDRESS_KEY: [u8; 32] = [0u8; 32];
const VALUE: [u8; 32] = u256_bytes(65536);

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	input!(code_hash: &[u8; 32],);

	let mut address = [0u8; 20];
	let salt = [47u8; 32];

	api::instantiate(
		u64::MAX,       /* How much ref_time weight to devote for the execution. u64::MAX = use
		                 * all. */
		u64::MAX, // How much proof_size weight to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&VALUE,
		code_hash,
		Some(&mut address),
		None,
		Some(&salt),
	)
	.unwrap();

	// Return the deployed contract address.
	api::set_storage(StorageFlags::empty(), &ADDRESS_KEY, &address);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut callee_addr = [0u8; 20];
	let callee = &mut &mut callee_addr[..];
	api::get_storage(StorageFlags::empty(), &ADDRESS_KEY, callee).unwrap();
	assert!(callee.len() == 20);

	// Calling the destination contract with non-empty input data should fail.
	let res = api::call(
		uapi::CallFlags::empty(),
		&callee_addr,
		u64::MAX, // How much ref_time weight to devote for the execution. u64::MAX = use all.
		u64::MAX, // How much proof_size weight to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&VALUE,
		&[0u8; 1],
		None,
	);
	assert!(matches!(res, Err(uapi::ReturnErrorCode::CalleeTrapped)));

	// Call the destination contract regularly, forcing it to self-destruct.
	api::call(
		uapi::CallFlags::empty(),
		&callee_addr,
		u64::MAX, // How much ref_time weight to devote for the execution. u64::MAX = use all.
		u64::MAX, // How much proof_size weight to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&VALUE,
		&[0u8; 0],
		None,
	)
	.unwrap();
}

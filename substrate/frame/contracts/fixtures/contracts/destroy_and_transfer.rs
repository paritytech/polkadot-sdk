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

use common::input;
use uapi::{HostFn, HostFnImpl as api};

const ADDRESS_KEY: [u8; 32] = [0u8; 32];
const VALUE: [u8; 8] = [0, 0, 1u8, 0, 0, 0, 0, 0];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	input!(code_hash: [u8; 32],);

	let input = [0u8; 0];
	let mut address = [0u8; 32];
	let address = &mut &mut address[..];
	let salt = [71u8, 17u8];

	api::instantiate_v2(
		code_hash,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&VALUE,
		&input,
		Some(address),
		None,
		&salt,
	)
	.unwrap();

	// Return the deployed contract address.
	api::set_storage(&ADDRESS_KEY, address);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut callee_addr = [0u8; 32];
	let callee_addr = &mut &mut callee_addr[..];
	api::get_storage(&ADDRESS_KEY, callee_addr).unwrap();

	// Calling the destination contract with non-empty input data should fail.
	let res = api::call_v2(
		uapi::CallFlags::empty(),
		callee_addr,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&VALUE,
		&[0u8; 1],
		None,
	);
	assert!(matches!(res, Err(uapi::ReturnErrorCode::CalleeTrapped)));

	// Call the destination contract regularly, forcing it to self-destruct.
	api::call_v2(
		uapi::CallFlags::empty(),
		callee_addr,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None, // No deposit limit.
		&VALUE,
		&[0u8; 0],
		None,
	)
	.unwrap();
}

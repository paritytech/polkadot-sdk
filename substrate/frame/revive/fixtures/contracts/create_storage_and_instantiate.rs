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

//! This instantiates another contract and passes some input to its constructor.
#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, u256_bytes, HostFn, HostFnImpl as api, StorageFlags};

static BUFFER: [u8; 16 * 1024 + 1] = [0u8; 16 * 1024 + 1];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		code_hash: &[u8; 32],
		input: [u8; 4],
		deposit_limit: &[u8; 32],
	);

	let len = u32::from_le_bytes(input.try_into().unwrap());
	let data = &BUFFER[..len as usize];
	let mut key = [0u8; 32];
	key[0] = 1;
	api::set_storage(StorageFlags::empty(), &key, data);

	let value = u256_bytes(10_000u64);
	let salt = [0u8; 32];
	let mut address = [0u8; 20];
	let mut deploy_input = [0; 32 + 4];
	deploy_input[..32].copy_from_slice(code_hash);
	deploy_input[32..].copy_from_slice(&input);

	let ret = api::instantiate(
		u64::MAX, // How much ref_time weight to devote for the execution. u64::MAX = use all.
		u64::MAX, // How much proof_size weight to devote for the execution. u64::MAX = use all.
		deposit_limit,
		&value,
		&deploy_input,
		Some(&mut address),
		None,
		Some(&salt),
	);
	if let Err(code) = ret {
		api::return_value(uapi::ReturnFlags::REVERT, &(code as u32).to_le_bytes());
	};

	// Return the deployed contract address.
	api::return_value(uapi::ReturnFlags::empty(), &address);
}

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

//! This calls another contract as passed as its account id. It also creates some transient storage.
#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api, StorageFlags};

static BUFFER: [u8; 416] = [0u8; 416];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		buffer,
		len: u32,
		input: [u8; 4],
		callee: &[u8; 20],
	);

	let rounds = len as usize / BUFFER.len();
	let rest = len as usize / BUFFER.len();
	for i in 0..rounds {
		api::set_storage(StorageFlags::TRANSIENT, &i.to_le_bytes(), &BUFFER);
	}
	api::set_storage(StorageFlags::TRANSIENT, &u32::MAX.to_le_bytes(), &BUFFER[..rest]);

	// Call the callee
	api::call(
		uapi::CallFlags::empty(),
		callee,
		u64::MAX,       // How much ref_time weight to devote for the execution. u64::MAX = all.
		u64::MAX,       // How much proof_size weight to devote for the execution. u64::MAX = all.
		&[u8::MAX; 32], // No deposit limit.
		&[0u8; 32],     // Value transferred to the contract.
		input,
		None,
	)
	.unwrap();
}

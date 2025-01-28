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

use common::input;
use uapi::{HostFn, HostFnImpl as api};

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
		callee: [u8; 32],
	);

	let data = [0u8; 16 * 1024];
	let value = &data[..len as usize];
	#[allow(deprecated)]
	api::set_transient_storage(buffer, value);

	// Call the callee
	api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		None,
		&0u64.to_le_bytes(), // Value transferred to the contract.
		input,
		None,
	)
	.unwrap();
}

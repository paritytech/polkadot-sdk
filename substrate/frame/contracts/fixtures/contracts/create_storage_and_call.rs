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
		input: [u8; 4],
		callee: [u8; 32],
		deposit_limit: [u8; 8],
	);

	// create 4 byte of storage before calling
	api::set_storage(buffer, &[1u8; 4]);

	// Call the callee
	api::call_v2(
		uapi::CallFlags::empty(),
		callee,
		0u64, // How much ref_time weight to devote for the execution. 0 = all.
		0u64, // How much proof_size weight to devote for the execution. 0 = all.
		Some(deposit_limit),
		&0u64.to_le_bytes(), // Value transferred to the contract.
		input,
		None,
	)
	.unwrap();

	// create 8 byte of storage after calling
	// item of 12 bytes because we override 4 bytes
	api::set_storage(buffer, &[1u8; 12]);
}

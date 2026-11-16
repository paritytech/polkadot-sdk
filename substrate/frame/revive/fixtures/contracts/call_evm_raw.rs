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

//! Calls the `call_evm` host function directly, without the uapi wrapper, so the caller can pass
//! undefined flags and a value pointer outside the contract's memory. Returns the call return code.
#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api, ReturnFlags};

#[polkavm_derive::polkavm_import]
extern "C" {
	pub fn call_evm(
		flags: u32,
		callee: u32,
		value_ptr: u32,
		gas: u64,
		input_data: u64,
		output_data: u64,
	) -> u32;
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		callee: &[u8; 20],
		flags: u32,
		gas: u64,
		value_out_of_bounds: u32,
	);

	let value = [0u8; 32];
	let value_ptr = if value_out_of_bounds == 0 { value.as_ptr() as u32 } else { u32::MAX };
	// Empty input at a valid address, and the sentinel output pointer so nothing is written back.
	let input_data = value.as_ptr() as u64;
	let output_data = u32::MAX as u64;
	let return_code =
		unsafe { call_evm(flags, callee.as_ptr() as u32, value_ptr, gas, input_data, output_data) };
	api::return_value(ReturnFlags::empty(), &return_code.to_le_bytes());
}

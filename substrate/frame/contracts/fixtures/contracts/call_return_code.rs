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

//! This calls the supplied dest and transfers 100 balance during this call and copies
//! the return code of this call to the output buffer.
//! It also forwards its input to the callee.
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
		100,
		callee_addr: [u8; 32],
		input: [u8],
	);

	// Call the callee
	let err_code = match api::call_v2(
		uapi::CallFlags::empty(),
		callee_addr,
		0u64,                  // How much ref_time to devote for the execution. 0 = all.
		0u64,                  // How much proof_size to devote for the execution. 0 = all.
		None,                  // No deposit limit.
		&100u64.to_le_bytes(), // Value transferred to the contract.
		input,
		None,
	) {
		Ok(_) => 0u32,
		Err(code) => code as u32,
	};

	api::return_value(uapi::ReturnFlags::empty(), &err_code.to_le_bytes());
}

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

//! This fixture calls itself as many times as passed as argument.

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, ReturnFlags, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(calls_left: u32, );

	// own address
	let mut addr = [0u8; 20];
	api::address(&mut addr);

	let mut return_buffer = calls_left.to_le_bytes();

	if calls_left > 0 {
		let _ = api::call(
			uapi::CallFlags::ALLOW_REENTRY,
			&addr,
			u64::MAX, // How much ref_time to devote for the execution. u64::MAX = use all resources.
			u64::MAX, // How much proof_size to devote for the execution. u64::MAX = use all resources.
			&[u8::MAX; 32], // No deposit limit.
			&[0u8; 32], // Value transferred to the contract.
			&(calls_left - 1).to_le_bytes(),
			Some(&mut &mut return_buffer[..]),
		);
	}

	api::return_value(ReturnFlags::empty(), &return_buffer);
}

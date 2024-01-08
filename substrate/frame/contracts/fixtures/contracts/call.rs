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

//! This calls another contract as passed as its account id.
#![no_std]
#![no_main]

extern crate common;
use uapi::{CallFlags, HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut buffer = [0u8; 40];
	let callee_input = 0..4;
	let callee_addr = 4..36;
	let value = 36..40;

	// Read the input data.
	api::input(&mut &mut buffer[..]);

	// Call the callee
	api::call_v1(
		CallFlags::empty(),
		&buffer[callee_addr],
		0u64, // How much gas to devote for the execution. 0 = all.
		&buffer[value],
		&buffer[callee_input],
		None,
	)
	.unwrap();
}

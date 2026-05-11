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

//! This fixture delegate-calls another contract and returns its output.

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		512,
		callee: &[u8; 20],
		callee_input: [u8],
	);

	let mut output = [0u8; 32];
	let output = &mut &mut output[..];

	api::delegate_call(
		uapi::CallFlags::empty(),
		callee,
		u64::MAX,
		u64::MAX,
		&[u8::MAX; 32],
		callee_input,
		Some(output),
	)
	.unwrap();

	api::return_value(uapi::ReturnFlags::empty(), output);
}

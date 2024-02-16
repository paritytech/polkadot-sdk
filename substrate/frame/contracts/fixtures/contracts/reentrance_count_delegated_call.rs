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

// This fixture tests if account_reentrance_count works as expected.
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
		input,
		code_hash: [u8; 32],
		call_stack_height: u32,
	);

	let call_stack_height = call_stack_height + 1;

	#[allow(deprecated)]
	let reentrance_count = api::reentrance_count();

	// Reentrance count stays 0.
	assert_eq!(reentrance_count, 0);

	// Re-enter 5 times in a row and assert that the reentrant counter works as expected.
	if call_stack_height != 5 {
		let mut input = [0u8; 36];
		input[0..32].copy_from_slice(code_hash);
		input[32..36].copy_from_slice(&call_stack_height.to_le_bytes());
		api::delegate_call(uapi::CallFlags::empty(), code_hash, &input, None).unwrap();
	}
}

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

//! Call chain extension by passing through input and output of this contract.
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
	input!(input, 8, func_id: u32,);

	// the chain extension passes through the input and returns it as output
	let mut output_buffer = [0u8; 32];
	let output = &mut &mut output_buffer[0..input.len()];

	let ret_id = api::call_chain_extension(func_id, input, Some(output));
	assert_eq!(ret_id, func_id);

	api::return_value(uapi::ReturnFlags::empty(), output);
}

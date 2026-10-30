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

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{input, HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

/// Called by the tests.
///
/// The input bytes encode the data that is directly fed into the Keccak-256 bit
/// crypto hash function. The result is put into the output buffer.
///
/// After contract execution the test driver then asserts that the returned
/// values are equal to the expected bytes for the input and hash function.

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		256,
		input: [u8],
	);

	let mut output = [0u8; 32];
	api::hash_keccak_256(input, &mut output);
	api::return_value(uapi::ReturnFlags::empty(), &output);
}

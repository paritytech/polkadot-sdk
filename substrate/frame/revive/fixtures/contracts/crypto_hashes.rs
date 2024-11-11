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

use common::input;
use uapi::{HostFn, HostFnImpl as api};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

/// Called by the tests.
///
/// The `call` function expects data in a certain format in the input buffer.
///
/// 1. The first byte encodes an identifier for the crypto hash function under test. (*)
/// 2. The rest encodes the input data that is directly fed into the crypto hash function chosen in
///    1.
///
/// The `deploy` function then computes the chosen crypto hash function
/// given the input and puts the result into the output buffer.
/// After contract execution the test driver then asserts that the returned
/// values are equal to the expected bytes for the input and chosen hash
/// function.
///
/// (*) The possible value for the crypto hash identifiers can be found below:
///
/// | value | Algorithm | Bit Width |
/// |-------|-----------|-----------|
/// |     0 |      SHA2 |       256 |
/// |     1 |    KECCAK |       256 |
/// |     2 |    BLAKE2 |       256 |
/// |     3 |    BLAKE2 |       128 |
/// ---------------------------------

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		256,
		chosen_hash_fn: u8,
		input: [u8],
	);

	match chosen_hash_fn {
		1 => {
			let mut output = [0u8; 32];
			api::hash_sha2_256(input, &mut output);
			api::return_value(uapi::ReturnFlags::empty(), &output);
		},
		2 => {
			let mut output = [0u8; 32];
			api::hash_keccak_256(input, &mut output);
			api::return_value(uapi::ReturnFlags::empty(), &output);
		},
		3 => {
			let mut output = [0u8; 32];
			api::hash_blake2_256(input, &mut output);
			api::return_value(uapi::ReturnFlags::empty(), &output);
		},
		4 => {
			let mut output = [0u8; 16];
			api::hash_blake2_128(input, &mut output);
			api::return_value(uapi::ReturnFlags::empty(), &output);
		},
		_ => panic!("unknown crypto hash function identifier"),
	}
}

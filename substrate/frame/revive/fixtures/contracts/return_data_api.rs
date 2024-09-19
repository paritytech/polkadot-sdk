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

//! This tests that the correct output data is written when the provided
//! output buffer length is smaller than what was actually returned during
//! calls and instantiations.
//!
//! To not need an additional callee fixture, we call ourself recursively
//! and also instantiate our own code hash (constructor and recursive calls
//! always return `BUF_SIZE` bytes of data).

#![no_std]
#![no_main]

use common::{input, u256_bytes};
use uapi::{HostFn, HostFnImpl as api};

const BUF_SIZE: usize = 128;
static DATA: [u8; BUF_SIZE] = [0xff; BUF_SIZE];

fn assert_call(callee: &[u8; 20]) {
	let value = &[0u8; 32];
	let mut input = DATA;

	input[0] = 0;
	api::call(uapi::CallFlags::empty(), callee, 0u64, 0u64, None, value, &input, None).unwrap();

	assert_return_data_size_of(BUF_SIZE as u64);
}

/// Instantiate `code_hash` and expect the return_data_{size,copy} APIs to work correctly.
fn assert_instantiate(code_hash: &[u8; 32]) -> [u8; 20] {
	let value = &[0; 32];
	let mut input = DATA;
	let mut address_buf = [0; 20];

	/// The return data API should work for reverted executions too
	for exit_flag in [0, 1] {
		input[0] = exit_flag;
		input[7] = exit_flag;
		let _ = api::instantiate(
			code_hash,
			0u64,
			0u64,
			None,
			&value,
			&input,
			Some(&mut address_buf),
			None,
			None,
		);
		assert_return_data_size_of(BUF_SIZE as u64 - 4);
		assert_plain_transfer_does_not_reset(BUF_SIZE as u64 - 4);

		reset_return_data();
	}

	address_buf
}

fn recursion_guard() -> [u8; 20] {
	let mut caller_address = [0u8; 20];
	api::caller(&mut caller_address);

	let mut own_address = [0u8; 20];
	api::address(&mut own_address);

	assert_ne!(caller_address, own_address);

	own_address
}

/// Call ourselves recursively, which panics the callee and thus resets the return data.
fn reset_return_data() {
	let mut output_buf = [0u8; BUF_SIZE];

	let own_address = recursion_guard();

	let mut output_buf = [0; BUF_SIZE];
	let return_buf = &mut &mut output_buf[..];
	api::call(
		uapi::CallFlags::ALLOW_REENTRY,
		&own_address,
		0u64,
		0u64,
		None,
		&[0u8; 32],
		&[],
		Some(return_buf),
	)
	.unwrap_err();
	//assert_eq!(return_buf.len(), 0);
}

fn assert_return_data_size_of(expected: u64) {
	let mut return_data_size = [0xff; 32];
	api::return_data_size(&mut return_data_size);
	assert_eq!(return_data_size, u256_bytes(expected));
}

/// A plain transfer should not reset the return data.
fn assert_plain_transfer_does_not_reset(expected: u64) {
	api::transfer(&[0; 20], &u256_bytes(128)).unwrap();
	assert_return_data_size_of(expected);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(code_hash: &[u8; 32],);

	recursion_guard();

	// we didn't do any calls yet; return data size should be 0
	assert_return_data_size_of(0);

	let callee = assert_instantiate(code_hash);
	//assert_call(&callee);
}

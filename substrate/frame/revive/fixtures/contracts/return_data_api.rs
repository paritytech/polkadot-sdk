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

//! This tests that the `return_data_size` and `return_data_copy` APIs work.
//!
//! It does so by calling and instantiating the "return_with_data" fixture,
//! which always echoes back the input[4..] regardless of the call outcome.
//!
//! We also check that the saved return data is properly reset after a trap
//! and unaffected by plain transfers.

#![no_std]
#![no_main]

use common::{input, u256_bytes};
use uapi::{HostFn, HostFnImpl as api};

const INPUT_BUF_SIZE: usize = 128;
static INPUT_DATA: [u8; INPUT_BUF_SIZE] = [0xFF; INPUT_BUF_SIZE];
/// The "return_with_data" fixture echoes back 4 bytes less than the input
const OUTPUT_BUF_SIZE: usize = INPUT_BUF_SIZE - 4;
static OUTPUT_DATA: [u8; OUTPUT_BUF_SIZE] = [0xEE; OUTPUT_BUF_SIZE];

fn assert_return_data_after_call(input: &[u8]) {
	assert_return_data_size_of(OUTPUT_BUF_SIZE as u64);
	assert_plain_transfer_does_not_reset(OUTPUT_BUF_SIZE as u64);
	assert_return_data_copy(&input[4..]);
	reset_return_data();
}

/// Assert that what we get from [api::return_data_copy] matches `whole_return_data`,
/// either fully or partially with an offset and limited size.
fn assert_return_data_copy(whole_return_data: &[u8]) {
	// The full return data should match
	let mut buf = OUTPUT_DATA;
	let mut full = &mut buf[..whole_return_data.len()];
	api::return_data_copy(&mut full, 0);
	assert_eq!(whole_return_data, full);

	// Partial return data should match
	let mut buf = OUTPUT_DATA;
	let offset = 5; // we just pick some offset
	let size = 32; // we just pick some size
	let mut partial = &mut buf[offset..offset + size];
	api::return_data_copy(&mut partial, offset as u32);
	assert_eq!(*partial, whole_return_data[offset..offset + size]);
}

/// This function panics in a recursive contract call context.
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
	api::call(
		uapi::CallFlags::ALLOW_REENTRY,
		&recursion_guard(),
		0u64,
		0u64,
		None,
		&[0u8; 32],
		&[0u8; 32],
		None,
	)
	.unwrap_err();
	assert_return_data_size_of(0);
}

/// Assert [api::return_data_size] to match the `expected` value.
fn assert_return_data_size_of(expected: u64) {
	let mut return_data_size = [0xff; 32];
	api::return_data_size(&mut return_data_size);
	assert_eq!(return_data_size, u256_bytes(expected));
}

/// Assert [api::return_data_size] to match the `expected` value after a plain transfer
/// (plain transfers don't issue a call and so should not reset the return data)
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

	// We didn't do anything yet; return data size should be 0
	assert_return_data_size_of(0);

	recursion_guard();

	let mut address_buf = [0; 20];
	let construct_input = |exit_flag| {
		let mut input = INPUT_DATA;
		input[0] = exit_flag;
		input[9] = 7;
		input[17 / 2] = 127;
		input[89 / 2] = 127;
		input
	};
	let mut instantiate = |exit_flag| {
		api::instantiate(
			code_hash,
			0u64,
			0u64,
			None,
			&[0; 32],
			&construct_input(exit_flag),
			Some(&mut address_buf),
			None,
			None,
		)
	};
	let call = |exit_flag, address_buf| {
		api::call(
			uapi::CallFlags::empty(),
			address_buf,
			0u64,
			0u64,
			None,
			&[0; 32],
			&construct_input(exit_flag),
			None,
		)
	};

	instantiate(0).unwrap();
	assert_return_data_after_call(&construct_input(0)[..]);

	instantiate(1).unwrap_err();
	assert_return_data_after_call(&construct_input(1)[..]);

	call(0, &address_buf).unwrap();
	assert_return_data_after_call(&construct_input(0)[..]);

	call(1, &address_buf).unwrap_err();
	assert_return_data_after_call(&construct_input(1)[..]);
}

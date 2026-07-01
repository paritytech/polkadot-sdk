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
include!("../panic_handler.rs");

use uapi::{input, u256_bytes, HostFn, HostFnImpl as api};

const INPUT_BUF_SIZE: usize = 128;
static INPUT_DATA: [u8; INPUT_BUF_SIZE] = [0xFF; INPUT_BUF_SIZE];
/// The "return_with_data" fixture echoes back 4 bytes less than the input
const OUTPUT_BUF_SIZE: usize = INPUT_BUF_SIZE - 4;
static OUTPUT_DATA: [u8; OUTPUT_BUF_SIZE] = [0xEE; OUTPUT_BUF_SIZE];

/// Assert correct return data after calls and finally reset the return data.
fn assert_return_data_after_call(input: &[u8]) {
	assert_return_data_size_of(OUTPUT_BUF_SIZE as u64);
	assert_return_data_copy(&input[4..]);
	assert_balance_transfer_does_reset();
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

/// Assert [api::return_data_size] to match the `expected` value.
fn assert_return_data_size_of(expected: u64) {
	assert_eq!(api::return_data_size(), expected);
}

/// Assert the return data to be reset after a balance transfer.
fn assert_balance_transfer_does_reset() {
	api::call(
		uapi::CallFlags::empty(),
		&[0u8; 20],
		u64::MAX,
		u64::MAX,
		&[u8::MAX; 32],
		&u256_bytes(128_000_000),
		&[],
		None,
	)
	.unwrap();
	assert_return_data_size_of(0);
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
		let input = construct_input(exit_flag);
		let mut deploy_input = [0; 32 + INPUT_BUF_SIZE];
		deploy_input[..32].copy_from_slice(code_hash);
		deploy_input[32..].copy_from_slice(&input);
		api::instantiate(
			u64::MAX,
			u64::MAX,
			&[u8::MAX; 32],
			&[0; 32],
			&deploy_input,
			Some(&mut address_buf),
			None,
			None,
		)
	};
	let call = |exit_flag, address_buf| {
		api::call(
			uapi::CallFlags::empty(),
			address_buf,
			u64::MAX,
			u64::MAX,
			&[u8::MAX; 32],
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

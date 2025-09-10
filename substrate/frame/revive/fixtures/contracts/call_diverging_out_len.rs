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
include!("../panic_handler.rs");

use uapi::{HostFn, HostFnImpl as api, u256_bytes};

const BUF_SIZE: usize = 8;
static DATA: [u8; BUF_SIZE] = [1, 2, 3, 4, 5, 6, 7, 8];

/// Call `callee_address` with an output buf of size `N`
/// and expect the call output to match `expected_output`.
fn assert_call<const N: usize>(callee_address: &[u8; 20], expected_output: [u8; BUF_SIZE]) {
	let mut output_buf = [0u8; BUF_SIZE];
	let output_buf_capped = &mut &mut output_buf[..N];

	api::call(
		uapi::CallFlags::ALLOW_REENTRY,
		callee_address,
		u64::MAX,
		u64::MAX,
		&[u8::MAX; 32],
		&[0u8; 32],
		&[],
		Some(output_buf_capped),
	)
	.unwrap();

	// The (capped) output buf should get properly resized
	assert_eq!(output_buf_capped.len(), N);
	assert_eq!(output_buf, expected_output);
}

/// Instantiate this contract with an output buf of size `N`
/// and expect the instantiate output to match `expected_output`.
fn assert_instantiate<const N: usize>(expected_output: [u8; BUF_SIZE]) {
	let mut output_buf1 = [0u8; 32];
	let output1 = &mut &mut output_buf1[..];
	let _ = api::call(
		uapi::CallFlags::READ_ONLY,
		&uapi::SYSTEM_PRECOMPILE_ADDR,
		u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
		u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
		&[u8::MAX; 32], // No deposit limit.
		&[0u8; 32],     // Value transferred to the contract.
		&uapi::solidity_selector("ownCodeHash()"),
		Some(output1),
	).unwrap();
	assert_ne!(output_buf1, [0u8; 32]);

	let mut output_buf = [0u8; BUF_SIZE];
	let output_buf_capped = &mut &mut output_buf[..N];

	api::instantiate(
		u64::MAX,
		u64::MAX,
		&[u8::MAX; 32],
		&u256_bytes(0),
		output_buf1.clone().as_slice(),
		None,
		Some(output_buf_capped),
		None,
	)
	.unwrap();

	// The (capped) output buf should get properly resized
	assert_eq!(output_buf_capped.len(), N);
	assert_eq!(output_buf, expected_output);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	api::return_value(uapi::ReturnFlags::empty(), &DATA);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let mut caller_address = [0u8; 20];
	api::caller(&mut caller_address);

	let mut callee_address = [0u8; 20];
	api::address(&mut callee_address);

	// we already recurse; return data
	if caller_address == callee_address {
		api::return_value(uapi::ReturnFlags::empty(), &DATA);
	}

	assert_call::<0>(&callee_address, [0; 8]);
	assert_call::<4>(&callee_address, [1, 2, 3, 4, 0, 0, 0, 0]);

	assert_instantiate::<0>([0; 8]);
	assert_instantiate::<4>([1, 2, 3, 4, 0, 0, 0, 0]);
}

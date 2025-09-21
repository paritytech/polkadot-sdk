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

// const DJANGO_FALLBACK: [u8; 20] = [4u8; 20];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
	// make sure that the deposit for the immutable data is refunded
	api::set_immutable_data(&[1, 2, 3, 4, 5])
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	// If the input data is not empty, then recursively call self with empty input data.
	// This should trap instead of self-destructing since a contract cannot be removed, while it's
	// in the execution stack. If the recursive call traps, then trap here as well.
	input!(input, 4,);

	if !input.is_empty() {
		let mut addr = [0u8; 20];
		api::address(&mut addr);

		api::call(
			uapi::CallFlags::ALLOW_REENTRY,
			&addr,
			u64::MAX,       // How much ref_time to devote for the execution. u64 = all.
			u64::MAX,       // How much proof_size to devote for the execution. u64 = all.
			&[u8::MAX; 32], // No deposit limit.
			&[0u8; 32],     // Value to transfer.
			&[0u8; 0],
			None,
		)
		.unwrap();
	} else {
		// // Try to terminate and give balance to django.
		// api::terminate(&DJANGO_FALLBACK);


		// Call the system terminate precompile instead of the host helper.
		// Build calldata: 4-byte selector + 32-byte ABI-encoded address (right-aligned).
		// Compute the selector as keccak256("terminate(address)")[:4] and put it in TERMINATE_SELECTOR.
		// Set SYSTEM_PRECOMPILE_ADDR to the 20-byte builtin address you registered for the System builtin.
        let _ = api::call(
            uapi::CallFlags::READ_ONLY,
            &uapi::SYSTEM_PRECOMPILE_ADDR,
            u64::MAX,       // How much ref_time to devote for the execution. u64::MAX = use all.
            u64::MAX,       // How much proof_size to devote for the execution. u64::MAX = use all.
            &[u8::MAX; 32], // No deposit limit.
            &[0u8; 32],     // Value transferred to the contract.
            &uapi::solidity_selector("terminate()"),
            None,  // output
        ).unwrap();
		// const TERMINATE_SELECTOR: [u8; 4] = [0x0b, 0x0c, 0x0d, 0x0e];
		// const SYSTEM_PRECOMPILE_ADDR: [u8; 20] = [
		// 	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		// 	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00,
		// ];

		// let mut calldata = [0u8; 4 + 32];
		// calldata[0..4].copy_from_slice(&TERMINATE_SELECTOR);
		// // ABI encode address: right-align into 32 bytes (pad with 12 leading zeros).
		// calldata[4 + 12..4 + 32].copy_from_slice(&DJANGO_FALLBACK);

		// // call the precompile (allow reentry if needed)
		// api::call(
		// 	uapi::CallFlags::ALLOW_REENTRY,
		// 	&SYSTEM_PRECOMPILE_ADDR,
		// 	u64::MAX,
		// 	u64::MAX,
		// 	&[u8::MAX; 32], // deposit_limit (use appropriate value for your runtime)
		// 	&[0u8; 32],     // value
		// 	&calldata,
		// 	None,
		// )
		// .unwrap();
	}
}
